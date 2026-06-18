#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use core::any::type_name;

pub use plugkit_macros::driver;
pub use plugkit_sys::{
    device_exists, emit_event, log_error, log_info, log_warn, register_device, register_interface,
    take_events, take_logs, unregister_device, unregister_interface, DeviceBus, DeviceBytes,
    DeviceClass, DeviceId, DeviceName, DevicePath, DeviceProperty, DeviceSpec, DeviceString,
    DmaBuffer, DmaHandle, DriverDescriptor, InterfaceHandle, Irq, IrqEvent, Mmio, MmioHandle,
    PciConfig, PciConfigHandle, PlugKitDevice, PlugKitError, PlugKitEvent, PlugKitResources,
    PlugKitResult, ProbeResult,
};

pub trait PlugKitDriver {
    fn probe(device: &PlugKitDevice) -> ProbeResult;
    fn start(device: PlugKitDevice, resources: PlugKitResources) -> PlugKitResult<()>;
    fn stop(device: PlugKitDevice) -> PlugKitResult<()>;
}

pub fn driver_descriptor<T: PlugKitDriver>() -> &'static DriverDescriptor {
    let name = type_name::<T>();
    Box::leak(Box::new(DriverDescriptor {
        name,
        type_name: name,
        api_version: 1,
        probe: T::probe,
        start: T::start,
        stop: T::stop,
    }))
}

pub mod prelude {
    pub use crate::{
        device_exists, driver, driver_descriptor, emit_event, log_error, log_info, log_warn,
        register_device, register_interface, take_events, take_logs, unregister_device,
        unregister_interface, DeviceBus, DeviceBytes, DeviceClass, DeviceId, DeviceName,
        DevicePath, DeviceProperty, DeviceSpec, DeviceString, DmaBuffer, DmaHandle,
        InterfaceHandle, Irq, IrqEvent, Mmio, MmioHandle, PciConfig, PciConfigHandle,
        PlugKitDevice, PlugKitDriver, PlugKitError, PlugKitEvent, PlugKitResources, PlugKitResult,
        ProbeResult,
    };
}
