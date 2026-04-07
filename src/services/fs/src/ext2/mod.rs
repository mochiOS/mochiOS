//! EXT2 ファイルシステム実装（高速化版）
//!
//! Linux標準のext2ファイルシステムをサポート
//! パフォーマンス最適化:
//! - ブロックキャッシュ: 512エントリ LRU
//! - inodeキャッシュ: 256エントリ LRU
//! - 間接ブロックキャッシュ（lookup/readdir/readで再利用）
//! - ディレクトリデータ一括読み取り

use core::mem::size_of;
use std::boxed::Box;
use std::string::String;
use std::sync::Mutex;
use std::vec::Vec;

use crate::common::vfs::{DirEntry, FileAttr, FileSystem, FileType, VfsError, VfsResult};

/// ブロックデバイストレイト
#[allow(unused)]
pub trait BlockDevice: Send + Sync {
    fn block_size(&self) -> usize;
    fn read_block(&self, block_num: u64, buf: &mut [u8]) -> Result<(), ()>;
    fn read_blocks(&self, start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), ()> {
        if count == 0 {
            return Ok(());
        }
        let block_size = self.block_size();
        let total = count.checked_mul(block_size).ok_or(())?;
        if buf.len() < total {
            return Err(());
        }
        for i in 0..count {
            let lba = start_block.checked_add(i as u64).ok_or(())?;
            let begin = i * block_size;
            let end = begin + block_size;
            self.read_block(lba, &mut buf[begin..end])?;
        }
        Ok(())
    }
    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> Result<(), ()>;
}

/// EXT2スーパーブロック
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Ext2Superblock {
    s_inodes_count: u32,
    s_blocks_count: u32,
    s_r_blocks_count: u32,
    s_free_blocks_count: u32,
    s_free_inodes_count: u32,
    s_first_data_block: u32,
    s_log_block_size: u32,
    s_log_frag_size: u32,
    s_blocks_per_group: u32,
    s_frags_per_group: u32,
    s_inodes_per_group: u32,
    s_mtime: u32,
    s_wtime: u32,
    s_mnt_count: u16,
    s_max_mnt_count: u16,
    s_magic: u16,
    s_state: u16,
    s_errors: u16,
    s_minor_rev_level: u16,
    s_lastcheck: u32,
    s_checkinterval: u32,
    s_creator_os: u32,
    s_rev_level: u32,
    s_def_resuid: u16,
    s_def_resgid: u16,
    s_first_ino: u32,
    s_inode_size: u16,
}

const EXT2_MAGIC: u16 = 0xEF53;
const EXT2_SUPERBLOCK_OFFSET: u64 = 1024;

/// EXT2 inode
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Ext2Inode {
    i_mode: u16,
    i_uid: u16,
    i_size: u32,
    i_atime: u32,
    i_ctime: u32,
    i_mtime: u32,
    i_dtime: u32,
    i_gid: u16,
    i_links_count: u16,
    i_blocks: u32,
    i_flags: u32,
    i_osd1: u32,
    i_block: [u32; 15],
    i_generation: u32,
    i_file_acl: u32,
    i_dir_acl: u32,
    i_faddr: u32,
    i_osd2: [u8; 12],
}

const EXT2_S_IFREG: u16 = 0x8000;
const EXT2_S_IFDIR: u16 = 0x4000;
const EXT2_S_IFLNK: u16 = 0xA000;

/// EXT2ディレクトリエントリ
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Ext2DirEntry {
    inode: u32,
    rec_len: u16,
    name_len: u8,
    file_type: u8,
}

/// ブロックグループディスクリプタ
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Ext2GroupDesc {
    bg_block_bitmap: u32,
    bg_inode_bitmap: u32,
    bg_inode_table: u32,
    bg_free_blocks_count: u16,
    bg_free_inodes_count: u16,
    bg_used_dirs_count: u16,
    bg_pad: u16,
    bg_reserved: [u32; 3],
}

// ─── キャッシュ ───────────────────────────────────────────────

const BLOCK_CACHE_MAX: usize = 512;
const INODE_CACHE_MAX: usize = 256;

struct LruCacheEntry<T> {
    key: u64,
    value: T,
    age: u64,
}

