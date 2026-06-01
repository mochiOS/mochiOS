//! cext ローダ用の簡易レジストリ
//!
//! これまで `kmod::load_modules()` は `"disk"`/`"fs"` を match で直書きしていた。
//! それだと cext を増やすたびにカーネル側を編集する必要があり尖りすぎる。
//!
//! ここでは「モジュール名 -> (init symbol, register fn)」を表で定義し、
//! load_modules 側は共通処理だけにする。

use alloc::vec::Vec;
use alloc::vec;

pub type RegisterFn = fn(init_symbol_addr: u64, module_version: u16) -> bool;

#[derive(Clone, Copy)]
pub struct ModuleRegistration {
    pub name: &'static str,
    pub register: RegisterFn,
}

pub fn registrations() -> Vec<ModuleRegistration> {
    // NOTE: 追加したい cext はここに 1 行追加するだけでよい。
    vec![
        ModuleRegistration {
            name: "disk",
            register: super::register_disk_module,
        },
        ModuleRegistration {
            name: "fs",
            register: super::register_fs_module,
        },
    ]
}
