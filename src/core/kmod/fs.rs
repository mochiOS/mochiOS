use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU16, Ordering};

use super::{McxBuffer, McxFsOps, McxPath, MODULE_MAX_READ_BYTES};

static LOADED: AtomicBool = AtomicBool::new(false);
static MOUNTED: AtomicBool = AtomicBool::new(false);
static VERSION: AtomicU16 = AtomicU16::new(0);
static FS_OPS_PTR: AtomicPtr<McxFsOps> = AtomicPtr::new(core::ptr::null_mut());

pub fn register(ops: *const McxFsOps, version: u16) -> bool {
    if ops.is_null() {
        return false;
    }
    // Disable SMAP/SMEP while reading module-provided ops struct
    let _smap_guard = crate::cpu::SmapSmepGuard::new();
    let ops_ref = unsafe { &*ops };
    if (ops_ref.mount as usize) == 0
        || (ops_ref.set_disk_ops as usize) == 0
        || (ops_ref.create as usize) == 0
        || (ops_ref.remove as usize) == 0
        || (ops_ref.rename as usize) == 0
        || (ops_ref.read as usize) == 0
        || (ops_ref.write as usize) == 0
        || (ops_ref.truncate as usize) == 0
        || (ops_ref.stat as usize) == 0
        || (ops_ref.readdir as usize) == 0
    {
        return false;
    }
    FS_OPS_PTR.store(ops as *mut McxFsOps, Ordering::Release);
    VERSION.store(version, Ordering::Release);
    LOADED.store(true, Ordering::Release);
    true
}

pub fn is_loaded() -> bool {
    LOADED.load(Ordering::Acquire)
}

#[allow(dead_code)]
pub fn is_mounted() -> bool {
    MOUNTED.load(Ordering::Acquire)
}

pub fn mount(device_id: u32) -> i32 {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return -38;
    }
    // Disable SMAP/SMEP while dereferencing ops in module memory
    let _smap_guard = crate::cpu::SmapSmepGuard::new();
    if (unsafe { (*ops).mount } as usize) == 0 {
        return -38;
    }
    let rc = unsafe { ((*ops).mount)(device_id) };
    if rc == 0 {
        MOUNTED.store(true, Ordering::Release);
    }
    rc
}

pub fn set_disk_ops(disk_ops: *const crate::kmod::disk::McxDiskOps) -> i32 {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return -38;
    }
    let _smap_guard = crate::cpu::SmapSmepGuard::new();
    unsafe { ((*ops).set_disk_ops)(disk_ops) }
}

pub fn create(path: &str, mode: u32) -> i32 {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return -38;
    }
    let _smap_guard = crate::cpu::SmapSmepGuard::new();
    let path_bytes = path.as_bytes();
    let path_arg = McxPath {
        ptr: path_bytes.as_ptr(),
        len: path_bytes.len(),
    };
    unsafe { ((*ops).create)(path_arg, mode) }
}

pub fn remove(path: &str, is_dir: bool) -> i32 {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return -38;
    }
    let _smap_guard = crate::cpu::SmapSmepGuard::new();
    let path_bytes = path.as_bytes();
    let path_arg = McxPath {
        ptr: path_bytes.as_ptr(),
        len: path_bytes.len(),
    };
    unsafe { ((*ops).remove)(path_arg, is_dir as u32) }
}

pub fn rename(src: &str, dst: &str) -> i32 {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return -38;
    }
    let _smap_guard = crate::cpu::SmapSmepGuard::new();
    let src_bytes = src.as_bytes();
    let dst_bytes = dst.as_bytes();
    let src_arg = McxPath {
        ptr: src_bytes.as_ptr(),
        len: src_bytes.len(),
    };
    let dst_arg = McxPath {
        ptr: dst_bytes.as_ptr(),
        len: dst_bytes.len(),
    };
    unsafe { ((*ops).rename)(src_arg, dst_arg) }
}

