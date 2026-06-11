use core::arch::{asm, global_asm};
use core::ptr;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use spin::Once;
use x86_64::registers::model_specific::{ApicBase, ApicBaseFlags};
use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};

use crate::{BootInfo, SmpHandoff};

const AP_BOOT_STACK_SIZE: usize = 0x2000;
const MAX_SMP_STACKS: usize = crate::MAX_CPU_IDS;
const X2APIC_SIVR_MSR: u32 = 0x80F;
const X2APIC_ICR_MSR: u32 = 0x830;
const APIC_SIVR_ENABLE: u64 = 1 << 8;
const START_SECONDARY_CPUS: bool = false;

#[repr(align(16))]
struct ApBootStack([u8; AP_BOOT_STACK_SIZE]);

static mut AP_BOOT_STACKS: [ApBootStack; MAX_SMP_STACKS] =
    [const { ApBootStack([0; AP_BOOT_STACK_SIZE]) }; MAX_SMP_STACKS];

static BOOT_INFO_PTR: AtomicU64 = AtomicU64::new(0);
static SMP_HANDOFF_ADDR: AtomicU64 = AtomicU64::new(0);
static TRAMPOLINE_PHYS: AtomicU64 = AtomicU64::new(0);
static TRAMPOLINE_SIZE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct TrampolineLayout {
    size: usize,
    gdtr_load_off: usize,
    pm32_jump_off: usize,
    pm32_entry_off: usize,
    kernel_cr3_load_off: usize,
    lm64_jump_off: usize,
    lm64_entry_off: usize,
    kernel_cr3_off: usize,
    boot_info_ptr_off: usize,
    kernel_secondary_entry_off: usize,
    stack_top_off: usize,
    gdt_off: usize,
    gdtr_off: usize,
}

static TRAMPOLINE_LAYOUT: Once<TrampolineLayout> = Once::new();

global_asm!(
    r#"
    .section .data.ap_trampoline, "aw"
    .global __mochi_ap_trampoline_start
    .global __mochi_ap_trampoline_end
    .global __mochi_ap_trampoline_gdtr_load
    .global __mochi_ap_trampoline_pm32_jump
    .global __mochi_ap_trampoline_pm32_entry
    .global __mochi_ap_trampoline_kernel_cr3_load
    .global __mochi_ap_trampoline_lm64_jump
    .global __mochi_ap_trampoline_lm64_entry
    .global __mochi_ap_trampoline_kernel_cr3
    .global __mochi_ap_trampoline_boot_info_ptr
    .global __mochi_ap_trampoline_kernel_secondary_entry
    .global __mochi_ap_trampoline_stack_top
    .global __mochi_ap_trampoline_gdt
    .global __mochi_ap_trampoline_gdtr

__mochi_ap_trampoline_start:
    .code16
    cli
    mov ax, cs
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0xfff0
__mochi_ap_trampoline_gdtr_load:
    mov bx, 0x1234
    lgdt [bx]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
__mochi_ap_trampoline_pm32_jump:
    ljmp 0x08, 0x1234

    .code32
__mochi_ap_trampoline_pm32_entry:
__mochi_ap_trampoline_kernel_cr3_load:
    mov ebx, 0x12345678
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x0ff0
    mov eax, dword ptr [ebx]
    mov cr3, eax
    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr
    mov eax, cr0
    or eax, 1 << 31
    mov cr0, eax
__mochi_ap_trampoline_lm64_jump:
    ljmp 0x18, 0x12345678

    .code64
__mochi_ap_trampoline_lm64_entry:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    lea rbx, [rip + __mochi_ap_trampoline_stack_top]
    mov rsp, qword ptr [rbx]
    sub rsp, 8
    mov rdi, qword ptr [rbx - 16]
    mov rax, qword ptr [rbx - 8]
    jmp rax

    .align 8
__mochi_ap_trampoline_kernel_cr3:
    .quad 0
__mochi_ap_trampoline_boot_info_ptr:
    .quad 0
__mochi_ap_trampoline_kernel_secondary_entry:
    .quad 0
__mochi_ap_trampoline_stack_top:
    .quad 0
__mochi_ap_trampoline_gdt:
    .quad 0x0000000000000000
    .quad 0x00CF9A000000FFFF
    .quad 0x00CF92000000FFFF
    .quad 0x00AF9A000000FFFF
__mochi_ap_trampoline_gdtr:
    .word 0
    .long 0
__mochi_ap_trampoline_gdt_end:
__mochi_ap_trampoline_end:
"#
);

