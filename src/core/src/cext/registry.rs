//! cext ローダ用の簡易レジストリ

use alloc::vec;
use alloc::vec::Vec;

pub type RegisterFn = fn(init_symbol_addr: u64, module_version: u16) -> bool;

#[derive(Clone, Copy)]
pub struct ModuleRegistration {
    pub name: &'static str,
    pub version: u16,
    pub register: RegisterFn,
}

pub fn registrations() -> Vec<ModuleRegistration> {
    vec![ModuleRegistration {
        name: "disk",
        version: 1,
        register: super::register_disk_module,
    }]
}
