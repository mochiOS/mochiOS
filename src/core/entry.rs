#![no_std]
#![no_main]

//! カーネルスタンドアローンバイナリのエントリポイント
//!
//! ブートローダーは sysv64 呼び出し規約で kernel_entry(boot_info_ptr) を呼ぶ。
//! ここで自前の LockedHeap アロケータを設定してから swiftcore のカーネル本体へ移譲する。

extern crate alloc;

use linked_list_allocator::LockedHeap;

/// カーネルのグローバルアロケータ
/// mem::init 内の init_heap がこの LockedHeap を初期化する
#[global_allocator]
static KERNEL_ALLOCATOR: LockedHeap = LockedHeap::empty();

/// ELF エントリポイント
///
/// ブートローダーが構築した BootInfo の kernel_heap_addr フィールドを
/// 自分の KERNEL_ALLOCATOR のアドレスで上書きしてから kernel_entry を呼ぶ。
/// これにより swiftcore の init_heap が正しいアロケータを初期化できる。
#[no_mangle]
pub unsafe extern "sysv64" fn kernel_entry(boot_info_ptr: *mut swiftcore::BootInfo) -> ! {
    // kernel_heap_addr = &KERNEL_ALLOCATOR（init_heap がここを初期化する）
    (*boot_info_ptr).kernel_heap_addr =
        &KERNEL_ALLOCATOR as *const LockedHeap as u64;

    let boot_info: &'static swiftcore::BootInfo = &*(boot_info_ptr as *const _);
    swiftcore::kernel_entry(boot_info)
}