extern "C" {
    static __mochi_ap_trampoline_start: u8;
    static __mochi_ap_trampoline_end: u8;
    static __mochi_ap_trampoline_gdtr_load: u8;
    static __mochi_ap_trampoline_pm32_jump: u8;
    static __mochi_ap_trampoline_pm32_entry: u8;
    static __mochi_ap_trampoline_kernel_cr3_load: u8;
    static __mochi_ap_trampoline_lm64_jump: u8;
    static __mochi_ap_trampoline_lm64_entry: u8;
    static mut __mochi_ap_trampoline_kernel_cr3: u64;
    static mut __mochi_ap_trampoline_boot_info_ptr: u64;
    static mut __mochi_ap_trampoline_kernel_secondary_entry: u64;
    static mut __mochi_ap_trampoline_stack_top: u64;
    static mut __mochi_ap_trampoline_gdt: u8;
    static mut __mochi_ap_trampoline_gdtr: u8;
}

#[inline]
unsafe fn trampoline_start_ptr() -> *const u8 {
    core::ptr::addr_of!(__mochi_ap_trampoline_start)
}

#[inline]
unsafe fn trampoline_end_ptr() -> *const u8 {
    core::ptr::addr_of!(__mochi_ap_trampoline_end)
}

fn trampoline_layout() -> &'static TrampolineLayout {
    TRAMPOLINE_LAYOUT.call_once(|| unsafe {
        let start = trampoline_start_ptr() as usize;
        let end = trampoline_end_ptr() as usize;
        let size = end.saturating_sub(start);
        TrampolineLayout {
            size,
            gdtr_load_off: core::ptr::addr_of!(__mochi_ap_trampoline_gdtr_load) as usize - start,
            pm32_jump_off: core::ptr::addr_of!(__mochi_ap_trampoline_pm32_jump) as usize - start,
            pm32_entry_off: core::ptr::addr_of!(__mochi_ap_trampoline_pm32_entry) as usize - start,
            kernel_cr3_load_off: core::ptr::addr_of!(__mochi_ap_trampoline_kernel_cr3_load)
                as usize
                - start,
            lm64_jump_off: core::ptr::addr_of!(__mochi_ap_trampoline_lm64_jump) as usize - start,
            lm64_entry_off: core::ptr::addr_of!(__mochi_ap_trampoline_lm64_entry) as usize - start,
            kernel_cr3_off: core::ptr::addr_of!(__mochi_ap_trampoline_kernel_cr3) as usize - start,
            boot_info_ptr_off: core::ptr::addr_of!(__mochi_ap_trampoline_boot_info_ptr) as usize
                - start,
            kernel_secondary_entry_off: core::ptr::addr_of!(
                __mochi_ap_trampoline_kernel_secondary_entry
            ) as usize
                - start,
            stack_top_off: core::ptr::addr_of!(__mochi_ap_trampoline_stack_top) as usize - start,
            gdt_off: core::ptr::addr_of!(__mochi_ap_trampoline_gdt) as usize - start,
            gdtr_off: core::ptr::addr_of!(__mochi_ap_trampoline_gdtr) as usize - start,
        }
    })
}

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

pub fn set_boot_info_addr(addr: u64) {
    BOOT_INFO_PTR.store(addr, Ordering::Release);
}

