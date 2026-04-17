use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{compiler_fence, Ordering as AtomicOrdering};

use swiftlib::{mmio, port, privileged, task, time};

const PCI_CFG_ADDR_PORT: u16 = 0xCF8;
const PCI_CFG_DATA_PORT: u16 = 0xCFC;

const PCI_COMMAND_IO: u16 = 1 << 0;
const PCI_COMMAND_MEM: u16 = 1 << 1;
const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;

const CLASS_NETWORK: u8 = 0x02;
const VIRTIO_NET_F_MAC: u32 = 5;
const VIRTIO_NET_F_STATUS: u32 = 16;

const VIRTIO_PIO_DEVICE_FEATURES: u16 = 0x00;
const VIRTIO_PIO_GUEST_FEATURES: u16 = 0x04;
const VIRTIO_PIO_QUEUE_ADDR_PFN: u16 = 0x08;
const VIRTIO_PIO_QUEUE_SIZE: u16 = 0x0C;
const VIRTIO_PIO_QUEUE_SELECT: u16 = 0x0E;
const VIRTIO_PIO_QUEUE_NOTIFY: u16 = 0x10;
const VIRTIO_PIO_DEVICE_STATUS: u16 = 0x12;
const VIRTIO_PIO_ISR_STATUS: u16 = 0x13;
const VIRTIO_PIO_DEVICE_CONFIG: u16 = 0x14;

const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1 << 0;
const VIRTIO_STATUS_DRIVER: u8 = 1 << 1;
const VIRTIO_STATUS_DRIVER_OK: u8 = 1 << 2;

const VIRTIO_QUEUE_RX: u16 = 0;
const VIRTIO_QUEUE_TX: u16 = 1;
const PAGE_SIZE: usize = 4096;
const RX_BUFFER_LEN: u32 = 2048;
const RX_BUFFER_COUNT: usize = 32;
const ETH_TYPE_ARP: u16 = 0x0806;
const ETH_TYPE_IPV4: u16 = 0x0800;
const ARP_OP_REQUEST: u16 = 1;
const ARP_OP_REPLY: u16 = 2;
const IP_PROTO_ICMP: u8 = 1;
const ICMP_ECHO_REQUEST: u8 = 8;
const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_ID: u16 = 0x1337;
const ICMP_ECHO_SEQ: u16 = 1;
const GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];
const LOCAL_IP: [u8; 4] = [10, 0, 2, 15];

const VRING_DESC_F_WRITE: u16 = 2;
const VIRTIO_NET_HDR_LEN: usize = 10;
const RX_POLL_BUDGET: usize = 64;
const TX_POLL_BUDGET: usize = 64;

#[derive(Clone, Copy, Debug)]
struct PciBdf {
    bus: u8,
    device: u8,
    function: u8,
}

#[derive(Clone, Copy, Debug)]
enum NetKind {
    VirtioNet,
    E1000,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
struct NetDevice {
    bdf: PciBdf,
    vendor_id: u16,
    device_id: u16,
    kind: NetKind,
    bar0: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VringDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VringUsedElem {
    id: u32,
    len: u32,
}

#[derive(Clone, Copy)]
struct SharedBuf {
    virt: *mut u8,
    phys: u64,
    len: u32,
}

struct VirtQueue {
    index: u16,
    size: u16,
    base: *mut u8,
    phys: u64,
    avail_ring_off: usize,
    used_ring_off: usize,
    next_avail_idx: u16,
    last_used_idx: u16,
}

impl VirtQueue {
    fn desc_ptr(&self, idx: u16) -> *mut VringDesc {
        unsafe { self.base.add(idx as usize * core::mem::size_of::<VringDesc>()) as *mut VringDesc }
    }

    fn avail_idx_ptr(&self) -> *mut u16 {
        unsafe { self.base.add(self.avail_ring_off + 2) as *mut u16 }
    }

    fn avail_ring_entry_ptr(&self, slot: usize) -> *mut u16 {
        unsafe { self.base.add(self.avail_ring_off + 4 + slot * 2) as *mut u16 }
    }

    fn used_idx(&self) -> u16 {
        unsafe { read_volatile(self.base.add(self.used_ring_off + 2) as *const u16) }
    }

