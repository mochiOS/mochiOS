//! ディスクサービスを使用したブロックデバイス実装

use core::mem::size_of;

use mochi_syscall::fs_consts::IPC_MAX_MSG_SIZE;
use mochi_syscall::ipc;
use mochi_syscall::task;
use std::vec::Vec;

use crate::common::vfs::{VfsError, VfsResult};
use crate::ext2::BlockDevice;

#[derive(Clone, Copy)]
struct StashedMsg {
    sender: u64,
    len: usize,
    data: [u8; IPC_MAX_MSG_SIZE],
}

// fs.service は単一スレッドで動かしているため、簡易キューを unsafe で持つ。
// disk.service とのやり取り中に他クライアントから fs IPC が届くと、
// `ipc_recv()` がそれを先に受信してしまうため退避が必要。
static mut STASH: Option<Vec<StashedMsg>> = None;
const STASH_CAP: usize = 32;

fn stash_push(sender: u64, bytes: &[u8]) {
    unsafe {
        if STASH.is_none() {
            STASH = Some(Vec::new());
        }
        let Some(ref mut q) = STASH else { return };
        if q.len() >= STASH_CAP {
            // 古いものから捨てる（ブート時の一時的な混線対策）
            q.remove(0);
        }
        let mut msg = StashedMsg {
            sender,
            len: core::cmp::min(bytes.len(), IPC_MAX_MSG_SIZE),
            data: [0u8; IPC_MAX_MSG_SIZE],
        };
        msg.data[..msg.len].copy_from_slice(&bytes[..msg.len]);
        q.push(msg);
    }
}

/// disk_device が退避したメッセージを 1 件取り出す（無ければ None）
pub fn pop_stashed(into: &mut [u8]) -> Option<(u64, u64)> {
    unsafe {
        let Some(ref mut q) = STASH else { return None };
        if q.is_empty() {
            return None;
        }
        let msg = q.remove(0);
        let n = core::cmp::min(into.len(), msg.len);
        into[..n].copy_from_slice(&msg.data[..n]);
        Some((msg.sender, n as u64))
    }
}

/// ディスク操作リクエスト（書き込みデータを含む）
#[repr(C)]
#[derive(Clone, Copy)]
struct DiskRequest {
    op: u64,
    disk_id: u64,
    lba: u64,
    count: u64,
    data: [u8; 512], // OP_WRITE のときに使用
}

#[allow(unused)]
impl DiskRequest {
    const OP_READ: u64 = 1;
    const OP_WRITE: u64 = 2;
    const OP_INFO: u64 = 3;
}

/// ディスク操作レスポンス
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct DiskResponse {
    status: i64,
    len: u64,
    data: [u8; 512],
}

/// ディスクサービスを使用したブロックデバイス
pub struct DiskServiceDevice {
    disk_service_pid: u64,
    disk_id: u64,
    sector_size: usize,
}

impl DiskServiceDevice {
    /// 新しいディスクデバイスを作成
    pub fn new(disk_service_pid: u64, disk_id: u64) -> Self {
        Self {
            disk_service_pid,
            disk_id,
            sector_size: 512,
        }
    }

    /// セクタを読み取る（内部用）
    fn read_sector(&self, lba: u64, buf: &mut [u8]) -> VfsResult<()> {
        if buf.len() < 512 {
            return Err(VfsError::InvalidArgument);
        }

        let req = DiskRequest {
            op: DiskRequest::OP_READ,
            disk_id: self.disk_id,
            lba,
            count: 1,
            data: [0u8; 512],
        };

        let req_slice = unsafe {
            core::slice::from_raw_parts(&req as *const _ as *const u8, size_of::<DiskRequest>())
        };

        // リクエストを送信
        let result = ipc::ipc_send(self.disk_service_pid, req_slice);
        if result != 0 {
            return Err(VfsError::IoError);
        }

        // レスポンスを受信（他クライアントの fs リクエストが混ざるため退避する）
        let mut inbox = vec![0u8; IPC_MAX_MSG_SIZE];
        let (sender, len) = loop {
            let (s, l) = ipc::ipc_recv(&mut inbox);
            if s == 0 || l == 0 {
                task::yield_now();
                continue;
            }
            if s != self.disk_service_pid {
                let n = core::cmp::min(l as usize, inbox.len());
                stash_push(s, &inbox[..n]);
                continue;
            }
            break (s, l);
        };

        if sender != self.disk_service_pid || (len as usize) < size_of::<DiskResponse>() {
            return Err(VfsError::IoError);
        }

        let resp: DiskResponse = unsafe {
            core::ptr::read(inbox.as_ptr() as *const DiskResponse)
        };

        if resp.status != 0 {
            return Err(VfsError::IoError);
        }

        // データをコピー
        buf[..512].copy_from_slice(&resp.data);
        Ok(())
    }

    /// セクタに書き込む（内部用）
    fn write_sector(&self, lba: u64, buf: &[u8]) -> VfsResult<()> {
        if buf.len() < 512 {
            return Err(VfsError::InvalidArgument);
        }

        let mut req = DiskRequest {
            op: DiskRequest::OP_WRITE,
            disk_id: self.disk_id,
            lba,
            count: 1,
            data: [0u8; 512],
        };
        req.data.copy_from_slice(&buf[..512]);

        let req_slice = unsafe {
            core::slice::from_raw_parts(&req as *const _ as *const u8, size_of::<DiskRequest>())
        };

        let result = ipc::ipc_send(self.disk_service_pid, req_slice);
        if result != 0 {
            return Err(VfsError::IoError);
        }

        // レスポンスを受信（他クライアントの fs リクエストが混ざるため退避する）
        let mut inbox = vec![0u8; IPC_MAX_MSG_SIZE];
        let (sender, len) = loop {
            let (s, l) = ipc::ipc_recv(&mut inbox);
            if s == 0 || l == 0 {
                task::yield_now();
                continue;
            }
            if s != self.disk_service_pid {
                let n = core::cmp::min(l as usize, inbox.len());
                stash_push(s, &inbox[..n]);
                continue;
            }
            break (s, l);
        };

        if sender != self.disk_service_pid || (len as usize) < size_of::<DiskResponse>() {
            return Err(VfsError::IoError);
        }

        let resp: DiskResponse =
            unsafe { core::ptr::read(inbox.as_ptr() as *const DiskResponse) };

        if resp.status != 0 {
            Err(VfsError::IoError)
        } else {
            Ok(())
        }
    }
}

impl BlockDevice for DiskServiceDevice {
    fn block_size(&self) -> usize {
        self.sector_size
    }

    fn read_block(&self, block_num: u64, buf: &mut [u8]) -> Result<(), ()> {
        self.read_sector(block_num, buf).map_err(|_| ())
    }

    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> Result<(), ()> {
        self.write_sector(block_num, buf).map_err(|_| ())
    }
}
