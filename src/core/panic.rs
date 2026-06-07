//! パニックハンドラ
//!
//! カーネル内部では Result ベースのエラー処理を優先し、ここは最終退避先のみを担う。

use crate::warn;

#[allow(deprecated)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    crate::info!("!!! KERNEL PANIC !!!");

    if let Some(loc) = info.location() {
        warn!("Location: {}:{}:{}", loc.file(), loc.line(), loc.column());
    }

    if let Some(msg) = info.message().as_str() {
        warn!("Message: {}", msg);
    } else if let Some(s) = info.payload().downcast_ref::<&str>() {
        warn!("Message: {}", s);
    }

    warn!("system halted. Please reset.");

    // 割り込みを無効化
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::instructions::interrupts::disable();
    }

    // システムを停止
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}
