use core::sync::atomic::{AtomicU64, Ordering};

use crate::SmpHandoff;

static SMP_HANDOFF_ADDR: AtomicU64 = AtomicU64::new(0);

pub fn set_handoff_addr(addr: u64) {
    SMP_HANDOFF_ADDR.store(addr, Ordering::Release);
}

pub fn handoff() -> Option<&'static SmpHandoff> {
    let addr = SMP_HANDOFF_ADDR.load(Ordering::Acquire);
    if addr == 0 {
        None
    } else {
        Some(unsafe { &*(addr as *const SmpHandoff) })
    }
}
