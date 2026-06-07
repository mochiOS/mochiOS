#![no_std]
#![no_main]

use core::arch::asm;
use core::cell::UnsafeCell;
use core::cmp::min;
use core::sync::atomic::{AtomicBool, Ordering};

const ATA_DATA: u16 = 0x1F0;
const ATA_SECTOR_COUNT: u16 = 0x1F2;
const ATA_LBA_LOW: u16 = 0x1F3;
const ATA_LBA_MID: u16 = 0x1F4;
const ATA_LBA_HIGH: u16 = 0x1F5;
const ATA_DRIVE_HEAD: u16 = 0x1F6;
const ATA_STATUS_CMD: u16 = 0x1F7;
const ATA_ALT_STATUS: u16 = 0x3F6;

const ATA_CMD_READ_SECTORS: u8 = 0x20;
const ATA_CMD_READ_DMA: u8 = 0xC8;
const ATA_STATUS_ERR: u8 = 1 << 0;
const ATA_STATUS_DRQ: u8 = 1 << 3;
const ATA_STATUS_DF: u8 = 1 << 5;
const ATA_STATUS_BSY: u8 = 1 << 7;

const EXT2_MAGIC: u16 = 0xEF53;
const BLOCK_CACHE_SLOTS: usize = 64;
const INODE_CACHE_SLOTS: usize = 128;
const PATH_CACHE_SLOTS: usize = 128;
const PATH_CACHE_MAX: usize = 192;

const PCI_CFG_ADDR: u16 = 0xCF8;
const PCI_CFG_DATA: u16 = 0xCFC;
const PCI_CLASS_MASS_STORAGE: u8 = 0x01;
const PCI_SUBCLASS_IDE: u8 = 0x01;

#[repr(C)]
pub struct McxBuffer {
    pub ptr: *mut u8,
    pub len: usize,
}

#[repr(C)]
pub struct McxPath {
    pub ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
pub struct McxFsOps {
    pub mount: extern "C" fn(device_id: u32) -> i32,
    pub set_disk_ops: extern "C" fn(ops: *const McxDiskOps) -> i32,
    pub create: extern "C" fn(path: McxPath, mode: u32) -> i32,
    pub remove: extern "C" fn(path: McxPath, is_dir: u32) -> i32,
    pub rename: extern "C" fn(src: McxPath, dst: McxPath) -> i32,
    pub read:
        extern "C" fn(path: McxPath, offset: u64, buf: McxBuffer, out_read: *mut usize) -> i32,
    pub write:
        extern "C" fn(path: McxPath, offset: u64, buf: McxBuffer, out_written: *mut usize) -> i32,
    pub truncate: extern "C" fn(path: McxPath, len: u64) -> i32,
    pub stat: extern "C" fn(path: McxPath, out_mode: *mut u16, out_size: *mut u64) -> i32,
    pub readdir: extern "C" fn(path: McxPath, buf: McxBuffer, out_len: *mut usize) -> i32,
}

#[repr(C)]
pub struct McxDiskOps {
    pub probe: extern "C" fn() -> i32,
    pub read_sector: extern "C" fn(disk_id: u32, lba: u64, buf: *mut u8, buf_len: usize) -> i32,
    pub write_sector: extern "C" fn(disk_id: u32, lba: u64, buf: *const u8, buf_len: usize) -> i32,
}

#[derive(Clone, Copy)]
struct FsMount {
    disk_id: u32,
    block_size: u32,
    sectors_per_block: u32,
    inode_size: u16,
    inodes_per_group: u32,
    gdt_block: u32,
}

#[derive(Clone, Copy)]
struct GroupDesc {
    block_bitmap: u32,
    inode_bitmap: u32,
    inode_table: u32,
}

#[derive(Clone, Copy)]
struct BlockCacheEntry {
    valid: bool,
    disk_id: u32,
    block_num: u32,
    data: [u8; 4096],
}

impl BlockCacheEntry {
    const fn empty() -> Self {
        Self {
            valid: false,
            disk_id: 0,
            block_num: 0,
            data: [0u8; 4096],
        }
    }
}

#[derive(Clone, Copy)]
struct InodeCacheEntry {
    valid: bool,
    disk_id: u32,
    inode_num: u32,
    inode: [u8; 256],
}

impl InodeCacheEntry {
    const fn empty() -> Self {
        Self {
            valid: false,
            disk_id: 0,
            inode_num: 0,
            inode: [0u8; 256],
        }
    }
}

#[derive(Clone, Copy)]
struct PathCacheEntry {
    valid: bool,
    disk_id: u32,
    path_len: u16,
    path_hash: u64,
    inode_num: u32,
    path: [u8; PATH_CACHE_MAX],
}

impl PathCacheEntry {
    const fn empty() -> Self {
        Self {
            valid: false,
            disk_id: 0,
            path_len: 0,
            path_hash: 0,
            inode_num: 0,
            path: [0u8; PATH_CACHE_MAX],
        }
    }
}

static mut MOUNT: Option<FsMount> = None;
static mut DISK_OPS_PTR: *const McxDiskOps = core::ptr::null();
static OP_LOCK: AtomicBool = AtomicBool::new(false);

struct SharedBuf(UnsafeCell<[u8; 4096]>);

unsafe impl Sync for SharedBuf {}

impl SharedBuf {
    const fn new() -> Self {
        Self(UnsafeCell::new([0u8; 4096]))
    }

    #[inline]
    unsafe fn as_mut(&self) -> &mut [u8; 4096] {
        &mut *self.0.get()
    }

