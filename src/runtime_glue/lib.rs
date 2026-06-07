#![no_std]

use core::alloc::{GlobalAlloc, Layout};

/// システムコールの共通インターフェース（Newlibグルー用）
pub mod sys;

/// 入出力関連のシステムコール（Newlibグルー用）
pub mod io;

/// ポートI/O関連のシステムコール（Newlibグルー用）
pub mod port;

/// Newlib サポート用のシステムコールグルーコード
pub mod newlib;

/// libc の薄いラッパ（NewlibAllocator 用）
pub mod libc;

/// Linux/POSIX 互換スタブ（std リンク用）
pub mod posix_stubs;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe {
        core::arch::asm!(
            "int 0x80",
            in("rax") sys::SyscallNumber::ExitGroup as u64,
            in("rdi") 1u64,
            options(nostack, noreturn)
        )
    }
}

struct NewlibAllocator;

unsafe impl GlobalAlloc for NewlibAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        libc::memalign(layout.align(), layout.size())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        libc::free(ptr);
    }

    unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
        libc::realloc(ptr, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: NewlibAllocator = NewlibAllocator;

