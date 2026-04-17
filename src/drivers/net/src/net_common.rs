use core::fmt::Debug;

pub const PCI_CFG_ADDR_PORT: u16 = 0xCF8;
pub const PCI_CFG_DATA_PORT: u16 = 0xCFC;

pub const PCI_COMMAND_IO: u16 = 1 << 0;
pub const PCI_COMMAND_MEM: u16 = 1 << 1;
pub const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;

pub const CLASS_NETWORK: u8 = 0x02;

pub const VIRTIO_NET_F_MAC: u32 = 5;
pub const VIRTIO_NET_F_STATUS: u32 = 16;

pub const VIRTIO_PIO_DEVICE_FEATURES: u16 = 0x00;
pub const VIRTIO_PIO_GUEST_FEATURES: u16 = 0x04;
pub const VIRTIO_PIO_QUEUE_ADDR_PFN: u16 = 0x08;
pub const VIRTIO_PIO_QUEUE_SIZE: u16 = 0x0C;
pub const VIRTIO_PIO_QUEUE_SELECT: u16 = 0x0E;
pub const VIRTIO_PIO_QUEUE_NOTIFY: u16 = 0x10;
pub const VIRTIO_PIO_DEVICE_STATUS: u16 = 0x12;
pub const VIRTIO_PIO_ISR_STATUS: u16 = 0x13;
pub const VIRTIO_PIO_DEVICE_CONFIG: u16 = 0x14;

pub const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1 << 0;
pub const VIRTIO_STATUS_DRIVER: u8 = 1 << 1;
pub const VIRTIO_STATUS_DRIVER_OK: u8 = 1 << 2;

pub const VIRTIO_QUEUE_RX: u16 = 0;
pub const VIRTIO_QUEUE_TX: u16 = 1;
pub const PAGE_SIZE: usize = 4096;
pub const RX_BUFFER_LEN: u32 = 2048;
pub const RX_BUFFER_COUNT: usize = 32;

pub const ETH_TYPE_ARP: u16 = 0x0806;
pub const ETH_TYPE_IPV4: u16 = 0x0800;
pub const ARP_OP_REQUEST: u16 = 1;
pub const ARP_OP_REPLY: u16 = 2;
pub const IP_PROTO_ICMP: u8 = 1;
pub const ICMP_ECHO_REQUEST: u8 = 8;
pub const ICMP_ECHO_REPLY: u8 = 0;
pub const ICMP_ECHO_ID: u16 = 0x1337;
pub const ICMP_ECHO_SEQ: u16 = 1;

pub const GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];
pub const LOCAL_IP: [u8; 4] = [10, 0, 2, 15];

pub const VRING_DESC_F_WRITE: u16 = 2;
pub const VIRTIO_NET_HDR_LEN: usize = 10;
pub const RX_POLL_BUDGET: usize = 64;
pub const TX_POLL_BUDGET: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct PciBdf {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[derive(Clone, Copy, Debug)]
pub enum NetKind {
    VirtioNet,
    E1000,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct NetDevice {
    pub bdf: PciBdf,
    pub vendor_id: u16,
    pub device_id: u16,
    pub kind: NetKind,
    pub bar0: u32,
}