pub fn boot_info() -> Option<&'static BootInfo> {
    let addr = BOOT_INFO_PTR.load(Ordering::Acquire);
    if addr == 0 {
        None
    } else {
        Some(unsafe { &*(addr as *const BootInfo) })
    }
}

fn cpu_has_x2apic() -> bool {
    let ecx: u32;
    unsafe {
        asm!(
            "xchg {rbx_tmp}, rbx",
            "cpuid",
            "xchg {rbx_tmp}, rbx",
            inout("eax") 1u32 => _,
            inout("ecx") 0u32 => ecx,
            rbx_tmp = inout(reg) 0u64 => _,
            out("edx") _,
            options(nomem, nostack)
        );
    }
    (ecx & (1 << 21)) != 0
}

#[inline]
unsafe fn read_msr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") lo,
        out("edx") hi,
        options(nomem, nostack)
    );
    ((hi as u64) << 32) | (lo as u64)
}

#[inline]
unsafe fn write_msr(msr: u32, val: u64) {
    let lo = val as u32;
    let hi = (val >> 32) as u32;
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") lo,
        in("edx") hi,
        options(nomem, nostack)
    );
}

#[inline]
unsafe fn x2apic_write(msr: u32, val: u64) {
    write_msr(msr, val);
}

#[inline]
unsafe fn x2apic_read(msr: u32) -> u64 {
    read_msr(msr)
}

#[inline]
unsafe fn xapic_mmio_base() -> Option<*mut u32> {
    let (frame, _) = ApicBase::read();
    let phys = frame.start_address().as_u64();
    let phys_off = crate::mem::paging::physical_memory_offset().unwrap_or(0);
    let virt = phys.checked_add(phys_off)?;
    Some(virt as *mut u32)
}

#[inline]
unsafe fn xapic_write(offset: usize, value: u32) -> Option<()> {
    let base = xapic_mmio_base()?;
    core::ptr::write_volatile(base.add(offset / 4), value);
    Some(())
}

#[inline]
unsafe fn xapic_read(offset: usize) -> Option<u32> {
    let base = xapic_mmio_base()?;
    Some(core::ptr::read_volatile(base.add(offset / 4)))
}

fn apic_mode_is_x2apic() -> bool {
    let (_, flags) = ApicBase::read();
    flags.contains(ApicBaseFlags::X2APIC_ENABLE)
}

pub fn init_local_apic() {
    let x2apic_supported = cpu_has_x2apic();
    let (frame, flags) = ApicBase::read();

    unsafe {
        if x2apic_supported {
            ApicBase::write(
                frame,
                flags | ApicBaseFlags::LAPIC_ENABLE | ApicBaseFlags::X2APIC_ENABLE,
            );
            x2apic_write(X2APIC_SIVR_MSR, APIC_SIVR_ENABLE | 0xff);
            crate::info!("Local APIC initialized in x2APIC mode");
        } else {
            ApicBase::write(frame, flags | ApicBaseFlags::LAPIC_ENABLE);
            if crate::mem::paging::physical_memory_offset().is_some() {
                let _ = xapic_write(0xF0, APIC_SIVR_ENABLE as u32 | 0xff);
                crate::info!("Local APIC initialized in xAPIC mode");
            } else {
                crate::warn!(
                    "xAPIC mode selected but physical offset is unavailable; deferring MMIO setup"
                );
            }
        }
    }
}

#[inline]
fn wait_for_icr_idle_x2apic() {
    loop {
        let icr = unsafe { x2apic_read(X2APIC_ICR_MSR) };
        if (icr & (1 << 12)) == 0 {
            break;
        }
        core::hint::spin_loop();
    }
}

#[inline]
fn wait_for_icr_idle_xapic() {
    loop {
        let icr = unsafe { xapic_read(0x300) };
        if let Some(icr) = icr {
            if (icr & (1 << 12)) == 0 {
                break;
            }
        }
        core::hint::spin_loop();
    }
}

fn apic_delay_ticks(ticks: u64) {
    let start = crate::interrupt::timer::get_ticks();
    while crate::interrupt::timer::get_ticks().saturating_sub(start) < ticks {
        core::hint::spin_loop();
    }
}

