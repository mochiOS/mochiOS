use core::ptr::{read_volatile, write_volatile};

use swiftlib::{keyboard, mmio, mouse, port, time};

const PCI_CFG_ADDR_PORT: u16 = 0xCF8;
const PCI_CFG_DATA_PORT: u16 = 0xCFC;

const XHCI_CLASS_CODE: u8 = 0x0C;
const XHCI_SUBCLASS: u8 = 0x03;
const XHCI_PROG_IF: u8 = 0x30;

const XHCI_MMIO_MAP_SIZE: usize = 0x10000;

#[derive(Clone, Copy)]
struct PciBdf {
    bus: u8,
    device: u8,
    function: u8,
}

#[derive(Clone, Copy)]
struct XhciController {
    bdf: PciBdf,
    vendor_id: u16,
    device_id: u16,
    bar0: u32,
    bar1: u32,
    mmio_base: u64,
    bar_is_64bit: bool,
}

#[derive(Clone, Copy)]
struct XhciRegs {
    base: *mut u8,
    cap_len: usize,
    op_base: usize,
    db_off: usize,
    rt_off: usize,
    max_ports: u8,
    max_slots: u8,
    hci_version: u16,
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

fn pci_function_exists(bdf: PciBdf) -> bool {
    pci_read_u16(bdf, 0x00) != 0xFFFF
}

fn probe_xhci_controller(bdf: PciBdf) -> Option<XhciController> {
    let class_reg = pci_read_u32(bdf, 0x08);
    let class_code = ((class_reg >> 24) & 0xFF) as u8;
    let subclass = ((class_reg >> 16) & 0xFF) as u8;
    let prog_if = ((class_reg >> 8) & 0xFF) as u8;

    if class_code != XHCI_CLASS_CODE || subclass != XHCI_SUBCLASS || prog_if != XHCI_PROG_IF {
        return None;
    }

    let vendor_device = pci_read_u32(bdf, 0x00);
    let vendor_id = (vendor_device & 0xFFFF) as u16;
    let device_id = ((vendor_device >> 16) & 0xFFFF) as u16;

    let bar0 = pci_read_u32(bdf, 0x10);
    let bar1 = pci_read_u32(bdf, 0x14);

    if (bar0 & 0x1) != 0 {
        println!(
            "[xHCI] controller {:02x}:{:02x}.{} uses I/O BAR (unsupported)",
            bdf.bus, bdf.device, bdf.function
        );
        return None;
    }

    let bar_is_64bit = (bar0 & 0x6) == 0x4;
    let mut mmio_base = u64::from(bar0 & 0xFFFF_FFF0);
    if bar_is_64bit {
        mmio_base |= u64::from(bar1) << 32;
    }

    if mmio_base == 0 {
        return None;
    }

    Some(XhciController {
        bdf,
        vendor_id,
        device_id,
        bar0,
        bar1,
        mmio_base,
        bar_is_64bit,
    })
}

fn find_xhci_controller() -> Option<XhciController> {
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
                if let Some(controller) = probe_xhci_controller(bdf) {
                    return Some(controller);
                }
            }
        }
    }
    None
}

#[inline]
fn mmio_read_u8(base: *mut u8, offset: usize) -> u8 {
    unsafe { read_volatile(base.add(offset) as *const u8) }
}

#[inline]
fn mmio_read_u16(base: *mut u8, offset: usize) -> u16 {
    unsafe { read_volatile(base.add(offset) as *const u16) }
}

#[inline]
fn mmio_read_u32(base: *mut u8, offset: usize) -> u32 {
    unsafe { read_volatile(base.add(offset) as *const u32) }
}

#[inline]
fn mmio_write_u32(base: *mut u8, offset: usize, value: u32) {
    unsafe {
        write_volatile(base.add(offset) as *mut u32, value);
    }
}

fn wait_until(timeout_ms: u64, mut condition: impl FnMut() -> bool) -> bool {
    for _ in 0..timeout_ms {
        if condition() {
            return true;
        }
        time::sleep_ms(1);
    }
    false
}

fn map_xhci_mmio(controller: &XhciController) -> Result<*mut u8, u64> {
    let page_base = controller.mmio_base & !0xFFF;
    let page_offset = (controller.mmio_base & 0xFFF) as usize;
    let map_size = XHCI_MMIO_MAP_SIZE.saturating_add(page_offset);
    let mapped = mmio::map_physical(page_base, map_size)?;
    Ok(unsafe { mapped.add(page_offset) })
}

