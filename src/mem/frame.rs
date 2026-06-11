//! 物理フレームアロケータ
//!
//! 4KBページ単位で物理メモリを管理

use crate::{
    result::{Kernel, Memory, Result},
    MemoryRegion, MemoryType,
};
use spin::Mutex;
use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
    PhysAddr,
};

/// グローバルフレームアロケータ
pub static FRAME_ALLOCATOR: Mutex<Option<BitmapFrameAllocator>> = Mutex::new(None);

/// ビットマップベースのフレームアロケータ
///
/// 解放済みフレームはフレーム自身の先頭8バイトにリンクリストのnextポインタを
/// 埋め込むことで上限なしに再利用できる。
pub struct BitmapFrameAllocator {
    /// メモリマップ
    memory_map: &'static [MemoryRegion],
    /// バンプアロケータの次フレームインデックス
    next_frame: usize,
    /// 解放済みフレームのフリーリスト先頭（物理アドレス、0 = 空）
    free_list_head: u64,
    /// 解放済みフレームの短期退避領域
    quarantine: [u64; FRAME_QUARANTINE_CAP],
    quarantine_head: usize,
    quarantine_len: usize,
    free_cookie_seed: u64,
    /// HHDM オフセット（phys → virt 変換用）
    phys_offset: u64,
}

const FRAME_QUARANTINE_CAP: usize = 32;
const FRAME_FREE_COOKIE_CONST: u64 = 0x8f1d_3b79_2c4a_6e15;

impl BitmapFrameAllocator {
    /// 新しいフレームアロケータを作成
    pub fn new(memory_map: &'static [MemoryRegion], phys_offset: u64) -> Self {
        let seed = crate::cpu::boot_entropy_u64()
            ^ (memory_map.as_ptr() as u64).rotate_left(17)
            ^ (memory_map.len() as u64).rotate_left(41)
            ^ FRAME_FREE_COOKIE_CONST;
        Self {
            memory_map,
            next_frame: 0x100000 / 4096, // 1MB から開始（低位メモリ予約領域をスキップ）
            free_list_head: 0,
            quarantine: [0; FRAME_QUARANTINE_CAP],
            quarantine_head: 0,
            quarantine_len: 0,
            free_cookie_seed: seed,
            phys_offset,
        }
    }

    fn free_cookie(&self, phys_addr: u64) -> u64 {
        let mut value = self.free_cookie_seed ^ phys_addr ^ FRAME_FREE_COOKIE_CONST;
        value ^= value >> 33;
        value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
        value ^= value >> 33;
        value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        value ^ (value >> 33)
    }

    fn frame_meta_ptr(&self, phys_addr: u64) -> Option<*mut u64> {
        if self.phys_offset == 0 {
            return None;
        }
        Some((phys_addr + self.phys_offset) as *mut u64)
    }

    fn read_frame_meta(&self, phys_addr: u64) -> Option<(u64, u64)> {
        let ptr = self.frame_meta_ptr(phys_addr)?;
        unsafe { Some((*ptr, *ptr.add(1))) }
    }

    fn write_frame_meta(&self, phys_addr: u64, next: u64, cookie: u64) -> bool {
        let Some(ptr) = self.frame_meta_ptr(phys_addr) else {
            return false;
        };
        unsafe {
            *ptr = next;
            *ptr.add(1) = cookie;
        }
        true
    }

    fn clear_frame_meta(&self, phys_addr: u64) {
        if let Some(ptr) = self.frame_meta_ptr(phys_addr) {
            unsafe {
                *ptr = 0;
                *ptr.add(1) = 0;
            }
        }
    }

    fn push_free_list(&mut self, phys_addr: u64) -> bool {
        let cookie = self.free_cookie(phys_addr);
        self.write_frame_meta(phys_addr, self.free_list_head, cookie);
        self.free_list_head = phys_addr;
        true
    }

    fn quarantine_full(&self) -> bool {
        self.quarantine_len >= FRAME_QUARANTINE_CAP
    }

    fn quarantine_push(&mut self, phys_addr: u64) {
        if FRAME_QUARANTINE_CAP == 0 {
            self.push_free_list(phys_addr);
            return;
        }

        if self.quarantine_full() {
            self.release_quarantine_oldest();
        }

        let insert_at = (self.quarantine_head + self.quarantine_len) % FRAME_QUARANTINE_CAP;
        self.quarantine[insert_at] = phys_addr;
        self.quarantine_len += 1;
        let cookie = self.free_cookie(phys_addr);
        let _ = self.write_frame_meta(phys_addr, 0, cookie);
    }