unsafe fn write_icr_x2apic(apic_id: u32, low: u32) {
    x2apic_write(X2APIC_ICR_MSR, ((apic_id as u64) << 32) | (low as u64));
}

unsafe fn write_icr_xapic(apic_id: u32, low: u32) {
    let _ = xapic_write(0x310, apic_id << 24);
    let _ = xapic_write(0x300, low);
}

fn startup_ipi_command(vector: u8) -> u32 {
    0x0000_0600 | u32::from(vector)
}

fn init_ipi_command() -> u32 {
    0x0000_4500
}

pub fn start_ap(apic_id: u32, vector: u8) -> bool {
    if apic_mode_is_x2apic() {
        unsafe {
            write_icr_x2apic(apic_id, init_ipi_command());
            wait_for_icr_idle_x2apic();
            apic_delay_ticks(10);
            write_icr_x2apic(apic_id, startup_ipi_command(vector));
            wait_for_icr_idle_x2apic();
            apic_delay_ticks(1);
            write_icr_x2apic(apic_id, startup_ipi_command(vector));
            wait_for_icr_idle_x2apic();
        }
        true
    } else {
        unsafe {
            write_icr_xapic(apic_id, init_ipi_command());
            wait_for_icr_idle_xapic();
            apic_delay_ticks(10);
            write_icr_xapic(apic_id, startup_ipi_command(vector));
            wait_for_icr_idle_xapic();
            apic_delay_ticks(1);
            write_icr_xapic(apic_id, startup_ipi_command(vector));
            wait_for_icr_idle_xapic();
        }
        true
    }
}

unsafe fn write_trampoline_field(base_virt: u64, offset: usize, value: u64) {
    let ptr = (base_virt + offset as u64) as *mut u64;
    ptr::write_unaligned(ptr, value);
}

unsafe fn patch_trampoline_u16(base_virt: u64, instr_offset: usize, value: u16) {
    let ptr = (base_virt + instr_offset as u64 + 1) as *mut u16;
    ptr::write_unaligned(ptr, value);
}

unsafe fn patch_trampoline_u32(base_virt: u64, instr_offset: usize, value: u32) {
    let ptr = (base_virt + instr_offset as u64 + 1) as *mut u32;
    ptr::write_unaligned(ptr, value);
}

unsafe fn patch_trampoline_u32_prefixed(base_virt: u64, instr_offset: usize, value: u32) {
    let ptr = (base_virt + instr_offset as u64 + 2) as *mut u32;
    ptr::write_unaligned(ptr, value);
}

unsafe fn patch_gdt_descriptor_base(base_virt: u64, descriptor_offset: usize, base: u32) {
    let desc = (base_virt + descriptor_offset as u64) as *mut u8;
    ptr::write(desc.add(2), (base & 0xFF) as u8);
    ptr::write(desc.add(3), ((base >> 8) & 0xFF) as u8);
    ptr::write(desc.add(4), ((base >> 16) & 0xFF) as u8);
    ptr::write(desc.add(7), ((base >> 24) & 0xFF) as u8);
}

unsafe fn patch_trampoline_u64(base_virt: u64, instr_offset: usize, value: u64) {
    let ptr = (base_virt + instr_offset as u64 + 2) as *mut u64;
    ptr::write_unaligned(ptr, value);
}

unsafe fn write_trampoline_gdtr(base_virt: u64, gdtr_offset: usize, gdt_phys: u64) {
    let gdtr_ptr = (base_virt + gdtr_offset as u64) as *mut u8;
    ptr::write_unaligned(gdtr_ptr as *mut u16, ((4 * 8) - 1) as u16);
    ptr::write_unaligned(gdtr_ptr.add(2) as *mut u32, gdt_phys as u32);
}