fn read_xhci_regs(base: *mut u8) -> Option<XhciRegs> {
    let cap_len = mmio_read_u8(base, 0x00) as usize;
    if cap_len < 0x20 {
        return None;
    }

    let hci_version = mmio_read_u16(base, 0x02);
    let hcs_params1 = mmio_read_u32(base, 0x04);
    let max_slots = (hcs_params1 & 0xFF) as u8;
    let max_ports = ((hcs_params1 >> 24) & 0xFF) as u8;
    let db_off = (mmio_read_u32(base, 0x14) & !0x3) as usize;
    let rt_off = (mmio_read_u32(base, 0x18) & !0x1F) as usize;

    Some(XhciRegs {
        base,
        cap_len,
        op_base: cap_len,
        db_off,
        rt_off,
        max_ports,
        max_slots,
        hci_version,
    })
}

fn halt_xhci(regs: &XhciRegs) -> bool {
    let usbcmd_off = regs.op_base;
    let usbsts_off = regs.op_base + 0x04;

    let cmd = mmio_read_u32(regs.base, usbcmd_off);
    if (cmd & 0x1) != 0 {
        mmio_write_u32(regs.base, usbcmd_off, cmd & !0x1);
    }

    wait_until(300, || (mmio_read_u32(regs.base, usbsts_off) & 0x1) != 0)
}

fn reset_xhci(regs: &XhciRegs) -> bool {
    if !halt_xhci(regs) {
        println!("[xHCI] halt timeout");
        return false;
    }

    let usbcmd_off = regs.op_base;
    let usbsts_off = regs.op_base + 0x04;

    let cmd = mmio_read_u32(regs.base, usbcmd_off);
    mmio_write_u32(regs.base, usbcmd_off, cmd | (1 << 1));

    if !wait_until(1000, || (mmio_read_u32(regs.base, usbcmd_off) & (1 << 1)) == 0) {
        println!("[xHCI] controller reset timeout");
        return false;
    }
    if !wait_until(1000, || (mmio_read_u32(regs.base, usbsts_off) & (1 << 11)) == 0) {
        println!("[xHCI] CNR clear timeout");
        return false;
    }
    true
}

fn decode_port_speed(speed: u8) -> &'static str {
    match speed {
        1 => "full",
        2 => "low",
        3 => "high",
        4 => "super",
        5 => "super+",
        _ => "unknown",
    }
}

fn dump_ports(regs: &XhciRegs) {
    if regs.max_ports == 0 {
        println!("[xHCI] no root hub ports reported");
        return;
    }

    for port_index in 0..usize::from(regs.max_ports) {
        let portsc_off = regs.op_base + 0x400 + port_index * 0x10;
        let portsc = mmio_read_u32(regs.base, portsc_off);
        let connected = (portsc & (1 << 0)) != 0;
        let enabled = (portsc & (1 << 1)) != 0;
        let connect_change = (portsc & (1 << 17)) != 0;
        let speed = ((portsc >> 10) & 0x0F) as u8;

        if connected || enabled || connect_change {
            println!(
                "[xHCI] port {:02}: connected={} enabled={} speed={}({})",
                port_index + 1,
                connected as u8,
                enabled as u8,
                speed,
                decode_port_speed(speed)
            );
        }
    }
}

fn init_xhci_controller() {
    let Some(controller) = find_xhci_controller() else {
        println!("[xHCI] no controller found on PCI bus");
        return;
    };

    println!(
        "[xHCI] controller {:02x}:{:02x}.{} vendor={:04x} device={:04x}",
        controller.bdf.bus,
        controller.bdf.device,
        controller.bdf.function,
        controller.vendor_id,
        controller.device_id
    );
    println!(
        "[xHCI] BAR0={:#010x} BAR1={:#010x} {} MMIO={:#018x}",
        controller.bar0,
        controller.bar1,
        if controller.bar_is_64bit { "64-bit" } else { "32-bit" },
        controller.mmio_base
    );

    let mapped = match map_xhci_mmio(&controller) {
        Ok(ptr) => ptr,
        Err(err) => {
            println!("[xHCI] map mmio failed: {:#x}", err);
            return;
        }
    };

    let Some(regs) = read_xhci_regs(mapped) else {
        println!("[xHCI] invalid capability header");
        return;
    };

    println!(
        "[xHCI] version={:x}.{:02x} caplen=0x{:x} max_slots={} max_ports={}",
        regs.hci_version >> 8,
        regs.hci_version & 0xFF,
        regs.cap_len,
        regs.max_slots,
        regs.max_ports
    );
    println!(
        "[xHCI] runtime_off=0x{:x} doorbell_off=0x{:x}",
        regs.rt_off,
        regs.db_off
    );

    if reset_xhci(&regs) {
        println!("[xHCI] controller halted+reset complete");
    } else {
        println!("[xHCI] controller reset skipped due to timeout");
    }

    dump_ports(&regs);
}

