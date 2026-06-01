use core::mem::size_of;
use std::boxed;
use mochi_syscall::ipc;
use mochi_syscall::fs_consts::{FS_DATA_MAX, FS_PATH_MAX, IPC_MAX_MSG_SIZE};
use mochi_syscall::task;

mod common;
mod disk_device;
mod kernel_block_device;
mod ext2;
mod initfs;

use common::{resolve_path, FileHandle, FileSystem, VfsError};
use disk_device::DiskServiceDevice;
use disk_device::pop_stashed;
use kernel_block_device::KernelBlockDevice;
use ext2::Ext2Fs;
use initfs::InitFs;

const MAX_HANDLES: usize = 16;

#[derive(Clone, Copy)]
struct OpenFile {
    used: bool,
    handle: FileHandle,
    fs_id: usize,
    is_special: bool,
    path: [u8; FS_PATH_MAX],
    path_len: usize,
}

impl OpenFile {
    const fn new() -> Self {
        Self {
            used: false,
            handle: FileHandle {
                inode: 0,
                offset: 0,
                flags: 0,
            },
            fs_id: 0,
            is_special: false,
            path: [0u8; FS_PATH_MAX],
            path_len: 0,
        }
    }
}

static mut HANDLES: [OpenFile; MAX_HANDLES] = [OpenFile::new(); MAX_HANDLES];

/// マウントされたファイルシステム（ext2 優先、InitFs フォールバック）
static mut MOUNTED_FS: Option<Box<dyn FileSystem>> = None;
static mut EXT2_MOUNTED: bool = false;

/// READY通知
const OP_NOTIFY_READY: u64 = 0xFF;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FsRequest {
    op: u64,
    arg1: u64,
    arg2: u64,
    path: [u8; FS_PATH_MAX],
}

impl FsRequest {
    const OP_OPEN: u64 = 1;
    const OP_READ: u64 = 2;
    const OP_WRITE: u64 = 3;
    const OP_CLOSE: u64 = 4;
    const OP_STAT: u64 = 6;
    const OP_FSTAT: u64 = 7;
    const OP_READDIR: u64 = 8;
    const OP_EXEC_STREAM: u64 = 9;
    const OP_READDIR_ALL: u64 = 10;
    const OP_SEEK: u64 = 11;
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FsResponse {
    status: i64,
    len: u64,
    data: [u8; FS_DATA_MAX],
}

#[repr(align(8))]
struct AlignedBuffer([u8; IPC_MAX_MSG_SIZE]);

// open(2) 互換フラグ（kernel と同一値）
const O_ACCMODE: u64 = 0o3;
const O_WRONLY: u64 = 0o1;
const O_RDWR: u64 = 0o2;
const O_CREAT: u64 = 0o100;
const O_TRUNC: u64 = 0o1000;

fn vfs_error_to_errno(err: VfsError) -> i64 {
    match err {
        VfsError::NotFound => -2,          // ENOENT
        VfsError::PermissionDenied => -13, // EACCES
        VfsError::AlreadyExists => -17,    // EEXIST
        VfsError::IsDirectory => -21,      // EISDIR
        VfsError::NotDirectory => -20,     // ENOTDIR
        VfsError::InvalidArgument => -22,  // EINVAL
        VfsError::IoError => -5,           // EIO
        VfsError::OutOfSpace => -28,       // ENOSPC
        VfsError::ReadOnlyFs => -30,       // EROFS
        VfsError::TooManyOpenFiles => -24, // EMFILE
        VfsError::FileTooBig => -27,       // EFBIG
        VfsError::NotSupported => -38,     // ENOSYS
    }
}

fn cap_for_path(path: &str, write: bool) -> &'static str {
    // パスから必要な capability を決定する。
    // ここで決定した capability を kernel に照会し、呼び出し元を強制する。
    //
    // 注意: write は read を含めない（明示的に分離する）。
    if path.starts_with("/tmp") || path.starts_with("tmp") {
        if write {
            "fs.write.tmp"
        } else {
            "fs.read.tmp"
        }
    } else if path.starts_with("/mount/removable") || path.starts_with("mount/removable") {
        if write {
            "fs.write.removable"
        } else {
            "fs.read.removable"
        }
    } else if let Some(rest) = path.strip_prefix("/home/") {
        // /home/<user>/X...
        let mut it = rest.split('/');
        let _user = it.next().unwrap_or("");
        let folder = it.next().unwrap_or("");
        match folder {
            "Documents" => {
                if write { "fs.write.user.documents" } else { "fs.read.user.documents" }
            }
            "Downloads" => {
                if write { "fs.write.user.downloads" } else { "fs.read.user.downloads" }
            }
            "Desktop" => {
                if write { "fs.write.user.desktop" } else { "fs.read.user.desktop" }
            }
            "Pictures" => {
                if write { "fs.write.user.pictures" } else { "fs.read.user.pictures" }
            }
            "Music" => {
                if write { "fs.write.user.music" } else { "fs.read.user.music" }
            }
            "Videos" => {
                if write { "fs.write.user.videos" } else { "fs.read.user.videos" }
            }
            _ => {
                if write { "fs.write.user" } else { "fs.read.user" }
            }
        }
    } else {
        if write {
            "fs.write.all"
        } else {
            "fs.read.all"
        }
    }
}