struct LruCache<T> {
    entries: Vec<Option<LruCacheEntry<T>>>,
    age: u64,
    capacity: usize,
}

impl<T: Clone> LruCache<T> {
    fn new(capacity: usize) -> Self {
        let mut entries = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            entries.push(None);
        }
        Self {
            entries,
            age: 0,
            capacity,
        }
    }

    fn get(&mut self, key: u64) -> Option<T> {
        self.age = self.age.wrapping_add(1);
        for entry in self.entries.iter_mut() {
            if let Some(e) = entry {
                if e.key == key {
                    e.age = self.age;
                    return Some(e.value.clone());
                }
            }
        }
        None
    }

    fn insert(&mut self, key: u64, value: T) {
        self.age = self.age.wrapping_add(1);

        // 既存エントリを更新
        for entry in self.entries.iter_mut() {
            if let Some(e) = entry {
                if e.key == key {
                    e.value = value;
                    e.age = self.age;
                    return;
                }
            }
        }

        // 空きスロットに挿入
        for entry in self.entries.iter_mut() {
            if entry.is_none() {
                *entry = Some(LruCacheEntry {
                    key,
                    value,
                    age: self.age,
                });
                return;
            }
        }

        // LRUエントリを回収
        let mut lru_idx = 0;
        let mut lru_age = u64::MAX;
        for (i, entry) in self.entries.iter().enumerate() {
            if let Some(e) = entry {
                if e.age < lru_age {
                    lru_age = e.age;
                    lru_idx = i;
                }
            }
        }
        self.entries[lru_idx] = Some(LruCacheEntry {
            key,
            value,
            age: self.age,
        });
    }
}

// ─── Ext2Fs ───────────────────────────────────────────────────

pub struct Ext2Fs {
    device: Box<dyn BlockDevice>,
    block_size: usize,
    inodes_per_group: u32,
    group_desc_table: Vec<Ext2GroupDesc>,
    inode_size: usize,
    // 共有参照時の安全性確保のため Mutex で内部可変性を保護
    block_cache: Mutex<LruCache<Vec<u8>>>,
    inode_cache: Mutex<LruCache<Ext2Inode>>,
}

impl Ext2Fs {
    pub fn new(device: Box<dyn BlockDevice>) -> VfsResult<Self> {
        let dev_block_size = device.block_size();
        if dev_block_size == 0 {
            return Err(VfsError::InvalidArgument);
        }
        let mut sb_buf = vec![0u8; dev_block_size];
        let sb_block_index = EXT2_SUPERBLOCK_OFFSET / dev_block_size as u64;
        let sb_in_block_offset = (EXT2_SUPERBLOCK_OFFSET % dev_block_size as u64) as usize;
        device
            .read_block(sb_block_index, &mut sb_buf)
            .map_err(|_| VfsError::IoError)?;

        if sb_in_block_offset + core::mem::size_of::<Ext2Superblock>() > sb_buf.len() {
            return Err(VfsError::IoError);
        }

        let superblock: Ext2Superblock = unsafe {
            core::ptr::read_unaligned(
                sb_buf[sb_in_block_offset..].as_ptr() as *const Ext2Superblock,
            )
        };

        if superblock.s_magic != EXT2_MAGIC {
            return Err(VfsError::InvalidArgument);
        }

        // Validate critical superblock fields to prevent OOM, div-by-zero, and overflow
        if superblock.s_log_block_size > 2 {
            return Err(VfsError::InvalidArgument); // block_size > 4096
        }
        if superblock.s_blocks_per_group == 0 {
            return Err(VfsError::InvalidArgument);
        }
        if superblock.s_inodes_per_group == 0 {
            return Err(VfsError::InvalidArgument);
        }
        if superblock.s_blocks_count == 0 {
            return Err(VfsError::InvalidArgument);
        }
        if superblock.s_rev_level >= 1 && superblock.s_inode_size == 0 {
            return Err(VfsError::InvalidArgument);
        }

        let block_size = 1024 << superblock.s_log_block_size;
        let inode_size = if superblock.s_rev_level >= 1 {
            superblock.s_inode_size as usize
        } else {
            128
        };

        // Prevent overflow in num_groups calculation
        let num_groups = ((superblock.s_blocks_count as u64 + superblock.s_blocks_per_group as u64
            - 1)
            / superblock.s_blocks_per_group as u64) as usize;

        if num_groups == 0 || num_groups > 65536 {
            return Err(VfsError::InvalidArgument);
        }

        let gdt_block = if block_size == 1024 { 2 } else { 1 };
        let gdt_size = num_groups * size_of::<Ext2GroupDesc>();
        let gdt_blocks = (gdt_size + block_size - 1) / block_size;

        let mut gdt_buf = vec![0u8; gdt_blocks * block_size];
        for i in 0..gdt_blocks {
            let mut block_buf = vec![0u8; block_size];
            let blocks_per_fs_block = block_size / device.block_size();
            let start_block = (gdt_block + i) as u64 * blocks_per_fs_block as u64;
            for j in 0..blocks_per_fs_block {
                let offset = j * device.block_size();
                device
                    .read_block(
                        start_block + j as u64,
                        &mut block_buf[offset..offset + device.block_size()],
                    )
                    .map_err(|_| VfsError::IoError)?;
            }
            gdt_buf[i * block_size..(i + 1) * block_size].copy_from_slice(&block_buf);
        }

        let mut group_desc_table = Vec::new();
        for i in 0..num_groups {
            let offset = i * size_of::<Ext2GroupDesc>();
            let desc: Ext2GroupDesc = unsafe {
                core::ptr::read_unaligned(
                    (gdt_buf.as_ptr() as usize + offset) as *const Ext2GroupDesc,
                )
            };
            group_desc_table.push(desc);
        }

        Ok(Self {
            device,
            block_size,
            inodes_per_group: superblock.s_inodes_per_group,
            group_desc_table,
            inode_size,
            block_cache: Mutex::new(LruCache::new(BLOCK_CACHE_MAX)),
            inode_cache: Mutex::new(LruCache::new(INODE_CACHE_MAX)),
        })
    }

