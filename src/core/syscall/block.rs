//! ブロックデバイスI/O (高速パス)
//!
//! disk.service などの IPC 経由のセクタ読み書きは遅すぎるため、
//! カーネル内 (cext) のドライバへ直結する syscalls を用意する。
//!
//! # セキュリティ
//! raw ブロックI/O は強力なので、呼び出し元が `device.storage` capability を
//! 持つことを必須にする。

use crate::syscall::{copy_from_user, copy_to_user, EACCES, EINVAL, EIO, EPERM, SUCCESS};

const SECTOR_SIZE: usize = 512;
#[inline]
fn max_sectors_per_call() -> u64 {
    crate::config::kernel().block.max_sectors_per_call
}

fn caller_has_storage_capability() -> bool {
    crate::syscall::security::caller_has_any_capability(&[
        crate::capability::Capability::DeviceStorage,
    ])
}

/// ブロック読み取り: (disk_id, lba, buf_ptr, sector_count)
pub fn block_read(disk_id: u64, lba: u64, buf_ptr: u64, sector_count: u64) -> u64 {
    // privilege: 最低でも Service/Core を要求 (ユーザへ raw disk は出さない)
    if !crate::syscall::security::caller_is_core_or_service() {
        return EPERM;
    }

    if !caller_has_storage_capability() {
        return EACCES;
    }

    if sector_count == 0 || sector_count > max_sectors_per_call() {
        return EINVAL;
    }

    let total = match (sector_count as usize).checked_mul(SECTOR_SIZE) {
        Some(n) => n as u64,
        None => return EINVAL,
    };
    if !crate::syscall::validate_user_ptr(buf_ptr, total) {
        return EINVAL;
    }

    // 1セクタずつ読み取って user へコピー（今後: まとめ読みの ABI へ拡張可能）
    let mut sector = [0u8; SECTOR_SIZE];
    for i in 0..sector_count {
        let ret = crate::kmod::disk::read_sector(disk_id as u32, lba + i, &mut sector) as i64;
        if ret < 0 {
            return EIO;
        }
        let off = (i as u64) * (SECTOR_SIZE as u64);
        if copy_to_user(buf_ptr + off, &sector).is_err() {
            return EINVAL;
        }
    }

    SUCCESS
}

/// ブロック書き込み: (disk_id, lba, buf_ptr, sector_count)
pub fn block_write(disk_id: u64, lba: u64, buf_ptr: u64, sector_count: u64) -> u64 {
    if !crate::syscall::security::caller_is_core_or_service() {
        return EPERM;
    }

    if !caller_has_storage_capability() {
        return EACCES;
    }

    if sector_count == 0 || sector_count > max_sectors_per_call() {
        return EINVAL;
    }

    let total = match (sector_count as usize).checked_mul(SECTOR_SIZE) {
        Some(n) => n as u64,
        None => return EINVAL,
    };
    if !crate::syscall::validate_user_ptr(buf_ptr, total) {
        return EINVAL;
    }

    let mut sector = [0u8; SECTOR_SIZE];
    for i in 0..sector_count {
        let off = (i as u64) * (SECTOR_SIZE as u64);
        if copy_from_user(buf_ptr + off, &mut sector).is_err() {
            return EINVAL;
        }
        let ret = crate::kmod::disk::write_sector(disk_id as u32, lba + i, &sector) as i64;
        if ret < 0 {
            return EIO;
        }
    }

    SUCCESS
}
