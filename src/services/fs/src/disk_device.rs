//! ディスクサービスを使用したブロックデバイス実装

use core::mem::size_of;

use swiftlib::ipc;

use crate::common::vfs::{VfsError, VfsResult};
use crate::enqueue_pending_message;
use crate::IPC_MAX_MSG_SIZE;
use crate::ext2::BlockDevice;
use crate::take_pending_message_for_sender;

const MAX_SECTORS_PER_REQ: usize = 64;
const BULK_SECTORS_PER_MSG: usize = 4;

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

/// ディスク操作レスポンス（1セクタ）
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

    #[inline]
    fn copy_bulk_chunk(
        &self,
        chunk_index: usize,
        expected_chunk_bytes: usize,
        src: &[u8],
        dst: &mut [u8],
    ) -> VfsResult<()> {
        if src.len() < expected_chunk_bytes {
            return Err(VfsError::IoError);
        }
        let start = chunk_index
            .checked_mul(BULK_SECTORS_PER_MSG)
            .and_then(|v| v.checked_mul(self.sector_size))
            .ok_or(VfsError::InvalidArgument)?;
        let end = start
            .checked_add(expected_chunk_bytes)
            .ok_or(VfsError::InvalidArgument)?;
        if end > dst.len() {
            return Err(VfsError::InvalidArgument);
        }
        dst[start..end].copy_from_slice(&src[..expected_chunk_bytes]);
        Ok(())
    }

    /// セクタを読み取る（内部用）
    fn read_sector(&self, lba: u64, buf: &mut [u8]) -> VfsResult<()> {
        if buf.len() < self.sector_size {
            return Err(VfsError::InvalidArgument);
        }

        self.read_sectors(lba, 1, &mut buf[..self.sector_size])
    }

    /// 連続セクタを読み取る（内部用）
    fn read_sectors(&self, lba: u64, count: usize, buf: &mut [u8]) -> VfsResult<()> {
        if count == 0 || count > MAX_SECTORS_PER_REQ {
            return Err(VfsError::InvalidArgument);
        }
        let total = count
            .checked_mul(self.sector_size)
            .ok_or(VfsError::InvalidArgument)?;
        if buf.len() < total {
            return Err(VfsError::InvalidArgument);
        }

        let req = DiskRequest {
            op: DiskRequest::OP_READ,
            disk_id: self.disk_id,
            lba,
            count: count as u64,
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

        let mut resp_buf = [0u8; IPC_MAX_MSG_SIZE];
        let chunk_count = count.div_ceil(BULK_SECTORS_PER_MSG);
        for chunk_idx in 0..chunk_count {
            let remaining = count - chunk_idx * BULK_SECTORS_PER_MSG;
            let chunk_sectors = core::cmp::min(BULK_SECTORS_PER_MSG, remaining);
            let expected_chunk_bytes = chunk_sectors
                .checked_mul(self.sector_size)
                .ok_or(VfsError::InvalidArgument)?;
            let expected_bulk_len = size_of::<i64>() + size_of::<u64>() + expected_chunk_bytes;

            if let Some(n) = take_pending_message_for_sender(self.disk_service_pid, &mut resp_buf) {
                if n == size_of::<DiskResponse>() && chunk_sectors == 1 {
                    let resp: DiskResponse = unsafe {
                        core::ptr::read_unaligned(resp_buf.as_ptr() as *const DiskResponse)
                    };
                    if resp.status != 0 || resp.len != self.sector_size as u64 {
                        return Err(VfsError::IoError);
                    }
                    self.copy_bulk_chunk(chunk_idx, self.sector_size, &resp.data, buf)?;
                    continue;
                }
                if n < expected_bulk_len {
                    return Err(VfsError::IoError);
                }
                let status = i64::from_le_bytes(resp_buf[0..8].try_into().map_err(|_| VfsError::IoError)?);
                let data_len =
                    u64::from_le_bytes(resp_buf[8..16].try_into().map_err(|_| VfsError::IoError)?)
                        as usize;
                if status != 0 || data_len != expected_chunk_bytes {
                    return Err(VfsError::IoError);
                }
                self.copy_bulk_chunk(chunk_idx, expected_chunk_bytes, &resp_buf[16..16 + data_len], buf)?;
                continue;
            }

            let (_, len) = loop {
                let (s, l) = ipc::ipc_recv_wait(&mut resp_buf);
                if s == 0 && l == 0 {
                    continue;
                }
                if s != self.disk_service_pid {
                    let msg_len = core::cmp::min(l as usize, resp_buf.len());
                    if !enqueue_pending_message(s, &resp_buf[..msg_len]) {
                        println!(
                            "[FS] WARN: pending IPC queue full while stashing req (sender={}, len={})",
                            s, l
                        );
                    }
                    continue;
                }
                break (s, l);
            };

            let len = len as usize;
            if len == size_of::<DiskResponse>() && chunk_sectors == 1 {
                let resp: DiskResponse = unsafe {
                    core::ptr::read_unaligned(resp_buf.as_ptr() as *const DiskResponse)
                };
                if resp.status != 0 || resp.len != self.sector_size as u64 {
                    return Err(VfsError::IoError);
                }
                self.copy_bulk_chunk(chunk_idx, self.sector_size, &resp.data, buf)?;
                continue;
            }
            if len < expected_bulk_len {
                return Err(VfsError::IoError);
            }

            let status = i64::from_le_bytes(resp_buf[0..8].try_into().map_err(|_| VfsError::IoError)?);
            let data_len =
                u64::from_le_bytes(resp_buf[8..16].try_into().map_err(|_| VfsError::IoError)?)
                    as usize;
            if status != 0 || data_len != expected_chunk_bytes {
                return Err(VfsError::IoError);
            }
            self.copy_bulk_chunk(chunk_idx, expected_chunk_bytes, &resp_buf[16..16 + data_len], buf)?;
        }

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

        // レスポンスを受信（ブロッキング）
        let mut resp_buf = [0u8; size_of::<DiskResponse>()];
        if let Some(n) = take_pending_message_for_sender(self.disk_service_pid, &mut resp_buf) {
            if n < size_of::<DiskResponse>() {
                return Err(VfsError::IoError);
            }
        } else {
            let (_, len) = loop {
                let (s, l) = ipc::ipc_recv_wait(&mut resp_buf);
                if s == 0 && l == 0 {
                    continue;
                }
                if s != self.disk_service_pid {
                    let msg_len = core::cmp::min(l as usize, resp_buf.len());
                    if !enqueue_pending_message(s, &resp_buf[..msg_len]) {
                        println!(
                            "[FS] WARN: pending IPC queue full while stashing req (sender={}, len={})",
                            s, l
                        );
                    }
                    continue;
                }
                break (s, l);
            };

            if (len as usize) < size_of::<DiskResponse>() {
                return Err(VfsError::IoError);
            }
        }

        let resp: DiskResponse = unsafe {
            core::ptr::read_unaligned(resp_buf.as_ptr() as *const DiskResponse)
        };

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

    fn read_blocks(&self, start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), ()> {
        if count == 0 {
            return Ok(());
        }
        let total = count
            .checked_mul(self.sector_size)
            .ok_or(())?;
        if buf.len() < total {
            return Err(());
        }

        let mut done = 0usize;
        while done < count {
            let chunk = core::cmp::min(MAX_SECTORS_PER_REQ, count - done);
            let lba = start_block
                .checked_add(done as u64)
                .ok_or(())?;
            let begin = done * self.sector_size;
            let end = begin + chunk * self.sector_size;
            self.read_sectors(lba, chunk, &mut buf[begin..end]).map_err(|_| ())?;
            done += chunk;
        }
        Ok(())
    }

    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> Result<(), ()> {
        self.write_sector(block_num, buf).map_err(|_| ())
    }
}