unsafe fn install_trampoline(boot_info: &'static BootInfo) -> Option<()> {
    let trampoline_phys = boot_info.smp_trampoline_addr;
    if trampoline_phys == 0 {
        crate::warn!("SMP trampoline address missing");
        return None;
    }
    let layout = trampoline_layout();
    if boot_info.smp_trampoline_size < layout.size {
        crate::warn!(
            "SMP trampoline too small: have={} need={}",
            boot_info.smp_trampoline_size,
            layout.size
        );
        return None;
    }

    let phys_off = crate::mem::paging::physical_memory_offset().unwrap_or(0);
    let template = core::slice::from_raw_parts(trampoline_start_ptr(), layout.size);
    let trampoline_virt = trampoline_phys + phys_off;
    let dst = trampoline_virt as *mut u8;
    ptr::copy_nonoverlapping(template.as_ptr(), dst, layout.size);

    let kernel_cr3 = handoff()?.kernel_cr3.load(Ordering::Acquire);
    let entry_ptr = handoff()?.kernel_secondary_entry.load(Ordering::Acquire);
    let boot_info_ptr = BOOT_INFO_PTR.load(Ordering::Acquire);
    if kernel_cr3 > u32::MAX as u64 {
        crate::warn!("kernel_cr3 too large for AP trampoline: {:#x}", kernel_cr3);
        return None;
    }
    if entry_ptr == 0 {
        crate::warn!("secondary CPU entry unavailable for AP trampoline");
        return None;
    }
    if boot_info_ptr == 0 {
        crate::warn!("boot_info pointer unavailable for AP trampoline");
        return None;
    }

    patch_trampoline_u16(
        trampoline_virt,
        layout.gdtr_load_off,
        layout.gdtr_off as u16,
    );
    patch_trampoline_u16(
        trampoline_virt,
        layout.pm32_jump_off,
        layout.pm32_entry_off as u16,
    );
    patch_trampoline_u32(
        trampoline_virt,
        layout.kernel_cr3_load_off,
        layout.kernel_cr3_off as u32,
    );
    patch_trampoline_u32(
        trampoline_virt,
        layout.lm64_jump_off,
        (trampoline_phys + layout.lm64_entry_off as u64) as u32,
    );
    write_trampoline_field(trampoline_virt, layout.kernel_cr3_off, kernel_cr3);
    write_trampoline_field(trampoline_virt, layout.boot_info_ptr_off, boot_info_ptr);
    write_trampoline_field(
        trampoline_virt,
        layout.kernel_secondary_entry_off,
        entry_ptr,
    );
    write_trampoline_gdtr(
        trampoline_virt,
        layout.gdtr_off,
        trampoline_phys + layout.gdt_off as u64,
    );
    patch_gdt_descriptor_base(trampoline_virt, layout.gdt_off + 8, trampoline_phys as u32);
    patch_gdt_descriptor_base(trampoline_virt, layout.gdt_off + 16, trampoline_phys as u32);
    TRAMPOLINE_PHYS.store(trampoline_phys, Ordering::Release);
    TRAMPOLINE_SIZE.store(layout.size, Ordering::Release);

    crate::info!(
        "SMP trampoline installed at {:#x} size={} kernel_cr3={:#x}",
        trampoline_phys,
        layout.size,
        kernel_cr3
    );
    Some(())
}