fn check_caller_capability(caller_tid: u64, capability: &str) -> Result<(), i64> {
    match mochi_syscall::capability::check_thread_capability(caller_tid, capability) {
        Ok(true) => Ok(()),
        Ok(false) => Err(-13), // EACCES
        Err(e) => Err(e),
    }
}

fn has_write_intent(flags: u64) -> bool {
    let acc = flags & O_ACCMODE;
    acc == O_WRONLY || acc == O_RDWR || (flags & (O_CREAT | O_TRUNC)) != 0
}

fn needs_read(flags: u64) -> bool {
    let acc = flags & O_ACCMODE;
    acc != O_WRONLY
}

fn needs_write(flags: u64) -> bool {
    has_write_intent(flags)
}

fn is_special_path(path: &str) -> bool {
    path == "/var/zero" || path == "/dev/zero" || path == "/dev/null"
}

//noinspection ALL
/// disk.service から ext2 をマウントする（失敗時は InitFs にフォールバック）
///
/// # 注意
/// core.service の都合で disk.service の起動が遅れる場合があるため、
/// 「見つからない＝即フォールバック」で固定せず、後から ext2 へ切り替えられるようにする。
fn try_mount_ext2_fastpath() -> bool {
    unsafe {
        if EXT2_MOUNTED {
            return true;
        }
    }

    // まずカーネルの block_read syscall を使う（IPCより圧倒的に速い）
    println!("[FS] Trying ext2 mount via kernel block syscalls...");
    let device = KernelBlockDevice::new(1); // disk 1 = Primary Slave = mochiOS.img
    match Ext2Fs::new(Box::new(device)) {
        Ok(fs) => {
            println!("[FS] ext2 filesystem mounted via kernel block syscalls.");
            unsafe {
                MOUNTED_FS = Some(Box::new(fs));
                EXT2_MOUNTED = true;
            }
            return true;
        }
        Err(e) => {
            // 互換: 古いカーネル（syscall 未実装）やドライバ未ロードの場合は IPC へフォールバック
            println!("[FS] kernel block mount failed: {:?}, falling back to disk.service...", e);
        }
    }

    if let Some(pid) = task::find_process_by_name("disk.service") {
        println!("[FS] Mounting ext2 from disk 1 via PID={}...", pid);
        let device = DiskServiceDevice::new(pid, 1); // disk 1 = Primary Slave = mochiOS.img
        println!("[FS] Calling Ext2Fs::new...");
        match Ext2Fs::new(Box::new(device)) {
            Ok(fs) => {
                println!("[FS] ext2 filesystem mounted from disk.service.");
                unsafe {
                    MOUNTED_FS = Some(Box::new(fs));
                    EXT2_MOUNTED = true;
                }
                return true;
            }
            Err(e) => {
                println!("[FS] ext2 mount failed: {:?}.", e);
            }
        }
    }

    false
}

