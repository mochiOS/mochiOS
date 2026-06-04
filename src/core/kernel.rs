use crate::result::handle_kernel_error;
use crate::result::{Kernel, Process};
use crate::syscall::exec::exec_kernel_with_name_and_caps;
use crate::util::log::LogLevel;
use crate::{debug, info};
use crate::{init::kinit, task, util, BootInfo, MemoryRegion, Result};
use core::sync::atomic::Ordering;

const KERNEL_THREAD_STACK_SIZE: usize = 4096 * 8;

#[repr(align(16))]
struct KernelStack([u8; KERNEL_THREAD_STACK_SIZE]);

static mut KERNEL_THREAD_STACK: KernelStack = KernelStack([0; KERNEL_THREAD_STACK_SIZE]);

/// カーネルメイン関数
fn kernel_main() -> ! {
    util::log::set_level(LogLevel::Info);
    debug!("Kernel started");

    // core.serviceのみ起動（他のサービスはcore.serviceが管理）
    info!("Starting core.service");
    let mut caps = crate::capability::CapabilitySet::empty();
    caps.insert(crate::capability::Capability::IpcServer);
    caps.insert(crate::capability::Capability::IpcClient);
    caps.insert(crate::capability::Capability::ProcessSpawn);
    caps.insert(crate::capability::Capability::ServiceControl);
    caps.insert(crate::capability::Capability::SystemInfoRead);
    // core.service はサービス起動のため /system/services/*.manifest.toml を読む必要がある。
    // ブートストラップとして読み取り専用の全体権限を与える。
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

    if let Some(handoff) = crate::smp::handoff() {
        let kernel_cr3 = crate::percpu::kernel_cr3();
        let ap_count = handoff.ap_count.load(Ordering::Acquire);
        handoff.kernel_cr3.store(kernel_cr3, Ordering::Release);
        handoff.ready.store(1, Ordering::Release);
        crate::info!(
            "SMP handoff released secondary CPUs: kernel_cr3={:#x} ap_count={}",
            kernel_cr3,
            ap_count
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
    crate::cpu::init();
    crate::syscall::syscall_entry::init_syscall_current_cpu();
    if let Some(handoff) = crate::smp::handoff() {
        let before = handoff.ap_count.fetch_add(1, Ordering::SeqCst);
        crate::info!(
            "Secondary CPU online: ap_count {} -> {}",
            before,
            before + 1
        );
    }
    task::start_scheduling();
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
