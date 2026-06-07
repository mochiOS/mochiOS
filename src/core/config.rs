use alloc::string::String;
use core::convert::TryFrom;
use spin::Once;

#[derive(Clone, Copy)]
pub struct SchedulerConfig {
    pub default_time_slice_ms: u64,
    pub interactive_min_ms: u64,
    pub interactive_mid_ms: u64,
    pub interactive_max_ms: u64,
    pub normal_ms: u64,
    pub cpu_bound_ms: u64,
    pub background_ms: u64,
    pub low_priority_threshold: u8,
    pub cpu_bias_priority_threshold: u8,
    pub interactive_priority_threshold: u8,
}

impl SchedulerConfig {
    pub fn default_time_slice_ticks(self) -> u64 {
        crate::interrupt::timer::ms_to_ticks_ceil(self.default_time_slice_ms)
    }

    pub fn interactive_min_ticks(self) -> u64 {
        crate::interrupt::timer::ms_to_ticks_ceil(self.interactive_min_ms)
    }

    pub fn interactive_mid_ticks(self) -> u64 {
        crate::interrupt::timer::ms_to_ticks_ceil(self.interactive_mid_ms)
    }

    pub fn interactive_max_ticks(self) -> u64 {
        crate::interrupt::timer::ms_to_ticks_ceil(self.interactive_max_ms)
    }

    pub fn normal_ticks(self) -> u64 {
        crate::interrupt::timer::ms_to_ticks_ceil(self.normal_ms)
    }

    pub fn cpu_bound_ticks(self) -> u64 {
        crate::interrupt::timer::ms_to_ticks_ceil(self.cpu_bound_ms)
    }

    pub fn background_ticks(self) -> u64 {
        crate::interrupt::timer::ms_to_ticks_ceil(self.background_ms)
    }
}

#[derive(Clone, Copy)]
pub struct IpcConfig {
    pub mailbox_cap: usize,
    pub max_msg_size: usize,
    pub max_external_pages: usize,
}

#[derive(Clone, Copy)]
pub struct FsConfig {
    pub service_retry_count: usize,
    pub service_retry_ms: u64,
}

#[derive(Clone, Copy)]
pub struct BlockConfig {
    pub max_sectors_per_call: u64,
}

#[derive(Clone, Copy)]
pub struct IoConfig {
    pub max_iov: u64,
}

#[derive(Clone, Copy)]
pub struct CapabilityConfig {
    pub max_name_len: usize,
}

#[derive(Clone, Copy)]
pub struct ExecConfig {
    pub stack_top_base: u64,
    pub stack_aslr_max_pages: u64,
    pub user_stack_size_pages: usize,
    pub tls_base_min: u64,
    pub tls_aslr_max_pages: u64,
    pub initial_tls_size: u64,
    pub brk_heap_base_min: u64,
    pub brk_heap_aslr_max_pages: u64,
    pub mmap_heap_base_min: u64,
    pub mmap_heap_aslr_max_pages: u64,
    pub kernel_thread_stack_size: usize,
}

#[derive(Clone, Copy)]
pub struct KmodConfig {
    pub module_load_base_start: u64,
    pub module_load_guard: u64,
    pub max_read_bytes: usize,
}

#[derive(Clone, Copy)]
pub struct KernelConfig {
    pub scheduler: SchedulerConfig,
    pub ipc: IpcConfig,
    pub fs: FsConfig,
    pub block: BlockConfig,
    pub io: IoConfig,
    pub capability: CapabilityConfig,
    pub exec: ExecConfig,
    pub kmod: KmodConfig,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            scheduler: SchedulerConfig {
                default_time_slice_ms: 10,
                interactive_min_ms: 4,
                interactive_mid_ms: 6,
                interactive_max_ms: 8,
                normal_ms: 10,
                cpu_bound_ms: 20,
                background_ms: 30,
                low_priority_threshold: 192,
                cpu_bias_priority_threshold: 128,
                interactive_priority_threshold: 31,
            },
            ipc: IpcConfig {
                mailbox_cap: 64,
                max_msg_size: 4128,
                max_external_pages: 128,
            },
            fs: FsConfig {
                service_retry_count: 3,
                service_retry_ms: 10,
            },
            block: BlockConfig {
                max_sectors_per_call: 128,
            },
            io: IoConfig { max_iov: 1024 },
            capability: CapabilityConfig { max_name_len: 128 },
            exec: ExecConfig {
                stack_top_base: 0x0000_7FFF_FFF0_0000,
                stack_aslr_max_pages: 4096,
                user_stack_size_pages: 32,
                tls_base_min: 0x3000_0000,
                tls_aslr_max_pages: 0x4000,
                initial_tls_size: 4096,
                brk_heap_base_min: 0x4000_0000,
                brk_heap_aslr_max_pages: 0x8000,
                mmap_heap_base_min: 0x5000_0000,
                mmap_heap_aslr_max_pages: 0x10000,
                kernel_thread_stack_size: 4096 * 4,
            },
            kmod: KmodConfig {
                module_load_base_start: 0x0000_6000_0000_0000,
                module_load_guard: 0x20_0000,
                max_read_bytes: 64 * 1024 * 1024,
            },
        }
    }
}

static CONFIG: Once<KernelConfig> = Once::new();

#[inline]
pub fn kernel() -> &'static KernelConfig {
    CONFIG.call_once(load_kernel_config)
}

pub fn init() {
    let _ = kernel();
}

fn load_kernel_config() -> KernelConfig {
    let mut config = KernelConfig::default();
    let Some(bytes) = crate::init::fs::read("/config/kernel.conf") else {
        return config;
    };
    let Ok(text) = String::from_utf8(bytes) else {
        crate::warn!("kernel.conf is not valid UTF-8, using defaults");
        return config;
    };

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        apply_key_value(&mut config, key.trim(), value.trim());
    }

    config
}