    #[inline]
    unsafe fn as_ref(&self) -> &[u8; 4096] {
        &*self.0.get()
    }
}

static READ_INODE_GDT_BLK: SharedBuf = SharedBuf::new();
static READ_INODE_IBLK: SharedBuf = SharedBuf::new();
static LOOKUP_BLK: SharedBuf = SharedBuf::new();
static LOOKUP_IND: SharedBuf = SharedBuf::new();
static READ_RANGE_BLK: SharedBuf = SharedBuf::new();
static READ_RANGE_IND: SharedBuf = SharedBuf::new();
static READDIR_BLK: SharedBuf = SharedBuf::new();
static READDIR_IND: SharedBuf = SharedBuf::new();

static mut BLOCK_CACHE: [BlockCacheEntry; BLOCK_CACHE_SLOTS] =
    [BlockCacheEntry::empty(); BLOCK_CACHE_SLOTS];
static mut BLOCK_CACHE_CURSOR: usize = 0;

static mut INODE_CACHE: [InodeCacheEntry; INODE_CACHE_SLOTS] =
    [InodeCacheEntry::empty(); INODE_CACHE_SLOTS];
static mut INODE_CACHE_CURSOR: usize = 0;

static mut PATH_CACHE: [PathCacheEntry; PATH_CACHE_SLOTS] =
    [PathCacheEntry::empty(); PATH_CACHE_SLOTS];
static mut PATH_CACHE_CURSOR: usize = 0;
static mut BMIDE_BASE: u16 = 0;
static mut BMIDE_SCANNED: bool = false;

#[repr(C)]
#[derive(Clone, Copy)]
struct PrdtEntry {
    base_phys: u32,
    byte_count: u16,
    flags: u16,
}

#[repr(align(16))]
struct PrdtAligned([PrdtEntry; 2]);

#[repr(align(65536))]
struct DmaBuf([u8; 4096]);

static mut DMA_PRDT: PrdtAligned = PrdtAligned(
    [PrdtEntry {
        base_phys: 0,
        byte_count: 0,
        flags: 0x8000,
    }; 2],
);
static mut DMA_BUF: DmaBuf = DmaBuf([0u8; 4096]);

struct OpLockGuard;

impl Drop for OpLockGuard {
    fn drop(&mut self) {
        OP_LOCK.store(false, Ordering::Release);
    }
}

#[inline]
fn lock_ops() -> OpLockGuard {
    while OP_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    OpLockGuard
}

#[inline]
fn path_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

extern "C" fn fs_set_disk_ops(ops: *const McxDiskOps) -> i32 {
    if ops.is_null() {
        return -22; // EINVAL
    }
    unsafe {
        DISK_OPS_PTR = ops;
    }
    0
}

#[inline]
unsafe fn read_sector_disk(disk_id: u32, lba: u32, out: &mut [u8; 512]) -> bool {
    let ops = DISK_OPS_PTR;
    if ops.is_null() {
        return false;
    }
    let rc = ((*ops).read_sector)(disk_id, lba as u64, out.as_mut_ptr(), out.len());
    rc == 0
}

#[inline]
unsafe fn write_sector_disk(disk_id: u32, lba: u32, buf: &[u8]) -> bool {
    let ops = DISK_OPS_PTR;
    if ops.is_null() || buf.len() < 512 {
        return false;
    }
    let rc = ((*ops).write_sector)(disk_id, lba as u64, buf.as_ptr(), 512);
    rc == 0
}

unsafe fn reset_caches() {
    for e in &mut BLOCK_CACHE {
        e.valid = false;
    }
    BLOCK_CACHE_CURSOR = 0;
    for e in &mut INODE_CACHE {
        e.valid = false;
    }
    INODE_CACHE_CURSOR = 0;
    for e in &mut PATH_CACHE {
        e.valid = false;
    }
    PATH_CACHE_CURSOR = 0;
}

unsafe fn block_cache_lookup(
    disk_id: u32,
    block_num: u32,
    out: &mut [u8],
    block_size: usize,
) -> bool {
    for e in &BLOCK_CACHE {
        if e.valid && e.disk_id == disk_id && e.block_num == block_num {
            out[..block_size].copy_from_slice(&e.data[..block_size]);
            return true;
        }
    }
    false
}

unsafe fn block_cache_insert(disk_id: u32, block_num: u32, data: &[u8], block_size: usize) {
    let slot = BLOCK_CACHE_CURSOR % BLOCK_CACHE_SLOTS;
    BLOCK_CACHE_CURSOR = (BLOCK_CACHE_CURSOR + 1) % BLOCK_CACHE_SLOTS;
    let ent = &mut BLOCK_CACHE[slot];
    ent.valid = true;
    ent.disk_id = disk_id;
    ent.block_num = block_num;
    ent.data[..block_size].copy_from_slice(&data[..block_size]);
}

unsafe fn inode_cache_lookup(
    disk_id: u32,
    inode_num: u32,
    out: &mut [u8; 256],
    isz: usize,
) -> bool {
    for e in &INODE_CACHE {
        if e.valid && e.disk_id == disk_id && e.inode_num == inode_num {
            out[..isz].copy_from_slice(&e.inode[..isz]);
            return true;
        }
    }
    false
}

unsafe fn inode_cache_insert(disk_id: u32, inode_num: u32, inode: &[u8; 256], isz: usize) {
    let slot = INODE_CACHE_CURSOR % INODE_CACHE_SLOTS;
    INODE_CACHE_CURSOR = (INODE_CACHE_CURSOR + 1) % INODE_CACHE_SLOTS;
    let ent = &mut INODE_CACHE[slot];
    ent.valid = true;
    ent.disk_id = disk_id;
    ent.inode_num = inode_num;
    ent.inode[..isz].copy_from_slice(&inode[..isz]);
}

unsafe fn path_cache_lookup(disk_id: u32, path: &[u8]) -> Option<u32> {
    if path.len() > PATH_CACHE_MAX {
        return None;
    }
    let h = path_hash(path);
    for e in &PATH_CACHE {
        if !e.valid || e.disk_id != disk_id || e.path_hash != h {
            continue;
        }
        let n = e.path_len as usize;
        if n == path.len() && e.path[..n] == path[..] {
            return Some(e.inode_num);
        }
    }
    None
}

unsafe fn path_cache_insert(disk_id: u32, path: &[u8], inode_num: u32) {
    if path.len() > PATH_CACHE_MAX {
        return;
    }
    let slot = PATH_CACHE_CURSOR % PATH_CACHE_SLOTS;
    PATH_CACHE_CURSOR = (PATH_CACHE_CURSOR + 1) % PATH_CACHE_SLOTS;
    let ent = &mut PATH_CACHE[slot];
    ent.valid = true;
    ent.disk_id = disk_id;
    ent.path_len = path.len() as u16;
    ent.path_hash = path_hash(path);
    ent.inode_num = inode_num;
    ent.path[..path.len()].copy_from_slice(path);
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let mut value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}

#[inline]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let mut value: u16;
    asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags));
    value
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    let mut value: u32;
    asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack, preserves_flags));
    value
}

#[inline]
unsafe fn outl(port: u16, value: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
}

#[inline]
unsafe fn io_wait_400ns() {
    let _ = inb(ATA_ALT_STATUS);
    let _ = inb(ATA_ALT_STATUS);
    let _ = inb(ATA_ALT_STATUS);
    let _ = inb(ATA_ALT_STATUS);
}

#[inline]
unsafe fn select_drive(drive: u8, lba: u32) {
    let head = 0xE0 | ((drive & 1) << 4) | (((lba >> 24) & 0x0F) as u8);
    outb(ATA_DRIVE_HEAD, head);
    io_wait_400ns();
}

