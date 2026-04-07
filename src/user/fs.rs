//! ファイルシステム関連のシステムコール（ユーザー側）

use super::sys::{syscall1, syscall2, syscall3, SyscallNumber};
use alloc::format;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

fn path_buf(path: &str) -> ([u8; 512], usize) {
    let mut buf = [0u8; 512];
    let bytes = path.as_bytes();
    let len = bytes.len().min(511);
    buf[..len].copy_from_slice(&bytes[..len]);
    (buf, len)
}

pub fn mkdir(path: &str, mode: u32) -> u64 {
    let (buf, _) = path_buf(path);
    syscall2(SyscallNumber::Mkdir as u64, buf.as_ptr() as u64, mode as u64)
}

pub fn rmdir(path: &str) -> u64 {
    let (buf, _) = path_buf(path);
    syscall1(SyscallNumber::Rmdir as u64, buf.as_ptr() as u64)
}

pub fn readdir(fd: u64, buf: &mut [u8]) -> u64 {
    syscall3(
        SyscallNumber::Readdir as u64,
        fd,
        buf.as_mut_ptr() as u64,
        buf.len() as u64,
    )
}

pub fn chdir(path: &str) -> u64 {
    let (buf, _) = path_buf(path);
    syscall1(SyscallNumber::Chdir as u64, buf.as_ptr() as u64)
}

