use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU16, Ordering};

#[repr(C)]
pub struct McxDiskOps {
    pub probe: extern "C" fn() -> i32,
    /// 512バイトセクタを読み取る。buf_len は 512 以上を要求する。
    pub read_sector: extern "C" fn(disk_id: u32, lba: u64, buf: *mut u8, buf_len: usize) -> i32,
    /// 512バイトセクタを書き込む。buf_len は 512 以上を要求する。
    pub write_sector: extern "C" fn(disk_id: u32, lba: u64, buf: *const u8, buf_len: usize) -> i32,
}

static LOADED: AtomicBool = AtomicBool::new(false);
static VERSION: AtomicU16 = AtomicU16::new(0);
static DISK_OPS_PTR: AtomicPtr<McxDiskOps> = AtomicPtr::new(core::ptr::null_mut());

pub fn register(ops: *const McxDiskOps, version: u16) -> bool {
    if ops.is_null() {
        return false;
    }
    DISK_OPS_PTR.store(ops as *mut McxDiskOps, Ordering::Release);
    VERSION.store(version, Ordering::Release);
    LOADED.store(true, Ordering::Release);
    true
}

pub fn ops_ptr() -> *const McxDiskOps {
    DISK_OPS_PTR.load(Ordering::Acquire) as *const McxDiskOps
}

pub fn is_loaded() -> bool {
    LOADED.load(Ordering::Acquire)
}

#[allow(dead_code)]
pub fn version() -> u16 {
    VERSION.load(Ordering::Acquire)
}

#[allow(dead_code)]
pub fn probe() -> i32 {
    let ops = DISK_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return -38;
    }
    unsafe { ((*ops).probe)() }
}

#[allow(dead_code)]
pub fn read_sector(disk_id: u32, lba: u64, buf: &mut [u8]) -> i32 {
    let ops = DISK_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return -38;
    }
    unsafe { ((*ops).read_sector)(disk_id, lba, buf.as_mut_ptr(), buf.len()) }
}

#[allow(dead_code)]
pub fn write_sector(disk_id: u32, lba: u64, buf: &[u8]) -> i32 {
    let ops = DISK_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return -38;
    }
    unsafe { ((*ops).write_sector)(disk_id, lba, buf.as_ptr(), buf.len()) }
}