    fn release_quarantine_oldest(&mut self) {
        if self.quarantine_len == 0 {
            return;
        }

        let phys_addr = self.quarantine[self.quarantine_head];
        self.quarantine[self.quarantine_head] = 0;
        self.quarantine_head = (self.quarantine_head + 1) % FRAME_QUARANTINE_CAP;
        self.quarantine_len -= 1;
        self.push_free_list(phys_addr);
    }

    fn is_usable_frame_addr(&self, phys_addr: u64) -> bool {
        self.memory_map.iter().any(|r| {
            r.region_type == MemoryType::Usable
                && phys_addr >= r.start
                && phys_addr < r.start + r.len
        })
    }

    pub fn deallocate_frame(&mut self, frame: PhysFrame) -> bool {
        let phys_addr = frame.start_address().as_u64();
        if phys_addr & 0xfff != 0 || !self.is_usable_frame_addr(phys_addr) {
            crate::audit::log(
                crate::audit::AuditEventKind::Memory,
                "frame deallocation rejected",
            );
            return false;
        }
        if self.phys_offset == 0 {
            return false;
        }
        let expected_cookie = self.free_cookie(phys_addr);
        if let Some((_, cookie)) = self.read_frame_meta(phys_addr) {
            if cookie == expected_cookie {
                crate::audit::log(
                    crate::audit::AuditEventKind::Quarantine,
                    "frame double free detected",
                );
                return false;
            }
        }
        self.quarantine_push(phys_addr);
        true
    }

    /// 使用可能な物理メモリの総量を計算（バイト）
    pub fn usable_memory(&self) -> u64 {
        self.memory_map
            .iter()
            .filter(|r| r.region_type == MemoryType::Usable)
            .map(|r| r.len)
            .sum()
    }

    /// 使用可能なフレーム数を計算
    pub fn usable_frames(&self) -> usize {
        (self.usable_memory() / 4096) as usize
    }

    fn usable_frames_iter(&self) -> impl Iterator<Item = PhysFrame> + '_ {
        self.memory_map
            .iter()
            .filter(|r| r.region_type == MemoryType::Usable)
            .flat_map(|r| {
                let start_addr = r.start;
                let end_addr = r.start + r.len;
                let start_frame = start_addr / 4096;
                let end_frame = end_addr / 4096;
                (start_frame..end_frame)
                    .map(|f| PhysFrame::containing_address(PhysAddr::new(f * 4096)))
            })
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        if self.free_list_head == 0 && self.quarantine_len != 0 {
            self.release_quarantine_oldest();
        }

        // フリーリストから再利用
        let mut attempts = 0;
        while self.free_list_head != 0 && attempts < 128 {
            attempts += 1;
            let phys = self.free_list_head;
            if phys & 0xfff != 0 || !self.is_usable_frame_addr(phys) {
                crate::warn!("frame allocator free list corruption at {:#x}", phys);
                crate::audit::log(
                    crate::audit::AuditEventKind::Fault,
                    "frame free list corruption",
                );
                self.free_list_head = 0;
                break;
            }
            let Some((next, cookie)) = self.read_frame_meta(phys) else {
                break;
            };
            if cookie != self.free_cookie(phys) {
                crate::warn!("frame allocator cookie mismatch at {:#x}", phys);
                crate::audit::log(
                    crate::audit::AuditEventKind::Memory,
                    "frame cookie mismatch",
                );
                self.free_list_head = 0;
                break;
            }
            if next != 0 && next & 0xfff == 0 && self.is_usable_frame_addr(next) {
                self.free_list_head = next;
            } else {
                self.free_list_head = 0;
            }
            self.clear_frame_meta(phys);
            return Some(PhysFrame::containing_address(PhysAddr::new(phys)));
        }

        // バンプアロケータから新規割り当て
        let mut f = self.next_frame as u64;
        let max_frame = self
            .memory_map
            .iter()
            .map(|r| (r.start + r.len) / 4096)
            .max()
            .unwrap_or(0);

        while f <= max_frame {
            let phys_addr = f * 4096;
            let mut usable = false;

            for r in self.memory_map.iter() {
                if r.region_type != MemoryType::Usable {
                    continue;
                }
                if phys_addr >= r.start && phys_addr < r.start + r.len {
                    usable = true;
                    break;
                }
            }

            if usable {
                self.next_frame = (f + 1) as usize;
                return Some(PhysFrame::containing_address(PhysAddr::new(phys_addr)));
            }
            f += 1;
        }
        None
    }
}

/// フレームアロケータを初期化
pub fn init(memory_map: &'static [MemoryRegion]) {
    let allocator = BitmapFrameAllocator::new(memory_map, 0);
    *FRAME_ALLOCATOR.lock() = Some(allocator);
}

