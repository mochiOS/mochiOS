//! ファイルシステム関連のシステムコール

use super::types::{
    EACCES, EBADF, EEXIST, EFAULT, EINVAL, EIO, ENOENT, ENOSYS, ENOTDIR, ESRCH, SUCCESS,
};
use crate::task::fd_table::{FdTable, FileHandle, FD_BASE, O_CLOEXEC, PROCESS_MAX_FDS};
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

// グローバル FD テーブルは廃止。各プロセスの Process::fd_table を使用する。

#[inline]
fn current_process_id_raw() -> Option<u64> {
    crate::task::current_thread_id()
        .and_then(|tid| crate::task::with_thread(tid, |t| t.process_id().as_u64()))
}

/// 現在プロセスの FD テーブルを読み取り専用で操作する。
fn with_fd_table<F, R>(pid_raw: u64, f: F) -> Option<R>
where
    F: FnOnce(&FdTable) -> R,
{
    let pid = crate::task::ids::ProcessId::from_u64(pid_raw);
    crate::task::with_process(pid, |p| f(p.fd_table()))
}

/// 現在プロセスの FD テーブルを可変で操作する。
fn with_fd_table_mut<F, R>(pid_raw: u64, f: F) -> Option<R>
where
    F: FnOnce(&mut FdTable) -> R,
{
    let pid = crate::task::ids::ProcessId::from_u64(pid_raw);
    crate::task::with_process_mut(pid, |p| f(p.fd_table_mut()))
}

fn read_cstring(ptr: u64) -> Result<String, u64> {
    crate::syscall::read_user_cstring(ptr, 1024)
}

fn resolve_path_at(pid_raw: u64, dirfd: i64, path_ptr: u64) -> Result<String, u64> {
    const AT_FDCWD: i64 = -100;

    if dirfd == AT_FDCWD {
        return read_cstring(path_ptr).map(|path| normalize_path(&path));
    }

    let idx = dirfd as usize;
    if idx >= PROCESS_MAX_FDS {
        return Err(EBADF);
    }
    let dir_path = match with_fd_table(pid_raw, |t| t.get(idx).and_then(|fh| fh.dir_path.clone())) {
        Some(Some(p)) => p,
        _ => return Err(EBADF),
    };
    let path = read_cstring(path_ptr)?;
    let full_path = if path.starts_with('/') {
        path
    } else {
        alloc::format!("{}/{}", dir_path.trim_end_matches('/'), path)
    };
    Ok(normalize_path(&full_path))
}

