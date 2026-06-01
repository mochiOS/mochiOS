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
const MAX_SECTORS_PER_CALL: u64 = 128; // 64KiB

fn caller_has_storage_capability() -> bool {
    crate::task::current_thread_id()
        .and_then(|tid| crate::task::with_thread(tid, |t| t.process_id()))
        .map(|pid| {
            crate::task::process::process_has_capability(
                pid,
                crate::capability::Capability::DeviceStorage,
            )
        })
        .unwrap_or(false)
}

/// ブロック読み取り: (disk_id, lba, buf_ptr, sector_count)
pub fn block_read(disk_id: u64, lba: u64, buf_ptr: u64, sector_count: u64) -> u64 {
    // privilege: 最低でも Service/Core を要求 (ユーザへ raw disk は出さない)
    let privilege_ok = crate::task::current_thread_id()
        .and_then(|tid| crate::task::with_thread(tid, |t| t.process_id()))
        .and_then(|pid| crate::task::with_process(pid, |p| p.privilege()))
        .map(|pl| {
            matches!(
                pl,
                crate::task::PrivilegeLevel::Core | crate::task::PrivilegeLevel::Service
            )
        })
        .unwrap_or(false);
    if !privilege_ok {
        return EPERM;
    }

    if !caller_has_storage_capability() {
        return EACCES;
    }

    if sector_count == 0 || sector_count > MAX_SECTORS_PER_CALL {
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
    let privilege_ok = crate::task::current_thread_id()
        .and_then(|tid| crate::task::with_thread(tid, |t| t.process_id()))
        .and_then(|pid| crate::task::with_process(pid, |p| p.privilege()))
        .map(|pl| {
            matches!(
                pl,
                crate::task::PrivilegeLevel::Core | crate::task::PrivilegeLevel::Service
            )
        })
        .unwrap_or(false);
    if !privilege_ok {
        return EPERM;
    }

    if !caller_has_storage_capability() {
        return EACCES;
    }

    if sector_count == 0 || sector_count > MAX_SECTORS_PER_CALL {
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
