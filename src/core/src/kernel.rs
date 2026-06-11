use crate::result::handle_kernel_error;
use crate::result::{Kernel, Process};
use crate::syscall::exec::exec_kernel_with_name_and_caps;
use crate::util::log::LogLevel;
use crate::{debug, info};
use crate::{init::kinit, task, util, BootInfo, MemoryRegion, Result};
use core::sync::atomic::Ordering;
use core::sync::atomic::{AtomicU64, AtomicUsize};

const KERNEL_THREAD_STACK_SIZE: usize = 4096 * 8;
static KERNEL_PROCESS_ID_RAW: AtomicU64 = AtomicU64::new(0);
static AP_IDLE_THREAD_SEQ: AtomicUsize = AtomicUsize::new(0);

#[repr(align(16))]
struct KernelStack([u8; KERNEL_THREAD_STACK_SIZE]);

static mut KERNEL_THREAD_STACK: KernelStack = KernelStack([0; KERNEL_THREAD_STACK_SIZE]);

fn kernel_process_id() -> Option<task::ProcessId> {
    let raw = KERNEL_PROCESS_ID_RAW.load(Ordering::Acquire);
    (raw != 0).then(|| task::ProcessId::from_u64(raw))
}

fn ap_idle_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

fn spawn_ap_idle_thread() -> Result<(task::ThreadId, usize)> {
    let kernel_pid = kernel_process_id().ok_or(Kernel::Process(Process::ProcessNotFound))?;
    let kernel_stack = task::allocate_kernel_stack(KERNEL_THREAD_STACK_SIZE)
        .ok_or(Kernel::Memory(crate::result::Memory::OutOfMemory))?;
    let seq = AP_IDLE_THREAD_SEQ.fetch_add(1, Ordering::AcqRel) + 1;
    let name = alloc::format!("ap-idle-{}", seq);
    let thread = task::Thread::new(
        kernel_pid,
        &name,
        ap_idle_loop,
        kernel_stack,
        KERNEL_THREAD_STACK_SIZE,
    );
    let Some(thread_id) = task::add_thread(thread) else {
        task::free_kernel_stack(kernel_stack);
        return Err(Kernel::Process(Process::MaxProcessesReached));
    };
    let slot =
        task::thread_slot_index(thread_id).ok_or(Kernel::Process(Process::ProcessNotFound))?;
    Ok((thread_id, slot))
}

/// カーネルメイン関数
fn kernel_main() -> ! {
    util::log::set_level(LogLevel::Info);
    debug!("Kernel started");

    if let Some(handoff) = crate::smp::handoff() {
        let kernel_cr3 = crate::percpu::kernel_cr3();
        let secondary_entry = secondary_cpu_entry as *const () as usize as u64;
        let ap_count = handoff.ap_count.load(Ordering::Acquire);
        handoff.kernel_cr3.store(kernel_cr3, Ordering::Release);
        handoff
            .kernel_secondary_entry
            .store(secondary_entry, Ordering::Release);
        handoff.ready.store(1, Ordering::Release);
        info!(
            "SMP handoff released secondary CPUs: kernel_cr3={:#x} ap_count={}",
            kernel_cr3, ap_count
        );
    }

    crate::smp::start_secondary_cpus();

    // core.serviceのみ起動（他のサービスはcore.serviceが管理）
    info!("Starting core.service");
    let mut caps = crate::capability::CapabilitySet::empty();
    caps.insert(crate::capability::Capability::IpcServer);
    caps.insert(crate::capability::Capability::IpcClient);
    caps.insert(crate::capability::Capability::ProcessSpawn);
    caps.insert(crate::capability::Capability::ServiceControl);
    caps.insert(crate::capability::Capability::SystemInfoRead);
    caps.insert(crate::capability::Capability::FsReadAll);
    let manager_pid = exec_kernel_with_name_and_caps("core.service", "core.service", caps);
    if manager_pid != 0
        && task::with_process(task::ProcessId::from_u64(manager_pid), |_| ()).is_some()
    {
        crate::syscall::exec::register_service_manager_pid(manager_pid);
    } else {
        crate::warn!(
            "Failed to register core.service as service manager (ret={:#x})",
            manager_pid
        );
    }

    // カーネルはアイドル状態に入る
    info!("Kernel initialization complete. Entering idle loop...");
    loop {
        x86_64::instructions::hlt();
    }
}