    /// デバイスから直接ブロックを読み取る（キャッシュなし）
    fn read_block_raw(&self, block_num: u32, buf: &mut [u8]) -> VfsResult<()> {
        if buf.len() < self.block_size {
            return Err(VfsError::InvalidArgument);
        }
        let blocks_per_fs_block = self.block_size / self.device.block_size();
        let start_block = block_num as u64 * blocks_per_fs_block as u64;
        let total = blocks_per_fs_block
            .checked_mul(self.device.block_size())
            .ok_or(VfsError::InvalidArgument)?;
        self.device
            .read_blocks(start_block, blocks_per_fs_block, &mut buf[..total])
            .map_err(|_| VfsError::IoError)?;
        Ok(())
    }

    /// ブロックをキャッシュ経由で読み取る
    fn read_block(&self, block_num: u32, buf: &mut [u8]) -> VfsResult<()> {
        if buf.len() < self.block_size {
            return Err(VfsError::InvalidArgument);
        }

        // キャッシュヒット
        {
            let mut cache = self.block_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = cache.get(block_num as u64) {
                buf[..self.block_size].copy_from_slice(&cached);
                return Ok(());
            }
        }

        // キャッシュミス
        self.read_block_raw(block_num, buf)?;

        {
            let mut cache = self.block_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.insert(block_num as u64, buf[..self.block_size].to_vec());
        }

        Ok(())
    }