/// カレントワーキングディレクトリを取得する
pub fn getcwd(buf: &mut [u8]) -> Option<&str> {
    let ret = syscall2(
        SyscallNumber::Getcwd as u64,
        buf.as_mut_ptr() as u64,
        buf.len() as u64,
    );
    if ret == 0 || ret > 0xFFFF_FFFF_0000_0000 {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    core::str::from_utf8(&buf[..len]).ok()
}

// --- FS service IPC helpers ---
use crate::ipc;
use crate::task;
use crate::time;
use core::mem::size_of;

use crate::fs_consts::{FS_DATA_MAX, FS_PATH_MAX};
const FS_REQ_TIMEOUT_MS: u64 = 2000;
const FS_PENDING_CAPACITY: usize = 16;

#[repr(C)]
#[derive(Clone, Copy)]
struct FsRequestIp {
    op: u64,
    arg1: u64,
    arg2: u64,
    path: [u8; FS_PATH_MAX],
}

impl FsRequestIp {
    const OP_OPEN: u64 = 1;
    const OP_READ: u64 = 2;
    const OP_CLOSE: u64 = 4;
    const OP_EXEC: u64 = 5;

    fn exec(path: &str) -> Option<Self> {
        let mut path_buf = [0u8; FS_PATH_MAX];
        let bytes = path.as_bytes();
        if bytes.len() >= FS_PATH_MAX {
            return None;
        }
        path_buf[..bytes.len()].copy_from_slice(bytes);
        Some(Self { op: Self::OP_EXEC, arg1: 0, arg2: 0, path: path_buf })
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FsResponseIp {
    pub status: i64,
    pub len: u64,
    pub data: [u8; FS_DATA_MAX],
}

#[derive(Clone, Copy)]
struct PendingIpcMessage {
    used: bool,
    sender: u64,
    len: usize,
    data: [u8; size_of::<FsResponseIp>()],
}

impl PendingIpcMessage {
    const fn empty() -> Self {
        Self {
            used: false,
            sender: 0,
            len: 0,
            data: [0; size_of::<FsResponseIp>()],
        }
    }
}

static FS_PENDING_LOCK: AtomicBool = AtomicBool::new(false);
static mut FS_PENDING_MESSAGES: [PendingIpcMessage; FS_PENDING_CAPACITY] =
    [PendingIpcMessage::empty(); FS_PENDING_CAPACITY];

struct PendingQueueGuard;

impl PendingQueueGuard {
    fn lock() -> Self {
        while FS_PENDING_LOCK
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        Self
    }
}

impl Drop for PendingQueueGuard {
    fn drop(&mut self) {
        FS_PENDING_LOCK.store(false, Ordering::Release);
    }
}

fn enqueue_pending_message(sender: u64, len: usize, bytes: &[u8]) -> bool {
    let copy_len = core::cmp::min(len, core::cmp::min(bytes.len(), size_of::<FsResponseIp>()));
    let _lock = PendingQueueGuard::lock();
    unsafe {
        for slot in FS_PENDING_MESSAGES.iter_mut() {
            if !slot.used {
                slot.used = true;
                slot.sender = sender;
                slot.len = copy_len;
                if copy_len > 0 {
                    slot.data[..copy_len].copy_from_slice(&bytes[..copy_len]);
                }
                return true;
            }
        }
    }
    let msg = format!(
        "[swiftlib::fs] WARN: pending queue full (sender={}, len={})\n",
        sender, len
    );
    let _ = crate::io::write_stderr(msg.as_bytes());
    false
}

fn take_pending_message_for(sender: u64) -> Option<FsResponseIp> {
    let _lock = PendingQueueGuard::lock();
    unsafe {
        for slot in FS_PENDING_MESSAGES.iter_mut() {
            if slot.used && slot.sender == sender && slot.len >= size_of::<FsResponseIp>() {
                let resp = core::ptr::read_unaligned(slot.data.as_ptr() as *const FsResponseIp);
                slot.used = false;
                slot.sender = 0;
                slot.len = 0;
                return Some(resp);
            }
        }
    }
    None
}

fn find_fs_service() -> Option<u64> {
    task::find_process_by_name("fs.service")
}

fn fs_ipc_request(fs_tid: u64, req: &FsRequestIp) -> Result<FsResponseIp, ()> {
    let req_slice = unsafe {
        core::slice::from_raw_parts(req as *const _ as *const u8, size_of::<FsRequestIp>())
    };
    if ipc::ipc_send(fs_tid, req_slice) != 0 {
        return Err(());
    }

    let mut resp_buf = [0u8; size_of::<FsResponseIp>()];
    let start_tick = time::get_ticks();
    loop {
        if let Some(resp) = take_pending_message_for(fs_tid) {
            return Ok(resp);
        }

        let (sender, len) = ipc::ipc_recv(&mut resp_buf);
        if sender == 0 && len == 0 {
            if time::get_ticks().saturating_sub(start_tick) > FS_REQ_TIMEOUT_MS {
                return Err(());
            }
            time::sleep_ms(0); // yield のみ
            continue;
        }
        if sender != fs_tid || (len as usize) < size_of::<FsResponseIp>() {
            let msg_len = core::cmp::min(len as usize, resp_buf.len());
            let _ = enqueue_pending_message(sender, msg_len, &resp_buf[..msg_len]);
            continue;
        }
        let resp: FsResponseIp = unsafe { core::ptr::read_unaligned(resp_buf.as_ptr() as *const FsResponseIp) };
        return Ok(resp);
    }
}

/// Execute a file via fs.service. Returns PID on success or negative errno on failure.
pub fn exec_via_fs(path: &str) -> Result<u64, i64> {
    let fs_tid = find_fs_service().ok_or(-3)?; // ESRCH
    let exec_req = FsRequestIp::exec(path).ok_or(-22)?; // EINVAL
    let resp = fs_ipc_request(fs_tid, &exec_req).map_err(|_| -5)?; // EIO
    if resp.status < 0 {
        return Err(resp.status);
    }
    Ok(resp.status as u64)
}

/// Open via fs.service. Returns fd or negative errno.
pub fn open_via_fs(path: &str) -> Result<u64, i64> {
    let fs_tid = find_fs_service().ok_or(-3)?;
    let mut path_buf = [0u8; FS_PATH_MAX];
    let bytes = path.as_bytes();
    if bytes.len() >= FS_PATH_MAX {
        return Err(-22);
    }
    path_buf[..bytes.len()].copy_from_slice(bytes);
    let req = FsRequestIp { op: FsRequestIp::OP_OPEN, arg1: 0, arg2: 0, path: path_buf };
    let resp = fs_ipc_request(fs_tid, &req).map_err(|_| -5)?;
    if resp.status < 0 {
        return Err(resp.status);
    }
    Ok(resp.status as u64)
}

/// Read via fs.service into out buffer. Returns bytes read or negative errno.
pub fn read_via_fs(fd: u64, out: &mut [u8]) -> Result<usize, i64> {
    let fs_tid = find_fs_service().ok_or(-3)?;
    let req = FsRequestIp { op: FsRequestIp::OP_READ, arg1: fd, arg2: out.len() as u64, path: [0u8; FS_PATH_MAX] };
    let resp = fs_ipc_request(fs_tid, &req).map_err(|_| -5)?;
    if resp.status < 0 {
        return Err(resp.status);
    }
    let n = resp.len as usize;
    if n > out.len() || n > FS_DATA_MAX {
        return Err(-5);
    }
    out[..n].copy_from_slice(&resp.data[..n]);
    Ok(n)
}

/// Close via fs.service (best effort)
pub fn close_via_fs(fd: u64) {
    if let Some(fs_tid) = find_fs_service() {
        let req = FsRequestIp { op: FsRequestIp::OP_CLOSE, arg1: fd, arg2: 0, path: [0u8; FS_PATH_MAX] };
        let _ = fs_ipc_request(fs_tid, &req);
    }
}

/// Convenience: read whole file via fs.service.
/// Returns Ok(None) if the file is missing, Err on other errors, and Ok(Some(empty)) for empty files.
pub fn read_file_via_fs(path: &str, max_size: usize) -> Result<Option<Vec<u8>>, i64> {
    let fd = match open_via_fs(path) {
        Ok(fd) => fd,
        Err(errno) if errno == -2 => return Ok(None),
        Err(errno) => return Err(errno),
    };
    let mut out = Vec::new();
    let mut chunk = [0u8; FS_DATA_MAX];
    while out.len() < max_size {
        let to_read = core::cmp::min(chunk.len(), max_size - out.len());
        match read_via_fs(fd, &mut chunk[..to_read]) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(errno) => {
                close_via_fs(fd);
                return Err(errno);
            }
        }
    }
    close_via_fs(fd);
    Ok(Some(out))
}
