//! カーネルの block_read/block_write syscall を使うブロックデバイス実装
//!
//! disk.service との IPC 往復を避け、ext2 マウント/読み書きを高速化する。

use crate::ext2::BlockDevice;

pub struct KernelBlockDevice {
    disk_id: u64,
    sector_size: usize,
}

impl KernelBlockDevice {
    pub fn new(disk_id: u64) -> Self {
        Self {
            disk_id,
            sector_size: mochi_syscall::block::SECTOR_SIZE,
        }
    }
}

impl BlockDevice for KernelBlockDevice {
    fn block_size(&self) -> usize {
        self.sector_size
    }

    fn read_block(&self, block_num: u64, buf: &mut [u8]) -> Result<(), ()> {
        mochi_syscall::block::block_read(self.disk_id, block_num, buf, 1)
            .map_err(|_| ())
    }

    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> Result<(), ()> {
        mochi_syscall::block::block_write(self.disk_id, block_num, buf, 1)
            .map_err(|_| ())
    }
}

