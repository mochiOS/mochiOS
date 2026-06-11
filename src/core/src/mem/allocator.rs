use core::{
    alloc::{GlobalAlloc, Layout},
    mem::{align_of, size_of},
    ptr::{self, NonNull},
    sync::atomic::{AtomicU64, Ordering},
};
use linked_list_allocator::LockedHeap;
use spin::Mutex;
use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

/// 仮想アドレス空間のどこからヒープを開始するか
pub const HEAP_START: usize = 0x_4444_4444_0000;
/// ヒープのサイズ
pub const HEAP_SIZE: usize = 32 * 1024 * 1024; // 32 MiB
const HEAP_QUARANTINE_CAP: usize = 64;
const HEAP_HEADER_MAGIC: u64 = 0x9d7d_5f1b_1b7d_2b41;
const HEAP_TAIL_MAGIC: u64 = 0xc6c4_19b8_4d7f_53a9;
const HEAP_HEADER_ALIGNMENT: usize = 16;

#[derive(Clone, Copy)]
struct QuarantinedBlock {
    user_ptr: *mut u8,
    user_layout: Layout,
    raw_ptr: *mut u8,
    raw_layout: Layout,
}

unsafe impl Send for QuarantinedBlock {}

#[repr(C, align(16))]
struct HeapHeader {
    magic: u64,
    cookie: u64,
    raw_ptr: u64,
    raw_size: usize,
    raw_align: usize,
    user_size: usize,
    user_align: usize,
    checksum: u64,
}

impl HeapHeader {
    fn checksum(&self, cookie: u64) -> u64 {
        self.magic
            ^ self.raw_ptr
            ^ (self.raw_size as u64)
            ^ (self.raw_align as u64)
            ^ (self.user_size as u64)
            ^ (self.user_align as u64)
            ^ cookie
            ^ HEAP_HEADER_MAGIC
    }

    fn is_valid(&self) -> bool {
        self.magic == HEAP_HEADER_MAGIC && self.checksum(self.cookie) == self.checksum
    }
}

const _: () = {
    assert!(size_of::<HeapHeader>() % HEAP_HEADER_ALIGNMENT == 0);
    assert!(align_of::<HeapHeader>() == HEAP_HEADER_ALIGNMENT);
};

struct HeapQuarantine {
    blocks: [Option<QuarantinedBlock>; HEAP_QUARANTINE_CAP],
    head: usize,
    len: usize,
}

unsafe impl Send for HeapQuarantine {}

impl HeapQuarantine {
    const fn new() -> Self {
        Self {
            blocks: [None; HEAP_QUARANTINE_CAP],
            head: 0,
            len: 0,
        }
    }

    fn contains(&self, ptr: *mut u8) -> bool {
        self.blocks
            .iter()
            .flatten()
            .any(|block| block.user_ptr == ptr)
    }

    fn push(&mut self, block: QuarantinedBlock) -> Option<QuarantinedBlock> {
        if self.len < HEAP_QUARANTINE_CAP {
            let index = (self.head + self.len) % HEAP_QUARANTINE_CAP;
            self.blocks[index] = Some(block);
            self.len += 1;
            None
        } else {
            let evicted = self.blocks[self.head].take();
            self.blocks[self.head] = Some(block);
            self.head = (self.head + 1) % HEAP_QUARANTINE_CAP;
            evicted
        }
    }

    fn pop_oldest(&mut self) -> Option<QuarantinedBlock> {
        if self.len == 0 {
            return None;
        }
        let block = self.blocks[self.head].take();
        self.head = (self.head + 1) % HEAP_QUARANTINE_CAP;
        self.len -= 1;
        block
    }
}

/// カーネルヒープのラッパー
///
/// 内部の `LockedHeap` は従来どおり first-fit のまま使い、
/// 解放済みブロックは固定長 quarantine に一時退避させる。
pub struct HardenedKernelHeap {
    inner: LockedHeap,
    quarantine: Mutex<HeapQuarantine>,
    cookie_seed: AtomicU64,
}

