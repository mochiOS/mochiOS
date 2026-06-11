//! コンソール系システムコール（ユーザー側）

use super::sys::{syscall2, SyscallNumber};

/// コンソールへ書き込み
pub fn write(buf: &[u8]) -> u64 {
    syscall2(SyscallNumber::ConsoleWrite as u64, buf.as_ptr() as u64, buf.len() as u64)
}
