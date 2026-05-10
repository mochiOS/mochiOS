use crate::net_common::*;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{compiler_fence, Ordering as AtomicOrdering};
use swiftlib::{privileged, task};

pub fn align_up(value: usize, align: usize) -> usize {
    if align == 0 {
        return value;
    }
    (value + (align - 1)) & !(align - 1)
}

pub fn compute_virtqueue_bytes(queue_size: usize) -> usize {
    let desc_bytes = 16usize.saturating_mul(queue_size);
    let avail_bytes = 6usize.saturating_add(2usize.saturating_mul(queue_size));
    let used_bytes = 6usize.saturating_add(8usize.saturating_mul(queue_size));
    let used_off = align_up(desc_bytes.saturating_add(avail_bytes), PAGE_SIZE);
    used_off.saturating_add(used_bytes)
}

pub fn is_syscall_error(value: u64) -> bool {
    (-4095..=-1).contains(&(value as i64))
}

#[derive(Clone, Copy)]
pub struct SharedBuf {
    pub virt: *mut u8,
    pub phys: u64,
    pub len: u32,
}

pub fn alloc_shared_buf(len: u32) -> Option<SharedBuf> {
    let page_count = align_up(len as usize, PAGE_SIZE) / PAGE_SIZE;
    let mut phys_addrs = vec![0u64; page_count];
    let virt = unsafe { privileged::alloc_shared_pages(page_count as u64, Some(&mut phys_addrs), 0) };
    if is_syscall_error(virt) {
        println!("[NETDRV] alloc_shared_pages(buf) failed: errno={}", virt as i64);
        return None;
    }

    if phys_addrs.is_empty() {
        return None;
    }

    Some(SharedBuf {
        virt: virt as *mut u8,
        phys: phys_addrs[0],
        len,
    })
}

pub fn alloc_phys_contiguous(bytes: usize) -> Option<(u64, *mut u8)> {
    #[derive(Clone, Copy)]
    struct PageAlloc {
        virt: u64,
        phys: u64,
    }

    let page_count = align_up(bytes, PAGE_SIZE) / PAGE_SIZE;
    let required_run = page_count;
    let max_probe_pages = 64usize;
    let mut pool: Vec<PageAlloc> = Vec::new();

    for _ in 0..max_probe_pages {
        let mut phys_buf = [0u64; 1];
        let virt = unsafe { privileged::alloc_shared_pages(1, Some(&mut phys_buf), 0) };
        if is_syscall_error(virt) {
            println!("[NETDRV] alloc_shared_pages failed: errno={}", virt as i64);
            break;
        }
        pool.push(PageAlloc {
            virt,
            phys: phys_buf[0],
        });

        if pool.len() < required_run {
            continue;
        }

        let mut phys_sorted: Vec<u64> = pool.iter().map(|p| p.phys).collect();
        phys_sorted.sort_unstable();
        phys_sorted.dedup();

        for start_idx in 0..=phys_sorted.len().saturating_sub(required_run) {
            let start_phys = phys_sorted[start_idx];
            let mut contiguous = true;
            for step in 1..required_run {
                let expected = start_phys + (step as u64 * PAGE_SIZE as u64);
                if phys_sorted[start_idx + step] != expected {
                    contiguous = false;
                    break;
                }
            }
            if !contiguous {
                continue;
            }

            let selected_phys: Vec<u64> = (0..required_run)
                .map(|step| start_phys + (step as u64 * PAGE_SIZE as u64))
                .collect();

            let queue_virt = unsafe {
                privileged::map_physical_pages(task::gettid(), &selected_phys, 0)
            };
            if is_syscall_error(queue_virt) {
                println!(
                    "[NETDRV] map_physical_pages failed: errno={}",
                    queue_virt as i64
                );
                continue;
            }

            for page in &pool {
                let is_selected = selected_phys.iter().any(|&p| p == page.phys);
                let rc = privileged::unmap_pages(page.virt, 1, !is_selected);
                if rc != 0 {
                    println!(
                        "[NETDRV] unmap_pages failed virt={:#x} rc={}",
                        page.virt, rc as i64
                    );
                }
            }

            return Some((start_phys, queue_virt as *mut u8));
        }
    }

    for page in &pool {
        let rc = privileged::unmap_pages(page.virt, 1, true);
        if rc != 0 {
            println!(
                "[NETDRV] unmap_pages cleanup failed virt={:#x} rc={}",
                page.virt, rc as i64
            );
        }
    }

    None
}

pub fn write_be16(dst: &mut [u8], value: u16) {
    dst[0] = (value >> 8) as u8;
    dst[1] = value as u8;
}

pub fn checksum16(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0usize;
    while i + 1 < data.len() {
        sum = sum.wrapping_add(u16::from_be_bytes([data[i], data[i + 1]]) as u32);
        i += 2;
    }
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}