    /// inodeをキャッシュ経由で読み取る
    fn read_inode(&self, inode_num: u64) -> VfsResult<Ext2Inode> {
        if inode_num == 0 {
            return Err(VfsError::NotFound);
        }

        // inodeキャッシュ
        {
            let mut cache = self.inode_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(inode) = cache.get(inode_num) {
                return Ok(inode);
            }
        }

        let inode_idx = inode_num - 1;
        let group = (inode_idx / self.inodes_per_group as u64) as usize;
        let local_idx = inode_idx % self.inodes_per_group as u64;

        if group >= self.group_desc_table.len() {
            return Err(VfsError::NotFound);
        }

        let gd = &self.group_desc_table[group];
        let inode_table_block = gd.bg_inode_table;
        let inode_offset = local_idx as usize * self.inode_size;
        let block_offset = inode_offset / self.block_size;
        let byte_offset = inode_offset % self.block_size;

        let target_block = inode_table_block + block_offset as u32;

        // ブロックキャッシュ経由でinodeテーブルブロックを読み取り
        let mut block_buf = vec![0u8; self.block_size];
        self.read_block(target_block, &mut block_buf)?;

        let inode: Ext2Inode = if byte_offset + self.inode_size <= self.block_size {
            unsafe {
                core::ptr::read_unaligned(
                    (block_buf.as_ptr() as usize + byte_offset) as *const Ext2Inode,
                )
            }
        } else {
            let first_len = self.block_size - byte_offset;
            let second_len = self.inode_size - first_len;
            if second_len > self.block_size {
                return Err(VfsError::IoError);
            }

            let mut inode_buf = vec![0u8; self.inode_size];
            inode_buf[..first_len].copy_from_slice(&block_buf[byte_offset..byte_offset + first_len]);

            let mut next_block = vec![0u8; self.block_size];
            let next_target_block = target_block.checked_add(1).ok_or(VfsError::IoError)?;
            self.read_block(next_target_block, &mut next_block)?;
            inode_buf[first_len..].copy_from_slice(&next_block[..second_len]);

            unsafe { core::ptr::read_unaligned(inode_buf.as_ptr() as *const Ext2Inode) }
        };

        {
            let mut cache = self.inode_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.insert(inode_num, inode);
        }

        Ok(inode)
    }

    /// ディレクトリ/ファイルの全データブロックを一度に読み取る
    /// 間接ブロックキャッシュ付き
    fn read_all_blocks(&self, inode: &Ext2Inode) -> VfsResult<Vec<u8>> {
        const MAX_DIR_DATA_SIZE: usize = 16 * 1024 * 1024;
        let size = inode.i_size as usize;
        if size == 0 {
            return Ok(Vec::new());
        }
        if size > MAX_DIR_DATA_SIZE {
            return Err(VfsError::InvalidArgument);
        }

        let mut data = vec![0u8; size];
        let ptrs_per_block = (self.block_size / 4) as u32;
        let mut indirect_cache: Option<Vec<u8>> = None;

        let mut read_offset = 0;
        let mut block_idx: u32 = 0;

        while read_offset < size {
            let block_num =
                self.get_block_num_cached(inode, block_idx, ptrs_per_block, &mut indirect_cache)?;
            let to_copy = core::cmp::min(self.block_size, size - read_offset);
            if block_num == 0 {
                data[read_offset..read_offset + to_copy].fill(0);
            } else {
                let mut block_buf = vec![0u8; self.block_size];
                self.read_block(block_num, &mut block_buf)?;
                data[read_offset..read_offset + to_copy].copy_from_slice(&block_buf[..to_copy]);
            }

            read_offset += to_copy;
            block_idx += 1;
        }

        Ok(data)
    }

