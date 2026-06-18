#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InterfaceHandle(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MmioHandle(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IrqHandle(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DmaHandle(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PciConfigHandle(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevicePath(String);

impl DevicePath {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceName(String);

impl DeviceName {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceString(String);

impl DeviceString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceBytes(Vec<u8>);

impl DeviceBytes {
    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        Self(value.into())
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceBus {
    Platform = 0,
    Pci = 1,
    Usb = 2,
    Virtio = 3,
    Other = 4,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceClass {
    Network = 0,
    Storage = 1,
    Block = 2,
    Character = 3,
    Input = 4,
    Gpu = 5,
    Display = 6,
    Audio = 7,
    Usb = 8,
    Virtio = 9,
    Bus = 10,
    Other = 255,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceProperty {
    Bool(bool),
    U32(u32),
    U64(u64),
    String(DeviceString),
    Bytes(DeviceBytes),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeResult {
    Reject,
    Match { score: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlugKitError {
    InvalidHandle,
    PermissionDenied,
    NotSupported,
    NoDevice,
    Busy,
    OutOfMemory,
    IoError,
    InvalidOffset,
    InvalidSize,
    Interrupted,
    Unknown,
}

pub type PlugKitResult<T> = Result<T, PlugKitError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IrqEvent {
    pub sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlugKitEvent {
    DeviceReady,
    DeviceStopped,
    LinkUp,
    LinkDown,
    MediaChanged,
    Error { code: u32 },
}

#[derive(Clone, Debug)]
pub struct Mmio {
    bytes: Vec<u8>,
}

impl Mmio {
    pub fn new(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    fn range(&self, offset: usize, size: usize) -> PlugKitResult<&[u8]> {
        let end = offset
            .checked_add(size)
            .ok_or(PlugKitError::InvalidOffset)?;
        self.bytes
            .get(offset..end)
            .ok_or(PlugKitError::InvalidOffset)
    }

    fn range_mut(&mut self, offset: usize, size: usize) -> PlugKitResult<&mut [u8]> {
        let end = offset
            .checked_add(size)
            .ok_or(PlugKitError::InvalidOffset)?;
        self.bytes
            .get_mut(offset..end)
            .ok_or(PlugKitError::InvalidOffset)
    }

    pub fn read_u8(&self, offset: usize) -> PlugKitResult<u8> {
        Ok(self.range(offset, 1)?[0])
    }

    pub fn read_u16(&self, offset: usize) -> PlugKitResult<u16> {
        let raw = self.range(offset, 2)?;
        Ok(u16::from_le_bytes([raw[0], raw[1]]))
    }

    pub fn read_u32(&self, offset: usize) -> PlugKitResult<u32> {
        let raw = self.range(offset, 4)?;
        Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    pub fn read_u64(&self, offset: usize) -> PlugKitResult<u64> {
        let raw = self.range(offset, 8)?;
        Ok(u64::from_le_bytes([
            raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
        ]))
    }

    pub fn write_u8(&mut self, offset: usize, value: u8) -> PlugKitResult<()> {
        self.range_mut(offset, 1)?[0] = value;
        Ok(())
    }

    pub fn write_u16(&mut self, offset: usize, value: u16) -> PlugKitResult<()> {
        self.range_mut(offset, 2)?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub fn write_u32(&mut self, offset: usize, value: u32) -> PlugKitResult<()> {
        self.range_mut(offset, 4)?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub fn write_u64(&mut self, offset: usize, value: u64) -> PlugKitResult<()> {
        self.range_mut(offset, 8)?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
}

#[derive(Debug)]
pub struct Irq {
    state: Mutex<IrqState>,
}

#[derive(Clone, Debug)]
struct IrqState {
    sequence: u64,
    acked: u64,
}

impl Irq {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(IrqState {
                sequence: 0,
                acked: 0,
            }),
        }
    }

    pub fn wait(&self) -> PlugKitResult<IrqEvent> {
        let state = self.state.lock();
        if state.sequence == state.acked {
            Err(PlugKitError::Interrupted)
        } else {
            Ok(IrqEvent {
                sequence: state.sequence,
            })
        }
    }

    pub fn ack(&mut self) -> PlugKitResult<()> {
        let mut state = self.state.lock();
        state.acked = state.sequence;
        Ok(())
    }

    pub fn signal(&mut self) {
        let mut state = self.state.lock();
        state.sequence = state.sequence.saturating_add(1);
    }
}

impl Clone for Irq {
    fn clone(&self) -> Self {
        let state = self.state.lock();
        Self {
            state: Mutex::new(IrqState {
                sequence: state.sequence,
                acked: state.acked,
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DmaBuffer {
    bytes: Vec<u8>,
    device_addr: u64,
}

impl DmaBuffer {
    pub fn new(size: usize, device_addr: u64) -> Self {
        Self {
            bytes: vec![0; size],
            device_addr,
        }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn device_addr(&self) -> u64 {
        self.device_addr
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    pub fn sync_for_device(&self) -> PlugKitResult<()> {
        Ok(())
    }

    pub fn sync_for_cpu(&self) -> PlugKitResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PciConfig {
    bytes: Vec<u8>,
}

impl PciConfig {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    fn range(&self, offset: usize, size: usize) -> PlugKitResult<&[u8]> {
        let end = offset
            .checked_add(size)
            .ok_or(PlugKitError::InvalidOffset)?;
        self.bytes
            .get(offset..end)
            .ok_or(PlugKitError::InvalidOffset)
    }

    fn range_mut(&mut self, offset: usize, size: usize) -> PlugKitResult<&mut [u8]> {
        let end = offset
            .checked_add(size)
            .ok_or(PlugKitError::InvalidOffset)?;
        self.bytes
            .get_mut(offset..end)
            .ok_or(PlugKitError::InvalidOffset)
    }

    pub fn read_u8(&self, offset: usize) -> PlugKitResult<u8> {
        Ok(self.range(offset, 1)?[0])
    }

    pub fn read_u16(&self, offset: usize) -> PlugKitResult<u16> {
        let raw = self.range(offset, 2)?;
        Ok(u16::from_le_bytes([raw[0], raw[1]]))
    }

    pub fn read_u32(&self, offset: usize) -> PlugKitResult<u32> {
        let raw = self.range(offset, 4)?;
        Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    pub fn write_u8(&mut self, offset: usize, value: u8) -> PlugKitResult<()> {
        self.range_mut(offset, 1)?[0] = value;
        Ok(())
    }

    pub fn write_u16(&mut self, offset: usize, value: u16) -> PlugKitResult<()> {
        self.range_mut(offset, 2)?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub fn write_u32(&mut self, offset: usize, value: u32) -> PlugKitResult<()> {
        self.range_mut(offset, 4)?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PlugKitResources {
    mmios: Vec<Mmio>,
    irqs: Vec<Irq>,
    dma_supported: bool,
    has_pci_config: bool,
    pci_config: Option<PciConfig>,
    next_dma_addr: u64,
}

impl PlugKitResources {
    pub fn new(
        mmios: Vec<Mmio>,
        irqs: Vec<Irq>,
        dma_supported: bool,
        has_pci_config: bool,
        pci_config: Option<PciConfig>,
    ) -> Self {
        Self {
            mmios,
            irqs,
            dma_supported,
            has_pci_config,
            pci_config,
            next_dma_addr: 0x1000,
        }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new(), Vec::new(), false, false, None)
    }

    pub fn mmio_count(&self) -> usize {
        self.mmios.len()
    }

    pub fn irq_count(&self) -> usize {
        self.irqs.len()
    }

    pub fn dma_supported(&self) -> bool {
        self.dma_supported
    }

    pub fn has_pci_config(&self) -> bool {
        self.has_pci_config
    }

    pub fn map_mmio(&self, index: usize) -> PlugKitResult<Mmio> {
        self.mmios
            .get(index)
            .cloned()
            .ok_or(PlugKitError::InvalidHandle)
    }

    pub fn bind_irq(&self, index: usize) -> PlugKitResult<Irq> {
        self.irqs
            .get(index)
            .cloned()
            .ok_or(PlugKitError::InvalidHandle)
    }

    pub fn alloc_dma(&mut self, size: usize) -> PlugKitResult<DmaBuffer> {
        if !self.dma_supported {
            return Err(PlugKitError::NotSupported);
        }
        let addr = self.next_dma_addr;
        self.next_dma_addr = self
            .next_dma_addr
            .checked_add(size as u64)
            .ok_or(PlugKitError::OutOfMemory)?;
        Ok(DmaBuffer::new(size, addr))
    }

    pub fn pci_config(&self) -> PlugKitResult<PciConfig> {
        if !self.has_pci_config {
            return Err(PlugKitError::NotSupported);
        }
        self.pci_config.clone().ok_or(PlugKitError::InvalidHandle)
    }
}

#[derive(Clone, Debug)]
struct DeviceRecord {
    path: DevicePath,
    name: DeviceName,
    bus: DeviceBus,
    class: DeviceClass,
    vendor_id: Option<u32>,
    device_id: Option<u32>,
    subsystem_vendor_id: Option<u32>,
    subsystem_device_id: Option<u32>,
    revision: Option<u8>,
    properties: BTreeMap<String, DeviceProperty>,
}

#[derive(Clone, Debug)]
pub struct DeviceSpec {
    pub path: DevicePath,
    pub name: DeviceName,
    pub bus: DeviceBus,
    pub class: DeviceClass,
    pub vendor_id: Option<u32>,
    pub device_id: Option<u32>,
    pub subsystem_vendor_id: Option<u32>,
    pub subsystem_device_id: Option<u32>,
    pub revision: Option<u8>,
    pub properties: BTreeMap<String, DeviceProperty>,
}

impl DeviceSpec {
    pub fn new(
        path: impl Into<String>,
        name: impl Into<String>,
        bus: DeviceBus,
        class: DeviceClass,
    ) -> Self {
        Self {
            path: DevicePath::new(path),
            name: DeviceName::new(name),
            bus,
            class,
            vendor_id: None,
            device_id: None,
            subsystem_vendor_id: None,
            subsystem_device_id: None,
            revision: None,
            properties: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlugKitDevice {
    id: DeviceId,
}

impl PlugKitDevice {
    fn record(&self) -> PlugKitResult<DeviceRecord> {
        with_device_registry(|registry| registry.get(&self.id.0).cloned())
            .ok_or(PlugKitError::InvalidHandle)
    }

    pub fn id(&self) -> DeviceId {
        self.id
    }

    pub fn path(&self) -> PlugKitResult<DevicePath> {
        Ok(self.record()?.path)
    }

    pub fn name(&self) -> PlugKitResult<DeviceName> {
        Ok(self.record()?.name)
    }

    pub fn bus(&self) -> DeviceBus {
        self.record().map(|r| r.bus).unwrap_or(DeviceBus::Other)
    }

    pub fn class(&self) -> DeviceClass {
        self.record().map(|r| r.class).unwrap_or(DeviceClass::Other)
    }

    pub fn vendor_id(&self) -> Option<u32> {
        self.record().ok().and_then(|r| r.vendor_id)
    }

    pub fn device_id(&self) -> Option<u32> {
        self.record().ok().and_then(|r| r.device_id)
    }

    pub fn subsystem_vendor_id(&self) -> Option<u32> {
        self.record().ok().and_then(|r| r.subsystem_vendor_id)
    }

    pub fn subsystem_device_id(&self) -> Option<u32> {
        self.record().ok().and_then(|r| r.subsystem_device_id)
    }

    pub fn revision(&self) -> Option<u8> {
        self.record().ok().and_then(|r| r.revision)
    }

    pub fn property(&self, key: &str) -> PlugKitResult<Option<DeviceProperty>> {
        Ok(self.record()?.properties.get(key).cloned())
    }
}

#[derive(Clone, Debug)]
struct InterfaceRecord {
    name: String,
}

#[derive(Clone, Copy, Debug)]
pub struct DriverDescriptor {
    pub name: &'static str,
    pub type_name: &'static str,
    pub api_version: u32,
    pub probe: fn(&PlugKitDevice) -> ProbeResult,
    pub start: fn(PlugKitDevice, PlugKitResources) -> PlugKitResult<()>,
    pub stop: fn(PlugKitDevice) -> PlugKitResult<()>,
}

static NEXT_DEVICE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_INTERFACE_ID: AtomicU64 = AtomicU64::new(1);
static DEVICE_REGISTRY: Mutex<Option<BTreeMap<u64, DeviceRecord>>> = Mutex::new(None);
static INTERFACE_REGISTRY: Mutex<Option<BTreeMap<u64, InterfaceRecord>>> = Mutex::new(None);
static EVENT_LOG: Mutex<Option<Vec<PlugKitEvent>>> = Mutex::new(None);
static LOG_LINES: Mutex<Option<Vec<(LogLevel, String)>>> = Mutex::new(None);

fn with_device_registry_mut<R>(f: impl FnOnce(&mut BTreeMap<u64, DeviceRecord>) -> R) -> R {
    let mut guard = DEVICE_REGISTRY.lock();
    let map = guard.get_or_insert_with(BTreeMap::new);
    f(map)
}

fn with_device_registry<R>(f: impl FnOnce(&BTreeMap<u64, DeviceRecord>) -> R) -> R {
    let mut guard = DEVICE_REGISTRY.lock();
    let map = guard.get_or_insert_with(BTreeMap::new);
    f(map)
}

fn with_interface_registry_mut<R>(f: impl FnOnce(&mut BTreeMap<u64, InterfaceRecord>) -> R) -> R {
    let mut guard = INTERFACE_REGISTRY.lock();
    let map = guard.get_or_insert_with(BTreeMap::new);
    f(map)
}

fn with_event_log_mut<R>(f: impl FnOnce(&mut Vec<PlugKitEvent>) -> R) -> R {
    let mut guard = EVENT_LOG.lock();
    let vec = guard.get_or_insert_with(Vec::new);
    f(vec)
}

fn with_log_lines_mut<R>(f: impl FnOnce(&mut Vec<(LogLevel, String)>) -> R) -> R {
    let mut guard = LOG_LINES.lock();
    let vec = guard.get_or_insert_with(Vec::new);
    f(vec)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogLevel {
    Info,
    Warn,
    Error,
}

pub fn register_device(spec: DeviceSpec) -> PlugKitDevice {
    let id = DeviceId(NEXT_DEVICE_ID.fetch_add(1, Ordering::Relaxed));
    with_device_registry_mut(|registry| {
        registry.insert(
            id.0,
            DeviceRecord {
                path: spec.path,
                name: spec.name,
                bus: spec.bus,
                class: spec.class,
                vendor_id: spec.vendor_id,
                device_id: spec.device_id,
                subsystem_vendor_id: spec.subsystem_vendor_id,
                subsystem_device_id: spec.subsystem_device_id,
                revision: spec.revision,
                properties: spec.properties,
            },
        );
    });
    PlugKitDevice { id }
}

pub fn unregister_device(id: DeviceId) -> bool {
    with_device_registry_mut(|registry| registry.remove(&id.0).is_some())
}

pub fn device_exists(id: DeviceId) -> bool {
    with_device_registry(|registry| registry.contains_key(&id.0))
}

pub fn register_interface(name: &str) -> PlugKitResult<InterfaceHandle> {
    let id = NEXT_INTERFACE_ID.fetch_add(1, Ordering::Relaxed);
    with_interface_registry_mut(|registry| {
        registry.insert(
            id,
            InterfaceRecord {
                name: name.to_string(),
            },
        );
    });
    Ok(InterfaceHandle(id))
}

pub fn unregister_interface(name: &str) -> PlugKitResult<()> {
    with_interface_registry_mut(|registry| {
        let key = registry
            .iter()
            .find_map(|(id, record)| (record.name == name).then_some(*id))
            .ok_or(PlugKitError::NoDevice)?;
        registry.remove(&key);
        Ok(())
    })
}

pub fn emit_event(event: PlugKitEvent) -> PlugKitResult<()> {
    with_event_log_mut(|events| events.push(event));
    Ok(())
}

pub fn log_info(message: &str) {
    with_log_lines_mut(|lines| lines.push((LogLevel::Info, message.to_string())));
}

pub fn log_warn(message: &str) {
    with_log_lines_mut(|lines| lines.push((LogLevel::Warn, message.to_string())));
}

pub fn log_error(message: &str) {
    with_log_lines_mut(|lines| lines.push((LogLevel::Error, message.to_string())));
}

pub fn take_events() -> Vec<PlugKitEvent> {
    with_event_log_mut(|events| events.drain(..).collect())
}

pub fn take_logs() -> Vec<(String, String)> {
    with_log_lines_mut(|lines| {
        lines
            .drain(..)
            .map(|(level, line)| {
                let name = match level {
                    LogLevel::Info => "info",
                    LogLevel::Warn => "warn",
                    LogLevel::Error => "error",
                };
                (name.to_string(), line)
            })
            .collect()
    })
}

pub fn make_descriptor(
    name: &'static str,
    type_name: &'static str,
    api_version: u32,
    probe: fn(&PlugKitDevice) -> ProbeResult,
    start: fn(PlugKitDevice, PlugKitResources) -> PlugKitResult<()>,
    stop: fn(PlugKitDevice) -> PlugKitResult<()>,
) -> DriverDescriptor {
    DriverDescriptor {
        name,
        type_name,
        api_version,
        probe,
        start,
        stop,
    }
}

impl fmt::Display for PlugKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PlugKitError::InvalidHandle => "InvalidHandle",
            PlugKitError::PermissionDenied => "PermissionDenied",
            PlugKitError::NotSupported => "NotSupported",
            PlugKitError::NoDevice => "NoDevice",
            PlugKitError::Busy => "Busy",
            PlugKitError::OutOfMemory => "OutOfMemory",
            PlugKitError::IoError => "IoError",
            PlugKitError::InvalidOffset => "InvalidOffset",
            PlugKitError::InvalidSize => "InvalidSize",
            PlugKitError::Interrupted => "Interrupted",
            PlugKitError::Unknown => "Unknown",
        })
    }
}