/// カーネルエントリポイント（kernel binary から呼ばれる）
pub fn kernel_entry(boot_info: &'static BootInfo) -> ! {
    crate::smp::set_handoff_addr(boot_info.smp_handoff_addr);
    let memory_map = match kinit(boot_info) {
        Ok(map) => map,
        Err(e) => {
            handle_kernel_error(e);
            halt_forever();
        }
    };

    create_kernel_proc(boot_info, memory_map).unwrap_or_else(|e| {
        handle_kernel_error(e);
        halt_forever();
    });
    task::start_scheduling();
}

#[unsafe(no_mangle)]
pub extern "sysv64" fn secondary_cpu_entry(boot_info: *const BootInfo) -> ! {
    let Some(boot_info) = (unsafe { boot_info.as_ref() }) else {
        halt_forever();
    };
    crate::smp::set_handoff_addr(boot_info.smp_handoff_addr);
    crate::info!(
        "Secondary CPU entering kernel: boot_info={:#x} handoff={:#x}",
        boot_info as *const BootInfo as u64,
        boot_info.smp_handoff_addr
    );
    crate::mem::gdt::init();
    crate::info!("Secondary CPU GDT/TSS initialized");
    crate::interrupt::init_idt();
    crate::info!("Secondary CPU IDT initialized");
    crate::cpu::init();
    crate::info!("Secondary CPU CPU features initialized");
    crate::syscall::syscall_entry::init_syscall_current_cpu();
    crate::info!("Secondary CPU syscall state initialized");
    if let Some(handoff) = crate::smp::handoff() {
        let before = handoff.ap_count.fetch_add(1, Ordering::SeqCst);
        crate::info!(
            "Secondary CPU online: ap_count {} -> {}",
            before,
            before + 1
        );
    }
    let (idle_thread_id, idle_thread_slot) = match spawn_ap_idle_thread() {
        Ok(v) => v,
        Err(err) => {
            crate::warn!("Failed to create AP idle thread: {:?}", err);
            halt_forever();
        }
    };
    crate::info!(
        "Secondary CPU switching to idle thread {:?} (slot={})",
        idle_thread_id,
        idle_thread_slot
    );
    task::set_thread_state(idle_thread_id, task::ThreadState::Running);
    unsafe {
        crate::task::context::switch_to_thread_with_slots(
            None,
            None,
            idle_thread_id,
            idle_thread_slot,
        );
    }
    crate::warn!("Secondary CPU idle thread switch returned unexpectedly");
    halt_forever();
}

#[used]
#[unsafe(no_mangle)]
pub static SECONDARY_CPU_ENTRY: unsafe extern "sysv64" fn(*const BootInfo) -> ! =
    secondary_cpu_entry;

/// カーネルメインプロセスの作成
fn create_kernel_proc(
    boot_info: &'static BootInfo,
    memory_map: &'static [MemoryRegion],
) -> Result<()> {
    let kernel_process = task::Process::new("kernel", task::PrivilegeLevel::Core, None, 0);
    let kernel_pid = kernel_process.id();
    KERNEL_PROCESS_ID_RAW.store(kernel_pid.as_u64(), Ordering::Release);

    if task::add_process(kernel_process).is_none() {
        return Err(Kernel::Process(Process::MaxProcessesReached));
    }

    let stack_addr = unsafe { (&raw const KERNEL_THREAD_STACK as *const u8) as u64 };
    let kernel_thread = task::Thread::new(
        kernel_pid,
        "core",
        kernel_main,
        stack_addr,
        KERNEL_THREAD_STACK_SIZE,
    );

    if task::add_thread(kernel_thread).is_none() {
        return Err(Kernel::Process(Process::MaxProcessesReached));
    }

    Ok(())
}

/// システムを無限ループで停止
fn halt_forever() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