    /// ブロック番号の解決（キャッシュなし版）
    fn get_block_num(&self, inode: &Ext2Inode, block_idx: u32) -> VfsResult<u32> {
        if block_idx < 12 {
            return Ok(inode.i_block[block_idx as usize]);
        }

        let ptrs_per_block = (self.block_size / 4) as u32;

        // 間接
        if block_idx < 12 + ptrs_per_block {
            let indirect_block = inode.i_block[12];
            if indirect_block == 0 {
                return Ok(0);
            }
            let mut buf = vec![0u8; self.block_size];
            self.read_block(indirect_block, &mut buf)?;
            let offset = ((block_idx - 12) * 4) as usize;
            return Ok(u32::from_le_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]));
        }

        // 二重間接
        if block_idx < 12 + ptrs_per_block + ptrs_per_block * ptrs_per_block {
            let double_indirect = inode.i_block[13];
            if double_indirect == 0 {
                return Ok(0);
            }
            let idx = block_idx - 12 - ptrs_per_block;
            let indirect_idx = idx / ptrs_per_block;
            let block_offset = idx % ptrs_per_block;

            let mut buf = vec![0u8; self.block_size];
            self.read_block(double_indirect, &mut buf)?;
            let offset = (indirect_idx * 4) as usize;
            let indirect_block = u32::from_le_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            if indirect_block == 0 {
                return Ok(0);
            }
            self.read_block(indirect_block, &mut buf)?;
            let offset = (block_offset * 4) as usize;
            return Ok(u32::from_le_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]));
        }

        Err(VfsError::NotSupported)
    }

    /// ブロック番号の解決（間接ブロックキャッシュ付き）
    #[inline]
    fn get_block_num_cached(
        &self,
        inode: &Ext2Inode,
        block_idx: u32,
        ptrs_per_block: u32,
        indirect_cache: &mut Option<Vec<u8>>,
    ) -> VfsResult<u32> {
        if block_idx < 12 {
            return Ok(inode.i_block[block_idx as usize]);
        }

        if block_idx < 12 + ptrs_per_block {
            let indirect_block = inode.i_block[12];
            if indirect_block == 0 {
                return Ok(0);
            }
            if indirect_cache.is_none() {
                let mut buf = vec![0u8; self.block_size];
                self.read_block(indirect_block, &mut buf)?;
                *indirect_cache = Some(buf);
            }
            if let Some(ref buf) = indirect_cache {
                let offset = ((block_idx - 12) * 4) as usize;
                if offset + 4 > buf.len() {
                    return Err(VfsError::IoError);
                }
                return Ok(u32::from_le_bytes([
                    buf[offset],
                    buf[offset + 1],
                    buf[offset + 2],
                    buf[offset + 3],
                ]));
            }
            return Ok(0);
        }

        self.get_block_num(inode, block_idx)
    }
}

impl FileSystem for Ext2Fs {
    fn name(&self) -> &str {
        "ext2"
    }

    fn root_inode(&self) -> u64 {
        2
    }

    fn stat(&self, inode: u64) -> VfsResult<FileAttr> {
        let ext2_inode = self.read_inode(inode)?;
        let file_type = match ext2_inode.i_mode & 0xF000 {
            EXT2_S_IFREG => FileType::RegularFile,
            EXT2_S_IFDIR => FileType::Directory,
            EXT2_S_IFLNK => FileType::SymbolicLink,
            _ => FileType::RegularFile,
        };
        Ok(FileAttr {
            file_type,
            size: ext2_inode.i_size as u64,
            blocks: ext2_inode.i_blocks as u64,
            atime: ext2_inode.i_atime as u64,
            mtime: ext2_inode.i_mtime as u64,
            ctime: ext2_inode.i_ctime as u64,
            mode: ext2_inode.i_mode,
            uid: ext2_inode.i_uid as u32,
            gid: ext2_inode.i_gid as u32,
            nlink: ext2_inode.i_links_count as u32,
        })
    }

    fn lookup(&self, parent_inode: u64, name: &str) -> VfsResult<u64> {
        let parent = self.read_inode(parent_inode)?;
        if parent.i_mode & 0xF000 != EXT2_S_IFDIR {
            return Err(VfsError::NotDirectory);
        }

        // ディレクトリデータを一括読み取り（キャッシュ＋間接ブロックキャッシュ）
        let data = self.read_all_blocks(&parent)?;
        let size = data.len();

        let mut offset = 0;
        while offset + size_of::<Ext2DirEntry>() <= size {
            let entry: Ext2DirEntry = unsafe {
                core::ptr::read_unaligned((data.as_ptr() as usize + offset) as *const Ext2DirEntry)
            };
            if entry.rec_len == 0 {
                break;
            }
            // rec_lenの検証: 最小サイズ未満、またはデータ範囲を超える場合は不正
            if entry.rec_len < size_of::<Ext2DirEntry>() as u16 {
                break;
            }
            if offset + entry.rec_len as usize > size {
                break;
            }
            if entry.inode != 0 && entry.name_len > 0 {
                let name_offset = offset + size_of::<Ext2DirEntry>();
                if name_offset + entry.name_len as usize <= size {
                    let entry_name = &data[name_offset..name_offset + entry.name_len as usize];
                    if let Ok(entry_name_str) = core::str::from_utf8(entry_name) {
                        if entry_name_str == name {
                            return Ok(entry.inode as u64);
                        }
                    }
                }
            }
            offset += entry.rec_len as usize;
        }

        Err(VfsError::NotFound)
    }

