//! プロセスグループ・セッション関連のシステムコール

use super::types::{EINVAL, EPERM, ESRCH, SUCCESS};

#[inline]
fn current_pid() -> Option<crate::task::ids::ProcessId> {
    crate::task::current_thread_id()
        .and_then(|tid| crate::task::with_thread(tid, |t| t.process_id()))
}

/// Getppid システムコール
pub fn getppid() -> u64 {
    let pid = match current_pid() {
        Some(p) => p,
        None => return 0,
    };
    crate::task::with_process(pid, |p| {
        p.parent_id().map(|ppid| ppid.as_u64()).unwrap_or(1)
    })
    .unwrap_or(1)
}

/// Getpgid システムコール
///
/// pid=0 の場合は呼び出しプロセス自身のグループ ID を返す。
pub fn getpgid(pid_arg: u64) -> u64 {
    let target_pid = if pid_arg == 0 {
        match current_pid() {
            Some(p) => p,
            None => return ESRCH,
        }
    } else {
        crate::task::ids::ProcessId::from_u64(pid_arg)
    };

    match crate::task::with_process(target_pid, |p| p.pgid()) {
        Some(pgid) => pgid,
        None => ESRCH,
    }
}

/// Setpgid システムコール
///
/// pid=0 は自プロセス、pgid=0 はプロセス自身の PID を使用する。
pub fn setpgid(pid_arg: u64, pgid_arg: u64) -> u64 {
    let caller = match current_pid() {
        Some(p) => p,
        None => return ESRCH,
    };
    let target_pid = if pid_arg == 0 {
        caller
    } else {
        crate::task::ids::ProcessId::from_u64(pid_arg)
    };

    // 呼び出し元は自分自身または直接の子プロセスのみ変更可能
    let is_child = if target_pid != caller {
        crate::task::with_process(target_pid, |p| p.parent_id() == Some(caller))
            .unwrap_or(false)
    } else {
        true
    };
    if !is_child {
        return EPERM;
    }

    let new_pgid = if pgid_arg == 0 { target_pid.as_u64() } else { pgid_arg };

    match crate::task::with_process_mut(target_pid, |p| {
        p.set_pgid(new_pgid);
    }) {
        Some(()) => SUCCESS,
        None => ESRCH,
    }
}

/// Setsid システムコール
///
/// 新しいセッションを作成し、呼び出しプロセスがそのリーダーになる。
/// sid = pgid = pid に設定する。
pub fn setsid() -> u64 {
    let pid = match current_pid() {
        Some(p) => p,
        None => return ESRCH,
    };
    let pid_val = pid.as_u64();
    match crate::task::with_process_mut(pid, |p| {
        p.set_pgid(pid_val);
        p.set_sid(pid_val);
        pid_val
    }) {
        Some(new_sid) => new_sid,
        None => ESRCH,
    }
}

/// Getsid システムコール
pub fn getsid(pid_arg: u64) -> u64 {
    let target_pid = if pid_arg == 0 {
        match current_pid() {
            Some(p) => p,
            None => return ESRCH,
        }
    } else {
        crate::task::ids::ProcessId::from_u64(pid_arg)
    };

    match crate::task::with_process(target_pid, |p| p.sid()) {
        Some(sid) => sid,
        None => ESRCH,
    }
}

/// ioctl システムコール（TIOCGPGRP / TIOCSPGRP のみ対応）
///
/// - TIOCGPGRP (0x540f): フォアグラウンドプロセスグループを取得
/// - TIOCSPGRP (0x5410): フォアグラウンドプロセスグループを設定
/// - その他のコマンドは EINVAL を返す
pub fn ioctl(fd: u64, request: u64, arg: u64) -> u64 {
    const TIOCGPGRP: u64 = 0x540f;
    const TIOCSPGRP: u64 = 0x5410;
    const TIOCGWINSZ: u64 = 0x5413;

    match request {
        TIOCGPGRP => {
            // フォアグラウンドプロセスグループを返す（自プロセスの pgid）
            if arg == 0 || !crate::syscall::validate_user_ptr(arg, 4) {
                return EINVAL;
            }
            let pgid = match current_pid() {
                Some(pid) => crate::task::with_process(pid, |p| p.pgid()).unwrap_or(1),
                None => return EINVAL,
            };
            crate::syscall::with_user_memory_access(|| unsafe {
                core::ptr::write_unaligned(arg as *mut u32, pgid as u32);
            });
            SUCCESS
        }
        TIOCSPGRP => {
            // フォアグラウンドプロセスグループの設定（スタブ: 常に成功）
            SUCCESS
        }
        TIOCGWINSZ => {
            // ウィンドウサイズ: struct winsize { ws_row, ws_col, ws_xpixel, ws_ypixel } (各 u16)
            if arg == 0 || !crate::syscall::validate_user_ptr(arg, 8) {
                return EINVAL;
            }
            crate::syscall::with_user_memory_access(|| unsafe {
                let buf = core::slice::from_raw_parts_mut(arg as *mut u8, 8);
                buf.fill(0);
                // ws_row = 24, ws_col = 80
                buf[0..2].copy_from_slice(&24u16.to_ne_bytes());
                buf[2..4].copy_from_slice(&80u16.to_ne_bytes());
            });
            SUCCESS
        }
        _ => EINVAL,
    }
}

/// access システムコール（ファイルアクセス可能性チェック）
///
/// initfs/rootfs にファイルが存在すれば常に成功を返す。
pub fn access(path_ptr: u64, _mode: u64) -> u64 {
    use super::types::ENOENT;
    if path_ptr == 0 {
        return EINVAL;
    }
    let path = match crate::syscall::read_user_cstring(path_ptr, 1024) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if crate::init::fs::file_metadata(&path).is_some() {
        SUCCESS
    } else {
        ENOENT
    }
}

/// getuid / geteuid / getgid / getegid システムコール（常に 0 = root を返す）
pub fn getuid() -> u64 { 0 }
pub fn getgid() -> u64 { 0 }
pub fn geteuid() -> u64 { 0 }
pub fn getegid() -> u64 { 0 }