unsafe impl Sync for HardenedKernelHeap {}

impl HardenedKernelHeap {
    pub const fn empty() -> Self {
        Self {
            inner: LockedHeap::empty(),
            quarantine: Mutex::new(HeapQuarantine::new()),
            cookie_seed: AtomicU64::new(0),
        }
    }

    pub unsafe fn init(&mut self, heap_bottom: *mut u8, heap_size: usize) {
        self.inner = LockedHeap::new(heap_bottom, heap_size);
    }

    fn release_block(&self, block: QuarantinedBlock) {
        unsafe {
            GlobalAlloc::dealloc(&self.inner, block.raw_ptr, block.raw_layout);
        }
    }

    fn release_one_quarantine_block(&self) -> Option<QuarantinedBlock> {
        self.quarantine.lock().pop_oldest()
    }
}

unsafe impl GlobalAlloc for HardenedKernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return NonNull::<u8>::dangling().as_ptr();
        }

        let user_align = layout.align().max(HEAP_HEADER_ALIGNMENT);
        let header_size = size_of::<HeapHeader>();
        let trailer_size = size_of::<u64>();
        let payload_size = match layout
            .size()
            .checked_add(header_size)
            .and_then(|v| v.checked_add(trailer_size))
            .and_then(|v| v.checked_add(user_align.saturating_sub(1)))
        {
            Some(v) => v,
            None => return ptr::null_mut(),
        };
        let raw_layout = match Layout::from_size_align(payload_size, user_align) {
            Ok(v) => v,
            Err(_) => return ptr::null_mut(),
        };

        for _ in 0..=HEAP_QUARANTINE_CAP {
            let raw_ptr = GlobalAlloc::alloc(&self.inner, raw_layout);
            if !raw_ptr.is_null() {
                let cookie = self.cookie_seed();
                let user_ptr = align_user_ptr(raw_ptr, header_size, user_align);
                let header_ptr = user_ptr.sub(header_size) as *mut HeapHeader;
                let header = HeapHeader {
                    magic: HEAP_HEADER_MAGIC,
                    cookie,
                    raw_ptr: raw_ptr as u64,
                    raw_size: raw_layout.size(),
                    raw_align: raw_layout.align(),
                    user_size: layout.size(),
                    user_align: layout.align(),
                    checksum: 0,
                };
                let mut header = header;
                header.checksum = header.checksum(cookie);
                ptr::write(header_ptr, header);
                ptr::write_unaligned(
                    user_ptr.add(layout.size()) as *mut u64,
                    cookie ^ HEAP_TAIL_MAGIC,
                );
                return user_ptr;
            }

            let Some(block) = self.release_one_quarantine_block() else {
                break;
            };
            self.release_block(block);
        }

        ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() || layout.size() == 0 {
            return;
        }

        let Some(header) = self.read_header(ptr) else {
            crate::warn!(
                "heap dealloc rejected: missing or corrupted header at {:#x}",
                ptr as u64
            );
            crate::audit::log(
                crate::audit::AuditEventKind::Memory,
                "heap header missing or corrupted",
            );
            return;
        };
        if !header.is_valid() {
            crate::warn!(
                "heap dealloc rejected: header checksum mismatch at {:#x}",
                ptr as u64
            );
            crate::audit::log(
                crate::audit::AuditEventKind::Memory,
                "heap header checksum mismatch",
            );
            return;
        }
        let tail_ptr = ptr.add(header.user_size) as *const u64;
        let expected_tail = header.cookie ^ HEAP_TAIL_MAGIC;
        let tail_ok = unsafe { tail_ptr.read_unaligned() == expected_tail };
        if !tail_ok {
            crate::warn!("heap tail canary mismatch at {:#x}", ptr as u64);
            crate::audit::log(
                crate::audit::AuditEventKind::Memory,
                "heap tail canary mismatch",
            );
        }
        let raw_ptr = header.raw_ptr as *mut u8;
        let raw_layout = match Layout::from_size_align(header.raw_size, header.raw_align) {
            Ok(v) => v,
            Err(_) => {
                crate::warn!(
                    "heap dealloc rejected: invalid raw layout at {:#x}",
                    ptr as u64
                );
                return;
            }
        };
        let block = QuarantinedBlock {
            user_ptr: ptr,
            user_layout: Layout::from_size_align(header.user_size, header.user_align)
                .unwrap_or(layout),
            raw_ptr,
            raw_layout,
        };
        let evicted = {
            let mut quarantine = self.quarantine.lock();
            if quarantine.contains(ptr) {
                crate::warn!("heap double free detected at {:#x}", ptr as u64);
                crate::audit::log(
                    crate::audit::AuditEventKind::Quarantine,
                    "heap double free detected",
                );
                return;
            }

            ptr::write_bytes(ptr, 0xDD, header.user_size);
            quarantine.push(block)
        };

        if let Some(evicted) = evicted {
            self.release_block(evicted);
        }
    }
}