    fn read(&self, inode: u64, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let ext2_inode = self.read_inode(inode)?;
        if ext2_inode.i_mode & 0xF000 != EXT2_S_IFREG {
            return Err(VfsError::IsDirectory);
        }

        let file_size = ext2_inode.i_size as u64;
        if offset >= file_size {
            return Ok(0);
        }

        let to_read = core::cmp::min(buf.len(), (file_size - offset) as usize);
        if to_read == 0 {
            return Ok(0);
        }

        let start_block = (offset / self.block_size as u64) as u32;
        let block_offset = (offset % self.block_size as u64) as usize;
        let ptrs_per_block = (self.block_size / 4) as u32;
        let mut indirect_cache: Option<Vec<u8>> = None;
        let mut block_buf = vec![0u8; self.block_size];

        let mut bytes_read = 0usize;
        let mut current_block = start_block;

        while bytes_read < to_read {
            let start_in_block = if current_block == start_block {
                block_offset
            } else {
                0
            };
            let remaining = to_read - bytes_read;
            let to_copy = core::cmp::min(remaining, self.block_size - start_in_block);

            let block_num = self.get_block_num_cached(
                &ext2_inode,
                current_block,
                ptrs_per_block,
                &mut indirect_cache,
            )?;

            if block_num == 0 {
                buf[bytes_read..bytes_read + to_copy].fill(0);
            } else {
                self.read_block(block_num, &mut block_buf)?;
                buf[bytes_read..bytes_read + to_copy]
                    .copy_from_slice(&block_buf[start_in_block..start_in_block + to_copy]);
            }

            bytes_read += to_copy;
            current_block += 1;
        }

        Ok(bytes_read)
    }

    fn write(&mut self, _inode: u64, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnlyFs)
    }

    fn readdir(&self, inode: u64) -> VfsResult<Vec<DirEntry>> {
        let ext2_inode = self.read_inode(inode)?;
        if ext2_inode.i_mode & 0xF000 != EXT2_S_IFDIR {
            return Err(VfsError::NotDirectory);
        }

        // ディレクトリデータを一括読み取り
        let data = self.read_all_blocks(&ext2_inode)?;
        let size = data.len();

        let mut entries = Vec::new();
        let mut offset = 0;

        while offset + size_of::<Ext2DirEntry>() <= size {
            let entry: Ext2DirEntry = unsafe {
                core::ptr::read_unaligned((data.as_ptr() as usize + offset) as *const Ext2DirEntry)
            };
            if entry.rec_len == 0 {
                break;
            }
            if entry.rec_len < size_of::<Ext2DirEntry>() as u16 {
                break;
            }
            if offset + entry.rec_len as usize > size {
                break;
            }
            if entry.inode != 0 && entry.name_len > 0 {
                let name_offset = offset + size_of::<Ext2DirEntry>();
                if name_offset + entry.name_len as usize <= size {
                    let entry_name = &data[name_offset..name_offset + entry.name_len as usize];
                    if let Ok(name_str) = core::str::from_utf8(entry_name) {
                        let file_type = match entry.file_type {
                            1 => FileType::RegularFile,
                            2 => FileType::Directory,
                            7 => FileType::SymbolicLink,
                            _ => FileType::RegularFile,
                        };
                        entries.push(DirEntry {
                            name: String::from(name_str),
                            inode: entry.inode as u64,
                            file_type,
                        });
                    }
                }
            }
            offset += entry.rec_len as usize;
        }

        Ok(entries)
    }

    fn create(&mut self, _parent_inode: u64, _name: &str, _mode: u16) -> VfsResult<u64> {
        Err(VfsError::ReadOnlyFs)
    }

    fn mkdir(&mut self, _parent_inode: u64, _name: &str, _mode: u16) -> VfsResult<u64> {
        Err(VfsError::ReadOnlyFs)
    }

    fn unlink(&mut self, _parent_inode: u64, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnlyFs)
    }

    fn rmdir(&mut self, _parent_inode: u64, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnlyFs)
    }

    fn truncate(&mut self, _inode: u64, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnlyFs)
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }
}