fn mount_filesystem() {
    println!("[FS] mount_filesystem: searching for disk.service...");

    // disk.service がすぐ起動していれば ext2 を使うが、
    // 起動が遅れる場合は先に InitFs で立ち上げてサービス提供を継続する。
    // （起動ブロックすると、shell.service などが fs.service に依存して停止する）
    for _ in 0..50 {
        if try_mount_ext2_fastpath() {
            return;
        }
        task::sleep(100);
    }

    println!("[FS] disk.service not found (timeout), falling back to InitFs");
    println!("[FS] Initializing InitFs...");

    // フォールバック: InitFs
    let mut initfs = InitFs::new();
    if let Err(e) = initfs.create_sample_files() {
        println!("[FS] Warning: Failed to create sample files: {:?}", e);
    }
    unsafe {
        MOUNTED_FS = Some(boxed::Box::new(initfs));
        EXT2_MOUNTED = false;
    }
    println!("[FS] InitFS mounted as fallback.");
}

/// core.service に準備完了を通知する
fn notify_ready_to_core() {
    let core_pid = match task::find_process_by_name("core.service") {
        Some(pid) => pid,
        None => {
            println!("[FS] WARNING: core.service not found, skipping READY notify");
            return;
        }
    };

    let op_bytes = OP_NOTIFY_READY.to_le_bytes();
    if ipc::ipc_send(core_pid, &op_bytes) == 0 {
        println!("[FS] Sent READY to core.service (PID={})", core_pid);
    }
}