pub fn read_all(path: &str) -> Option<Vec<u8>> {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return crate::init::fs::read(path);
    }

    // Disable SMAP/SMEP while calling into module ops
    let _smap_guard = crate::cpu::SmapSmepGuard::new();

    let mut out = Vec::new();
    let path_bytes = path.as_bytes();
    let path_arg = McxPath {
        ptr: path_bytes.as_ptr(),
        len: path_bytes.len(),
    };
    let mut offset: u64 = 0;
    let mut chunk = vec![0u8; 4096];

    loop {
        let mut nread: usize = 0;
        let rc = unsafe {
            ((*ops).read)(
                path_arg,
                offset,
                McxBuffer {
                    ptr: chunk.as_mut_ptr(),
                    len: chunk.len(),
                },
                &mut nread as *mut usize,
            )
        };
        if rc != 0 {
            if rc == -2 {
                return None;
            }
            if out.is_empty() {
                return crate::init::fs::read(path);
            }
            return Some(out);
        }
        if nread == 0 {
            break;
        }
        if nread > chunk.len() {
            return None;
        }
        out.extend_from_slice(&chunk[..nread]);
        offset = offset.saturating_add(nread as u64);
        if out.len() > MODULE_MAX_READ_BYTES {
            return None;
        }
    }
    Some(out)
}

pub fn write_all(path: &str, offset: u64, data: &[u8]) -> Option<usize> {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return None;
    }
    let _smap_guard = crate::cpu::SmapSmepGuard::new();

    let path_bytes = path.as_bytes();
    let path_arg = McxPath {
        ptr: path_bytes.as_ptr(),
        len: path_bytes.len(),
    };
    let mut written: usize = 0;
    let mut buf = McxBuffer {
        ptr: data.as_ptr() as *mut u8,
        len: data.len(),
    };
    let rc = unsafe { ((*ops).write)(path_arg, offset, buf, &mut written as *mut usize) };
    if rc != 0 {
        return None;
    }
    Some(written)
}

pub fn truncate(path: &str, len: u64) -> i32 {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return -38;
    }
    let _smap_guard = crate::cpu::SmapSmepGuard::new();
    let path_bytes = path.as_bytes();
    let path_arg = McxPath {
        ptr: path_bytes.as_ptr(),
        len: path_bytes.len(),
    };
    unsafe { ((*ops).truncate)(path_arg, len) }
}

pub fn file_metadata(path: &str) -> Option<(u16, u64)> {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return None;
    }
    // Disable SMAP/SMEP while calling into module ops
    let _smap_guard = crate::cpu::SmapSmepGuard::new();

    let path_bytes = path.as_bytes();
    let path_arg = McxPath {
        ptr: path_bytes.as_ptr(),
        len: path_bytes.len(),
    };
    let mut mode: u16 = 0;
    let mut size: u64 = 0;
    let rc = unsafe { ((*ops).stat)(path_arg, &mut mode as *mut u16, &mut size as *mut u64) };
    if rc != 0 {
        return None;
    }
    Some((mode, size))
}

pub fn is_directory(path: &str) -> bool {
    file_metadata(path)
        .map(|(mode, _)| (mode & 0xF000) == 0x4000)
        .unwrap_or(false)
}

pub fn readdir_path(path: &str) -> Option<Vec<alloc::string::String>> {
    let ops = FS_OPS_PTR.load(Ordering::Acquire);
    if ops.is_null() {
        return None;
    }
    // Disable SMAP/SMEP while calling into module ops
    let _smap_guard = crate::cpu::SmapSmepGuard::new();

    let path_bytes = path.as_bytes();
    let path_arg = McxPath {
        ptr: path_bytes.as_ptr(),
        len: path_bytes.len(),
    };
    let mut buf = vec![0u8; 16 * 1024];
    let mut out_len: usize = 0;
    let rc = unsafe {
        ((*ops).readdir)(
            path_arg,
            McxBuffer {
                ptr: buf.as_mut_ptr(),
                len: buf.len(),
            },
            &mut out_len as *mut usize,
        )
    };
    if rc != 0 || out_len > buf.len() {
        return None;
    }
    let bytes = &buf[..out_len];
    let mut out = Vec::new();
    for raw in bytes.split(|&b| b == b'\n') {
        if raw.is_empty() {
            continue;
        }
        if let Ok(s) = core::str::from_utf8(raw) {
            out.push(alloc::string::String::from(s));
        }
    }
    Some(out)
}
