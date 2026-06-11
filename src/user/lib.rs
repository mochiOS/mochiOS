#![cfg_attr(not(any(feature = "std-support", feature = "hosted-vga")), no_std)]

#[cfg(not(feature = "std-support"))]
extern crate alloc;
#[cfg(feature = "std-support")]
extern crate std as alloc;
#[cfg(feature = "std-support")]
extern crate std;

#[cfg(not(feature = "std-support"))]
use core::alloc::{GlobalAlloc, Layout};

/// システムコールの共通インターフェース
pub mod sys;

/// ipc関連のシステムコール
pub mod ipc;

/// タスク関連のシステムコール
pub mod task;

/// 時間関連のシステムコール
pub mod time;

/// 入出力関連のシステムコール
pub mod io;

/// プロセス管理関連のシステムコール
pub mod process;

/// ファイルシステム関連のシステムコール
pub mod fs;

/// ポートI/O関連のシステムコール
pub mod port;

/// Linux/POSIX 互換スタブ（主に std ベースのバイナリ向け）
pub mod posix_stubs;

/// フレームバッファアクセス
pub mod vga;
/// 描画ラッパー（mochiOS / Linux host 共通）
pub mod gfx;

/// キーボード入力
pub mod keyboard;
/// マウス入力
pub mod mouse;
/// 入力注入
pub mod input;
/// MMIO/物理メモリマップ
pub mod mmio;
/// ユーザー空間アドレス補助
pub mod user_space;

pub mod fs_consts;

/// 特権システムコール（Service権限専用）
pub mod privileged;

/// capability（権限）照会
pub mod capability;

/// ブロックデバイスI/O（高速パス）
pub mod block;

// ── no_std バイナリ向け最小ランタイム ────────────────────────────────

// NOTE:
// user/ は「syscall 周り」を目的にしているが、no_std のユーザー空間バイナリを
// 成り立たせるために panic_handler とグローバルアロケータだけはここに置く。
// （std ベースのバイナリでは不要なため std-support では無効化する）

#[cfg(not(feature = "std-support"))]
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

#[cfg(not(feature = "std-support"))]
struct NewlibAllocator;

#[cfg(not(feature = "std-support"))]
unsafe impl GlobalAlloc for NewlibAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        extern "C" {
            #[link_name = "memalign"]
            fn c_memalign(alignment: usize, size: usize) -> *mut u8;
        }
        c_memalign(layout.align(), layout.size())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        extern "C" {
            #[link_name = "free"]
            fn c_free(ptr: *mut u8);
        }
        c_free(ptr);
    }

    unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
        extern "C" {
            #[link_name = "realloc"]
            fn c_realloc(ptr: *mut u8, size: usize) -> *mut u8;
        }
        c_realloc(ptr, new_size)
    }
}

#[cfg(not(feature = "std-support"))]
#[global_allocator]
static ALLOCATOR: NewlibAllocator = NewlibAllocator;