    fn used_elem(&self, slot: usize) -> VringUsedElem {
        unsafe {
            read_volatile(
                self.base.add(self.used_ring_off + 4 + slot * core::mem::size_of::<VringUsedElem>())
                    as *const VringUsedElem,
            )
        }
    }
}

struct VirtioNetRuntime {
    base: u16,
    mac: [u8; 6],
    rxq: VirtQueue,
    txq: VirtQueue,
    rx_bufs: Vec<SharedBuf>,
    tx_buf: SharedBuf,
    tx_inflight: bool,
    arp_sent: bool,
    gateway_mac: Option<[u8; 6]>,
    ping_sent: bool,
    ping_reply_seen: bool,
    ping_pending: bool,
    ticks: u64,
}

fn pci_config_address(bdf: PciBdf, offset: u8) -> u32 {
    0x8000_0000
        | ((bdf.bus as u32) << 16)
        | ((bdf.device as u32) << 11)
        | ((bdf.function as u32) << 8)
        | (u32::from(offset) & 0xFC)
}

fn pci_read_u32(bdf: PciBdf, offset: u8) -> u32 {
    let addr = pci_config_address(bdf, offset);
    port::outl(PCI_CFG_ADDR_PORT, addr);
    port::inl(PCI_CFG_DATA_PORT)
}

fn pci_read_u16(bdf: PciBdf, offset: u8) -> u16 {
    let aligned = offset & 0xFC;
    let shift = u32::from(offset & 0x02) * 8;
    ((pci_read_u32(bdf, aligned) >> shift) & 0xFFFF) as u16
}

fn pci_write_u16(bdf: PciBdf, offset: u8, value: u16) {
    let aligned = offset & 0xFC;
    let mut reg = pci_read_u32(bdf, aligned);
    let shift = u32::from(offset & 0x02) * 8;
    reg &= !(0xFFFFu32 << shift);
    reg |= u32::from(value) << shift;
    let addr = pci_config_address(bdf, aligned);
    port::outl(PCI_CFG_ADDR_PORT, addr);
    port::outl(PCI_CFG_DATA_PORT, reg);
}

fn pci_function_exists(bdf: PciBdf) -> bool {
    pci_read_u16(bdf, 0x00) != 0xFFFF
}

fn classify_net_device(vendor_id: u16, device_id: u16) -> NetKind {
    match (vendor_id, device_id) {
        // virtio-net (legacy/transitional)
        (0x1AF4, 0x1000) => NetKind::VirtioNet,
        // virtio-net (modern)
        (0x1AF4, 0x1041) => NetKind::VirtioNet,
        // Intel e1000 family (QEMUでよく使う)
        (0x8086, 0x100E) | (0x8086, 0x100F) | (0x8086, 0x10D3) => NetKind::E1000,
        _ => NetKind::Unknown,
    }
}

fn enable_device_command_bits(bdf: PciBdf) {
    let mut command = pci_read_u16(bdf, 0x04);
    command |= PCI_COMMAND_IO | PCI_COMMAND_MEM | PCI_COMMAND_BUS_MASTER;
    pci_write_u16(bdf, 0x04, command);
}

fn find_network_devices() -> Vec<NetDevice> {
    let mut devices = Vec::new();

    for bus in 0u16..=255 {
        for device in 0u16..32 {
            let bdf0 = PciBdf {
                bus: bus as u8,
                device: device as u8,
                function: 0,
            };
            if !pci_function_exists(bdf0) {
                continue;
            }

            let header = pci_read_u32(bdf0, 0x0C);
            let header_type = ((header >> 16) & 0xFF) as u8;
            let function_count = if (header_type & 0x80) != 0 { 8 } else { 1 };

            for function in 0..function_count {
                let bdf = PciBdf {
                    bus: bus as u8,
                    device: device as u8,
                    function: function as u8,
                };
                if !pci_function_exists(bdf) {
                    continue;
                }

                let class_reg = pci_read_u32(bdf, 0x08);
                let class_code = ((class_reg >> 24) & 0xFF) as u8;
                if class_code != CLASS_NETWORK {
                    continue;
                }

                let vendor_device = pci_read_u32(bdf, 0x00);
                let vendor_id = (vendor_device & 0xFFFF) as u16;
                let device_id = ((vendor_device >> 16) & 0xFFFF) as u16;
                let bar0 = pci_read_u32(bdf, 0x10);
                let kind = classify_net_device(vendor_id, device_id);

                devices.push(NetDevice {
                    bdf,
                    vendor_id,
                    device_id,
                    kind,
                    bar0,
                });
            }
        }
    }

    devices
}

fn try_map_mmio_bar0(dev: NetDevice) {
    if (dev.bar0 & 0x1) != 0 {
        if let NetKind::VirtioNet = dev.kind {
            println!("[NETDRV] BAR0 is I/O space (legacy virtio-net PIO)");
        } else {
            println!("[NETDRV] BAR0 is I/O space (PIO), MMIO map skipped");
        }
        return;
    }

    let mmio_base = u64::from(dev.bar0 & 0xFFFF_FFF0);
    if mmio_base == 0 {
        println!("[NETDRV] BAR0 MMIO base is zero");
        return;
    }

    match mmio::map_physical(mmio_base, 0x1000) {
        Ok(mapped) => {
            println!(
                "[NETDRV] MMIO mapped phys={:#x} -> virt={:#x}",
                mmio_base, mapped as u64
            );
        }
        Err(errno) => {
            println!(
                "[NETDRV] MMIO map failed phys={:#x}, errno={}",
                mmio_base, errno
            );
        }
    }
}

fn virtio_pio_base(bar0: u32) -> Option<u16> {
    if (bar0 & 0x1) == 0 {
        return None;
    }
    let base = bar0 & 0xFFFF_FFFC;
    if base == 0 || base > 0xFFFF {
        return None;
    }
    Some(base as u16)
}

fn virtio_legacy_init_pio(dev: NetDevice) -> Option<VirtioNetRuntime> {
    let Some(base) = virtio_pio_base(dev.bar0) else {
        println!("[NETDRV] virtio-net BAR0 is not legacy PIO");
        return None;
    };

    let device_features = port::inl(base + VIRTIO_PIO_DEVICE_FEATURES);
    let guest_features = device_features & ((1u32 << VIRTIO_NET_F_MAC) | (1u32 << VIRTIO_NET_F_STATUS));
    println!("[NETDRV] virtio legacy PIO base={:#x}", base);
    println!("[NETDRV] virtio device_features={:#010x}", device_features);
    println!("[NETDRV] virtio guest_features ={:#010x}", guest_features);

    port::outb(base + VIRTIO_PIO_DEVICE_STATUS, 0);
    port::outb(
        base + VIRTIO_PIO_DEVICE_STATUS,
        VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
    );
    port::outl(base + VIRTIO_PIO_GUEST_FEATURES, guest_features);
    port::outb(
        base + VIRTIO_PIO_DEVICE_STATUS,
        VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK,
    );

    let status = port::inb(base + VIRTIO_PIO_DEVICE_STATUS);
    println!("[NETDRV] virtio status={:#04x}", status);

    let mut mac = [0u8; 6];
    if (guest_features & (1u32 << VIRTIO_NET_F_MAC)) != 0 {
        for (i, byte) in mac.iter_mut().enumerate() {
            *byte = port::inb(base + VIRTIO_PIO_DEVICE_CONFIG + i as u16);
        }
        println!(
            "[NETDRV] virtio MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );
    } else {
        println!("[NETDRV] virtio MAC feature not advertised");
    }

    let isr = port::inb(base + VIRTIO_PIO_ISR_STATUS);
    println!("[NETDRV] virtio isr={:#04x}", isr);

    let rxq = setup_virtio_legacy_queue(base, VIRTIO_QUEUE_RX)?;
    let txq = setup_virtio_legacy_queue(base, VIRTIO_QUEUE_TX)?;
    let tx_buf = alloc_shared_buf(PAGE_SIZE as u32)?;

    let mut rt = VirtioNetRuntime {
        base,
        mac,
        rxq,
        txq,
        rx_bufs: Vec::new(),
        tx_buf,
        tx_inflight: false,
        arp_sent: false,
        gateway_mac: None,
        ping_sent: false,
        ping_reply_seen: false,
        ping_pending: false,
        ticks: 0,
    };

    if !populate_rx_ring(&mut rt) {
        return None;
    }

    Some(rt)
}

fn align_up(value: usize, align: usize) -> usize {
    if align == 0 {
        return value;
    }
    (value + (align - 1)) & !(align - 1)
}

fn compute_virtqueue_bytes(queue_size: usize) -> usize {
    // descriptor table + avail ring + padding(used ring alignment) + used ring
    let desc_bytes = 16usize.saturating_mul(queue_size);
    let avail_bytes = 6usize.saturating_add(2usize.saturating_mul(queue_size));
    let used_bytes = 6usize.saturating_add(8usize.saturating_mul(queue_size));
    let used_off = align_up(desc_bytes.saturating_add(avail_bytes), PAGE_SIZE);
    used_off.saturating_add(used_bytes)
}

fn is_syscall_error(value: u64) -> bool {
    (-4095..=-1).contains(&(value as i64))
}

fn alloc_shared_buf(len: u32) -> Option<SharedBuf> {
    let page_count = align_up(len as usize, PAGE_SIZE) / PAGE_SIZE;
    let mut phys_addrs = vec![0u64; page_count];
    let virt = unsafe { privileged::alloc_shared_pages(page_count as u64, Some(&mut phys_addrs), 0) };
    if is_syscall_error(virt) {
        println!("[NETDRV] alloc_shared_pages(buf) failed: errno={}", virt as i64);
        return None;
    }

    if phys_addrs.is_empty() {
        return None;
    }

    Some(SharedBuf {
        virt: virt as *mut u8,
        phys: phys_addrs[0],
        len,
    })
}

fn alloc_phys_contiguous(bytes: usize) -> Option<(u64, *mut u8)> {
    #[derive(Clone, Copy)]
    struct PageAlloc {
        virt: u64,
        phys: u64,
    }

    let page_count = align_up(bytes, PAGE_SIZE) / PAGE_SIZE;
    let required_run = page_count;
    let max_probe_pages = 64usize;
    let mut pool: Vec<PageAlloc> = Vec::new();

    for _ in 0..max_probe_pages {
        let mut phys_buf = [0u64; 1];
        let virt = unsafe { privileged::alloc_shared_pages(1, Some(&mut phys_buf), 0) };
        if is_syscall_error(virt) {
            println!("[NETDRV] alloc_shared_pages failed: errno={}", virt as i64);
            break;
        }
        pool.push(PageAlloc {
            virt,
            phys: phys_buf[0],
        });

        if pool.len() < required_run {
            continue;
        }

        let mut phys_sorted: Vec<u64> = pool.iter().map(|p| p.phys).collect();
        phys_sorted.sort_unstable();
        phys_sorted.dedup();

        for start_idx in 0..=phys_sorted.len().saturating_sub(required_run) {
            let start_phys = phys_sorted[start_idx];
            let mut contiguous = true;
            for step in 1..required_run {
                let expected = start_phys + (step as u64 * PAGE_SIZE as u64);
                if phys_sorted[start_idx + step] != expected {
                    contiguous = false;
                    break;
                }
            }
            if !contiguous {
                continue;
            }

            let selected_phys: Vec<u64> = (0..required_run)
                .map(|step| start_phys + (step as u64 * PAGE_SIZE as u64))
                .collect();

            let queue_virt = unsafe {
                privileged::map_physical_pages(task::gettid(), &selected_phys, 0)
            };
            if is_syscall_error(queue_virt) {
                println!(
                    "[NETDRV] map_physical_pages failed: errno={}",
                    queue_virt as i64
                );
                continue;
            }

            for page in &pool {
                let is_selected = selected_phys.iter().any(|&p| p == page.phys);
                let rc = privileged::unmap_pages(page.virt, 1, !is_selected);
                if rc != 0 {
                    println!(
                        "[NETDRV] unmap_pages failed virt={:#x} rc={}",
                        page.virt, rc as i64
                    );
                }
            }

            return Some((start_phys, queue_virt as *mut u8));
        }
    }

    for page in &pool {
        let rc = privileged::unmap_pages(page.virt, 1, true);
        if rc != 0 {
            println!(
                "[NETDRV] unmap_pages cleanup failed virt={:#x} rc={}",
                page.virt, rc as i64
            );
        }
    }

    None
}

fn setup_virtio_legacy_queue(base: u16, queue_index: u16) -> Option<VirtQueue> {
    port::outw(base + VIRTIO_PIO_QUEUE_SELECT, queue_index);
    let queue_size = port::inw(base + VIRTIO_PIO_QUEUE_SIZE);
    if queue_size == 0 {
        println!("[NETDRV] queue {} not available", queue_index);
        return None;
    }

    let bytes = compute_virtqueue_bytes(queue_size as usize);
    let Some((phys, virt)) = alloc_phys_contiguous(bytes) else {
        println!(
            "[NETDRV] queue {} allocation failed (size={} bytes)",
            queue_index, bytes
        );
        return None;
    };

    let aligned = align_up(bytes, PAGE_SIZE);
    unsafe {
        core::ptr::write_bytes(virt, 0, aligned);
    }

    let pfn = (phys >> 12) as u32;
    port::outl(base + VIRTIO_PIO_QUEUE_ADDR_PFN, pfn);
    let programmed = port::inl(base + VIRTIO_PIO_QUEUE_ADDR_PFN);
    if programmed != pfn {
        println!(
            "[NETDRV] queue {} PFN mismatch: wrote={:#x} read={:#x}",
            queue_index, pfn, programmed
        );
        return None;
    }

    let avail_ring_off = queue_size as usize * core::mem::size_of::<VringDesc>();
    let used_ring_off = align_up(
        avail_ring_off + 6 + (queue_size as usize * 2),
        PAGE_SIZE,
    );

    println!(
        "[NETDRV] queue {} ready size={} bytes={} pfn={:#x}",
        queue_index, queue_size, bytes, pfn
    );

    Some(VirtQueue {
        index: queue_index,
        size: queue_size,
        base: virt,
        phys,
        avail_ring_off,
        used_ring_off,
        next_avail_idx: 0,
        last_used_idx: 0,
    })
}

fn enqueue_desc_to_avail(queue: &mut VirtQueue, desc_id: u16) {
    let slot = (queue.next_avail_idx % queue.size) as usize;
    unsafe {
        write_volatile(queue.avail_ring_entry_ptr(slot), desc_id);
    }
    compiler_fence(AtomicOrdering::SeqCst);
    queue.next_avail_idx = queue.next_avail_idx.wrapping_add(1);
    unsafe {
        write_volatile(queue.avail_idx_ptr(), queue.next_avail_idx);
    }
}

fn populate_rx_ring(rt: &mut VirtioNetRuntime) -> bool {
    let target_count = core::cmp::min(RX_BUFFER_COUNT, rt.rxq.size as usize);
    for desc_id in 0..target_count {
        let Some(buf) = alloc_shared_buf(RX_BUFFER_LEN) else {
            println!("[NETDRV] rx buffer allocation failed at {}", desc_id);
            return false;
        };
        rt.rx_bufs.push(buf);
        let desc = VringDesc {
            addr: buf.phys,
            len: buf.len,
            flags: VRING_DESC_F_WRITE,
            next: 0,
        };
        unsafe {
            write_volatile(rt.rxq.desc_ptr(desc_id as u16), desc);
        }
        enqueue_desc_to_avail(&mut rt.rxq, desc_id as u16);
    }
    port::outw(rt.base + VIRTIO_PIO_QUEUE_NOTIFY, VIRTIO_QUEUE_RX);
    println!("[NETDRV] RX ring primed with {} buffers", target_count);
    true
}

fn poll_tx(rt: &mut VirtioNetRuntime) {
    let used = rt.txq.used_idx();
    let mut processed = 0usize;
    while rt.txq.last_used_idx != used && processed < TX_POLL_BUDGET {
        let slot = (rt.txq.last_used_idx % rt.txq.size) as usize;
        let elem = rt.txq.used_elem(slot);
        println!("[NETDRV] TX complete: desc={} len={}", elem.id, elem.len);
        rt.txq.last_used_idx = rt.txq.last_used_idx.wrapping_add(1);
        rt.tx_inflight = false;
        processed += 1;
    }
}

fn poll_rx(rt: &mut VirtioNetRuntime) {
    let used = rt.rxq.used_idx();
    let mut recycled = 0usize;
    let mut processed = 0usize;
    while rt.rxq.last_used_idx != used && processed < RX_POLL_BUDGET {
        let slot = (rt.rxq.last_used_idx % rt.rxq.size) as usize;
        let elem = rt.rxq.used_elem(slot);
        let desc_id = elem.id as usize;
        if desc_id < rt.rx_bufs.len() {
            let frame_total = elem.len as usize;
            if frame_total > VIRTIO_NET_HDR_LEN {
                let frame_len = frame_total - VIRTIO_NET_HDR_LEN;
                let frame_ptr = unsafe { rt.rx_bufs[desc_id].virt.add(VIRTIO_NET_HDR_LEN) };
                let frame = unsafe { core::slice::from_raw_parts(frame_ptr as *const u8, frame_len) };
                handle_rx_frame(rt, frame);
            }
            enqueue_desc_to_avail(&mut rt.rxq, desc_id as u16);
            recycled = recycled.saturating_add(1);
        } else {
            println!(
                "[NETDRV] RX used elem out of range: desc={} len={}",
                elem.id, elem.len
            );
        }
        rt.rxq.last_used_idx = rt.rxq.last_used_idx.wrapping_add(1);
        processed += 1;
    }

    if recycled > 0 {
        port::outw(rt.base + VIRTIO_PIO_QUEUE_NOTIFY, VIRTIO_QUEUE_RX);
    }
    if processed == RX_POLL_BUDGET {
        println!("[NETDRV] RX poll budget reached");
    }
}

fn write_be16(dst: &mut [u8], value: u16) {
    dst[0] = (value >> 8) as u8;
    dst[1] = value as u8;
}

fn checksum16(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0usize;
    while i + 1 < data.len() {
        sum = sum.wrapping_add(u16::from_be_bytes([data[i], data[i + 1]]) as u32);
        i += 2;
    }
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn queue_tx_frame(rt: &mut VirtioNetRuntime, frame: &[u8]) -> bool {
    if rt.tx_inflight {
        println!("[NETDRV] TX busy, frame deferred");
        return false;
    }
    let wire_frame_len = core::cmp::max(frame.len(), 60);
    let needed = VIRTIO_NET_HDR_LEN + wire_frame_len;
    if needed > rt.tx_buf.len as usize {
        println!("[NETDRV] TX frame too large: {}", frame.len());
        return false;
    }

    let tx_region = unsafe { core::slice::from_raw_parts_mut(rt.tx_buf.virt, needed) };
    tx_region[..VIRTIO_NET_HDR_LEN].fill(0);
    tx_region[VIRTIO_NET_HDR_LEN..].fill(0);
    tx_region[VIRTIO_NET_HDR_LEN..(VIRTIO_NET_HDR_LEN + frame.len())].copy_from_slice(frame);

    let desc = VringDesc {
        addr: rt.tx_buf.phys,
        len: needed as u32,
        flags: 0,
        next: 0,
    };
    unsafe {
        write_volatile(rt.txq.desc_ptr(0), desc);
    }
    enqueue_desc_to_avail(&mut rt.txq, 0);
    port::outw(rt.base + VIRTIO_PIO_QUEUE_NOTIFY, VIRTIO_QUEUE_TX);
    rt.tx_inflight = true;
    true
}

fn send_arp_request(rt: &mut VirtioNetRuntime) {
    let mut frame = [0u8; 42];
    frame[0..6].fill(0xFF);
    frame[6..12].copy_from_slice(&rt.mac);
    write_be16(&mut frame[12..14], ETH_TYPE_ARP);
    write_be16(&mut frame[14..16], 1);
    write_be16(&mut frame[16..18], ETH_TYPE_IPV4);
    frame[18] = 6;
    frame[19] = 4;
    write_be16(&mut frame[20..22], ARP_OP_REQUEST);
    frame[22..28].copy_from_slice(&rt.mac);
    frame[28..32].copy_from_slice(&LOCAL_IP);
    frame[32..38].fill(0);
    frame[38..42].copy_from_slice(&GATEWAY_IP);

    if queue_tx_frame(rt, &frame) {
        rt.arp_sent = true;
        println!("[NETDRV] ARP who-has 10.0.2.2 sent");
    }
}

fn send_icmp_echo(rt: &mut VirtioNetRuntime, dst_mac: [u8; 6]) {
    println!(
        "[NETDRV] send_icmp_echo: tx_inflight={} ping_sent={} pending={}",
        rt.tx_inflight, rt.ping_sent, rt.ping_pending
    );
    let payload = b"mochios-net";
    let ip_total_len = (20 + 8 + payload.len()) as u16;
    let mut frame = [0u8; 128];

    frame[0..6].copy_from_slice(&dst_mac);
    frame[6..12].copy_from_slice(&rt.mac);
    write_be16(&mut frame[12..14], ETH_TYPE_IPV4);

    let ip = &mut frame[14..34];
    ip.fill(0);
    ip[0] = 0x45;
    write_be16(&mut ip[2..4], ip_total_len);
    write_be16(&mut ip[4..6], 0x1234);
    ip[8] = 64;
    ip[9] = IP_PROTO_ICMP;
    ip[12..16].copy_from_slice(&LOCAL_IP);
    ip[16..20].copy_from_slice(&GATEWAY_IP);
    let ip_csum = checksum16(ip);
    write_be16(&mut ip[10..12], ip_csum);

    let icmp_len = 8 + payload.len();
    let icmp = &mut frame[34..(34 + icmp_len)];
    icmp.fill(0);
    icmp[0] = ICMP_ECHO_REQUEST;
    write_be16(&mut icmp[4..6], ICMP_ECHO_ID);
    write_be16(&mut icmp[6..8], ICMP_ECHO_SEQ);
    icmp[8..].copy_from_slice(payload);
    let icmp_csum = checksum16(icmp);
    write_be16(&mut icmp[2..4], icmp_csum);

    let frame_len = 14 + ip_total_len as usize;
    if queue_tx_frame(rt, &frame[..frame_len]) {
        rt.ping_sent = true;
        println!("[NETDRV] ICMP echo request sent to 10.0.2.2");
    } else {
        println!("[NETDRV] ICMP enqueue failed");
    }
}

fn try_send_pending_icmp(rt: &mut VirtioNetRuntime, dst_mac: [u8; 6]) {
    println!(
        "[NETDRV] try_send_pending_icmp: tx_inflight={} ping_sent={} pending={}",
        rt.tx_inflight, rt.ping_sent, rt.ping_pending
    );
    if rt.tx_inflight {
        rt.ping_pending = true;
        return;
    }
    send_icmp_echo(rt, dst_mac);
    if !rt.tx_inflight {
        rt.ping_pending = true;
    } else {
        rt.ping_pending = false;
    }
}

fn handle_arp(rt: &mut VirtioNetRuntime, frame: &[u8]) {
    if frame.len() < 42 {
        return;
    }
    let arp = &frame[14..42];
    let op = u16::from_be_bytes([arp[6], arp[7]]);
    let sender_mac = [arp[8], arp[9], arp[10], arp[11], arp[12], arp[13]];
    let sender_ip = [arp[14], arp[15], arp[16], arp[17]];
    let target_ip = [arp[24], arp[25], arp[26], arp[27]];
    if op == ARP_OP_REPLY && sender_ip == GATEWAY_IP && target_ip == LOCAL_IP {
        rt.gateway_mac = Some(sender_mac);
        rt.ping_pending = true;
        println!(
            "[NETDRV][v2] ARP reply: gateway MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            sender_mac[0], sender_mac[1], sender_mac[2], sender_mac[3], sender_mac[4], sender_mac[5]
        );
        if rt.tx_inflight {
            println!("[NETDRV] ARP learned but TX busy, defer ICMP");
            rt.ping_pending = true;
        } else {
            println!("[NETDRV] ARP learned, send ICMP now");
            send_icmp_echo(rt, sender_mac);
            if rt.tx_inflight {
                rt.ping_pending = false;
            }
        }
    } else if op == ARP_OP_REPLY {
        println!(
            "[NETDRV] ARP reply ignored: sender={}.{}.{}.{} target={}.{}.{}.{}",
            sender_ip[0], sender_ip[1], sender_ip[2], sender_ip[3],
            target_ip[0], target_ip[1], target_ip[2], target_ip[3]
        );
    }
}

fn handle_ipv4(rt: &mut VirtioNetRuntime, frame: &[u8]) {
    if frame.len() < 14 + 20 {
        return;
    }
    let ip = &frame[14..];
    if (ip[0] >> 4) != 4 {
        return;
    }
    let ihl = ((ip[0] & 0x0F) as usize) * 4;
    if ihl < 20 || ip.len() < ihl + 8 {
        return;
    }
    if ip[9] != IP_PROTO_ICMP {
        return;
    }
    if ip[12..16] != GATEWAY_IP || ip[16..20] != LOCAL_IP {
        return;
    }
    let icmp = &ip[ihl..];
    if icmp[0] != ICMP_ECHO_REPLY {
        println!("[NETDRV] ICMP type={} code={}", icmp[0], icmp.get(1).copied().unwrap_or(0));
        return;
    }
    if icmp.len() < 8 {
        return;
    }
    let id = u16::from_be_bytes([icmp[4], icmp[5]]);
    let seq = u16::from_be_bytes([icmp[6], icmp[7]]);
    if id == ICMP_ECHO_ID && seq == ICMP_ECHO_SEQ {
        rt.ping_reply_seen = true;
        println!("[NETDRV] ICMP echo reply received from 10.0.2.2");
    }
}

fn handle_rx_frame(rt: &mut VirtioNetRuntime, frame: &[u8]) {
    if frame.len() < 14 {
        return;
    }
    let eth_type = u16::from_be_bytes([frame[12], frame[13]]);
    match eth_type {
        ETH_TYPE_ARP => handle_arp(rt, frame),
        ETH_TYPE_IPV4 => handle_ipv4(rt, frame),
        _ => {}
    }
}

fn drive_network(rt: &mut VirtioNetRuntime) {
    if rt.ping_reply_seen {
        return;
    }
    if rt.ticks % 100 == 0 {
        println!(
            "[NETDRV] state: arp_sent={} gw={} tx_inflight={} ping_sent={} pending={}",
            rt.arp_sent,
            rt.gateway_mac.is_some(),
            rt.tx_inflight,
            rt.ping_sent,
            rt.ping_pending
        );
    }
    if !rt.arp_sent {
        send_arp_request(rt);
        return;
    }
    if let Some(gw_mac) = rt.gateway_mac {
        if rt.tx_inflight {
            if rt.ping_pending {
                println!("[NETDRV] waiting to send ICMP: tx_inflight=true");
            }
            return;
        }
        if rt.ping_pending {
            try_send_pending_icmp(rt, gw_mac);
            return;
        }
        if !rt.ping_sent || rt.ticks % 100 == 0 {
            if rt.ping_sent {
                println!("[NETDRV] ICMP retry");
            }
            try_send_pending_icmp(rt, gw_mac);
        }
    } else if rt.ticks % 100 == 0 {
        println!("[NETDRV] ARP retry");
        send_arp_request(rt);
    }
}

fn run_virtio_loop(mut rt: VirtioNetRuntime) {
    println!(
        "[NETDRV] runtime ready: rxq={} txq={} rx_pfn={:#x} tx_pfn={:#x}",
        rt.rxq.size,
        rt.txq.size,
        rt.rxq.phys >> 12,
        rt.txq.phys >> 12
    );
    loop {
        rt.ticks = rt.ticks.wrapping_add(1);
        poll_tx(&mut rt);
        poll_rx(&mut rt);
        drive_network(&mut rt);
        time::sleep_ms(10);
    }
}

fn init_device(dev: NetDevice) {
    println!(
        "[NETDRV] net device {:02x}:{:02x}.{} vendor={:04x} device={:04x} kind={:?}",
        dev.bdf.bus,
        dev.bdf.device,
        dev.bdf.function,
        dev.vendor_id,
        dev.device_id,
        dev.kind
    );

    enable_device_command_bits(dev.bdf);

    match dev.kind {
        NetKind::VirtioNet => {
            println!("[NETDRV] virtio-net detected");
            if let Some(rt) = virtio_legacy_init_pio(dev) {
                try_map_mmio_bar0(dev);
                run_virtio_loop(rt);
            } else {
                println!("[NETDRV] virtio-net init failed");
                try_map_mmio_bar0(dev);
            }
        }
        NetKind::E1000 => {
            println!("[NETDRV] e1000 detected (phase1: probe only)");
            try_map_mmio_bar0(dev);
        }
        NetKind::Unknown => {
            println!("[NETDRV] unknown NIC class device (phase1: probe only)");
            try_map_mmio_bar0(dev);
        }
    }
}

fn main() {
    println!("[NETDRV] network driver started");

    let devices = find_network_devices();
    if devices.is_empty() {
        println!("[NETDRV] no PCI network controller found");
    } else {
        println!("[NETDRV] found {} network controller(s)", devices.len());
        for dev in devices {
            init_device(dev);
        }
    }

    println!("[NETDRV] driver idle");
    loop {
        time::sleep_ms(1000);
    }
}