fn parse_u64(value: &str) -> Option<u64> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else {
        value.parse::<u64>().ok()
    }
}

fn parse_usize(value: &str) -> Option<usize> {
    parse_u64(value).and_then(|v| usize::try_from(v).ok())
}

fn parse_u8(value: &str) -> Option<u8> {
    parse_u64(value).and_then(|v| u8::try_from(v).ok())
}

fn apply_key_value(config: &mut KernelConfig, key: &str, value: &str) {
    match key {
        "scheduler.default_time_slice_ms" => {
            if let Some(v) = parse_u64(value) {
                config.scheduler.default_time_slice_ms = v;
            }
        }
        "scheduler.interactive_min_ms" => {
            if let Some(v) = parse_u64(value) {
                config.scheduler.interactive_min_ms = v;
            }
        }
        "scheduler.interactive_mid_ms" => {
            if let Some(v) = parse_u64(value) {
                config.scheduler.interactive_mid_ms = v;
            }
        }
        "scheduler.interactive_max_ms" => {
            if let Some(v) = parse_u64(value) {
                config.scheduler.interactive_max_ms = v;
            }
        }
        "scheduler.normal_ms" => {
            if let Some(v) = parse_u64(value) {
                config.scheduler.normal_ms = v;
            }
        }
        "scheduler.cpu_bound_ms" => {
            if let Some(v) = parse_u64(value) {
                config.scheduler.cpu_bound_ms = v;
            }
        }
        "scheduler.background_ms" => {
            if let Some(v) = parse_u64(value) {
                config.scheduler.background_ms = v;
            }
        }
        "scheduler.low_priority_threshold" => {
            if let Some(v) = parse_u8(value) {
                config.scheduler.low_priority_threshold = v;
            }
        }
        "scheduler.cpu_bias_priority_threshold" => {
            if let Some(v) = parse_u8(value) {
                config.scheduler.cpu_bias_priority_threshold = v;
            }
        }
        "scheduler.interactive_priority_threshold" => {
            if let Some(v) = parse_u8(value) {
                config.scheduler.interactive_priority_threshold = v;
            }
        }
        "ipc.mailbox_cap" => {
            if let Some(v) = parse_usize(value) {
                config.ipc.mailbox_cap = v;
            }
        }
        "ipc.max_msg_size" => {
            if let Some(v) = parse_usize(value) {
                config.ipc.max_msg_size = v;
            }
        }
        "ipc.max_external_pages" => {
            if let Some(v) = parse_usize(value) {
                config.ipc.max_external_pages = v;
            }
        }
        "fs.service_retry_count" => {
            if let Some(v) = parse_usize(value) {
                config.fs.service_retry_count = v;
            }
        }
        "fs.service_retry_ms" => {
            if let Some(v) = parse_u64(value) {
                config.fs.service_retry_ms = v;
            }
        }
        "block.max_sectors_per_call" => {
            if let Some(v) = parse_u64(value) {
                config.block.max_sectors_per_call = v;
            }
        }
        "io.max_iov" => {
            if let Some(v) = parse_u64(value) {
                config.io.max_iov = v;
            }
        }
        "capability.max_name_len" => {
            if let Some(v) = parse_usize(value) {
                config.capability.max_name_len = v;
            }
        }
        "exec.stack_top_base" => {
            if let Some(v) = parse_u64(value) {
                config.exec.stack_top_base = v;
            }
        }
        "exec.stack_aslr_max_pages" => {
            if let Some(v) = parse_u64(value) {
                config.exec.stack_aslr_max_pages = v;
            }
        }
        "exec.user_stack_size_pages" => {
            if let Some(v) = parse_usize(value) {
                config.exec.user_stack_size_pages = v;
            }
        }
        "exec.tls_base_min" => {
            if let Some(v) = parse_u64(value) {
                config.exec.tls_base_min = v;
            }
        }
        "exec.tls_aslr_max_pages" => {
            if let Some(v) = parse_u64(value) {
                config.exec.tls_aslr_max_pages = v;
            }
        }
        "exec.initial_tls_size" => {
            if let Some(v) = parse_u64(value) {
                config.exec.initial_tls_size = v;
            }
        }
        "exec.brk_heap_base_min" => {
            if let Some(v) = parse_u64(value) {
                config.exec.brk_heap_base_min = v;
            }
        }
        "exec.brk_heap_aslr_max_pages" => {
            if let Some(v) = parse_u64(value) {
                config.exec.brk_heap_aslr_max_pages = v;
            }
        }
        "exec.mmap_heap_base_min" => {
            if let Some(v) = parse_u64(value) {
                config.exec.mmap_heap_base_min = v;
            }
        }
        "exec.mmap_heap_aslr_max_pages" => {
            if let Some(v) = parse_u64(value) {
                config.exec.mmap_heap_aslr_max_pages = v;
            }
        }
        "exec.kernel_thread_stack_size" => {
            if let Some(v) = parse_usize(value) {
                config.exec.kernel_thread_stack_size = v;
            }
        }
        "kmod.module_load_base_start" => {
            if let Some(v) = parse_u64(value) {
                config.kmod.module_load_base_start = v;
            }
        }
        "kmod.module_load_guard" => {
            if let Some(v) = parse_u64(value) {
                config.kmod.module_load_guard = v;
            }
        }
        "kmod.max_read_bytes" => {
            if let Some(v) = parse_usize(value) {
                config.kmod.max_read_bytes = v;
            }
        }
        _ => {}
    }
}