pub fn start_secondary_cpus() {
    if !START_SECONDARY_CPUS {
        crate::warn!("SMP startup temporarily disabled; booting single-core to avoid AP trampoline fault");
        return;
    }

    let boot_info = match boot_info() {
        Some(info) => info,
        None => {
            crate::warn!("boot_info unavailable; skipping AP startup");
            return;
        }
    };
    let handoff = match handoff() {
        Some(handoff) => handoff,
        None => {
            crate::warn!("SMP handoff unavailable; skipping AP startup");
            return;
        }
    };

    init_local_apic();

    if boot_info.cpu_enabled <= 1 {
        crate::info!(
            "single CPU detected (enabled={}); skipping AP startup",
            boot_info.cpu_enabled
        );
        return;
    }

    if unsafe { install_trampoline(boot_info) }.is_none() {
        crate::warn!("AP trampoline installation failed; skipping AP startup");
        return;
    }

    let trampoline_phys = TRAMPOLINE_PHYS.load(Ordering::Acquire);
    if trampoline_phys == 0 {
        crate::warn!("AP trampoline not installed");
        return;
    }
    let phys_off = crate::mem::paging::physical_memory_offset().unwrap_or(0);
    let trampoline_virt = trampoline_phys + phys_off;
    let trampoline_size = TRAMPOLINE_SIZE.load(Ordering::Acquire);
    let layout = trampoline_layout();
    let vector = (trampoline_phys >> 12) as u8;

    let trampoline_addr = VirtAddr::new(trampoline_phys);
    let identity_mapped = crate::mem::paging::translate_addr(trampoline_addr)
        .is_some_and(|phys| phys.as_u64() == trampoline_phys);
    if !identity_mapped {
        let trampoline_page = Page::<Size4KiB>::containing_address(trampoline_addr);
        let trampoline_frame =
            PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(trampoline_phys));
        if let Err(err) = crate::mem::paging::map_page(
            trampoline_page,
            trampoline_frame,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
        ) {
            crate::warn!(
                "Failed to identity-map AP trampoline page at {:#x}: {:?}",
                trampoline_phys,
                err
            );
            return;
        }
        crate::info!(
            "Identity-mapped AP trampoline page at {:#x}",
            trampoline_phys
        );
    } else {
        crate::info!(
            "AP trampoline page already identity-mapped at {:#x}",
            trampoline_phys
        );
    }

    let bsp_apic_id = boot_info.bsp_apic_id;
    let cpu_count = boot_info.cpu_apic_id_count.min(crate::MAX_CPU_IDS);
    let expected_ap_count = boot_info.cpu_enabled.saturating_sub(1);
    let mut started = 0usize;

    crate::info!(
        "Starting APIC IPIs: total={} enabled={} vector={:#x} trampoline_size={} expected_ap_count={}",
        boot_info.cpu_total,
        boot_info.cpu_enabled,
        vector,
        trampoline_size,
        expected_ap_count
    );

    for &apic_id in boot_info.cpu_apic_ids.iter().take(cpu_count) {
        if apic_id == bsp_apic_id {
            continue;
        }

        let stack_slot = apic_id as usize;
        if stack_slot >= MAX_SMP_STACKS {
            crate::warn!("APIC ID {} exceeds AP stack table; skipping", apic_id);
            continue;
        }
        let stack_top = unsafe {
            let stack = &AP_BOOT_STACKS[stack_slot];
            (stack as *const ApBootStack as u64) + AP_BOOT_STACK_SIZE as u64
        } & !0xFu64;

        let before = handoff.ap_count.load(Ordering::Acquire);
        crate::info!(
            "Starting APIC IPI for APIC ID {} (before ap_count={})",
            apic_id,
            before
        );

        unsafe {
            write_trampoline_field(trampoline_virt, layout.stack_top_off, stack_top);
        }
        if !start_ap(apic_id, vector) {
            crate::warn!("APIC IPI start failed for APIC ID {}", apic_id);
            continue;
        }

        let start_tick = crate::interrupt::timer::get_ticks();
        loop {
            let after = handoff.ap_count.load(Ordering::Acquire);
            if after > before {
                crate::info!(
                    "APIC ID {} came online (ap_count {} -> {})",
                    apic_id,
                    before,
                    after
                );
                started += 1;
                break;
            }
            if crate::interrupt::timer::get_ticks().saturating_sub(start_tick) > 250 {
                crate::warn!(
                    "APIC ID {} startup timed out (ap_count stayed at {})",
                    apic_id,
                    before
                );
                break;
            }
            core::hint::spin_loop();
        }
    }

    crate::info!(
        "APIC startup finished: started={} expected={}",
        started,
        expected_ap_count
    );
}
