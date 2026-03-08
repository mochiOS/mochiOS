//! I/O関連のシステムコール

use super::types::{EBADF, EFAULT, SUCCESS};
use crate::util::console;
use crate::{debug, error, info, warn};

/// 標準出力のファイルディスクリプタ
const STDOUT_FD: u64 = 1;
/// 標準エラー出力のファイルディスクリプタ
const STDERR_FD: u64 = 2;

/// 現在のプロセスの親プロセスのメインスレッドIDを返す
fn get_parent_thread_id() -> Option<u64> {
    let tid = crate::task::current_thread_id()?;
    let pid = crate::task::with_thread(tid, |t| t.process_id())?;
    let parent_pid = crate::task::with_process(pid, |p| p.parent_id())??;
    let mut parent_tid: Option<u64> = None;
    crate::task::for_each_thread(|t| {
        if parent_tid.is_none() && t.process_id() == parent_pid {
            parent_tid = Some(t.id().as_u64());
        }
    });
    parent_tid
}

/// Writeシステムコール
///
/// # 引数
/// - `fd`: ファイルディスクリプタ (1=stdout, 2=stderr)
/// - `buf_ptr`: 書き込むデータのポインタ
/// - `len`: 書き込むデータの長さ
///
/// # 戻り値
/// 書き込んだバイト数、またはエラーコード
pub fn write(fd: u64, buf_ptr: u64, len: u64) -> u64 {
    debug!("write: fd={}, buf_ptr={:#x}, len={}", fd, buf_ptr, len);

    if fd != STDOUT_FD && fd != STDERR_FD {
        return EBADF;
    }
    if len == 0 {
        return SUCCESS;
    }
    if buf_ptr == 0 {
        return EFAULT;
    }

    let mut buf = alloc::vec![0u8; len as usize];
    if let Err(err) = crate::syscall::copy_from_user(buf_ptr, &mut buf) {
        return err;
    }

    // シリアルには常に出力する（デバッグ用）
    x86_64::instructions::interrupts::without_interrupts(|| {
        use core::fmt::Write;
        let mut serial = console::SERIAL.lock();
        for &byte in &buf {
            serial.send_byte(byte);
        }
    });

    // 親プロセス（シェル）が存在すればIPCで転送して描画させる
    if let Some(parent_tid) = get_parent_thread_id() {
        const CHUNK: usize = 512;
        let mut offset = 0;
        while offset < buf.len() {
            let end = core::cmp::min(offset + CHUNK, buf.len());
            crate::syscall::ipc::send_from_kernel(parent_tid, &buf[offset..end]);
            offset = end;
        }
    }

    len
}

/// Readシステムコール
/// - fd == 0 の場合はキーボードから1バイト読み取る（なければ ENODATA を返す）
/// - fd >= 3 の場合は initfs から開かれたファイルを読み取る（fs::read に委譲）
pub fn read(fd: u64, buf_ptr: u64, len: u64) -> u64 {
    use super::types::{EFAULT, ENODATA};

    if buf_ptr == 0 {
        return EFAULT;
    }
    if len == 0 {
        return 0;
    }

    if fd == 0 {
        let ch = crate::syscall::keyboard::read_char();
        if ch == ENODATA {
            return ENODATA;
        }
        if !super::validate_user_ptr(buf_ptr, 1) {
            return EFAULT;
        }
        crate::syscall::with_user_memory_access(|| unsafe {
            let dst = core::slice::from_raw_parts_mut(buf_ptr as *mut u8, 1);
            dst[0] = ch as u8;
        });
        return 1;
    }

    crate::syscall::fs::read(fd, buf_ptr, len)
}

/// Logシステムコール
///
/// カーネルログにメッセージを書き込む
/// # 引数
/// msg: メッセージ
/// len: メッセージの長さ
/// level: ログレベル（0=ERROR、1=WARNING、2=INFO、3=DEBUG）
///
/// # 戻り値
/// 成功時はSUCCESS、エラー時はエラーコード
pub fn log(msg: u64, len: u64, level: u64) -> u64 {
    if msg == 0 || len == 0 {
        return super::types::EINVAL;
    }

    let mut copied = alloc::vec![0u8; len as usize];
    if let Err(err) = crate::syscall::copy_from_user(msg, &mut copied) {
        return err;
    }

    let msg = match core::str::from_utf8(&copied) {
        Ok(s) => s,
        Err(_) => return super::types::EINVAL,
    };

    match level {
        0 => error!("{}", msg),
        1 => warn!("{}", msg),
        2 => info!("{}", msg),
        3 => debug!("{}", msg),
        _ => return super::types::EINVAL,
    }
    SUCCESS
}