fn main() {
    println!("[FS] Service Started.");

    mount_filesystem();
    notify_ready_to_core();

    let mut recv_buf = AlignedBuffer([0u8; IPC_MAX_MSG_SIZE]);
    let mut retry_ticks: u64 = 0;
    let mut disk_seen: bool = false;

    loop {
        // disk.service とのやり取り中に退避されたメッセージを先に処理する
        let (sender, len) = match pop_stashed(&mut recv_buf.0) {
            Some(v) => v,
            None => ipc::ipc_recv(&mut recv_buf.0),
        };

        // EAGAIN (メッセージなし) の場合はCPUを譲る
        if sender == 0 && len == 0 {
            // InitFs フォールバック中に disk.service が遅延起動した場合、
            // 後から ext2 へ切り替えられるように定期的に試す。
            retry_ticks = retry_ticks.wrapping_add(1);
            if retry_ticks % 200 == 0 {
                if !disk_seen {
                    if let Some(pid) = task::find_process_by_name("disk.service") {
                        println!("[FS] disk.service detected (PID={}), trying ext2 mount...", pid);
                        disk_seen = true;
                    }
                }
                let _ = try_mount_ext2_fastpath();
            }
            task::yield_now();
            continue;
        }

        if sender != 0 && (len as usize) >= size_of::<FsRequest>() {
            // disk.service が後から起動した場合は、なるべく早く ext2 へ切り替える。
            if !disk_seen {
                if let Some(pid) = task::find_process_by_name("disk.service") {
                    println!("[FS] disk.service detected (PID={}), trying ext2 mount...", pid);
                    disk_seen = true;
                    let _ = try_mount_ext2_fastpath();
                }
            }
            let req: FsRequest = unsafe { core::ptr::read(recv_buf.0.as_ptr() as *const _) };
            println!("[FS] REQ op={} from PID={}", req.op, sender);

            let mut resp = FsResponse {
                status: -1,
                len: 0,
                data: [0; FS_DATA_MAX],
            };

            match req.op {
                FsRequest::OP_OPEN => {
                    let flags = req.arg2;

                    let mut path_len = 0;
                    while path_len < FS_PATH_MAX && req.path[path_len] != 0 {
                        path_len += 1;
                    }

                    if let Ok(path_str) = core::str::from_utf8(&req.path[..path_len]) {
                        // 特殊ファイルは fs.service が直接処理する（ディスク不要）
                        if is_special_path(path_str) {
                            // open 時点で read/write を強制する（open(RDWR) は両方要求）
                            if needs_read(flags) {
                                let cap = cap_for_path(path_str, false);
                                if let Err(errno) = check_caller_capability(sender, cap) {
                                    resp.status = errno;
                                    let resp_slice = unsafe {
                                        core::slice::from_raw_parts(
                                            &resp as *const _ as *const u8,
                                            core::mem::size_of::<FsResponse>(),
                                        )
                                    };
                                    let _ = ipc::ipc_send(sender, resp_slice);
                                    continue;
                                }
                            }
                            if needs_write(flags) {
                                let cap = cap_for_path(path_str, true);
                                if let Err(errno) = check_caller_capability(sender, cap) {
                                    resp.status = errno;
                                    let resp_slice = unsafe {
                                        core::slice::from_raw_parts(
                                            &resp as *const _ as *const u8,
                                            core::mem::size_of::<FsResponse>(),
                                        )
                                    };
                                    let _ = ipc::ipc_send(sender, resp_slice);
                                    continue;
                                }
                            }

                            unsafe {
                                let mut handle_idx: i64 = -1;
                                for i in 0..MAX_HANDLES {
                                    if !HANDLES[i].used {
                                        HANDLES[i].used = true;
                                        HANDLES[i].handle = FileHandle::new(0, flags as u32);
                                        HANDLES[i].handle.offset = 0;
                                        HANDLES[i].fs_id = 0;
                                        HANDLES[i].is_special = true;
                                        HANDLES[i].path[..path_len].copy_from_slice(&req.path[..path_len]);
                                        HANDLES[i].path_len = path_len;
                                        handle_idx = i as i64;
                                        break;
                                    }
                                }
                                resp.status = handle_idx;
                            }

                            let resp_slice = unsafe {
                                core::slice::from_raw_parts(
                                    &resp as *const _ as *const u8,
                                    size_of::<FsResponse>(),
                                )
                            };
                            let _ = ipc::ipc_send(sender, resp_slice);
                            continue;
                        }

                        // open 時点で read/write を強制する（open(RDWR) は両方要求）
                        if needs_read(flags) {
                            let cap = cap_for_path(path_str, false);
                            if let Err(errno) = check_caller_capability(sender, cap) {
                                resp.status = errno;
                                let resp_slice = unsafe {
                                    core::slice::from_raw_parts(
                                        &resp as *const _ as *const u8,
                                        core::mem::size_of::<FsResponse>(),
                                    )
                                };
                                let _ = ipc::ipc_send(sender, resp_slice);
                                continue;
                            }
                        }
                        if needs_write(flags) {
                            let cap = cap_for_path(path_str, true);
                            if let Err(errno) = check_caller_capability(sender, cap) {
                                resp.status = errno;
                                let resp_slice = unsafe {
                                    core::slice::from_raw_parts(
                                        &resp as *const _ as *const u8,
                                        core::mem::size_of::<FsResponse>(),
                                    )
                                };
                                let _ = ipc::ipc_send(sender, resp_slice);
                                continue;
                            }
                        }

                        unsafe {
                            if let Some(ref mut fs) = MOUNTED_FS {
                                // O_CREAT の場合は無ければ作る
                                let inode = match resolve_path(fs.as_ref(), path_str) {
                                    Ok(inode) => inode,
                                    Err(VfsError::NotFound) if (flags & O_CREAT) != 0 => {
                                        // 親ディレクトリを解決して create する
                                        let (parent, name) = match path_str.rsplit_once('/') {
                                            Some((p, n)) if !n.is_empty() => {
                                                let p = if p.is_empty() { "/" } else { p };
                                                (p, n)
                                            }
                                            _ => {
                                                resp.status = -22; // EINVAL
                                                let resp_slice = core::slice::from_raw_parts(
                                                    &resp as *const _ as *const u8,
                                                    size_of::<FsResponse>(),
                                                );
                                                let _ = ipc::ipc_send(sender, resp_slice);
                                                continue;
                                            }
                                        };

                                        match resolve_path(fs.as_ref(), parent) {
                                            Ok(parent_inode) => match fs.create(parent_inode, name, 0o644) {
                                                Ok(new_inode) => new_inode,
                                                Err(e) => {
                                                    resp.status = vfs_error_to_errno(e);
                                                    let resp_slice = core::slice::from_raw_parts(
                                                        &resp as *const _ as *const u8,
                                                        size_of::<FsResponse>(),
                                                    );
                                                    let _ = ipc::ipc_send(sender, resp_slice);
                                                    continue;
                                                }
                                            },
                                            Err(e) => {
                                                resp.status = vfs_error_to_errno(e);
                                                let resp_slice = core::slice::from_raw_parts(
                                                    &resp as *const _ as *const u8,
                                                    size_of::<FsResponse>(),
                                                );
                                                let _ = ipc::ipc_send(sender, resp_slice);
                                                continue;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        resp.status = vfs_error_to_errno(e);
                                        let resp_slice = core::slice::from_raw_parts(
                                            &resp as *const _ as *const u8,
                                            size_of::<FsResponse>(),
                                        );
                                        let _ = ipc::ipc_send(sender, resp_slice);
                                        continue;
                                    }
                                };

                                // O_TRUNC の場合は切り詰め
                                if (flags & O_TRUNC) != 0 {
                                    let _ = fs.truncate(inode, 0);
                                }

                                        let mut handle_idx: i64 = -1;
                                        for i in 0..MAX_HANDLES {
                                            if !HANDLES[i].used {
                                                HANDLES[i].used = true;
                                                HANDLES[i].handle = FileHandle::new(inode, flags as u32);
                                                HANDLES[i].fs_id = 0;
                                                HANDLES[i].is_special = false;
                                                HANDLES[i].path[..path_len]
                                                    .copy_from_slice(&req.path[..path_len]);
                                                HANDLES[i].path_len = path_len;
                                                handle_idx = i as i64;
                                                break;
                                            }
                                        }
                                        resp.status = handle_idx;
                            }
                        }
                    }
                }
                FsRequest::OP_READ => {
                    let fd = req.arg1 as usize;
                    let read_len = req.arg2 as usize;

                    if fd < MAX_HANDLES && unsafe { HANDLES[fd].used } {
                        if unsafe { HANDLES[fd].is_special } {
                            let path = unsafe {
                                core::str::from_utf8(&HANDLES[fd].path[..HANDLES[fd].path_len])
                                    .unwrap_or("")
                            };
                            // 呼び出し元の read 権限を強制
                            let cap = cap_for_path(path, false);
                            if let Err(errno) = check_caller_capability(sender, cap) {
                                resp.status = errno;
                                let resp_slice = unsafe {
                                    core::slice::from_raw_parts(
                                        &resp as *const _ as *const u8,
                                        core::mem::size_of::<FsResponse>(),
                                    )
                                };
                                let _ = ipc::ipc_send(sender, resp_slice);
                                continue;
                            }

                            if path == "/dev/null" {
                                resp.len = 0;
                                resp.status = 0;
                            } else {
                                let n = core::cmp::min(read_len, FS_DATA_MAX);
                                resp.data[..n].fill(0);
                                resp.len = n as u64;
                                resp.status = n as i64;
                                unsafe {
                                    HANDLES[fd].handle.offset =
                                        HANDLES[fd].handle.offset.saturating_add(n as u64);
                                }
                            }
                        } else {
                            // 呼び出し元の read 権限を強制
                            let path = unsafe {
                                core::str::from_utf8(&HANDLES[fd].path[..HANDLES[fd].path_len])
                                    .unwrap_or("")
                            };
                            let cap = cap_for_path(path, false);
                            if let Err(errno) = check_caller_capability(sender, cap) {
                                resp.status = errno;
                                let resp_slice = unsafe {
                                    core::slice::from_raw_parts(
                                        &resp as *const _ as *const u8,
                                        core::mem::size_of::<FsResponse>(),
                                    )
                                };
                                let _ = ipc::ipc_send(sender, resp_slice);
                                continue;
                            }

                            unsafe {
                                if let Some(ref fs) = MOUNTED_FS {
                                    let handle = &mut HANDLES[fd].handle;
                                    let inode = handle.inode;
                                    let offset = handle.offset;

                                    let actual_len = core::cmp::min(read_len, FS_DATA_MAX);

                                    match fs.read(inode, offset, &mut resp.data[..actual_len]) {
                                        Ok(bytes_read) => {
                                            resp.len = bytes_read as u64;
                                            resp.status = bytes_read as i64;
                                            handle.offset += bytes_read as u64;
                                        }
                                        Err(e) => {
                                            resp.status = vfs_error_to_errno(e);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        resp.status = -9; // EBADF
                    }
                }
                FsRequest::OP_WRITE => {
                    let fd = req.arg1 as usize;
                    let write_len = req.arg2 as usize;

                    if fd < MAX_HANDLES && unsafe { HANDLES[fd].used } {
                        if unsafe { HANDLES[fd].is_special } {
                            let path = unsafe {
                                core::str::from_utf8(&HANDLES[fd].path[..HANDLES[fd].path_len])
                                    .unwrap_or("")
                            };
                            // 呼び出し元の write 権限を強制
                            let cap = cap_for_path(path, true);
                            if let Err(errno) = check_caller_capability(sender, cap) {
                                resp.status = errno;
                                let resp_slice = unsafe {
                                    core::slice::from_raw_parts(
                                        &resp as *const _ as *const u8,
                                        core::mem::size_of::<FsResponse>(),
                                    )
                                };
                                let _ = ipc::ipc_send(sender, resp_slice);
                                continue;
                            }

                            // /dev/null, /var/zero, /dev/zero: 書き込みは捨てる
                            resp.len = write_len as u64;
                            resp.status = write_len as i64;
                            unsafe {
                                HANDLES[fd].handle.offset =
                                    HANDLES[fd].handle.offset.saturating_add(write_len as u64);
                            }
                        } else {
                            // 呼び出し元の write 権限を強制
                            let path = unsafe {
                                core::str::from_utf8(&HANDLES[fd].path[..HANDLES[fd].path_len])
                                    .unwrap_or("")
                            };
                            let cap = cap_for_path(path, true);
                            if let Err(errno) = check_caller_capability(sender, cap) {
                                resp.status = errno;
                                let resp_slice = unsafe {
                                    core::slice::from_raw_parts(
                                        &resp as *const _ as *const u8,
                                        core::mem::size_of::<FsResponse>(),
                                    )
                                };
                                let _ = ipc::ipc_send(sender, resp_slice);
                                continue;
                            }

                            // 書き込みデータは次のIPCメッセージで届く（カーネル側が分割送信する）
                            let mut remaining = write_len;
                            let mut wrote_total: u64 = 0;
                            let mut tmp_recv = AlignedBuffer([0u8; IPC_MAX_MSG_SIZE]);
                            while remaining > 0 {
                                let want = core::cmp::min(remaining, IPC_MAX_MSG_SIZE);
                                // 次のメッセージが目的の sender とは限らないため、
                                // 目的の sender から届くまで待ち続ける（単純化）。
                                let (mut s2, mut l2) = (0u64, 0u64);
                                loop {
                                    let (ss, ll) = ipc::ipc_recv_wait(&mut tmp_recv.0);
                                    if ss == sender && ll != 0 {
                                        s2 = ss;
                                        l2 = ll;
                                        break;
                                    }
                                }
                                if s2 != sender || l2 == 0 {
                                    resp.status = -5; // EIO
                                    break;
                                }
                                let got = core::cmp::min(want, l2 as usize);
                                unsafe {
                                    if let Some(ref mut fs) = MOUNTED_FS {
                                        let handle = &mut HANDLES[fd].handle;
                                        let inode = handle.inode;
                                        let offset = handle.offset;
                                        match fs.write(inode, offset, &tmp_recv.0[..got]) {
                                            Ok(n) => {
                                                handle.offset += n as u64;
                                                wrote_total += n as u64;
                                                remaining = remaining.saturating_sub(n);
                                            }
                                            Err(e) => {
                                                resp.status = vfs_error_to_errno(e);
                                                remaining = 0;
                                            }
                                        }
                                    }
                                }
                                if got == 0 {
                                    break;
                                }
                            }
                            if resp.status >= 0 {
                                resp.status = wrote_total as i64;
                            }
                        }
                    } else {
                        resp.status = -9; // EBADF
                    }
                }
                FsRequest::OP_CLOSE => {
                    let fd = req.arg1 as usize;
                    if fd < MAX_HANDLES && unsafe { HANDLES[fd].used } {
                        unsafe {
                            HANDLES[fd].used = false;
                        }
                        resp.status = 0;
                    } else {
                        resp.status = -9; // EBADF
                    }
                }
                FsRequest::OP_SEEK => {
                    let fd = req.arg1 as usize;
                    let new_off = req.arg2 as u64;
                    if fd < MAX_HANDLES && unsafe { HANDLES[fd].used } {
                        unsafe {
                            HANDLES[fd].handle.offset = new_off;
                        }
                        resp.status = new_off as i64;
                    } else {
                        resp.status = -9;
                    }
                }
                FsRequest::OP_STAT => {
                    let mut path_len = 0;
                    while path_len < FS_PATH_MAX && req.path[path_len] != 0 {
                        path_len += 1;
                    }
                    if let Ok(path_str) = core::str::from_utf8(&req.path[..path_len]) {
                        let cap = cap_for_path(path_str, false);
                        if let Err(errno) = check_caller_capability(sender, cap) {
                            resp.status = errno;
                        } else {
                            unsafe {
                                if let Some(ref fs) = MOUNTED_FS {
                                    match resolve_path(fs.as_ref(), path_str) {
                                        Ok(inode) => match fs.stat(inode) {
                                            Ok(attr) => {
                                                resp.status = attr.mode as i64;
                                                resp.len = attr.size;
                                            }
                                            Err(e) => resp.status = vfs_error_to_errno(e),
                                        },
                                        Err(e) => resp.status = vfs_error_to_errno(e),
                                    }
                                } else {
                                    resp.status = -5;
                                }
                            }
                        }
                    } else {
                        resp.status = -22;
                    }
                }
                FsRequest::OP_FSTAT => {
                    let fd = req.arg1 as usize;
                    if fd < MAX_HANDLES && unsafe { HANDLES[fd].used } {
                        let path = unsafe {
                            core::str::from_utf8(&HANDLES[fd].path[..HANDLES[fd].path_len])
                                .unwrap_or("")
                        };
                        let cap = cap_for_path(path, false);
                        if let Err(errno) = check_caller_capability(sender, cap) {
                            resp.status = errno;
                        } else {
                            unsafe {
                                if let Some(ref fs) = MOUNTED_FS {
                                    let inode = HANDLES[fd].handle.inode;
                                    match fs.stat(inode) {
                                        Ok(attr) => {
                                            resp.status = attr.mode as i64;
                                            resp.len = attr.size;
                                        }
                                        Err(e) => resp.status = vfs_error_to_errno(e),
                                    }
                                } else {
                                    resp.status = -5;
                                }
                            }
                        }
                    } else {
                        resp.status = -9;
                    }
                }
                FsRequest::OP_READDIR => {
                    let fd = req.arg1 as usize;
                    let start_index = (req.arg2 >> 32) as usize;
                    let max_bytes = (req.arg2 & 0xFFFF_FFFF) as usize;
                    if fd < MAX_HANDLES && unsafe { HANDLES[fd].used } {
                        let path = unsafe {
                            core::str::from_utf8(&HANDLES[fd].path[..HANDLES[fd].path_len])
                                .unwrap_or("")
                        };
                        let cap = cap_for_path(path, false);
                        if let Err(errno) = check_caller_capability(sender, cap) {
                            resp.status = errno;
                        } else {
                            unsafe {
                                if let Some(ref fs) = MOUNTED_FS {
                                    let inode = HANDLES[fd].handle.inode;
                                    match fs.readdir(inode) {
                                        Ok(entries) => {
                                            let mut cursor = 0usize;
                                            let mut out_len = 0usize;
                                            let mut next = entries.len();
                                            for (idx, e) in entries.iter().enumerate().skip(start_index) {
                                                let name = e.name.as_bytes();
                                                let need = name.len() + 1; // '\n'
                                                if out_len + need > resp.data.len().min(max_bytes) {
                                                    next = idx;
                                                    break;
                                                }
                                                resp.data[out_len..out_len + name.len()].copy_from_slice(name);
                                                out_len += name.len();
                                                resp.data[out_len] = b'\n';
                                                out_len += 1;
                                                cursor = idx + 1;
                                            }
                                            let next_index = if cursor >= entries.len() { entries.len() } else { next };
                                            resp.status = next_index as i64;
                                            resp.len = out_len as u64;
                                        }
                                        Err(e) => resp.status = vfs_error_to_errno(e),
                                    }
                                } else {
                                    resp.status = -5;
                                }
                            }
                        }
                    } else {
                        resp.status = -9;
                    }
                }
                FsRequest::OP_EXEC_STREAM => {
                    let mut path_len = 0;
                    while path_len < FS_PATH_MAX && req.path[path_len] != 0 {
                        path_len += 1;
                    }

                    if let Ok(path_str) = core::str::from_utf8(&req.path[..path_len]) {
                        // 呼び出し元の read 権限を強制
                        let cap = cap_for_path(path_str, false);
                        if let Err(errno) = check_caller_capability(sender, cap) {
                            resp.status = errno;
                            let resp_slice = unsafe {
                                core::slice::from_raw_parts(
                                    &resp as *const _ as *const u8,
                                    core::mem::size_of::<FsResponse>(),
                                )
                            };
                            let _ = ipc::ipc_send(sender, resp_slice);
                            continue;
                        }

                        println!("[FS] OP_EXEC_STREAM: path={}", path_str);
                        unsafe {
                            if let Some(ref fs) = MOUNTED_FS {
                                match resolve_path(fs.as_ref(), path_str) {
                                    Ok(inode) => {
                                        // ファイルサイズを取得
                                        match fs.stat(inode) {
                                            Ok(stat) => {
                                                let file_size = stat.size;
                                                resp.status = 0; // OK
                                                resp.len = file_size;
                                                
                                                println!("[FS] OP_EXEC_STREAM: file_size={}", file_size);
                                                
                                                // ヘッダーをまず送信
                                                let resp_slice = core::slice::from_raw_parts(
                                                    &resp as *const _ as *const u8,
                                                    size_of::<FsResponse>()
                                                );
                                                let _ = ipc::ipc_send(sender, resp_slice);
                                                println!("[FS] OP_EXEC_STREAM: sent header (len={})", file_size);
                                                
                                                // ファイルコンテンツを複数メッセージで送信
                                                // IPC_MAX_MSG_SIZE のため、小さなチャンクで送信
                                                const CHUNK_SIZE: usize = 4000; // IPC_MAX_MSG_SIZE の安全マージン
                                                let mut offset: u64 = 0;
                                                let mut chunk_count = 0u32;
                                                while offset < file_size {
                                                    let chunk_len = core::cmp::min(
                                                        CHUNK_SIZE,
                                                        (file_size - offset) as usize
                                                    );
                                                    let mut chunk_buf = vec![0u8; chunk_len];
                                                    match fs.read(inode, offset, &mut chunk_buf) {
                                                        Ok(bytes_read) => {
                                                            if bytes_read > 0 {
                                                                let _ = ipc::ipc_send(sender, &chunk_buf[..bytes_read]);
                                                                offset += bytes_read as u64;
                                                                chunk_count += 1;
                                                                if chunk_count % 100 == 0 {
                                                                    println!("[FS] OP_EXEC_STREAM: sent {} chunks, {} bytes", chunk_count, offset);
                                                                }
                                                            } else {
                                                                break;
                                                            }
                                                        }
                                                        Err(e) => {
                                                            println!("[FS] OP_EXEC_STREAM: read error {:?} at offset {}", e, offset);
                                                            break;
                                                        }
                                                    }
                                                }
                                                println!("[FS] OP_EXEC_STREAM: done, total chunks={}, total bytes={}", chunk_count, offset);
                                                
                                                // ヘッダーレスポンス送信済みなので以下のコードをスキップ
                                                continue;
                                            }
                                            Err(e) => {
                                                resp.status = vfs_error_to_errno(e);
                                                println!("[FS] OP_EXEC_STREAM: stat error {:?}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        resp.status = vfs_error_to_errno(e);
                                        println!("[FS] OP_EXEC_STREAM: resolve error {:?}", e);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {
                    println!("[FS] Unknown OP: {}", req.op);
                    continue;
                }
            }

            let resp_slice = unsafe {
                core::slice::from_raw_parts(&resp as *const _ as *const u8, size_of::<FsResponse>())
            };

            let _ = ipc::ipc_send(sender, resp_slice);
        }
    }
}
