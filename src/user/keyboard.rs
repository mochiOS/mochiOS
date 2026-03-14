//! キーボード系システムコール（ユーザー側）

use super::sys::{syscall0, SyscallNumber, ENODATA};

/// PS/2 rawスキャンコードを1バイト読み取り（なければ None）
/// 変換はユーザー空間で行う
pub fn read_scancode() -> Option<u8> {
    let ret = syscall0(SyscallNumber::KeyboardRead as u64);
    if ret == ENODATA {
        None
    } else {
        Some(ret as u8)
    }
}

/// ドライバ監視用キューから raw スキャンコードを1バイト読み取る
///
/// 通常入力キューを消費しないため、shell.service と並行して利用できる。
pub fn read_scancode_tap() -> Result<Option<u8>, u64> {
    let ret = syscall0(SyscallNumber::KeyboardReadTap as u64);
    if ret == ENODATA {
        Ok(None)
    } else if (ret as i64) < 0 {
        Err(ret)
    } else {
        Ok(Some(ret as u8))
    }
}
