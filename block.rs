//! ブロックデバイスI/O (高速パス)
//!
//! disk.service の IPC 経由のセクタI/O は遅すぎるため、
//! cext ドライバに直結した syscall を使う。

use crate::sys::{syscall4, SyscallNumber};

pub const SECTOR_SIZE: usize = 512;

/// セクタを読み取る（512バイト * `sector_count`）
#[inline]
pub fn block_read(disk_id: u64, lba: u64, out: &mut [u8], sector_count: u64) -> Result<(), i64> {
    let need = (sector_count as usize)
        .checked_mul(SECTOR_SIZE)
        .ok_or(-22i64)?;
    if out.len() < need {
        return Err(-22);
    }
    let ret = syscall4(
        SyscallNumber::BlockRead as u64,
        disk_id,
        lba,
        out.as_mut_ptr() as u64,
        sector_count,
    ) as i64;
    if ret < 0 {
        Err(ret)
    } else {
        Ok(())
    }
}

/// セクタを書き込む（512バイト * `sector_count`）
#[inline]
pub fn block_write(disk_id: u64, lba: u64, input: &[u8], sector_count: u64) -> Result<(), i64> {
    let need = (sector_count as usize)
        .checked_mul(SECTOR_SIZE)
        .ok_or(-22i64)?;
    if input.len() < need {
        return Err(-22);
    }
    let ret = syscall4(
        SyscallNumber::BlockWrite as u64,
        disk_id,
        lba,
        input.as_ptr() as u64,
        sector_count,
    ) as i64;
    if ret < 0 {
        Err(ret)
    } else {
        Ok(())
    }
}