fn path_has_prefix(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn path_requires_fs_read_all(path: &str) -> bool {
    path_has_prefix(path, "/system")
        || path_has_prefix(path, "/Modules")
        || path_has_prefix(path, "/config")
        || path_has_prefix(path, "/bin")
        || path_has_prefix(path, "/lib")
        || path_has_prefix(path, "/log")
        || path_has_prefix(path, "/var/log")
}

pub(crate) fn ensure_fs_path_readable(path: &str) -> Result<(), u64> {
    if !path_requires_fs_read_all(path) {
        return Ok(());
    }

    let pid = match current_process_id_raw() {
        Some(pid) => pid,
        None => return Err(EBADF),
    };
    let pid = crate::task::ids::ProcessId::from_u64(pid);
    if crate::task::process::process_has_capability(pid, crate::capability::Capability::FsReadAll) {
        Ok(())
    } else {
        Err(EACCES)
    }
}

pub(crate) fn close_remote_fd_from_kernel(_fd_remote: u64) {}

#[inline]
fn mode_is_directory(mode: u16) -> bool {
    (mode & 0xF000) == 0x4000
}

#[inline]
fn mode_for_stat(mode: u16) -> u32 {
    let mut out = mode as u32;
    if (out & 0xF000) == 0 {
        out |= 0x8000;
    }
    if (out & 0o777) == 0 {
        out |= 0o755;
    }
    out
}

#[inline]
pub(crate) fn metadata_rootfs_first(path: &str) -> Option<(u16, u64)> {
    crate::kmod::fs::file_metadata(path).or_else(|| crate::init::fs::file_metadata(path))
}

#[inline]
pub(crate) fn is_directory_rootfs_first(path: &str) -> bool {
    crate::kmod::fs::is_directory(path) || crate::init::fs::is_directory(path)
}

#[inline]
pub(crate) fn readdir_rootfs_first(path: &str) -> Option<Vec<String>> {
    let entries = crate::kmod::fs::readdir_path(path)
        .or_else(|| crate::init::fs::readdir_path(path))
        .unwrap_or_default();
    merge_special_dir_entries(entries, path)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpecialFileKind {
    Zero,
    Null,
    AuditLog,
    WaylandSocket,
    RuntimeDir,
}

#[inline]
fn special_file_kind(path: &str) -> Option<SpecialFileKind> {
    match path {
        "/var/zero" | "/dev/zero" => Some(SpecialFileKind::Zero),
        "/dev/null" => Some(SpecialFileKind::Null),
        "/log/audit.log" | "/var/log/audit.log" => Some(SpecialFileKind::AuditLog),
        "/run" | "/run/user" | "/run/user/0" | "/dev/shm" => Some(SpecialFileKind::RuntimeDir),
        "/run/user/0/wayland-0" => Some(SpecialFileKind::WaylandSocket),
        _ => None,
    }
}

#[inline]
fn special_file_metadata(path: &str) -> Option<(u16, u64)> {
    match special_file_kind(path)? {
        SpecialFileKind::Zero | SpecialFileKind::Null => Some((0x2000 | 0o666, 0)),
        SpecialFileKind::AuditLog => Some((0x8000 | 0o444, crate::audit::file_size() as u64)),
        SpecialFileKind::WaylandSocket => Some((0xC000 | 0o660, 0)),
        SpecialFileKind::RuntimeDir => Some((0x4000 | 0o755, 0)),
    }
}

#[inline]
fn special_dir_entries(path: &str) -> Option<Vec<String>> {
    match path {
        "/run" => Some(vec!["user".to_string()]),
        "/run/user" => Some(vec!["0".to_string()]),
        "/run/user/0" => Some(vec!["wayland-0".to_string()]),
        "/dev/shm" => Some(Vec::new()),
        _ => None,
    }
}

#[inline]
fn merge_special_dir_entries(mut entries: Vec<String>, path: &str) -> Option<Vec<String>> {
    if let Some(special) = special_dir_entries(path) {
        for name in special {
            if !entries.iter().any(|existing| existing == &name) {
                entries.push(name);
            }
        }
        Some(entries)
    } else if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

#[inline]
fn special_dir_entry_dtype(path: &str, name: &str) -> Option<u8> {
    match (path, name) {
        ("/run", "user") | ("/run/user", "0") => Some(4),
        ("/run/user/0", "wayland-0") => Some(12),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        merge_special_dir_entries, parse_readdir_typed, special_dir_entries, special_dir_entry_dtype,
        special_file_allows_open, special_file_metadata, special_path_blocks_mutation, O_CREAT,
        O_RDWR, O_TRUNC, O_WRONLY, SpecialFileKind,
    };

    const O_RDONLY: u64 = 0;

    #[test]
    fn runtime_dirs_expose_expected_children() {
        assert_eq!(
            special_dir_entries("/run").unwrap(),
            vec!["user".to_string()]
        );
        assert_eq!(
            special_dir_entries("/run/user").unwrap(),
            vec!["0".to_string()]
        );
        assert_eq!(
            special_dir_entries("/run/user/0").unwrap(),
            vec!["wayland-0".to_string()]
        );
    }

    #[test]
    fn runtime_dir_merge_preserves_real_entries() {
        assert_eq!(
            merge_special_dir_entries(vec!["existing".to_string()], "/run/user/0").unwrap(),
            vec!["existing".to_string(), "wayland-0".to_string()]
        );
    }

    #[test]
    fn runtime_dir_merge_deduplicates_special_entries() {
        assert_eq!(
            merge_special_dir_entries(
                vec!["wayland-0".to_string(), "existing".to_string()],
                "/run/user/0"
            )
            .unwrap(),
            vec!["wayland-0".to_string(), "existing".to_string()]
        );
    }

    #[test]
    fn runtime_dir_reports_special_entry_types() {
        assert_eq!(special_dir_entry_dtype("/run", "user"), Some(4));
        assert_eq!(special_dir_entry_dtype("/run/user", "0"), Some(4));
        assert_eq!(special_dir_entry_dtype("/run/user/0", "wayland-0"), Some(12));
        assert_eq!(special_dir_entry_dtype("/run/user/0", "other"), None);
    }

    #[test]
    fn parse_readdir_typed_accepts_socket_dtype() {
        let bytes = b"wayland-0\0\x0c\n";
        assert_eq!(
            parse_readdir_typed(bytes),
            vec![("wayland-0".to_string(), 12)]
        );
    }

    #[test]
    fn wayland_socket_reports_socket_mode() {
        assert_eq!(
            special_file_metadata("/run/user/0/wayland-0"),
            Some((0xC000 | 0o660, 0))
        );
        assert!(matches!(
            super::special_file_kind("/run/user/0/wayland-0"),
            Some(SpecialFileKind::WaylandSocket)
        ));
    }

    #[test]
    fn wayland_runtime_paths_reject_write_like_open() {
        assert!(special_file_allows_open("/run/user/0/wayland-0", O_RDONLY));
        assert!(!special_file_allows_open("/run/user/0/wayland-0", O_WRONLY));
        assert!(!special_file_allows_open("/run/user/0/wayland-0", O_RDWR));
        assert!(!special_file_allows_open("/run/user/0/wayland-0", O_CREAT));
        assert!(!special_file_allows_open("/run/user/0/wayland-0", O_TRUNC));
        assert!(special_file_allows_open("/dev/shm", O_RDONLY));
        assert!(!special_file_allows_open("/dev/shm", O_WRONLY));
    }

    #[test]
    fn wayland_runtime_paths_block_mutation() {
        assert!(special_path_blocks_mutation("/run/user/0/wayland-0"));
        assert!(special_path_blocks_mutation("/run/user/0"));
        assert!(special_path_blocks_mutation("/dev/shm"));
        assert!(!special_path_blocks_mutation("/dev/null"));
    }
}

#[inline]
fn is_special_local_path(path: &str) -> bool {
    special_file_metadata(path).is_some()
}

#[inline]
fn special_file_requires_read_cap(path: &str) -> bool {
    matches!(special_file_kind(path), Some(SpecialFileKind::AuditLog))
}

#[inline]
fn special_path_blocks_mutation(path: &str) -> bool {
    matches!(
        special_file_kind(path),
        Some(SpecialFileKind::RuntimeDir | SpecialFileKind::WaylandSocket | SpecialFileKind::AuditLog)
    )
}

#[inline]
fn special_file_allows_open(path: &str, flags: u64) -> bool {
    match special_file_kind(path) {
        Some(SpecialFileKind::RuntimeDir) | Some(SpecialFileKind::WaylandSocket) => {
            !has_write_intent(flags) && (flags & O_CREAT) == 0 && (flags & O_TRUNC) == 0
        }
        Some(SpecialFileKind::AuditLog) => !has_write_intent(flags),
        Some(SpecialFileKind::Zero) | Some(SpecialFileKind::Null) => true,
        None => true,
    }
}

#[inline]
fn stat_path_local_or_special(path: &str) -> Result<(u16, u64), u64> {
    if let Some((mode, size)) = special_file_metadata(path) {
        Ok((mode, size))
    } else {
        ensure_fs_path_readable(path)?;
        metadata_rootfs_first(path).ok_or(ENOENT)
    }
}

fn handle_is_special(fh: &FileHandle) -> bool {
    fh.dir_path
        .as_deref()
        .map(is_special_local_path)
        .unwrap_or(false)
}

fn handle_special_kind(fh: &FileHandle) -> Option<SpecialFileKind> {
    fh.dir_path.as_deref().and_then(special_file_kind)
}

fn parse_readdir_names(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for raw in bytes.split(|&b| b == b'\n') {
        if raw.is_empty() {
            continue;
        }
        if let Ok(name) = core::str::from_utf8(raw) {
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }
    out
}

fn parse_readdir_typed(bytes: &[u8]) -> Vec<(String, u8)> {
    let mut out = Vec::new();
    for record in bytes.split(|&b| b == b'\n') {
        if record.len() < 2 {
            continue;
        }
        let dtype = record[record.len() - 1];
        if dtype == 0 {
            continue;
        }
        if record.len() >= 2 && record[record.len() - 2] == 0 {
            let name_bytes = &record[..record.len() - 2];
            if let Ok(name) = core::str::from_utf8(name_bytes) {
                if !name.is_empty() {
                    out.push((name.to_string(), dtype));
                }
            }
        }
    }
    out
}

/// パスを正規化する（`.` / `..` を解決し重複スラッシュを除去）
fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        alloc::format!("/{}", parts.join("/"))
    }
}

/// プロセスの CWD を基に相対パスを絶対パスへ解決する
fn resolve_path(pid_raw: u64, path: &str) -> String {
    if path.starts_with('/') {
        normalize_path(path)
    } else {
        let pid = crate::task::ids::ProcessId::from_u64(pid_raw);
        let cwd = crate::task::with_process(pid, |p| {
            let mut s = alloc::string::String::new();
            s.push_str(p.cwd());
            s
        })
        .unwrap_or_else(|| "/".to_string());
        normalize_path(&alloc::format!("{}/{}", cwd.trim_end_matches('/'), path))
    }
}

const O_ACCMODE: u64 = 0o3;
const O_WRONLY: u64 = 0o1;
const O_RDWR: u64 = 0o2;
const O_CREAT: u64 = 0o100;
const O_EXCL: u64 = 0o200;
const O_TRUNC: u64 = 0o1000;
const O_APPEND: u64 = 0o2000;

pub(crate) fn is_tty_like_path(path: &str) -> bool {
    path == "/dev/tty"
        || path == "/dev/console"
        || path == "/dev/stdin"
        || path == "/dev/stdout"
        || path == "/dev/stderr"
        || path.starts_with("/dev/pts/")
}

fn make_tty_handle(path: &str) -> alloc::boxed::Box<FileHandle> {
    let tty_path = if is_tty_like_path(path) {
        path
    } else {
        "/dev/tty"
    };
    alloc::boxed::Box::new(FileHandle {
        data: alloc::boxed::Box::new([]),
        pos: 0,
        fs_path: None,
        dir_path: Some(tty_path.to_string()),
        is_remote: false,
        fd_remote: 0,
        remote_refs: None,
        pipe_id: None,
        pipe_write: false,
        open_flags: O_RDWR,
    })
}

fn has_write_intent(flags: u64) -> bool {
    let acc = flags & O_ACCMODE;
    acc == O_WRONLY || acc == O_RDWR || (flags & (O_CREAT | O_TRUNC)) != 0
}

fn open_resolved_for_pid(owner_pid: u64, path: &str, flags: u64) -> u64 {
    if is_tty_like_path(path) {
        let cloexec = (flags & O_CLOEXEC) != 0;
        return match with_fd_table_mut(owner_pid, |t| t.alloc(make_tty_handle(path), cloexec)) {
            Some(Some(fd)) => fd as u64,
            _ => ENOSYS,
        };
    }

    if let Err(errno) = ensure_fs_path_readable(path) {
        return errno;
    }
    if special_file_requires_read_cap(path) {
        // 監査ログは special file としても読み取りには通常の fs.read.all を要求する。
        if let Some(pid_raw) = current_process_id_raw() {
            let pid = crate::task::ids::ProcessId::from_u64(pid_raw);
            if !crate::task::process::process_has_capability(
                pid,
                crate::capability::Capability::FsReadAll,
            ) {
                return EACCES;
            }
        } else {
            return EBADF;
        }
    }

    // O_CREAT|O_EXCL は先に存在チェックしておく。
    if (flags & (O_CREAT | O_EXCL)) == (O_CREAT | O_EXCL) {
        let exists_in_fallback = metadata_rootfs_first(path).is_some();
        if exists_in_fallback {
            return EEXIST;
        }
    }

    if is_special_local_path(path) {
        if !special_file_allows_open(path, flags) {
            return EACCES;
        }
        let cloexec = (flags & O_CLOEXEC) != 0;
        let handle = alloc::boxed::Box::new(FileHandle {
            data: alloc::boxed::Box::new([]),
            pos: 0,
            fs_path: None,
            dir_path: Some(path.to_string()),
            is_remote: false,
            fd_remote: 0,
            remote_refs: None,
            pipe_id: None,
            pipe_write: false,
            open_flags: flags,
        });
        return match with_fd_table_mut(owner_pid, |t| t.alloc(handle, cloexec)) {
            Some(Some(fd)) => fd as u64,
            _ => ENOSYS,
        };
    }

    let visible_exists = metadata_rootfs_first(path).is_some();
    let existing_persistent = crate::kmod::fs::file_metadata(path).is_some();
    if has_write_intent(flags) && !existing_persistent && (flags & O_CREAT) == 0 {
        return EACCES;
    }
    if (flags & O_CREAT) != 0 && !visible_exists {
        if crate::kmod::fs::create(path, 0o644) != 0 {
            return crate::syscall::types::EIO;
        }
    }
    if (flags & O_TRUNC) != 0
        && (existing_persistent || crate::kmod::fs::file_metadata(path).is_some())
    {
        if crate::kmod::fs::truncate(path, 0) != 0 {
            return crate::syscall::types::EIO;
        }
    }
    let persistent_path = crate::kmod::fs::file_metadata(path).is_some();
    if has_write_intent(flags) && !persistent_path && (flags & O_CREAT) == 0 {
        return EACCES;
    }

    // 通常パス: disk.cext/fs.cext 経由で読む（IPC しない）
    let (data_vec, dir_path) = if is_directory_rootfs_first(path) {
        (Vec::new(), Some(path.to_string()))
    } else {
        match crate::kmod::fs::read_all(path) {
            Some(d) => (d, None),
            None => return ENOENT,
        }
    };
    let is_remote = false;
    let fd_remote = 0u64;

    let cloexec = (flags & O_CLOEXEC) != 0;
    let handle = alloc::boxed::Box::new(FileHandle {
        data: data_vec.into_boxed_slice(),
        pos: 0,
        fs_path: if persistent_path {
            Some(path.to_string())
        } else {
            None
        },
        dir_path,
        is_remote,
        fd_remote,
        remote_refs: None,
        pipe_id: None,
        pipe_write: false,
        open_flags: flags,
    });

    match with_fd_table_mut(owner_pid, |t| t.alloc(handle, cloexec)) {
        Some(Some(fd)) => fd as u64,
        _ => ENOSYS,
    }
}

/// Openシステムコール (initfs の読み取り専用をサポートする簡易実装)
pub fn open(path_ptr: u64, flags: u64) -> u64 {
    let owner_pid = match current_process_id_raw() {
        Some(pid) => pid,
        None => return EBADF,
    };

    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };

    let path = resolve_path(owner_pid, &path);
    open_resolved_for_pid(owner_pid, &path, flags)
}

/// Closeシステムコール
pub fn close(fd: u64) -> u64 {
    if fd < FD_BASE as u64 {
        return EBADF;
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let handle = with_fd_table_mut(pid, |t| t.take(idx));
    match handle {
        Some(Some(_h)) => SUCCESS,
        _ => EBADF,
    }
}

/// Seekシステムコール
pub fn seek(fd: u64, offset: i64, whence: u64) -> u64 {
    if fd < FD_BASE as u64 {
        return ENOSYS;
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };

    match with_fd_table_mut(pid, |t| {
        let fh = t.get_mut(idx).ok_or(EBADF)?;
        let new_pos = match whence {
            0 => offset,
            1 => fh.pos as i64 + offset,
            2 => {
                let len = if handle_special_kind(fh) == Some(SpecialFileKind::AuditLog) {
                    crate::audit::file_size() as i64
                } else {
                    fh.data.len() as i64
                };
                len + offset
            }
            _ => return Err(EINVAL),
        };
        if new_pos < 0 {
            return Err(EINVAL);
        }
        let limit = if handle_special_kind(fh) == Some(SpecialFileKind::AuditLog) {
            crate::audit::file_size()
        } else {
            fh.data.len()
        };
        let new_pos = core::cmp::min(new_pos as usize, limit);
        fh.pos = new_pos;
        Ok(fh.pos as u64)
    }) {
        Some(Ok(pos)) => pos,
        Some(Err(e)) => e,
        None => EBADF,
    }
}

/// Linux x86_64 struct stat をユーザーバッファに書き込む
///
/// struct stat のレイアウト (144 バイト):
///   0:  st_dev    (u64)
///   8:  st_ino    (u64)
///   16: st_nlink  (u64)
///   24: st_mode   (u32)
///   28: st_uid    (u32)
///   32: st_gid    (u32)
///   36: __pad0    (u32)
///   40: st_rdev   (u64)
///   48: st_size   (i64)
///   56: st_blksize (i64)
///   64: st_blocks  (i64)  — 512 バイト単位
///   72-143: timespec × 3 + unused (ゼロ)
fn write_stat_buf(stat_ptr: u64, mode: u32, size: u64) {
    const STAT_SIZE: usize = 144;
    let blocks = size.div_ceil(512);
    let mut buf = [0u8; STAT_SIZE];
    buf[0..8].copy_from_slice(&1u64.to_ne_bytes());
    buf[8..16].copy_from_slice(&1u64.to_ne_bytes());
    buf[16..24].copy_from_slice(&1u64.to_ne_bytes());
    buf[24..28].copy_from_slice(&mode.to_ne_bytes());
    buf[48..56].copy_from_slice(&size.to_ne_bytes());
    buf[56..64].copy_from_slice(&4096u64.to_ne_bytes());
    buf[64..72].copy_from_slice(&blocks.to_ne_bytes());
    let _ = crate::syscall::copy_to_user(stat_ptr, &buf);
}

/// Fstatシステムコール
pub fn fstat(fd: u64, stat_ptr: u64) -> u64 {
    if stat_ptr == 0 {
        return EFAULT;
    }
    const STAT_SIZE: u64 = 144;
    if !crate::syscall::validate_user_ptr(stat_ptr, STAT_SIZE) {
        return EFAULT;
    }

    if fd < FD_BASE as u64 {
        // stdin/stdout/stderr → キャラクタデバイス (S_IFCHR | 0666 = 0x2000 | 0o666)
        write_stat_buf(stat_ptr, 0x2000 | 0o666, 0);
        return SUCCESS;
    }

    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }

    // FileHandle からメタデータを取得する
    let file_info = with_fd_table(pid, |t| {
        t.get(idx).map(|fh| {
            let is_tty = fh
                .dir_path
                .as_deref()
                .map(is_tty_like_path)
                .unwrap_or(false);
            let is_special = handle_is_special(fh);
            let special_kind = handle_special_kind(fh);
            let size = if special_kind == Some(SpecialFileKind::AuditLog) {
                crate::audit::file_size() as u64
            } else {
                fh.data.len() as u64
            };
            (
                size,
                fh.dir_path.is_some(),
                is_tty,
                is_special,
                special_kind,
            )
        })
    });
    let (size, is_dir, is_tty, is_special, special_kind) = match file_info {
        Some(Some(v)) => v,
        _ => return EBADF,
    };
    let mode = if special_kind == Some(SpecialFileKind::AuditLog) {
        0x8000u32 | 0o444
    } else if is_special {
        0x2000u32 | 0o666
    } else if is_tty {
        0x2000u32 | 0o666
    } else if is_dir {
        0x4000u32 | 0o755
    } else {
        0x8000u32 | 0o755
    };
    write_stat_buf(stat_ptr, mode, size);
    SUCCESS
}

/// Statシステムコール
pub fn stat(path_ptr: u64, stat_ptr: u64) -> u64 {
    if path_ptr == 0 || stat_ptr == 0 {
        return EINVAL;
    }
    const STAT_SIZE: u64 = 144;
    if !crate::syscall::validate_user_ptr(stat_ptr, STAT_SIZE) {
        return EFAULT;
    }
    let owner_pid = match current_process_id_raw() {
        Some(pid) => pid,
        None => return EBADF,
    };
    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let resolved = resolve_path(owner_pid, &path);
    if let Err(errno) = ensure_fs_path_readable(&resolved) {
        return errno;
    }
    match stat_path_local_or_special(&resolved) {
        Ok((mode, size)) => {
            write_stat_buf(stat_ptr, mode_for_stat(mode), size);
            SUCCESS
        }
        Err(errno) => errno,
    }
}

/// Mkdirシステムコール（読み取り専用ファイルシステムのため未実装）
pub fn mkdir(_path_ptr: u64, _mode: u64) -> u64 {
    ENOSYS
}

/// Rmdirシステムコール
pub fn rmdir(path_ptr: u64) -> u64 {
    if path_ptr == 0 {
        return EINVAL;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let resolved = resolve_path(pid, &path);
    if let Err(errno) = ensure_fs_path_readable(&resolved) {
        return errno;
    }
    if special_path_blocks_mutation(&resolved) {
        return EACCES;
    }
    if !crate::kmod::fs::is_directory(&resolved) {
        return ENOTDIR;
    }
    if crate::kmod::fs::remove(&resolved, true) != 0 {
        return crate::syscall::types::EIO;
    }
    SUCCESS
}

/// Readdirシステムコール
pub fn readdir(fd: u64, buf_ptr: u64, buf_len: u64) -> u64 {
    if buf_ptr == 0 || buf_len == 0 {
        return EINVAL;
    }
    if !crate::syscall::validate_user_ptr(buf_ptr, buf_len) {
        return EFAULT;
    }
    if fd < FD_BASE as u64 {
        return EBADF;
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };

    let (dir_path, _is_special) = match with_fd_table(pid, |t| {
        t.get(idx)
            .map(|fh| (fh.dir_path.clone(), handle_is_special(fh)))
    }) {
        Some(Some((Some(p), false))) => (p, false),
        Some(Some((Some(_), true))) => return ENOTDIR,
        _ => return EBADF,
    };

    if let Err(errno) = ensure_fs_path_readable(&dir_path) {
        return errno;
    }

    let names = match readdir_rootfs_first(&dir_path) {
        Some(n) => n,
        None => return EINVAL,
    };
    let joined = names.join("\n");
    let bytes = joined.as_bytes();
    let to_copy = core::cmp::min(bytes.len(), buf_len as usize);
    if crate::syscall::copy_to_user(buf_ptr, &bytes[..to_copy]).is_err() {
        return EFAULT;
    }
    to_copy as u64
}

/// Chdirシステムコール
pub fn chdir(path_ptr: u64) -> u64 {
    if path_ptr == 0 {
        return EINVAL;
    }
    let pid_raw = match current_process_id_raw() {
        Some(pid) => pid,
        None => return EBADF,
    };
    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let resolved = resolve_path(pid_raw, &path);
    if let Err(errno) = ensure_fs_path_readable(&resolved) {
        return errno;
    }
    match stat_path_local_or_special(&resolved) {
        Ok((mode, _)) => {
            if !mode_is_directory(mode) {
                return ENOTDIR;
            }
        }
        Err(errno) => return errno,
    }
    let pid = crate::task::ids::ProcessId::from_u64(pid_raw);
    crate::task::with_process_mut(pid, |p| p.set_cwd(&resolved));
    SUCCESS
}

/// Getcwdシステムコール
pub fn getcwd(buf_ptr: u64, size: u64) -> u64 {
    if buf_ptr == 0 || size == 0 {
        return EINVAL;
    }
    if !crate::syscall::validate_user_ptr(buf_ptr, size) {
        return EFAULT;
    }
    let pid_raw = match current_process_id_raw() {
        Some(pid) => pid,
        None => return EFAULT,
    };
    let pid = crate::task::ids::ProcessId::from_u64(pid_raw);
    let mut tmp = [0u8; 256];
    let cwd_len = crate::task::with_process(pid, |p| {
        let s = p.cwd().as_bytes();
        let n = s.len().min(255);
        tmp[..n].copy_from_slice(&s[..n]);
        n
    })
    .unwrap_or(1);
    let needed = cwd_len + 1;
    if (size as usize) < needed {
        return EINVAL;
    }
    if crate::syscall::copy_to_user(buf_ptr, &tmp[..cwd_len]).is_err() {
        return EFAULT;
    }
    if crate::syscall::copy_to_user(buf_ptr + cwd_len as u64, &[0]).is_err() {
        return EFAULT;
    }
    buf_ptr
}

/// Read: 開かれたファイルからデータを読み込む
pub fn read(fd: u64, buf_ptr: u64, len: u64) -> u64 {
    if buf_ptr == 0 {
        return EFAULT;
    }
    if len == 0 {
        return 0;
    }
    if !crate::syscall::validate_user_ptr(buf_ptr, len) {
        return EFAULT;
    }
    if fd < FD_BASE as u64 {
        return EBADF;
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };

    let local = match with_fd_table_mut(pid, |t| {
        let fh = t.get_mut(idx)?;
        if handle_is_special(fh) {
            let to_read = match handle_special_kind(fh) {
                Some(SpecialFileKind::Null) => 0usize,
                Some(SpecialFileKind::Zero) => core::cmp::min(len as usize, len as usize),
                Some(SpecialFileKind::AuditLog) => {
                    let available = crate::audit::file_size().saturating_sub(fh.pos);
                    core::cmp::min(available, len as usize)
                }
                Some(SpecialFileKind::WaylandSocket) | Some(SpecialFileKind::RuntimeDir) => 0usize,
                None => 0usize,
            };
            if to_read == 0 {
                return Some(Vec::new());
            }
            let mut data = Vec::with_capacity(to_read);
            if matches!(handle_special_kind(fh), Some(SpecialFileKind::AuditLog)) {
                data.resize(to_read, 0);
                let copied = crate::audit::read_file_at(fh.pos, &mut data);
                data.truncate(copied);
                fh.pos = fh.pos.saturating_add(copied);
                return Some(data);
            }
            data.resize(to_read, 0);
            fh.pos = fh.pos.saturating_add(to_read);
            return Some(data);
        }
        let avail = fh.data.len().saturating_sub(fh.pos);
        if avail == 0 {
            return Some(Vec::new());
        }
        let to_read = core::cmp::min(avail, len as usize);
        let mut data = Vec::with_capacity(to_read);
        data.extend_from_slice(&fh.data[fh.pos..fh.pos + to_read]);
        fh.pos += to_read;
        Some(data)
    }) {
        Some(Some(v)) => v,
        _ => return EBADF,
    };

    if local.is_empty() {
        return 0;
    }

    if crate::syscall::copy_to_user(buf_ptr, &local).is_err() {
        return EFAULT;
    }
    local.len() as u64
}

/// Write: 開かれたファイルへデータを書き込む
pub fn write(fd: u64, buf_ptr: u64, len: u64) -> u64 {
    if buf_ptr == 0 {
        return EFAULT;
    }
    if len == 0 {
        return 0;
    }
    if !crate::syscall::validate_user_ptr(buf_ptr, len) {
        return EFAULT;
    }
    if fd < FD_BASE as u64 {
        return EBADF;
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };

    let mut buf = alloc::vec![0u8; len as usize];
    if let Err(errno) = crate::syscall::copy_from_user(buf_ptr, &mut buf) {
        return errno;
    }

    let (start_pos, fs_path, is_special, special_kind) = match with_fd_table(pid, |t| {
        t.get(idx).map(|fh| {
            (
                fh.pos,
                fh.fs_path.clone(),
                handle_is_special(fh),
                handle_special_kind(fh),
            )
        })
    }) {
        Some(Some(info)) => info,
        _ => return EBADF,
    };

    if is_special && special_kind == Some(SpecialFileKind::AuditLog) {
        return EACCES;
    }
    if is_special
        && matches!(
            special_kind,
            Some(SpecialFileKind::WaylandSocket | SpecialFileKind::RuntimeDir)
        )
    {
        return EACCES;
    }

    if let Some(path) = fs_path.as_deref() {
        match crate::kmod::fs::write_all(path, start_pos as u64, &buf) {
            Some(written) if written == buf.len() => {}
            _ => return crate::syscall::types::EIO,
        }
    }

    let wrote = with_fd_table_mut(pid, |t| {
        let fh = t.get_mut(idx).ok_or(EBADF)?;
        if handle_is_special(fh) {
            return Ok(buf.len() as u64);
        }
        let end = start_pos.checked_add(buf.len()).ok_or(EINVAL)?;
        let mut data = fh.data.to_vec();
        if end > data.len() {
            data.resize(end, 0);
        }
        data[start_pos..end].copy_from_slice(&buf);
        fh.data = data.into_boxed_slice();
        fh.pos = end;
        Ok(buf.len() as u64)
    });
    match wrote {
        Some(Ok(n)) => n,
        Some(Err(errno)) => errno,
        None => EBADF,
    }
}

/// Fcntl システムコール（FD フラグ操作）
///
/// - F_GETFD (1): FD フラグを取得
/// - F_SETFD (2): FD フラグを設定
/// - F_GETFL (3): ファイル状態フラグを取得（スタブ: 0 を返す）
/// - F_SETFL (4): ファイル状態フラグを設定（スタブ: 成功を返す）
pub fn fcntl(fd: u64, cmd: u64, arg: u64) -> u64 {
    use crate::task::fd_table::FD_CLOEXEC;
    const F_GETFD: u64 = 1;
    const F_SETFD: u64 = 2;
    const F_GETFL: u64 = 3;
    const F_SETFL: u64 = 4;
    const F_GETLK: u64 = 5;
    const F_SETLK: u64 = 6;
    const F_SETLKW: u64 = 7;

    if fd < FD_BASE as u64 {
        // stdin/stdout/stderr: FD フラグは 0
        return match cmd {
            F_GETFD | F_GETFL | F_GETLK => 0,
            F_SETFD | F_SETFL | F_SETLK | F_SETLKW => SUCCESS,
            _ => EINVAL,
        };
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };

    match cmd {
        F_GETFD => match with_fd_table(pid, |t| t.get_flags(idx)) {
            Some(Some(flags)) => flags as u64,
            _ => EBADF,
        },
        F_SETFD => {
            let cloexec = (arg & 1) != 0;
            let new_flags = if cloexec { FD_CLOEXEC } else { 0 };
            match with_fd_table_mut(pid, |t| t.set_flags(idx, new_flags)) {
                Some(true) => SUCCESS,
                _ => EBADF,
            }
        }
        F_GETFL => match with_fd_table(pid, |t| t.get(idx).map(|fh| fh.open_flags)) {
            Some(Some(v)) => v,
            _ => EBADF,
        },
        F_SETFL => {
            match with_fd_table_mut(pid, |t| {
                let fh = t.get_mut(idx).ok_or(EBADF)?;
                fh.open_flags = (fh.open_flags & O_ACCMODE) | (arg & !O_ACCMODE);
                Ok::<(), u64>(())
            }) {
                Some(Ok(())) => SUCCESS,
                Some(Err(errno)) => errno,
                None => EBADF,
            }
        }
        F_GETLK => SUCCESS,
        F_SETLK | F_SETLKW => SUCCESS,
        _ => EINVAL,
    }
}

/// fsync/fdatasync システムコール（最小実装）
pub fn fsync(fd: u64) -> u64 {
    if fd < FD_BASE as u64 {
        return SUCCESS;
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    match with_fd_table(pid, |t| t.get(idx).is_some()) {
        Some(true) => SUCCESS,
        _ => EBADF,
    }
}

/// truncate システムコール（最小実装）
pub fn truncate(path_ptr: u64, len: u64) -> u64 {
    if path_ptr == 0 {
        return EFAULT;
    }
    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let path = resolve_path(pid, &path);
    if special_path_blocks_mutation(&path) {
        return EACCES;
    }
    if special_file_requires_read_cap(&path) {
        return EACCES;
    }
    if crate::kmod::fs::file_metadata(&path).is_none() {
        return ENOENT;
    }
    if crate::kmod::fs::truncate(&path, len) != 0 {
        return crate::syscall::types::EIO;
    }
    SUCCESS
}

/// ftruncate システムコール（ローカル一時FDのみ）
pub fn ftruncate(fd: u64, len: u64) -> u64 {
    if fd < FD_BASE as u64 {
        return EBADF;
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let new_len = match usize::try_from(len) {
        Ok(v) => v,
        Err(_) => return EINVAL,
    };
    let res = with_fd_table_mut(pid, |t| {
        let fh = t.get_mut(idx).ok_or(EBADF)?;
        if handle_is_special(fh) {
            if handle_special_kind(fh) == Some(SpecialFileKind::AuditLog) {
                return Err(ENOSYS);
            }
            return Err(ENOSYS);
        }
        if let Some(path) = fh.fs_path.as_deref() {
            if crate::kmod::fs::truncate(path, len) != 0 {
                return Err(crate::syscall::types::EIO);
            }
        }
        let mut data = fh.data.to_vec();
        data.resize(new_len, 0);
        fh.data = data.into_boxed_slice();
        if fh.pos > new_len {
            fh.pos = new_len;
        }
        Ok(())
    });
    match res {
        Some(Ok(())) => SUCCESS,
        Some(Err(errno)) => errno,
        None => EBADF,
    }
}

/// Dup システムコール: FD を複製して最小の空き番号に割り当てる
pub fn dup(fd: u64) -> u64 {
    if fd < FD_BASE as u64 {
        let pid = match current_process_id_raw() {
            Some(p) => p,
            None => return EBADF,
        };
        return match with_fd_table_mut(pid, |t| t.alloc(make_tty_handle("/dev/tty"), false)) {
            Some(Some(new_fd)) => new_fd as u64,
            _ => ENOSYS,
        };
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };

    // 既存エントリをクローンして新しい FD を割り当てる
    let cloned = with_fd_table(pid, |t| {
        t.get(idx).map(|fh| {
            alloc::boxed::Box::new(FileHandle {
                data: fh.data.clone(),
                pos: fh.pos,
                fs_path: fh.fs_path.clone(),
                dir_path: fh.dir_path.clone(),
                is_remote: false,
                fd_remote: 0,
                remote_refs: None,
                pipe_id: fh.pipe_id,
                pipe_write: fh.pipe_write,
                open_flags: fh.open_flags,
            })
        })
    });
    let new_handle = match cloned {
        Some(Some(h)) => h,
        _ => return EBADF,
    };

    match with_fd_table_mut(pid, |t| t.alloc(new_handle, false)) {
        Some(Some(new_fd)) => new_fd as u64,
        _ => ENOSYS,
    }
}

/// Dup2 システムコール: FD を指定した番号に複製する
pub fn dup2(old_fd: u64, new_fd: u64) -> u64 {
    if new_fd < FD_BASE as u64 || new_fd as usize >= PROCESS_MAX_FDS {
        return EBADF;
    }
    if old_fd == new_fd {
        // old_fd が有効かどうかだけ確認
        if old_fd < FD_BASE as u64 {
            return old_fd;
        }
        let pid = match current_process_id_raw() {
            Some(p) => p,
            None => return EBADF,
        };
        return match with_fd_table(pid, |t| t.get(old_fd as usize).is_some()) {
            Some(true) => old_fd,
            _ => EBADF,
        };
    }

    let new_idx = new_fd as usize;
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };

    let new_handle = if old_fd < FD_BASE as u64 {
        make_tty_handle("/dev/tty")
    } else {
        let old_idx = old_fd as usize;
        if old_idx >= PROCESS_MAX_FDS {
            return EBADF;
        }
        let cloned = with_fd_table(pid, |t| {
            t.get(old_idx).map(|fh| {
                alloc::boxed::Box::new(FileHandle {
                    data: fh.data.clone(),
                    pos: fh.pos,
                    fs_path: fh.fs_path.clone(),
                    dir_path: fh.dir_path.clone(),
                    is_remote: false,
                    fd_remote: 0,
                    remote_refs: None,
                    pipe_id: fh.pipe_id,
                    pipe_write: fh.pipe_write,
                    open_flags: fh.open_flags,
                })
            })
        });
        match cloned {
            Some(Some(h)) => h,
            _ => return EBADF,
        }
    };

    // new_fd が使用中なら閉じる
    with_fd_table_mut(pid, |t| {
        t.close_fd(new_idx);
        let ptr = alloc::boxed::Box::into_raw(new_handle) as u64;
        t.entries[new_idx] = ptr;
        t.flags[new_idx] = 0;
    });

    new_fd
}

/// unlink システムコール
pub fn unlink(path_ptr: u64) -> u64 {
    if path_ptr == 0 {
        return EINVAL;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(errno) => return errno,
    };
    let resolved = resolve_path(pid, &path);
    if let Err(errno) = ensure_fs_path_readable(&resolved) {
        return errno;
    }
    if special_path_blocks_mutation(&resolved) {
        return EACCES;
    }
    if crate::kmod::fs::is_directory(&resolved) {
        return ENOTDIR;
    }
    if crate::kmod::fs::remove(&resolved, false) != 0 {
        return crate::syscall::types::EIO;
    }
    SUCCESS
}

/// unlinkat システムコール（最小実装）
pub fn unlinkat(_dirfd: i64, path_ptr: u64, _flags: u64) -> u64 {
    unlink(path_ptr)
}

/// renameat システムコール
pub fn renameat(old_dirfd: i64, old_path_ptr: u64, new_dirfd: i64, new_path_ptr: u64) -> u64 {
    if old_path_ptr == 0 || new_path_ptr == 0 {
        return EINVAL;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let old_path = match resolve_path_at(pid, old_dirfd, old_path_ptr) {
        Ok(path) => path,
        Err(errno) => return errno,
    };
    let new_path = match resolve_path_at(pid, new_dirfd, new_path_ptr) {
        Ok(path) => path,
        Err(errno) => return errno,
    };
    if let Err(errno) = ensure_fs_path_readable(&old_path) {
        return errno;
    }
    if let Err(errno) = ensure_fs_path_readable(&new_path) {
        return errno;
    }
    if special_path_blocks_mutation(&old_path) || special_path_blocks_mutation(&new_path) {
        return EACCES;
    }
    if crate::kmod::fs::rename(&old_path, &new_path) != 0 {
        return EIO;
    }
    SUCCESS
}

/// Openat システムコール
///
/// AT_FDCWD(-100) の場合は CWD 相対の open() と同等。
/// それ以外の dirfd は fd_table からディレクトリパスを取得してプレフィックスとして使用する。
pub fn openat(dirfd: i64, path_ptr: u64, flags: u64, _mode: u64) -> u64 {
    const AT_FDCWD: i64 = -100;

    if dirfd == AT_FDCWD {
        // CWD 相対 → 通常の open() と同じ
        return open(path_ptr, flags);
    }

    // dirfd が示すディレクトリを取得
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let idx = dirfd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let dir_path = match with_fd_table(pid, |t| t.get(idx).and_then(|fh| fh.dir_path.clone())) {
        Some(Some(p)) => p,
        _ => return EBADF,
    };

    // path を dir_path に対して解決する
    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let full_path = if path.starts_with('/') {
        path
    } else {
        alloc::format!("{}/{}", dir_path.trim_end_matches('/'), path)
    };

    open_resolved_for_pid(pid, &normalize_path(&full_path), flags)
}

/// Newfstatat (fstatat) システムコール
///
/// AT_FDCWD(-100) の場合は stat() と同等。
pub fn newfstatat(dirfd: i64, path_ptr: u64, stat_ptr: u64, flags: u64) -> u64 {
    const AT_FDCWD: i64 = -100;
    const AT_EMPTY_PATH: u64 = 0x1000;

    // AT_EMPTY_PATH: path が空の場合は dirfd 自体を fstat する
    if (flags & AT_EMPTY_PATH) != 0 {
        if dirfd == AT_FDCWD {
            return stat(path_ptr, stat_ptr);
        }
        return fstat(dirfd as u64, stat_ptr);
    }

    if dirfd == AT_FDCWD {
        return stat(path_ptr, stat_ptr);
    }

    // dirfd 相対パスを解決して stat
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let idx = dirfd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let dir_path = match with_fd_table(pid, |t| t.get(idx).and_then(|fh| fh.dir_path.clone())) {
        Some(Some(p)) => p,
        _ => return EBADF,
    };
    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let full = if path.starts_with('/') {
        normalize_path(&path)
    } else {
        normalize_path(&alloc::format!(
            "{}/{}",
            dir_path.trim_end_matches('/'),
            path
        ))
    };
    match stat_path_local_or_special(&full) {
        Ok((mode, size)) => {
            const STAT_SIZE: u64 = 144;
            if !crate::syscall::validate_user_ptr(stat_ptr, STAT_SIZE) {
                return EFAULT;
            }
            write_stat_buf(stat_ptr, mode_for_stat(mode), size);
            SUCCESS
        }
        Err(errno) => errno,
    }
}

/// Faccessat システムコール
pub fn faccessat(dirfd: i64, path_ptr: u64, _mode: u64, _flags: u64) -> u64 {
    const AT_FDCWD: i64 = -100;
    if path_ptr == 0 {
        return EINVAL;
    }
    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let resolved = if dirfd == AT_FDCWD || path.starts_with('/') {
        normalize_path(&path)
    } else {
        let pid = match current_process_id_raw() {
            Some(p) => p,
            None => return EBADF,
        };
        let idx = dirfd as usize;
        if idx >= PROCESS_MAX_FDS {
            return EBADF;
        }
        match with_fd_table(current_process_id_raw().unwrap_or(0), |t| {
            t.get(idx).and_then(|fh| fh.dir_path.clone())
        }) {
            Some(Some(d)) => {
                normalize_path(&alloc::format!("{}/{}", d.trim_end_matches('/'), path))
            }
            _ => return EBADF,
        }
    };
    match stat_path_local_or_special(&resolved) {
        Ok(_) => SUCCESS,
        Err(errno) => errno,
    }
}

/// statfs システムコール（最小実装）
///
/// Linux x86_64 の `struct statfs` (120 bytes) を埋めて返す。
pub fn statfs(path_ptr: u64, buf_ptr: u64) -> u64 {
    const STATFS_SIZE: u64 = 120;
    if path_ptr == 0 || buf_ptr == 0 {
        return EINVAL;
    }
    if !crate::syscall::validate_user_ptr(buf_ptr, STATFS_SIZE) {
        return EFAULT;
    }

    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };
    let path = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let resolved = resolve_path(pid, &path);
    if let Err(errno) = ensure_fs_path_readable(&resolved) {
        return errno;
    }
    if stat_path_local_or_special(&resolved).is_err() {
        return ENOENT;
    }

    // struct statfs {
    //   long f_type, f_bsize, f_blocks, f_bfree, f_bavail, f_files, f_ffree;
    //   fsid_t f_fsid; long f_namelen, f_frsize, f_flags, f_spare[4];
    // }
    let mut buf = [0u8; STATFS_SIZE as usize];
    buf[0..8].copy_from_slice(&0xEF53u64.to_ne_bytes()); // ext2 magic
    buf[8..16].copy_from_slice(&4096u64.to_ne_bytes()); // f_bsize
    buf[64..72].copy_from_slice(&255u64.to_ne_bytes()); // f_namelen
    buf[72..80].copy_from_slice(&4096u64.to_ne_bytes()); // f_frsize
    crate::syscall::copy_to_user(buf_ptr, &buf)
        .map(|_| SUCCESS)
        .unwrap_or_else(|e| e)
}

/// readlinkat システムコール（最小実装）
///
/// `/proc/self/exe` と `/proc/self/cwd` のみをサポートする。
pub fn readlinkat(dirfd: i64, path_ptr: u64, buf_ptr: u64, buf_len: u64) -> u64 {
    const AT_FDCWD: i64 = -100;
    if path_ptr == 0 || buf_ptr == 0 || buf_len == 0 {
        return EINVAL;
    }
    if !crate::syscall::validate_user_ptr(buf_ptr, buf_len) {
        return EFAULT;
    }
    let raw = match read_cstring(path_ptr) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let path = if raw.starts_with('/') || dirfd == AT_FDCWD {
        normalize_path(&raw)
    } else {
        // 最小実装: dirfd 相対は未対応
        return EBADF;
    };

    let pid = match current_process_id_raw() {
        Some(p) => crate::task::ids::ProcessId::from_u64(p),
        None => return EBADF,
    };
    let target = if path == "/proc/self/exe" {
        match crate::task::with_process(pid, |p| {
            let exe = p.exe_path();
            if exe.is_empty() {
                String::from(p.name())
            } else {
                String::from(exe)
            }
        }) {
            Some(name) if name.starts_with('/') => name,
            Some(name) => alloc::format!("/{}", name),
            None => return ESRCH,
        }
    } else if path == "/proc/self/cwd" {
        match crate::task::with_process(pid, |p| String::from(p.cwd())) {
            Some(cwd) => cwd,
            None => return ESRCH,
        }
    } else {
        return ENOENT;
    };

    let bytes = target.as_bytes();
    let copy_len = core::cmp::min(bytes.len(), buf_len as usize);
    if let Err(errno) = crate::syscall::copy_to_user(buf_ptr, &bytes[..copy_len]) {
        return errno;
    }
    copy_len as u64
}

/// Getdents64 システムコール
///
/// struct linux_dirent64 形式でエントリをバッファに書き込む。
/// - d_ino (8), d_off (8), d_reclen (2), d_type (1), d_name (可変長, null終端)
/// - レコードは 8 バイトアラインメント
/// FD の `pos` をエントリインデックスとして使用する。
pub fn getdents64(fd: u64, buf_ptr: u64, buf_len: u64) -> u64 {
    if buf_ptr == 0 || buf_len == 0 {
        return EINVAL;
    }
    if !crate::syscall::validate_user_ptr(buf_ptr, buf_len) {
        return EFAULT;
    }
    if fd < FD_BASE as u64 {
        return EBADF;
    }
    let idx = fd as usize;
    if idx >= PROCESS_MAX_FDS {
        return EBADF;
    }
    let pid = match current_process_id_raw() {
        Some(p) => p,
        None => return EBADF,
    };

    let (dir_path, start_pos, is_special) = match with_fd_table(pid, |t| {
        t.get(idx)
            .map(|fh| (fh.dir_path.clone(), fh.pos, handle_is_special(fh)))
    }) {
        Some(Some((Some(p), pos, is_special))) => (p, pos, is_special),
        _ => return EBADF,
    };
    if is_special {
        return ENOTDIR;
    }

    if let Err(errno) = ensure_fs_path_readable(&dir_path) {
        return errno;
    }

    let entries: Vec<(alloc::string::String, u8)> = match readdir_rootfs_first(&dir_path) {
        Some(e) => e
            .into_iter()
            // special runtime path で型が分かるものだけ最小限の d_type を返す。
            // それ以外は追加 stat を避けるため DT_UNKNOWN(0) のままにする。
            .map(|name| {
                let dtype = special_dir_entry_dtype(&dir_path, &name).unwrap_or(0u8);
                (name, dtype)
            })
            .collect(),
        None => return EINVAL,
    };

    let mut written: usize = 0;
    let mut new_pos = start_pos;

    // "." と ".." を先頭に追加
    let dot_entries: [(&str, u8); 2] = [(".", 4u8), ("..", 4u8)];
    let all_entries: Vec<(alloc::string::String, u8)> = {
        let mut v: Vec<(alloc::string::String, u8)> = dot_entries
            .iter()
            .map(|(n, t)| (alloc::string::String::from(*n), *t))
            .collect();
        for (name, dtype) in &entries {
            v.push((name.clone(), *dtype));
        }
        v
    };

    let mut out = alloc::vec![0u8; buf_len as usize];
    for (i, (name, dtype)) in all_entries.iter().enumerate().skip(start_pos) {
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len() + 1;
        let raw_size = 8 + 8 + 2 + 1 + name_len;
        let reclen = (raw_size + 7) & !7usize;
        if written + reclen > buf_len as usize {
            break;
        }
        let buf = &mut out[written..written + reclen];
        buf.fill(0);
        buf[0..8].copy_from_slice(&((i as u64 + 1).to_ne_bytes()));
        let next_off = (i + 1) as u64;
        buf[8..16].copy_from_slice(&next_off.to_ne_bytes());
        buf[16..18].copy_from_slice(&(reclen as u16).to_ne_bytes());
        buf[18] = *dtype;
        buf[19..19 + name_bytes.len()].copy_from_slice(name_bytes);
        buf[19 + name_bytes.len()] = 0;
        written += reclen;
        new_pos = i + 1;
    }
    if written > 0 && crate::syscall::copy_to_user(buf_ptr, &out[..written]).is_err() {
        return EFAULT;
    }

    // FD の pos を更新する
    with_fd_table_mut(pid, |t| {
        if let Some(fh) = t.get_mut(idx) {
            fh.pos = new_pos;
        }
    });

    written as u64
}