#[rustfmt::skip]
const MAP_NORMAL: [u8; 128] = [
    0,    0x1B, b'1', b'2', b'3', b'4', b'5', b'6',
    b'7', b'8', b'9', b'0', b'-', b'=', 0x08, b'\t',
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i',
    b'o', b'p', b'[', b']', b'\n', 0,   b'a', b's',
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',
    b'\'',b'`', 0,   b'\\',b'z', b'x', b'c', b'v',
    b'b', b'n', b'm', b',', b'.', b'/', 0,   b'*',
    0,    b' ', 0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    b'7',
    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1',
    b'2', b'3', b'0', b'.', 0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
];

#[rustfmt::skip]
const MAP_SHIFT: [u8; 128] = [
    0,    0x1B, b'!', b'@', b'#', b'$', b'%', b'^',
    b'&', b'*', b'(', b')', b'_', b'+', 0x08, b'\t',
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I',
    b'O', b'P', b'{', b'}', b'\n', 0,   b'A', b'S',
    b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':',
    b'"', b'~', 0,   b'|', b'Z', b'X', b'C', b'V',
    b'B', b'N', b'M', b'<', b'>', b'?', 0,   b'*',
    0,    b' ', 0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    b'7',
    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1',
    b'2', b'3', b'0', b'.', 0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,
];

const SC_LSHIFT: u8 = 0x2A;
const SC_RSHIFT: u8 = 0x36;
const SC_CAPSLOCK: u8 = 0x3A;
const SC_RELEASE: u8 = 0x80;

#[derive(Default)]
struct KeyboardDecoder {
    shift: bool,
    caps: bool,
}

impl KeyboardDecoder {
    fn decode_scancode(&mut self, scancode: u8) -> Option<u8> {
        if scancode & SC_RELEASE != 0 {
            let make = scancode & !SC_RELEASE;
            if make == SC_LSHIFT || make == SC_RSHIFT {
                self.shift = false;
            }
            return None;
        }

        match scancode {
            SC_LSHIFT | SC_RSHIFT => {
                self.shift = true;
                return None;
            }
            SC_CAPSLOCK => {
                self.caps = !self.caps;
                return None;
            }
            _ => {}
        }

        let idx = scancode as usize;
        if idx >= 128 {
            return None;
        }

        let use_shift = self.shift ^ (self.caps && MAP_NORMAL[idx].is_ascii_alphabetic());
        let ch = if use_shift { MAP_SHIFT[idx] } else { MAP_NORMAL[idx] };
        if ch == 0 {
            None
        } else {
            Some(ch)
        }
    }
}

fn log_key_event(ch: u8) {
    match ch {
        b'\n' => println!("[xHCI][KBD] <ENTER>"),
        b'\t' => println!("[xHCI][KBD] <TAB>"),
        0x08 => println!("[xHCI][KBD] <BACKSPACE>"),
        b' '..=b'~' => println!("[xHCI][KBD] '{}'", ch as char),
        _ => println!("[xHCI][KBD] 0x{:02X}", ch),
    }
}

fn log_mouse_event(packet: mouse::MousePacket, last_buttons: &mut u8) {
    let moved = packet.dx != 0 || packet.dy != 0;
    let buttons_changed = packet.buttons != *last_buttons;
    if !moved && !buttons_changed {
        return;
    }

    let dy_screen = -(packet.dy as i16);
    println!(
        "[xHCI][MOUSE] dx={:>4}, dy={:>4}, L={} R={} M={}",
        packet.dx as i16,
        dy_screen,
        packet.left() as u8,
        packet.right() as u8,
        packet.middle() as u8
    );
    *last_buttons = packet.buttons;
}

fn run_input_monitor_loop() {
    let mut decoder = KeyboardDecoder::default();
    let mut last_buttons = 0u8;
    let mut warned_keyboard_err = false;
    let mut warned_mouse_err = false;

    loop {
        let mut handled_any = false;

        loop {
            match keyboard::read_scancode_tap() {
                Ok(Some(scancode)) => {
                    handled_any = true;
                    if let Some(ch) = decoder.decode_scancode(scancode) {
                        log_key_event(ch);
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    if !warned_keyboard_err {
                        println!("[xHCI] keyboard tap error: {:#x}", err);
                        warned_keyboard_err = true;
                    }
                    break;
                }
            }
        }

        loop {
            match mouse::read_packet() {
                Ok(Some(packet)) => {
                    handled_any = true;
                    log_mouse_event(packet, &mut last_buttons);
                }
                Ok(None) => break,
                Err(err) => {
                    if !warned_mouse_err {
                        println!("[xHCI] mouse read error: {:#x}", err);
                        warned_mouse_err = true;
                    }
                    break;
                }
            }
        }

        if !handled_any {
            time::sleep_ms(2);
        }
    }
}

fn main() {
    println!("[xHCI] driver started");
    init_xhci_controller();
    println!("[xHCI] input monitor mode enabled (keyboard tap + mouse packet)");
    run_input_monitor_loop();
}