impl HardenedKernelHeap {
    fn cookie_seed(&self) -> u64 {
        let cached = self.cookie_seed.load(Ordering::Relaxed);
        if cached != 0 {
            return cached;
        }

        let mut seed = crate::cpu::boot_entropy_u64()
            ^ (self as *const Self as u64).rotate_left(17)
            ^ HEAP_TAIL_MAGIC;
        if seed == 0 {
            seed = HEAP_TAIL_MAGIC;
        }
        match self
            .cookie_seed
            .compare_exchange(0, seed, Ordering::SeqCst, Ordering::Relaxed)
        {
            Ok(_) => seed,
            Err(v) => v,
        }
    }

    fn read_header(&self, user_ptr: *mut u8) -> Option<HeapHeader> {
        let header_ptr =
            user_ptr.cast::<u8>().wrapping_sub(size_of::<HeapHeader>()) as *const HeapHeader;
        let header = unsafe { ptr::read(header_ptr) };
        if header.magic != HEAP_HEADER_MAGIC {
            return None;
        }
        Some(header)
    }
}

fn align_user_ptr(raw_ptr: *mut u8, header_size: usize, user_align: usize) -> *mut u8 {
    let base = raw_ptr as usize + header_size;
    let aligned = (base + user_align - 1) & !(user_align - 1);
    aligned as *mut u8
}

/// ヒープを初期化
///
/// ## Arguments
/// - `mapper`: 仮想アドレスと物理アドレスのマッピングを管理するオブジェクト
/// - `frame_allocator`: 物理フレームの割り当てを管理するオブジェクト
/// - `heap_allocator_ptr`: ヒープアロケータのロックされたヒープへのポインタ
///
/// ## Returns
/// - `Ok(())` ヒープの初期化に成功した場合
/// - `Err(MapToError<Size4KiB>)` マッピングのエラーが発生した場合
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    heap_allocator_ptr: u64,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    // ヒープの仮想アドレス空間を物理フレームにマッピング
    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        // カーネルヒープは実行不可（W^X: NO_EXECUTE でコード実行を防ぐ）
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    // ヒープアロケータを初期化
    unsafe {
        let allocator = &mut *(heap_allocator_ptr as *mut HardenedKernelHeap);
        allocator.init(HEAP_START as *mut u8, HEAP_SIZE);
    }

    Ok(())
}

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    crate::warn!("allocation error: {:?}", layout);
    // スケジューラが動作中でカレントスレッドがあれば、そのプロセスを終了して回復を試みる
    if crate::task::scheduler::is_scheduler_enabled() && crate::task::current_thread_id().is_some()
    {
        crate::warn!("OOM: terminating current process to recover");
        crate::task::scheduler::exit_current_process(-1);
    }
    // 回復不能: 割り込みを無効化してシステムを停止
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::instructions::interrupts::disable();
    }
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}