unsafe fn wait_not_busy(timeout: usize) -> bool {
    for _ in 0..timeout {
        let st = inb(ATA_STATUS_CMD);
        if (st & ATA_STATUS_BSY) == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn wait_drq(timeout: usize) -> bool {
    for _ in 0..timeout {
        let st = inb(ATA_STATUS_CMD);
        if (st & ATA_STATUS_BSY) != 0 {
            core::hint::spin_loop();
            continue;
        }
        if (st & (ATA_STATUS_ERR | ATA_STATUS_DF)) != 0 {
            return false;
        }
        if (st & ATA_STATUS_DRQ) != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn read_sector_ata(drive: u8, lba: u32, out: &mut [u8; 512]) -> bool {
    if !wait_not_busy(200_000) {
        return false;
    }
    select_drive(drive, lba);
    outb(ATA_SECTOR_COUNT, 1);
    outb(ATA_LBA_LOW, (lba & 0xFF) as u8);
    outb(ATA_LBA_MID, ((lba >> 8) & 0xFF) as u8);
    outb(ATA_LBA_HIGH, ((lba >> 16) & 0xFF) as u8);
    outb(ATA_STATUS_CMD, ATA_CMD_READ_SECTORS);
    if !wait_drq(200_000) {
        return false;
    }
    for i in 0..256 {
        let w = inw(ATA_DATA);
        let b = w.to_le_bytes();
        out[i * 2] = b[0];
        out[i * 2 + 1] = b[1];
    }
    true
}

#[inline]
unsafe fn pci_config_read_u32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xfc);
    outl(PCI_CFG_ADDR, addr);
    inl(PCI_CFG_DATA)
}

unsafe fn find_bmide_base() -> Option<u16> {
    if BMIDE_SCANNED {
        return if BMIDE_BASE == 0 {
            None
        } else {
            Some(BMIDE_BASE)
        };
    }
    BMIDE_SCANNED = true;
    for bus in 0u16..=255 {
        let bus = bus as u8;
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let vendor_device = pci_config_read_u32(bus, dev, func, 0x00);
                if vendor_device == 0xffff_ffff || (vendor_device & 0xffff) == 0xffff {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let class_reg = pci_config_read_u32(bus, dev, func, 0x08);
                let class_code = ((class_reg >> 24) & 0xff) as u8;
                let subclass = ((class_reg >> 16) & 0xff) as u8;
                if class_code != PCI_CLASS_MASS_STORAGE || subclass != PCI_SUBCLASS_IDE {
                    continue;
                }
                let bar4 = pci_config_read_u32(bus, dev, func, 0x20);
                if (bar4 & 0x1) == 0 {
                    continue;
                }
                let base = (bar4 & 0xfffc) as u16;
                if base != 0 {
                    BMIDE_BASE = base;
                    return Some(base);
                }
            }
        }
    }
    None
}

#[inline]
unsafe fn virt_to_phys(vaddr: u64) -> Option<u64> {
    let cr3: u64;
    asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    let l4 = cr3 & 0x000f_ffff_ffff_f000;
    let l4e_ptr = (l4 + (((vaddr >> 39) & 0x1ff) * 8)) as *const u64;
    let l4e = core::ptr::read_volatile(l4e_ptr);
    if (l4e & 1) == 0 {
        return None;
    }
    let l3 = l4e & 0x000f_ffff_ffff_f000;
    let l3e_ptr = (l3 + (((vaddr >> 30) & 0x1ff) * 8)) as *const u64;
    let l3e = core::ptr::read_volatile(l3e_ptr);
    if (l3e & 1) == 0 {
        return None;
    }
    if (l3e & (1 << 7)) != 0 {
        return Some((l3e & 0x000f_ffff_c000_0000) | (vaddr & 0x3fff_ffff));
    }
    let l2 = l3e & 0x000f_ffff_ffff_f000;
    let l2e_ptr = (l2 + (((vaddr >> 21) & 0x1ff) * 8)) as *const u64;
    let l2e = core::ptr::read_volatile(l2e_ptr);
    if (l2e & 1) == 0 {
        return None;
    }
    if (l2e & (1 << 7)) != 0 {
        return Some((l2e & 0x000f_ffff_ffe0_0000) | (vaddr & 0x1f_ffff));
    }
    let l1 = l2e & 0x000f_ffff_ffff_f000;
    let l1e_ptr = (l1 + (((vaddr >> 12) & 0x1ff) * 8)) as *const u64;
    let l1e = core::ptr::read_volatile(l1e_ptr);
    if (l1e & 1) == 0 {
        return None;
    }
    Some((l1e & 0x000f_ffff_ffff_f000) | (vaddr & 0xfff))
}

unsafe fn read_fs_block_dma(
    m: &FsMount,
    block_num: u32,
    out: &mut [u8],
    block_size: usize,
) -> bool {
    let _ = (m, block_num, out, block_size);
    false
}

#[inline]
fn read_u16(buf: &[u8], off: usize) -> Option<u16> {
    let s = buf.get(off..off + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

#[inline]
fn read_u32(buf: &[u8], off: usize) -> Option<u32> {
    let s = buf.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

#[inline]
fn write_u16(buf: &mut [u8], off: usize, value: u16) -> bool {
    let Some(dst) = buf.get_mut(off..off + 2) else {
        return false;
    };
    dst.copy_from_slice(&value.to_le_bytes());
    true
}

#[inline]
fn write_u32(buf: &mut [u8], off: usize, value: u32) -> bool {
    let Some(dst) = buf.get_mut(off..off + 4) else {
        return false;
    };
    dst.copy_from_slice(&value.to_le_bytes());
    true
}

unsafe fn read_fs_block(m: &FsMount, block_num: u32, out: &mut [u8]) -> bool {
    let block_size = m.block_size as usize;
    if out.len() < block_size {
        return false;
    }
    if block_cache_lookup(m.disk_id, block_num, out, block_size) {
        return true;
    }
    // NOTE: ここでは ATA PIO を直接叩かず、disk.cext の read_sector を使う。
    // DMA 経路は最適化だが安定性優先で無効化している。
    let spb = m.sectors_per_block as usize;
    for i in 0..spb {
        let lba = block_num
            .saturating_mul(m.sectors_per_block)
            .saturating_add(i as u32);
        let mut sec = [0u8; 512];
        if !read_sector_disk(m.disk_id, lba, &mut sec) {
            return false;
        }
        let dst = i * 512;
        out[dst..dst + 512].copy_from_slice(&sec);
    }
    block_cache_insert(m.disk_id, block_num, out, block_size);
    true
}

unsafe fn read_group_desc(m: &FsMount, group: u32) -> Option<GroupDesc> {
    let block_size = m.block_size as usize;
    let desc_off = group as usize * 32;
    let desc_block = m.gdt_block as usize + desc_off / block_size;
    let desc_inner = desc_off % block_size;
    if !read_fs_block(m, desc_block as u32, READ_INODE_GDT_BLK.as_mut()) {
        return None;
    }
    let blk = READ_INODE_GDT_BLK.as_ref();
    Some(GroupDesc {
        block_bitmap: read_u32(blk, desc_inner)?,
        inode_bitmap: read_u32(blk, desc_inner + 4)?,
        inode_table: read_u32(blk, desc_inner + 8)?,
    })
}

unsafe fn write_inode_raw(m: &FsMount, inode_num: u32, inode_bytes: &[u8; 256]) -> bool {
    if inode_num == 0 || m.inodes_per_group == 0 {
        return false;
    }
    let isz = m.inode_size as usize;
    let group = (inode_num - 1) / m.inodes_per_group;
    let index = (inode_num - 1) % m.inodes_per_group;
    let Some(gd) = read_group_desc(m, group) else {
        return false;
    };
    let inode_off = (index as usize) * isz;
    let blk = inode_off / (m.block_size as usize);
    let off = inode_off % (m.block_size as usize);
    if !read_fs_block(m, gd.inode_table + blk as u32, READ_INODE_IBLK.as_mut()) {
        return false;
    }
    let iblk = READ_INODE_IBLK.as_mut();
    if off + isz > iblk.len() {
        return false;
    }
    iblk[off..off + isz].copy_from_slice(&inode_bytes[..isz]);
    if !write_fs_block(m, gd.inode_table + blk as u32, iblk) {
        return false;
    }
    inode_cache_insert(m.disk_id, inode_num, inode_bytes, isz);
    true
}

unsafe fn bitmap_find_free(bitmap: &[u8], start_bit: usize, limit_bits: usize) -> Option<usize> {
    for bit in start_bit..limit_bits {
        let byte = bit / 8;
        let mask = 1u8 << (bit % 8);
        if bitmap.get(byte).copied().unwrap_or(0) & mask == 0 {
            return Some(bit);
        }
    }
    None
}

unsafe fn bitmap_set_bit(m: &FsMount, bitmap_block: u32, bit: usize, set: bool) -> bool {
    if !read_fs_block(m, bitmap_block, READ_RANGE_BLK.as_mut()) {
        return false;
    }
    let blk = READ_RANGE_BLK.as_mut();
    let byte = bit / 8;
    let mask = 1u8 << (bit % 8);
    let Some(slot) = blk.get_mut(byte) else {
        return false;
    };
    if set {
        *slot |= mask;
    } else {
        *slot &= !mask;
    }
    write_fs_block(m, bitmap_block, blk)
}

unsafe fn allocate_inode(m: &FsMount) -> Option<u32> {
    let gd = read_group_desc(m, 0)?;
    if !read_fs_block(m, gd.inode_bitmap, READ_RANGE_BLK.as_mut()) {
        return None;
    }
    let bitmap = READ_RANGE_BLK.as_ref();
    let total_bits = m.inodes_per_group as usize;
    let bit = bitmap_find_free(bitmap, 1, total_bits)?;
    if !bitmap_set_bit(m, gd.inode_bitmap, bit, true) {
        return None;
    }
    Some((bit + 1) as u32)
}

unsafe fn allocate_block(m: &FsMount) -> Option<u32> {
    let gd = read_group_desc(m, 0)?;
    if !read_fs_block(m, gd.block_bitmap, READ_RANGE_BLK.as_mut()) {
        return None;
    }
    let bitmap = READ_RANGE_BLK.as_ref();
    let total_bits = (m.block_size as usize) * 8;
    let bit = bitmap_find_free(bitmap, 1, total_bits)?;
    if !bitmap_set_bit(m, gd.block_bitmap, bit, true) {
        return None;
    }
    let block_num = bit as u32;
    let zero = READ_RANGE_IND.as_mut();
    zero.fill(0);
    if !write_fs_block(m, block_num, zero) {
        return None;
    }
    Some(block_num)
}

unsafe fn free_block(m: &FsMount, block_num: u32) -> bool {
    let Some(gd) = read_group_desc(m, 0) else {
        return false;
    };
    bitmap_set_bit(m, gd.block_bitmap, block_num as usize, false)
}

unsafe fn free_inode(m: &FsMount, inode_num: u32) -> bool {
    let Some(gd) = read_group_desc(m, 0) else {
        return false;
    };
    if inode_num == 0 {
        return false;
    }
    bitmap_set_bit(m, gd.inode_bitmap, (inode_num - 1) as usize, false)
}

unsafe fn free_inode_blocks(m: &FsMount, inode: &[u8; 256]) -> bool {
    let size = inode_size(inode) as usize;
    let block_size = m.block_size as usize;
    let blocks = size.div_ceil(block_size);
    for bi in 0..blocks.min(12) {
        let bnum = inode_block(inode, bi);
        if bnum != 0 && !free_block(m, bnum) {
            return false;
        }
    }
    let indirect = inode_block(inode, 12);
    if indirect != 0 {
        if !read_fs_block(m, indirect, READ_RANGE_IND.as_mut()) {
            return false;
        }
        let table = READ_RANGE_IND.as_ref();
        let per = block_size / 4;
        for idx in 0..per {
            let entry = read_u32(table, idx * 4).unwrap_or(0);
            if entry != 0 && !free_block(m, entry) {
                return false;
            }
        }
        if !free_block(m, indirect) {
            return false;
        }
    }
    true
}

unsafe fn find_child_entry_location(
    m: &FsMount,
    dir_inode_num: u32,
    name: &[u8],
) -> Option<(u32, usize, u32, usize)> {
    let mut inode = [0u8; 256];
    if !read_inode(m, dir_inode_num, &mut inode) || !is_dir(inode_mode(&inode)) {
        return None;
    }
    let dir_size = inode_size(&inode) as usize;
    let block_size = m.block_size as usize;
    let blocks = dir_size.div_ceil(block_size);
    for bi in 0..blocks {
        let bnum = read_data_block_num(m, &inode, bi, LOOKUP_IND.as_mut())?;
        if !read_fs_block(m, bnum, LOOKUP_BLK.as_mut()) {
            return None;
        }
        let blk = LOOKUP_BLK.as_ref();
        let mut off = 0usize;
        while off + 8 <= block_size {
            let ino = read_u32(blk, off)?;
            let rec_len = read_u16(blk, off + 4)? as usize;
            let nlen = *blk.get(off + 6)? as usize;
            if rec_len == 0 || off + rec_len > block_size {
                break;
            }
            if ino != 0 && nlen > 0 && off + 8 + nlen <= block_size {
                let nm = &blk[off + 8..off + 8 + nlen];
                if nm == name {
                    return Some((bnum, off, ino, rec_len));
                }
            }
            off += rec_len;
        }
    }
    None
}

unsafe fn directory_is_empty(m: &FsMount, dir_inode_num: u32) -> Option<bool> {
    let mut inode = [0u8; 256];
    if !read_inode(m, dir_inode_num, &mut inode) || !is_dir(inode_mode(&inode)) {
        return None;
    }
    let dir_size = inode_size(&inode) as usize;
    let block_size = m.block_size as usize;
    let blocks = dir_size.div_ceil(block_size);
    for bi in 0..blocks {
        let bnum = read_data_block_num(m, &inode, bi, LOOKUP_IND.as_mut())?;
        if !read_fs_block(m, bnum, LOOKUP_BLK.as_mut()) {
            return None;
        }
        let blk = LOOKUP_BLK.as_ref();
        let mut off = 0usize;
        while off + 8 <= block_size {
            let ino = read_u32(blk, off)?;
            let rec_len = read_u16(blk, off + 4)? as usize;
            let nlen = *blk.get(off + 6)? as usize;
            if rec_len == 0 || off + rec_len > block_size {
                break;
            }
            if ino != 0 && nlen > 0 && off + 8 + nlen <= block_size {
                let nm = &blk[off + 8..off + 8 + nlen];
                if nm != b"." && nm != b".." {
                    return Some(false);
                }
            }
            off += rec_len;
        }
    }
    Some(true)
}

unsafe fn remove_path(m: &FsMount, path: McxPath, want_dir: bool) -> i32 {
    let raw = core::slice::from_raw_parts(path.ptr, path.len);
    let (parent_raw, name_raw) = match split_parent_name(raw) {
        Some(v) => v,
        None => return -22,
    };
    let Some(parent_inode) = resolve_path_inode(
        m,
        if parent_raw.is_empty() {
            &[]
        } else {
            parent_raw
        },
    ) else {
        return -2;
    };
    let Some((bnum, off, child_inode_num, _rec_len)) =
        find_child_entry_location(m, parent_inode, name_raw)
    else {
        return -2;
    };
    let mut child_inode = [0u8; 256];
    if !read_inode(m, child_inode_num, &mut child_inode) {
        return -5;
    }
    let child_is_dir = is_dir(inode_mode(&child_inode));
    if want_dir && !child_is_dir {
        return -20;
    }
    if !want_dir && child_is_dir {
        return -21;
    }
    if child_is_dir {
        match directory_is_empty(m, child_inode_num) {
            Some(true) => {}
            Some(false) => return (-39i64) as i32, // ENOTEMPTY
            None => return -5,
        }
    }
    if !free_inode_blocks(m, &child_inode) {
        return -5;
    }
    if !free_inode(m, child_inode_num) {
        return -5;
    }
    if !read_fs_block(m, bnum, LOOKUP_BLK.as_mut()) {
        return -5;
    }
    let blk = LOOKUP_BLK.as_mut();
    if !write_u32(blk, off, 0) {
        return -5;
    }
    if !write_fs_block(m, bnum, blk) {
        return -5;
    }
    0
}

unsafe fn clear_dir_entry(m: &FsMount, parent_inode_num: u32, name: &[u8]) -> bool {
    let Some((bnum, off, _child_inode_num, _rec_len)) =
        find_child_entry_location(m, parent_inode_num, name)
    else {
        return false;
    };
    if !read_fs_block(m, bnum, LOOKUP_BLK.as_mut()) {
        return false;
    }
    let blk = LOOKUP_BLK.as_mut();
    if !write_u32(blk, off, 0) {
        return false;
    }
    write_fs_block(m, bnum, blk)
}

unsafe fn clear_dir_entry_at(m: &FsMount, bnum: u32, off: usize) -> bool {
    if !read_fs_block(m, bnum, LOOKUP_BLK.as_mut()) {
        return false;
    }
    let blk = LOOKUP_BLK.as_mut();
    if !write_u32(blk, off, 0) {
        return false;
    }
    write_fs_block(m, bnum, blk)
}

unsafe fn restore_dir_entry_at(m: &FsMount, bnum: u32, off: usize, inode_num: u32) -> bool {
    if !read_fs_block(m, bnum, LOOKUP_BLK.as_mut()) {
        return false;
    }
    let blk = LOOKUP_BLK.as_mut();
    if !write_u32(blk, off, inode_num) {
        return false;
    }
    write_fs_block(m, bnum, blk)
}

unsafe fn rename_path(m: &FsMount, old_path: McxPath, new_path: McxPath) -> i32 {
    let old_raw = core::slice::from_raw_parts(old_path.ptr, old_path.len);
    let new_raw = core::slice::from_raw_parts(new_path.ptr, new_path.len);
    let (old_parent_raw, old_name_raw) = match split_parent_name(old_raw) {
        Some(v) => v,
        None => return -22,
    };
    let (new_parent_raw, new_name_raw) = match split_parent_name(new_raw) {
        Some(v) => v,
        None => return -22,
    };
    let Some(old_parent_inode) = resolve_path_inode(
        m,
        if old_parent_raw.is_empty() {
            &[]
        } else {
            old_parent_raw
        },
    ) else {
        return -2;
    };
    let Some(new_parent_inode) = resolve_path_inode(
        m,
        if new_parent_raw.is_empty() {
            &[]
        } else {
            new_parent_raw
        },
    ) else {
        return -2;
    };
    if old_parent_inode == new_parent_inode && old_name_raw == new_name_raw {
        return 0;
    }
    let Some((old_bnum, old_off, child_inode_num, _)) =
        find_child_entry_location(m, old_parent_inode, old_name_raw)
    else {
        return -2;
    };
    let mut child_inode = [0u8; 256];
    if !read_inode(m, child_inode_num, &mut child_inode) {
        return -5;
    }
    let child_is_dir = is_dir(inode_mode(&child_inode));
    if child_is_dir {
        match directory_is_empty(m, child_inode_num) {
            Some(true) => {}
            Some(false) => return (-39i64) as i32,
            None => return -5,
        }
    }
    let file_type = if child_is_dir { 2 } else { 1 };

    let mut dst_inode_num = 0u32;
    let mut dst_inode = [0u8; 256];
    let mut dst_bnum = 0u32;
    let mut dst_off = 0usize;
    let mut has_dst = false;
    if let Some((bnum, off, existing_inode_num, _)) =
        find_child_entry_location(m, new_parent_inode, new_name_raw)
    {
        if existing_inode_num == child_inode_num && old_parent_inode == new_parent_inode {
            return 0;
        }
        if !read_inode(m, existing_inode_num, &mut dst_inode) {
            return -5;
        }
        let dst_is_dir = is_dir(inode_mode(&dst_inode));
        if dst_is_dir {
            match directory_is_empty(m, existing_inode_num) {
                Some(true) => {}
                Some(false) => return (-39i64) as i32,
                None => return -5,
            }
        }
        dst_inode_num = existing_inode_num;
        dst_bnum = bnum;
        dst_off = off;
        has_dst = true;
        if !clear_dir_entry_at(m, dst_bnum, dst_off) {
            return -5;
        }
    }

    if !allocate_dir_entry(
        m,
        new_parent_inode,
        child_inode_num,
        new_name_raw,
        file_type,
    ) {
        if has_dst {
            let _ = restore_dir_entry_at(m, dst_bnum, dst_off, dst_inode_num);
        }
        return -5;
    }
    if !clear_dir_entry_at(m, old_bnum, old_off) {
        let _ = clear_dir_entry(m, new_parent_inode, new_name_raw);
        if has_dst {
            let _ = restore_dir_entry_at(m, dst_bnum, dst_off, dst_inode_num);
        }
        return -5;
    }
    if has_dst {
        let _ = free_inode_blocks(m, &dst_inode);
        let _ = free_inode(m, dst_inode_num);
    }
    0
}

unsafe fn ensure_inode_block(
    m: &FsMount,
    _inode_num: u32,
    inode: &mut [u8; 256],
    block_idx: usize,
) -> Option<u32> {
    let block_size = m.block_size as usize;
    if block_idx < 12 {
        let existing = inode_block(inode, block_idx);
        if existing != 0 {
            return Some(existing);
        }
        let new_block = allocate_block(m)?;
        if !write_u32(inode, 40 + block_idx * 4, new_block) {
            return None;
        }
        return Some(new_block);
    }

    let idx = block_idx - 12;
    let per = block_size / 4;
    if idx >= per {
        return None;
    }
    let mut indirect = inode_block(inode, 12);
    if indirect == 0 {
        indirect = allocate_block(m)?;
        if !write_u32(inode, 40 + 12 * 4, indirect) {
            return None;
        }
        let zero = READ_RANGE_IND.as_mut();
        zero.fill(0);
        if !write_fs_block(m, indirect, zero) {
            return None;
        }
    }
    if !read_fs_block(m, indirect, READ_RANGE_IND.as_mut()) {
        return None;
    }
    let table = READ_RANGE_IND.as_mut();
    let entry_off = idx * 4;
    let existing = read_u32(table, entry_off).unwrap_or(0);
    if existing != 0 {
        return Some(existing);
    }
    let new_block = allocate_block(m)?;
    if !write_u32(table, entry_off, new_block) {
        return None;
    }
    if !write_fs_block(m, indirect, table) {
        return None;
    }
    Some(new_block)
}

unsafe fn write_fs_block(m: &FsMount, block_num: u32, data: &[u8]) -> bool {
    let block_size = m.block_size as usize;
    if data.len() < block_size {
        return false;
    }
    let spb = m.sectors_per_block as usize;
    for i in 0..spb {
        let lba = block_num
            .saturating_mul(m.sectors_per_block)
            .saturating_add(i as u32);
        let src_off = i * 512;
        if !write_sector_disk(m.disk_id, lba, &data[src_off..src_off + 512]) {
            return false;
        }
    }
    block_cache_insert(m.disk_id, block_num, data, block_size);
    true
}

unsafe fn probe_ext2_drive(disk_id: u32) -> Option<FsMount> {
    let mut s2 = [0u8; 512];
    let mut s3 = [0u8; 512];
    if !read_sector_disk(disk_id, 2, &mut s2) || !read_sector_disk(disk_id, 3, &mut s3) {
        return None;
    }
    let mut sb = [0u8; 1024];
    sb[..512].copy_from_slice(&s2);
    sb[512..].copy_from_slice(&s3);
    if read_u16(&sb, 56)? != EXT2_MAGIC {
        return None;
    }
    let log_block_size = read_u32(&sb, 24)?;
    if log_block_size > 2 {
        return None;
    }
    let block_size = 1024u32.checked_shl(log_block_size)?;
    if block_size < 1024 || block_size % 512 != 0 {
        return None;
    }
    let inode_size = read_u16(&sb, 88).unwrap_or(128);
    let inodes_per_group = read_u32(&sb, 40)?;
    if inodes_per_group == 0 {
        return None;
    }
    let gdt_block = if block_size == 1024 { 2 } else { 1 };
    Some(FsMount {
        disk_id,
        block_size,
        sectors_per_block: block_size / 512,
        inode_size,
        inodes_per_group,
        gdt_block,
    })
}

unsafe fn read_inode(m: &FsMount, inode_num: u32, inode_out: &mut [u8; 256]) -> bool {
    if inode_num == 0 || m.inodes_per_group == 0 {
        return false;
    }
    let isz = m.inode_size as usize;
    if inode_cache_lookup(m.disk_id, inode_num, inode_out, isz) {
        return true;
    }
    let group = (inode_num - 1) / m.inodes_per_group;
    let index = (inode_num - 1) % m.inodes_per_group;

    let gdt_entry_off = (group as usize) * 32;
    let gdt_block_off = gdt_entry_off / (m.block_size as usize);
    let gdt_inner = gdt_entry_off % (m.block_size as usize);
    if !read_fs_block(
        m,
        m.gdt_block + gdt_block_off as u32,
        READ_INODE_GDT_BLK.as_mut(),
    ) {
        return false;
    }
    let inode_table = match read_u32(READ_INODE_GDT_BLK.as_ref(), gdt_inner + 8) {
        Some(v) => v,
        None => return false,
    };
    let inode_off = (index as usize) * (m.inode_size as usize);
    let blk = inode_off / (m.block_size as usize);
    let off = inode_off % (m.block_size as usize);
    if !read_fs_block(m, inode_table + blk as u32, READ_INODE_IBLK.as_mut()) {
        return false;
    }
    let iblk = READ_INODE_IBLK.as_ref();
    if off + isz > iblk.len() || isz > inode_out.len() {
        return false;
    }
    inode_out[..isz].copy_from_slice(&iblk[off..off + isz]);
    inode_cache_insert(m.disk_id, inode_num, inode_out, isz);
    true
}

#[inline]
fn inode_mode(inode: &[u8]) -> u16 {
    read_u16(inode, 0).unwrap_or(0)
}

#[inline]
fn inode_size(inode: &[u8]) -> u32 {
    read_u32(inode, 4).unwrap_or(0)
}

#[inline]
fn inode_block(inode: &[u8], idx: usize) -> u32 {
    read_u32(inode, 40 + idx * 4).unwrap_or(0)
}

#[inline]
fn is_dir(mode: u16) -> bool {
    (mode & 0xF000) == 0x4000
}

unsafe fn read_data_block_num(
    m: &FsMount,
    inode: &[u8],
    block_idx: usize,
    scratch: &mut [u8; 4096],
) -> Option<u32> {
    if block_idx < 12 {
        let n = inode_block(inode, block_idx);
        return if n == 0 { None } else { Some(n) };
    }
    let idx = block_idx - 12;
    let per = (m.block_size / 4) as usize;
    if idx >= per {
        return None;
    }
    let indirect = inode_block(inode, 12);
    if indirect == 0 {
        return None;
    }
    if !read_fs_block(m, indirect, scratch) {
        return None;
    }
    let n = read_u32(scratch, idx * 4)?;
    if n == 0 {
        None
    } else {
        Some(n)
    }
}

unsafe fn lookup_child(m: &FsMount, dir_inode_num: u32, name: &[u8]) -> Option<u32> {
    let mut inode = [0u8; 256];
    if !read_inode(m, dir_inode_num, &mut inode) || !is_dir(inode_mode(&inode)) {
        return None;
    }
    let dir_size = inode_size(&inode) as usize;
    let block_size = m.block_size as usize;
    let blocks = dir_size.div_ceil(block_size);
    for bi in 0..blocks {
        let bnum = read_data_block_num(m, &inode, bi, LOOKUP_IND.as_mut())?;
        if !read_fs_block(m, bnum, LOOKUP_BLK.as_mut()) {
            return None;
        }
        let blk = LOOKUP_BLK.as_ref();
        let mut off = 0usize;
        while off + 8 <= block_size {
            let ino = read_u32(blk, off)?;
            let rec_len = read_u16(blk, off + 4)? as usize;
            let nlen = *blk.get(off + 6)? as usize;
            if rec_len == 0 || off + rec_len > block_size {
                break;
            }
            if ino != 0 && nlen > 0 && off + 8 + nlen <= block_size {
                let nm = &blk[off + 8..off + 8 + nlen];
                if nm == name {
                    return Some(ino);
                }
            }
            off += rec_len;
        }
    }
    None
}

unsafe fn resolve_path_inode(m: &FsMount, path: &[u8]) -> Option<u32> {
    if let Some(inode) = path_cache_lookup(m.disk_id, path) {
        return Some(inode);
    }
    let mut cur = 2u32;
    let mut i = 0usize;
    while i < path.len() {
        while i < path.len() && path[i] == b'/' {
            i += 1;
        }
        if i >= path.len() {
            break;
        }
        let start = i;
        while i < path.len() && path[i] != b'/' {
            i += 1;
        }
        let seg = &path[start..i];
        if seg.is_empty() || seg == b"." || seg == b".." {
            continue;
        }
        cur = lookup_child(m, cur, seg)?;
    }
    path_cache_insert(m.disk_id, path, cur);
    Some(cur)
}

unsafe fn read_inode_range(
    m: &FsMount,
    inode_num: u32,
    offset: u64,
    dst: &mut [u8],
) -> Option<usize> {
    let mut inode = [0u8; 256];
    if !read_inode(m, inode_num, &mut inode) {
        return None;
    }
    let size = inode_size(&inode) as u64;
    if offset >= size {
        return Some(0);
    }
    let to_read = min(dst.len() as u64, size - offset) as usize;
    let block_size = m.block_size as usize;
    let mut done = 0usize;
    while done < to_read {
        let file_off = offset as usize + done;
        let bi = file_off / block_size;
        let boff = file_off % block_size;
        let n = min(block_size - boff, to_read - done);
        let Some(bnum) = read_data_block_num(m, &inode, bi, READ_RANGE_IND.as_mut()) else {
            dst[done..done + n].fill(0);
            done += n;
            continue;
        };
        if !read_fs_block(m, bnum, READ_RANGE_BLK.as_mut()) {
            return None;
        }
        let blk = READ_RANGE_BLK.as_ref();
        dst[done..done + n].copy_from_slice(&blk[boff..boff + n]);
        done += n;
    }
    Some(done)
}

unsafe fn zero_fill_inode_range(
    m: &FsMount,
    inode_num: u32,
    inode: &mut [u8; 256],
    start: u64,
    end: u64,
) -> Option<()> {
    if end <= start {
        return Some(());
    }
    let block_size = m.block_size as usize;
    let mut cursor = start as usize;
    let end_usize = end as usize;
    while cursor < end_usize {
        let bi = cursor / block_size;
        let boff = cursor % block_size;
        let n = min(block_size - boff, end_usize - cursor);
        let bnum = ensure_inode_block(m, inode_num, inode, bi)?;
        if !read_fs_block(m, bnum, READ_RANGE_BLK.as_mut()) {
            return None;
        }
        let blk = READ_RANGE_BLK.as_mut();
        blk[boff..boff + n].fill(0);
        if !write_fs_block(m, bnum, blk) {
            return None;
        }
        cursor += n;
    }
    Some(())
}

unsafe fn write_inode_range(m: &FsMount, inode_num: u32, offset: u64, src: &[u8]) -> Option<usize> {
    let mut inode = [0u8; 256];
    if !read_inode(m, inode_num, &mut inode) {
        return None;
    }
    let size = inode_size(&inode) as u64;
    let end = offset.checked_add(src.len() as u64)?;
    if offset > size {
        zero_fill_inode_range(m, inode_num, &mut inode, size, offset)?;
    }
    let new_size = core::cmp::max(size, end);
    let block_size = m.block_size as usize;
    let blocks_needed = new_size.div_ceil(block_size as u64) as usize;
    for bi in 0..blocks_needed {
        ensure_inode_block(m, inode_num, &mut inode, bi)?;
    }
    let mut done = 0usize;
    while done < src.len() {
        let file_off = offset as usize + done;
        let bi = file_off / block_size;
        let boff = file_off % block_size;
        let n = min(block_size - boff, src.len() - done);
        let Some(bnum) = read_data_block_num(m, &inode, bi, READ_RANGE_IND.as_mut()) else {
            return None;
        };
        if !read_fs_block(m, bnum, READ_RANGE_BLK.as_mut()) {
            return None;
        }
        let blk = READ_RANGE_BLK.as_mut();
        blk[boff..boff + n].copy_from_slice(&src[done..done + n]);
        if !write_fs_block(m, bnum, blk) {
            return None;
        }
        done += n;
    }
    if !write_u32(&mut inode, 4, new_size as u32) {
        return None;
    }
    if !write_inode_raw(m, inode_num, &inode) {
        return None;
    }
    Some(done)
}

unsafe fn read_path_inode(path: McxPath) -> Option<u32> {
    let m = MOUNT.as_ref()?;
    let raw = core::slice::from_raw_parts(path.ptr, path.len);
    let p = if !raw.is_empty() && raw[0] == b'/' {
        &raw[1..]
    } else {
        raw
    };
    resolve_path_inode(m, p)
}

fn dirent_ideal_len(name_len: usize) -> usize {
    let raw = 8 + name_len;
    (raw + 3) & !3
}

unsafe fn split_parent_name<'a>(raw: &'a [u8]) -> Option<(&'a [u8], &'a [u8])> {
    let raw = if !raw.is_empty() && raw[0] == b'/' {
        &raw[1..]
    } else {
        raw
    };
    let trimmed = raw.strip_suffix(b"/").unwrap_or(raw);
    if trimmed.is_empty() {
        return None;
    }
    let pos = trimmed.iter().rposition(|b| *b == b'/');
    let (parent, name) = match pos {
        Some(idx) => (&trimmed[..idx], &trimmed[idx + 1..]),
        None => (&[][..], trimmed),
    };
    if name.is_empty() || name == b"." || name == b".." {
        return None;
    }
    Some((parent, name))
}

unsafe fn allocate_dir_entry(
    m: &FsMount,
    parent_inode_num: u32,
    child_inode_num: u32,
    name: &[u8],
    file_type: u8,
) -> bool {
    let mut parent = [0u8; 256];
    if !read_inode(m, parent_inode_num, &mut parent) || !is_dir(inode_mode(&parent)) {
        return false;
    }
    let block_size = m.block_size as usize;
    let blocks = inode_size(&parent) as usize / block_size;
    let need = dirent_ideal_len(name.len());
    for bi in 0..blocks {
        let Some(bnum) = read_data_block_num(m, &parent, bi, LOOKUP_IND.as_mut()) else {
            continue;
        };
        if !read_fs_block(m, bnum, LOOKUP_BLK.as_mut()) {
            return false;
        }
        let blk = LOOKUP_BLK.as_mut();
        let mut off = 0usize;
        while off + 8 <= block_size {
            let inode = match read_u32(blk, off) {
                Some(v) => v,
                None => return false,
            };
            let rec_len = match read_u16(blk, off + 4) {
                Some(v) => v as usize,
                None => return false,
            };
            let name_len = match blk.get(off + 6) {
                Some(v) => *v as usize,
                None => return false,
            };
            if rec_len == 0 || off + rec_len > block_size {
                break;
            }
            let ideal = dirent_ideal_len(name_len);
            if rec_len.saturating_sub(ideal) >= need {
                let new_off = off + ideal;
                if !write_u32(blk, new_off, child_inode_num) {
                    return false;
                }
                if !write_u16(blk, new_off + 4, (rec_len - ideal) as u16) {
                    return false;
                }
                if let Some(slot) = blk.get_mut(new_off + 6) {
                    *slot = name.len() as u8;
                } else {
                    return false;
                }
                if let Some(slot) = blk.get_mut(new_off + 7) {
                    *slot = file_type;
                } else {
                    return false;
                }
                let name_dst = match blk.get_mut(new_off + 8..new_off + 8 + name.len()) {
                    Some(dst) => dst,
                    None => return false,
                };
                name_dst.copy_from_slice(name);
                if !write_u16(blk, off + 4, ideal as u16) {
                    return false;
                }
                if !write_fs_block(m, bnum, blk) {
                    return false;
                }
                return true;
            }
            off += rec_len;
            if inode == 0 {
                continue;
            }
        }
    }
    let bnum = match ensure_inode_block(m, parent_inode_num, &mut parent, blocks) {
        Some(v) => v,
        None => return false,
    };
    if !read_fs_block(m, bnum, LOOKUP_BLK.as_mut()) {
        return false;
    }
    let blk = LOOKUP_BLK.as_mut();
    blk.fill(0);
    if !write_u32(blk, 0, child_inode_num) {
        return false;
    }
    if !write_u16(blk, 4, block_size as u16) {
        return false;
    }
    if let Some(slot) = blk.get_mut(6) {
        *slot = name.len() as u8;
    } else {
        return false;
    }
    if let Some(slot) = blk.get_mut(7) {
        *slot = file_type;
    } else {
        return false;
    }
    let name_dst = match blk.get_mut(8..8 + name.len()) {
        Some(dst) => dst,
        None => return false,
    };
    name_dst.copy_from_slice(name);
    let new_size = ((blocks + 1) * block_size) as u32;
    if !write_u32(&mut parent, 4, new_size) {
        return false;
    }
    if !write_inode_raw(m, parent_inode_num, &parent) {
        return false;
    }
    write_fs_block(m, bnum, blk)
}

unsafe fn create_path(m: &FsMount, path: McxPath, mode: u32) -> i32 {
    let raw = core::slice::from_raw_parts(path.ptr, path.len);
    let (parent_raw, name_raw) = match split_parent_name(raw) {
        Some(v) => v,
        None => return -22,
    };
    let Some(parent_inode) = resolve_path_inode(
        m,
        if parent_raw.is_empty() {
            &[]
        } else {
            parent_raw
        },
    ) else {
        return -2;
    };
    let mut existing_parent = [0u8; 256];
    if !read_inode(m, parent_inode, &mut existing_parent) || !is_dir(inode_mode(&existing_parent)) {
        return -20;
    }
    if lookup_child(m, parent_inode, name_raw).is_some() {
        return -17;
    }
    let inode_num = match allocate_inode(m) {
        Some(v) => v,
        None => return -28,
    };
    let mut inode = [0u8; 256];
    let regular_mode = 0x8000u16 | (mode as u16 & 0o777);
    if !write_u16(&mut inode, 0, regular_mode) {
        return -5;
    }
    if !write_u16(&mut inode, 26, 1) {
        return -5;
    }
    if !write_u32(&mut inode, 4, 0) {
        return -5;
    }
    if !write_u32(&mut inode, 28, 0) {
        return -5;
    }
    if !write_inode_raw(m, inode_num, &inode) {
        let _ = bitmap_set_bit(
            m,
            read_group_desc(m, 0).map(|gd| gd.inode_bitmap).unwrap_or(0),
            (inode_num - 1) as usize,
            false,
        );
        return -5;
    }
    if !allocate_dir_entry(m, parent_inode, inode_num, name_raw, 1) {
        let _ = bitmap_set_bit(
            m,
            read_group_desc(m, 0).map(|gd| gd.inode_bitmap).unwrap_or(0),
            (inode_num - 1) as usize,
            false,
        );
        return -5;
    }
    0
}

unsafe fn truncate_path(m: &FsMount, path: McxPath, len: u64) -> i32 {
    let inode_num = match read_path_inode(path) {
        Some(v) => v,
        None => return -2,
    };
    let mut inode = [0u8; 256];
    if !read_inode(m, inode_num, &mut inode) {
        return -5;
    }
    if is_dir(inode_mode(&inode)) {
        return -21;
    }
    let old_size = inode_size(&inode) as u64;
    if len == old_size {
        return 0;
    }
    if len < old_size {
        if !write_u32(&mut inode, 4, len as u32) {
            return -5;
        }
        return if write_inode_raw(m, inode_num, &inode) {
            0
        } else {
            -5
        };
    }

    let Some(()) = zero_fill_inode_range(m, inode_num, &mut inode, old_size, len) else {
        return -5;
    };
    if !write_u32(&mut inode, 4, len as u32) {
        return -5;
    }
    if !write_inode_raw(m, inode_num, &inode) {
        return -5;
    }
    0
}

extern "C" fn fs_mount(_device_id: u32) -> i32 {
    let _guard = lock_ops();
    unsafe {
        if DISK_OPS_PTR.is_null() {
            return -5;
        }
        // rootfs は qemu-runner の disk0 (IDE index=1, primary slave) を優先。
        // 起動直後はデバイス準備に時間がかかるため複数回リトライする。
        for _ in 0..16 {
            if let Some(m) = probe_ext2_drive(1) {
                reset_caches();
                MOUNT = Some(m);
                return 0;
            }
            if let Some(m) = probe_ext2_drive(0) {
                reset_caches();
                MOUNT = Some(m);
                return 0;
            }
            for _ in 0..2_000_000 {
                core::hint::spin_loop();
            }
        }
    }
    -5
}

extern "C" fn fs_read(path: McxPath, offset: u64, buf: McxBuffer, out_read: *mut usize) -> i32 {
    if path.ptr.is_null() || buf.ptr.is_null() || out_read.is_null() {
        return -22;
    }
    let _guard = lock_ops();
    unsafe {
        let inode = match read_path_inode(path) {
            Some(v) => v,
            None => {
                return if MOUNT.is_some() { -2 } else { -5 };
            }
        };
        let m = match MOUNT.as_ref() {
            Some(v) => v,
            None => return -5,
        };
        let dst = core::slice::from_raw_parts_mut(buf.ptr, buf.len);
        match read_inode_range(m, inode, offset, dst) {
            Some(n) => {
                *out_read = n;
                0
            }
            None => -5,
        }
    }
}

extern "C" fn fs_write(path: McxPath, offset: u64, buf: McxBuffer, out_written: *mut usize) -> i32 {
    if path.ptr.is_null() || buf.ptr.is_null() || out_written.is_null() {
        return -22;
    }
    let _guard = lock_ops();
    unsafe {
        let inode = match read_path_inode(path) {
            Some(v) => v,
            None => {
                return if MOUNT.is_some() { -2 } else { -5 };
            }
        };
        let m = match MOUNT.as_ref() {
            Some(v) => v,
            None => return -5,
        };
        let mut meta = [0u8; 256];
        if !read_inode(m, inode, &mut meta) {
            return -5;
        }
        if is_dir(inode_mode(&meta)) {
            return -21;
        }
        let src = core::slice::from_raw_parts(buf.ptr, buf.len);
        match write_inode_range(m, inode, offset, src) {
            Some(n) => {
                *out_written = n;
                0
            }
            None => -5,
        }
    }
}

extern "C" fn fs_create(path: McxPath, mode: u32) -> i32 {
    if path.ptr.is_null() {
        return -22;
    }
    let _guard = lock_ops();
    unsafe {
        let m = match MOUNT.as_ref() {
            Some(v) => v,
            None => return -5,
        };
        create_path(m, path, mode)
    }
}

extern "C" fn fs_remove(path: McxPath, is_dir: u32) -> i32 {
    if path.ptr.is_null() {
        return -22;
    }
    let _guard = lock_ops();
    unsafe {
        let m = match MOUNT.as_ref() {
            Some(v) => v,
            None => return -5,
        };
        remove_path(m, path, is_dir != 0)
    }
}

extern "C" fn fs_rename(old_path: McxPath, new_path: McxPath) -> i32 {
    if old_path.ptr.is_null() || new_path.ptr.is_null() {
        return -22;
    }
    let _guard = lock_ops();
    unsafe {
        let m = match MOUNT.as_ref() {
            Some(v) => v,
            None => return -5,
        };
        rename_path(m, old_path, new_path)
    }
}

extern "C" fn fs_truncate(path: McxPath, len: u64) -> i32 {
    if path.ptr.is_null() {
        return -22;
    }
    let _guard = lock_ops();
    unsafe {
        let m = match MOUNT.as_ref() {
            Some(v) => v,
            None => return -5,
        };
        truncate_path(m, path, len)
    }
}

extern "C" fn fs_stat(path: McxPath, out_mode: *mut u16, out_size: *mut u64) -> i32 {
    if path.ptr.is_null() || out_mode.is_null() || out_size.is_null() {
        return -22;
    }
    let _guard = lock_ops();
    unsafe {
        let inode_num = match read_path_inode(path) {
            Some(v) => v,
            None => {
                return if MOUNT.is_some() { -2 } else { -5 };
            }
        };
        let m = match MOUNT.as_ref() {
            Some(v) => v,
            None => return -5,
        };
        let mut inode = [0u8; 256];
        if !read_inode(m, inode_num, &mut inode) {
            return -5;
        }
        *out_mode = inode_mode(&inode);
        *out_size = inode_size(&inode) as u64;
        0
    }
}

extern "C" fn fs_readdir(path: McxPath, buf: McxBuffer, out_len: *mut usize) -> i32 {
    if path.ptr.is_null() || buf.ptr.is_null() || out_len.is_null() {
        return -22;
    }
    let _guard = lock_ops();
    unsafe {
        let inode_num = match read_path_inode(path) {
            Some(v) => v,
            None => {
                return if MOUNT.is_some() { -2 } else { -5 };
            }
        };
        let m = match MOUNT.as_ref() {
            Some(v) => v,
            None => return -5,
        };
        let mut inode = [0u8; 256];
        if !read_inode(m, inode_num, &mut inode) {
            return -5;
        }
        if !is_dir(inode_mode(&inode)) {
            return -20;
        }

        let block_size = m.block_size as usize;
        let dir_size = inode_size(&inode) as usize;
        let mut written = 0usize;
        let out = core::slice::from_raw_parts_mut(buf.ptr, buf.len);
        let blocks = dir_size.div_ceil(block_size);
        for bi in 0..blocks {
            let bnum = match read_data_block_num(m, &inode, bi, READDIR_IND.as_mut()) {
                Some(v) => v,
                None => return -5,
            };
            if !read_fs_block(m, bnum, READDIR_BLK.as_mut()) {
                return -5;
            }
            let data_blk = READDIR_BLK.as_ref();
            let mut off = 0usize;
            while off + 8 <= block_size {
                let ino = match read_u32(data_blk, off) {
                    Some(v) => v,
                    None => break,
                };
                let rec_len = match read_u16(data_blk, off + 4) {
                    Some(v) => v as usize,
                    None => break,
                };
                let nlen = match data_blk.get(off + 6) {
                    Some(v) => *v as usize,
                    None => break,
                };
                if rec_len == 0 || off + rec_len > block_size {
                    break;
                }
                if ino != 0 && nlen > 0 && off + 8 + nlen <= block_size {
                    let nm = &data_blk[off + 8..off + 8 + nlen];
                    if nm != b"." && nm != b".." {
                        let need = nlen + if written == 0 { 0 } else { 1 };
                        if written + need > out.len() {
                            *out_len = written;
                            return 0;
                        }
                        if written != 0 {
                            out[written] = b'\n';
                            written += 1;
                        }
                        out[written..written + nlen].copy_from_slice(nm);
                        written += nlen;
                    }
                }
                off += rec_len;
            }
        }
        *out_len = written;
        0
    }
}

static FS_OPS: McxFsOps = McxFsOps {
    mount: fs_mount,
    set_disk_ops: fs_set_disk_ops,
    create: fs_create,
    remove: fs_remove,
    rename: fs_rename,
    read: fs_read,
    write: fs_write,
    truncate: fs_truncate,
    stat: fs_stat,
    readdir: fs_readdir,
};

#[no_mangle]
pub extern "C" fn mochi_module_init() -> *const McxFsOps {
    &FS_OPS
}

#[used]
static KEEP_INIT_REF: extern "C" fn() -> *const McxFsOps = mochi_module_init;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