/// ページングが初期化された後に HHDM オフセットをセット
pub fn set_phys_offset(offset: u64) {
    if let Some(alloc) = FRAME_ALLOCATOR.lock().as_mut() {
        alloc.phys_offset = offset;
    }
}

/// フレームを割り当て
pub fn allocate_frame() -> Result<PhysFrame> {
    FRAME_ALLOCATOR
        .lock()
        .as_mut()
        .and_then(|a| a.allocate_frame())
        .ok_or(Kernel::Memory(Memory::OutOfMemory))
}

/// フレームを解放
pub fn deallocate_frame(frame: PhysFrame) -> Result<()> {
    let mut guard = FRAME_ALLOCATOR.lock();
    let allocator = guard.as_mut().ok_or(Kernel::Memory(Memory::OutOfMemory))?;
    if allocator.deallocate_frame(frame) {
        Ok(())
    } else {
        Err(Kernel::Memory(Memory::InvalidAddress))
    }
}

/// 使用可能なメモリ情報を取得
pub fn get_memory_info() -> Option<(u64, usize)> {
    FRAME_ALLOCATOR
        .lock()
        .as_ref()
        .map(|a| (a.usable_memory(), a.usable_frames()))
}

/// 指定した物理アドレスがアロケータ管理対象の Usable フレームか判定
pub fn is_usable_physical_address(phys_addr: u64) -> bool {
    let guard = FRAME_ALLOCATOR.lock();
    let Some(alloc) = guard.as_ref() else {
        return false;
    };
    alloc.is_usable_frame_addr(phys_addr)
}

// ACPI reclaimable / bootloader reclaimable は将来通常RAMとして回収されうるため、
// 既定では MMIO 許可対象から除外する（必要なら明示設定で有効化する）。
const ALLOW_RECLAIMABLE_MMIO: bool = false;

fn mmio_region_type_allowed(region_type: MemoryType) -> bool {
    match region_type {
        MemoryType::BadMemory => {
            // 不良メモリへの MMIO マップは常に拒否する。
            false
        }
        MemoryType::Reserved | MemoryType::AcpiNvs | MemoryType::Framebuffer => true,
        MemoryType::AcpiReclaimable | MemoryType::BootloaderReclaimable => ALLOW_RECLAIMABLE_MMIO,
        _ => false,
    }
}

/// MMIO として扱ってよい物理アドレス範囲か判定
///
/// 許可条件:
/// - 範囲が非Usable領域 (Reserved/AcpiNvs/Framebuffer) に完全に含まれる、または
/// - 範囲が UEFI メモリマップのどの領域とも重ならない (= 高位 PCI MMIO ホール)
///
/// 拒否条件:
/// - Usable RAM やカーネル所有領域と少しでも重なる
pub fn is_allowed_mmio_range(start_phys: u64, size: u64) -> bool {
    if size == 0 {
        return false;
    }
    let end_phys = match start_phys.checked_add(size - 1) {
        Some(v) => v,
        None => return false,
    };

    let guard = FRAME_ALLOCATOR.lock();
    let Some(alloc) = guard.as_ref() else {
        return false;
    };

    let mut overlaps_any = false;
    let mut fully_contained_in_allowed = false;
    for r in alloc.memory_map.iter() {
        if r.len == 0 {
            continue;
        }
        let region_end = match r.start.checked_add(r.len - 1) {
            Some(v) => v,
            None => continue,
        };
        let overlaps = start_phys <= region_end && end_phys >= r.start;
        if !overlaps {
            continue;
        }
        overlaps_any = true;
        if !mmio_region_type_allowed(r.region_type) {
            return false;
        }
        if start_phys >= r.start && end_phys <= region_end {
            fully_contained_in_allowed = true;
        }
    }

    if fully_contained_in_allowed {
        return true;
    }
    // メモリマップに記述のない領域 = PCI MMIO ホール (高位 BAR など) は許可
    !overlaps_any
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmio_region_type_policy_allowed_cases() {
        for t in [
            MemoryType::Reserved,
            MemoryType::AcpiNvs,
            MemoryType::Framebuffer,
        ] {
            assert!(mmio_region_type_allowed(t));
        }
    }

    #[test]
    fn mmio_region_type_policy_denied_cases() {
        for t in [
            MemoryType::Usable,
            MemoryType::AcpiReclaimable,
            MemoryType::BootloaderReclaimable,
            MemoryType::BadMemory,
            MemoryType::KernelStack,
            MemoryType::PageTable,
        ] {
            assert!(!mmio_region_type_allowed(t));
        }
    }
}
