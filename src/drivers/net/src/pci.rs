use crate::net_common::*;
use swiftlib::{mmio, port};

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
        (0x1AF4, 0x1000) => NetKind::VirtioNet,
        (0x1AF4, 0x1041) => NetKind::VirtioNet,
        (0x8086, 0x100E) | (0x8086, 0x100F) | (0x8086, 0x10D3) => NetKind::E1000,
        _ => NetKind::Unknown,
    }
}

pub fn enable_device_command_bits(bdf: PciBdf) {
    let mut command = pci_read_u16(bdf, 0x04);
    command |= PCI_COMMAND_IO | PCI_COMMAND_MEM | PCI_COMMAND_BUS_MASTER;
    pci_write_u16(bdf, 0x04, command);
}

pub fn find_network_devices() -> Vec<NetDevice> {
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

pub fn try_map_mmio_bar0(dev: NetDevice) {
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

pub fn virtio_pio_base(bar0: u32) -> Option<u16> {
    if (bar0 & 0x1) == 0 {
        return None;
    }
    let base = bar0 & 0xFFFF_FFFC;
    if base == 0 || base > 0xFFFF {
        return None;
    }
    Some(base as u16)
}
