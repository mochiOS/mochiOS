//! Event syscall support.
//!
//! Minimal event objects used for wait/signal/poll style synchronization.

extern crate alloc;

use crate::interrupt::spinlock::SpinLock;
use crate::task::ThreadId;
use alloc::vec::Vec;

use super::{EAGAIN, EFAULT, EINVAL, ENOSYS, SUCCESS};

const MAX_EVENTS: usize = 64;
const MAX_WAITERS: usize = crate::task::ThreadQueue::MAX_THREADS;

#[derive(Clone, Copy)]
struct Event {
    in_use: bool,
    signaled: bool,
    waiter: u64,
}

impl Event {
    const fn empty() -> Self {
        Self {
            in_use: false,
            signaled: false,
            waiter: 0,
        }
    }
}

static EVENTS: SpinLock<[Event; MAX_EVENTS]> = SpinLock::new([Event::empty(); MAX_EVENTS]);
static EVENT_WAIT_LIST: SpinLock<[Option<(u64, u64)>; MAX_WAITERS]> =
    SpinLock::new([None; MAX_WAITERS]);

fn valid_event_id(event_id: u64) -> Option<usize> {
    let idx = usize::try_from(event_id).ok()?;
    if idx < MAX_EVENTS {
        Some(idx)
    } else {
        None
    }
}

fn register_waiter(tid: u64, event_id: u64) -> bool {
    let mut waiters = EVENT_WAIT_LIST.lock();
    for slot in waiters.iter_mut() {
        if slot.is_none() {
            *slot = Some((tid, event_id));
            return true;
        }
    }
    false
}

fn remove_waiter(tid: u64, event_id: u64) {
    let mut waiters = EVENT_WAIT_LIST.lock();
    for slot in waiters.iter_mut() {
        if slot.is_some_and(|(wtid, weid)| wtid == tid && weid == event_id) {
            *slot = None;
        }
    }
}

fn wake_waiters_for_event(event_id: u64) {
    let mut wake_list = [0u64; MAX_WAITERS];
    let mut count = 0usize;
    {
        let mut waiters = EVENT_WAIT_LIST.lock();
        for slot in waiters.iter_mut() {
            if let Some((tid, eid)) = *slot {
                if eid == event_id {
                    if count < wake_list.len() {
                        wake_list[count] = tid;
                        count += 1;
                    }
                    *slot = None;
                }
            }
        }
    }
    for tid in wake_list.iter().take(count) {
        crate::task::wake_thread(ThreadId::from_u64(*tid));
    }
}

/// Create a new event and return its handle.
pub fn create(_flags: u64, _reserved: u64) -> u64 {
    let mut events = EVENTS.lock();
    for (idx, event) in events.iter_mut().enumerate() {
        if !event.in_use {
            *event = Event {
                in_use: true,
                signaled: false,
                waiter: 0,
            };
            return idx as u64;
        }
    }
    ENOSYS
}

/// Wait for a single event.
///
/// `timeout_ms == 0` means non-blocking, `u64::MAX` means infinite.
pub fn wait(event_id: u64, timeout_ms: u64, _reserved: u64) -> u64 {
    let Some(idx) = valid_event_id(event_id) else {
        return EINVAL;
    };

    let current = match crate::task::current_thread_id() {
        Some(id) => id.as_u64(),
        None => return EINVAL,
    };

    let mut events = EVENTS.lock();
    let event = &mut events[idx];
    if !event.in_use {
        return EINVAL;
    }
    if event.signaled {
        event.signaled = false;
        return SUCCESS;
    }
    if timeout_ms == 0 {
        return EAGAIN;
    }

    event.waiter = current;
    drop(events);
    if !register_waiter(current, event_id) {
        return ENOSYS;
    }

    if timeout_ms != u64::MAX {
        let deadline = crate::syscall::time::get_ticks()
            .saturating_add(crate::interrupt::timer::ms_to_ticks_ceil(timeout_ms));
        while crate::syscall::time::get_ticks() < deadline {
            if signal_state(event_id) {
                remove_waiter(current, event_id);
                return SUCCESS;
            }
            crate::task::yield_now();
        }
        remove_waiter(current, event_id);
        return EAGAIN;
    }

    loop {
        if signal_state(event_id) {
            remove_waiter(current, event_id);
            return SUCCESS;
        }
        if crate::task::sleep_thread_unless_woken(crate::task::ThreadId::from_u64(current)) {
            crate::task::yield_now();
        } else {
            remove_waiter(current, event_id);
            return EAGAIN;
        }
    }
}

fn signal_state(event_id: u64) -> bool {
    let Some(idx) = valid_event_id(event_id) else {
        return false;
    };
    let mut events = EVENTS.lock();
    let event = &mut events[idx];
    if !event.in_use {
        return false;
    }
    if event.signaled {
        true
    } else {
        false
    }
}

/// Signal an event.
pub fn signal(event_id: u64, _reserved: u64, _reserved2: u64) -> u64 {
    let Some(idx) = valid_event_id(event_id) else {
        return EINVAL;
    };
    let mut events = EVENTS.lock();
    let event = &mut events[idx];
    if !event.in_use {
        return EINVAL;
    }
    event.signaled = true;
    let waiter = event.waiter;
    event.waiter = 0;
    drop(events);
    if waiter != 0 {
        crate::task::wake_thread(ThreadId::from_u64(waiter));
    }
    wake_waiters_for_event(event_id);
    SUCCESS
}

/// Poll a set of event IDs.
///
/// `event_ids_ptr` points to an array of `u64`.
pub fn poll(event_ids_ptr: u64, count: u64, timeout_ms: u64) -> u64 {
    if count == 0 {
        return EINVAL;
    }
    let count_usize = match usize::try_from(count) {
        Ok(v) => v,
        Err(_) => return EINVAL,
    };
    if !crate::syscall::validate_user_ptr(event_ids_ptr, count.saturating_mul(8)) {
        return EFAULT;
    }
    let mut event_ids = Vec::with_capacity(count_usize);
    event_ids.resize(count_usize, 0);
    for i in 0..count_usize {
        event_ids[i] = match crate::syscall::read_user_u64(event_ids_ptr + (i as u64 * 8)) {
            Ok(v) => v,
            Err(e) => return e,
        };
    }
    for event_id in &event_ids {
        if signal_state(*event_id) {
            return *event_id;
        }
    }
    if timeout_ms == 0 {
        return EAGAIN;
    }
    let current = match crate::task::current_thread_id() {
        Some(id) => id.as_u64(),
        None => return EINVAL,
    };
    for event_id in &event_ids {
        let _ = register_waiter(current, *event_id);
    }
    if timeout_ms != u64::MAX {
        let deadline = crate::syscall::time::get_ticks()
            .saturating_add(crate::interrupt::timer::ms_to_ticks_ceil(timeout_ms));
        while crate::syscall::time::get_ticks() < deadline {
            for event_id in &event_ids {
                if signal_state(*event_id) {
                    remove_waiter(current, *event_id);
                    return *event_id;
                }
            }
            crate::task::yield_now();
        }
        return EAGAIN;
    }
    loop {
        for event_id in &event_ids {
            if signal_state(*event_id) {
                remove_waiter(current, *event_id);
                return *event_id;
            }
        }
        if crate::task::sleep_thread_unless_woken(crate::task::ThreadId::from_u64(current)) {
            crate::task::yield_now();
        } else {
            return EAGAIN;
        }
    }
}
