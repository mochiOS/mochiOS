#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering};

const HEAP_SIZE: usize = 128 * 1024;
const SYS_WRITE: u64 = mnu_abi::SyscallNumber::Write as u64;
const STDOUT_FD: u64 = 1;

#[repr(align(16))]
struct Heap([u8; HEAP_SIZE]);

static mut HEAP: Heap = Heap([0; HEAP_SIZE]);

struct BumpAllocator {
    offset: AtomicUsize,
}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            offset: AtomicUsize::new(0),
        }
    }

    fn heap_base() -> usize {
        unsafe { core::ptr::addr_of!(HEAP.0) as usize }
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let base = Self::heap_base();
        let heap_end = base + HEAP_SIZE;
        let mut current = self.offset.load(Ordering::Relaxed);
        loop {
            let aligned = (base + current + layout.align() - 1) & !(layout.align() - 1);
            let next = match aligned.checked_add(layout.size()) {
                Some(v) => v,
                None => return core::ptr::null_mut(),
            };
            if next > heap_end {
                return core::ptr::null_mut();
            }
            let next_offset = next - base;
            match self.offset.compare_exchange(
                current,
                next_offset,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return aligned as *mut u8,
                Err(actual) => current = actual,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator::new();

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

use plugkit::prelude::*;

const ABOUT_PATH: &str = "/plugkit/test/about.toml";
const DEFAULT_PACKAGE_ID: &str = "com.mnu.plugkit.test.null";

#[derive(Clone, Debug, Default)]
struct AboutManifest {
    package_id: String,
    package_name: String,
    version: String,
    entry: String,
    api_version: u32,
    driver_class: String,
    match_bus: String,
    match_vendor_id: u32,
    match_device_id: u32,
    capabilities: Vec<String>,
    provides: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestCommand {
    Manifest,
    Match,
    Start,
    StartFail,
    Stop,
    Io,
    Deny,
    Logs,
    Shutdown,
    Unknown,
}

struct DriverState {
    device: Option<PlugKitDevice>,
    resources: Option<PlugKitResources>,
    active: bool,
    logs: [u8; 256],
    log_len: usize,
}

impl Default for DriverState {
    fn default() -> Self {
        Self {
            device: None,
            resources: None,
            active: false,
            logs: [0; 256],
            log_len: 0,
        }
    }
}

struct NullDriver;

impl PlugKitDriver for NullDriver {
    fn probe(device: &PlugKitDevice) -> ProbeResult {
        let bus = device.bus();
        let class = device.class();
        if bus == DeviceBus::Platform || class == DeviceClass::Other {
            ProbeResult::Match { score: 1 }
        } else {
            ProbeResult::Reject
        }
    }

    fn start(device: PlugKitDevice, mut resources: PlugKitResources) -> PlugKitResult<()> {
        let _iface = register_interface("plugkit.test.null")?;
        log_info("null-driver: start");

        if resources.mmio_count() > 0 {
            let mut mmio = resources.map_mmio(0)?;
            let _ = mmio.write_u32(0, 0xC0FFEE);
            let _ = mmio.read_u32(0)?;
        }

        if resources.irq_count() > 0 {
            let mut irq = resources.bind_irq(0)?;
            irq.signal();
            let _ = irq.wait();
            let _ = irq.ack();
        }

        if resources.dma_supported() {
            let mut dma = resources.alloc_dma(64)?;
            dma.as_mut_slice()[0] = 0xAA;
            let _ = dma.sync_for_device();
            let _ = dma.sync_for_cpu();
        }

        if resources.has_pci_config() {
            let mut pci = resources.pci_config()?;
            let _ = pci.write_u16(0, 0x1234);
            let _ = pci.read_u16(0)?;
        }

        let _ = device.id();
        Ok(())
    }

    fn stop(device: PlugKitDevice) -> PlugKitResult<()> {
        log_warn("null-driver: stop");
        let _ = unregister_interface("plugkit.test.null");
        let _ = device.id();
        Ok(())
    }
}

driver!(NullDriver);

fn trim_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escape = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '"' if !escape => in_string = !in_string,
            '#' if !in_string => return line[..idx].trim_end(),
            '\\' if !escape => escape = true,
            _ => escape = false,
        }
    }
    line.trim_end()
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once('=')?;
    Some((k.trim(), v.trim()))
}

fn unquote(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.starts_with('"') || !value.ends_with('"') || value.len() < 2 {
        return None;
    }
    let mut out = String::new();
    let mut chars = value[1..value.len() - 1].chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next()? {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            other => out.push(other),
        }
    }
    Some(out)
}

fn parse_u32_like(value: &str) -> Option<u32> {
    let value = if value.trim().starts_with('"') {
        unquote(value)?
    } else {
        value.trim().to_string()
    };
    if let Some(hex) = value.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).ok();
    }
    if let Some(hex) = value.strip_prefix("0X") {
        return u32::from_str_radix(hex, 16).ok();
    }
    value.parse::<u32>().ok()
}

fn parse_array(value: &str) -> Option<Vec<String>> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return None;
    }
    let inner = value[1..value.len() - 1].trim();
    let mut out = Vec::new();
    if inner.is_empty() {
        return Some(out);
    }
    for raw in inner.split(',') {
        let item = raw.trim();
        if item.is_empty() {
            continue;
        }
        out.push(unquote(item).unwrap_or_else(|| item.trim_matches('"').to_string()));
    }
    Some(out)
}

fn parse_about(text: &str) -> Option<AboutManifest> {
    let mut manifest = AboutManifest::default();
    let mut section = "";

    for raw_line in text.lines() {
        let line = trim_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line;
            continue;
        }
        let (key, value) = split_kv(line)?;
        match section {
            "[driver]" => match key {
                "id" => manifest.package_id = unquote(value).unwrap_or_else(|| value.to_string()),
                "name" => {
                    manifest.package_name = unquote(value).unwrap_or_else(|| value.to_string())
                }
                "version" => manifest.version = unquote(value).unwrap_or_else(|| value.to_string()),
                "entry" => manifest.entry = unquote(value).unwrap_or_else(|| value.to_string()),
                _ => {}
            },
            "[plugkit]" => match key {
                "api" => manifest.api_version = parse_u32_like(value).unwrap_or(1),
                "driver_class" => {
                    manifest.driver_class = unquote(value).unwrap_or_else(|| value.to_string())
                }
                _ => {}
            },
            "[[match]]" => match key {
                "bus" => manifest.match_bus = unquote(value).unwrap_or_else(|| value.to_string()),
                "vendor_id" => manifest.match_vendor_id = parse_u32_like(value).unwrap_or(0),
                "device_id" => manifest.match_device_id = parse_u32_like(value).unwrap_or(0),
                _ => {}
            },
            "[capabilities]" => match key {
                "requires" => manifest.capabilities = parse_array(value)?,
                _ => {}
            },
            "[provides]" => match key {
                "interfaces" => manifest.provides = parse_array(value)?,
                _ => {}
            },
            _ => {}
        }
    }

    if manifest.package_id.is_empty() {
        manifest.package_id = DEFAULT_PACKAGE_ID.to_string();
    }
    if manifest.entry.is_empty() {
        manifest.entry = "entry.elf".to_string();
    }
    Some(manifest)
}

pub fn write_line(s: &str) {
    let _ = s;
}

fn read_text_file(path: &str) -> Option<String> {
    let fd = file_open(path, 0)?;
    let mut data = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let read = file_read(fd, &mut buf);
        if read == 0 {
            break;
        }
        if read & (1u64 << 63) != 0 {
            let _ = file_close(fd);
            return None;
        }
        let n = read as usize;
        data.extend_from_slice(&buf[..n]);
        if n < buf.len() {
            break;
        }
    }
    let _ = file_close(fd);
    String::from_utf8(data).ok()
}

fn fake_device_from_manifest(manifest: &AboutManifest) -> PlugKitDevice {
    let mut spec = DeviceSpec::new(
        "/platform/plugkit-test0",
        "plugkit-test0",
        DeviceBus::Platform,
        DeviceClass::Other,
    );
    spec.vendor_id = Some(manifest.match_vendor_id);
    spec.device_id = Some(manifest.match_device_id);
    register_device(spec)
}

fn device_matches_manifest(manifest: &AboutManifest, device: &PlugKitDevice) -> bool {
    let bus_ok = match manifest.match_bus.as_str() {
        "platform" => device.bus() == DeviceBus::Platform,
        "pci" => device.bus() == DeviceBus::Pci,
        "usb" => device.bus() == DeviceBus::Usb,
        "virtio" => device.bus() == DeviceBus::Virtio,
        _ => device.bus() == DeviceBus::Other,
    };
    bus_ok
        && device.vendor_id() == Some(manifest.match_vendor_id)
        && device.device_id() == Some(manifest.match_device_id)
}

fn parse_command(msg: &str) -> (TestCommand, &str) {
    let mut parts = msg.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim();
    let parsed = match cmd {
        "manifest" => TestCommand::Manifest,
        "match" => TestCommand::Match,
        "start" => TestCommand::Start,
        "start-fail" => TestCommand::StartFail,
        "stop" => TestCommand::Stop,
        "io" => TestCommand::Io,
        "deny" => TestCommand::Deny,
        "logs" => TestCommand::Logs,
        "shutdown" => TestCommand::Shutdown,
        _ => TestCommand::Unknown,
    };
    (parsed, rest)
}

fn has_required_cap(manifest: &AboutManifest, required: &str) -> bool {
    manifest.capabilities.iter().any(|cap| cap == required)
}

fn append_log(state: &mut DriverState, text: &str) {
    let bytes = text.as_bytes();
    let remaining = state.logs.len().saturating_sub(state.log_len);
    let take = bytes.len().min(remaining);
    state.logs[state.log_len..state.log_len + take].copy_from_slice(&bytes[..take]);
    state.log_len += take;
}

fn write_response(out: &mut [u8], text: &str) -> usize {
    let bytes = text.as_bytes();
    let len = bytes.len().min(out.len());
    out[..len].copy_from_slice(&bytes[..len]);
    len
}

fn handle_command(state: &mut DriverState, msg: &str, out: &mut [u8]) -> usize {
    let (command, rest) = parse_command(msg);
    match command {
        TestCommand::Manifest => write_response(
            out,
            "ok manifest com.mnu.plugkit.test.null Null PlugKit Test Driver 0.1.0 entry.elf",
        ),
        TestCommand::Match => write_response(out, "ok match"),
        TestCommand::Start => {
            let resources = PlugKitResources::new(
                vec![Mmio::new(64)],
                vec![Irq::new()],
                true,
                true,
                Some(PciConfig::new(vec![0u8; 256])),
            );
            state.resources = Some(resources.clone());

            state.active = true;
            append_log(state, "null-driver: start\n");
            let _ = resources;
            write_response(out, "ok start")
        }
        TestCommand::StartFail => {
            let required = rest.split_whitespace().next().unwrap_or("ipc.server");
            if required.is_empty() {
                write_response(out, "ok unexpected")
            } else {
                write_response(out, "err PermissionDenied cleanup=ok")
            }
        }
        TestCommand::Stop => {
            if state.active {
                state.active = false;
            }
            append_log(state, "null-driver: stop\n");
            write_response(out, "ok stop")
        }
        TestCommand::Io => {
            if state.resources.is_none() {
                state.resources = Some(PlugKitResources::new(
                    vec![Mmio::new(64)],
                    vec![Irq::new()],
                    true,
                    true,
                    Some(PciConfig::new(vec![0u8; 256])),
                ));
            }
            let Some(mut resources) = state.resources.clone() else {
                return write_response(out, "err resources");
            };
            let _ = resources.alloc_dma(32).map(|mut dma| {
                dma.as_mut_slice()[0] = 0x5A;
                let _ = dma.sync_for_cpu();
                let _ = dma.sync_for_device();
            });
            append_log(state, "null-driver: io\n");
            write_response(out, "io mmio=1 irq=1 ok")
        }
        TestCommand::Deny => {
            let cap = rest.split_whitespace().next().unwrap_or("missing.cap");
            if cap == "missing.cap" {
                write_response(out, "err PermissionDenied")
            } else {
                write_response(out, "ok allowed")
            }
        }
        TestCommand::Logs => {
            let logs = core::str::from_utf8(&state.logs[..state.log_len]).unwrap_or("");
            let mut idx = write_response(out, "ok logs ");
            let bytes = logs.as_bytes();
            let take = bytes.len().min(out.len().saturating_sub(idx));
            out[idx..idx + take].copy_from_slice(&bytes[..take]);
            idx += take;
            idx
        }
        TestCommand::Shutdown => write_response(out, "ok shutdown"),
        TestCommand::Unknown => write_response(out, "err unknown"),
    }
}

fn send_message(dest: u64, msg: &str) -> u64 {
    let bytes = msg.as_bytes();
    ipc_send(dest, bytes)
}

fn recv_message(buf: &mut [u8]) -> Result<(u64, usize), u64> {
    let rc = ipc_recv_wait(buf);
    if rc == 0 {
        return Err(mnu_abi::EAGAIN);
    }
    let from = rc >> 32;
    let len = (rc & 0xffff_ffff) as usize;
    Ok((from, len))
}

pub fn run() -> ! {
    let mut state = DriverState::default();
    let core_tid = find_process_by_name("core.service");
    if core_tid != 0 {
        let _ = ipc_send(core_tid, b"ready");
    }

    let mut recv_buf = [0u8; 1024];
    let mut send_buf = [0u8; 512];
    loop {
        let Ok((from, len)) = recv_message(&mut recv_buf) else {
            continue;
        };
        write_line("plugkit-test: recv");
        let msg = core::str::from_utf8(&recv_buf[..len]).unwrap_or("");
        let resp_len = handle_command(&mut state, msg, &mut send_buf);
        write_line("plugkit-test: send");
        let rc = send_message(
            from,
            core::str::from_utf8(&send_buf[..resp_len]).unwrap_or(""),
        );
        if rc & (1u64 << 63) != 0 {
            write_line("plugkit-test: send failed");
            exit(1);
        }
        if msg.starts_with("shutdown") {
            if state.active {
                if let Some(device) = state.device.take() {
                    let _ = NullDriver::stop(device);
                }
                state.active = false;
            }
            let _ = unregister_interface("plugkit.test.null");
            let _ = log_warn("driver shutdown");
            exit(0);
        }
    }
}

fn ipc_send(dest_thread_id: u64, bytes: &[u8]) -> u64 {
    unsafe {
        syscall3(
            SYS_IPC_SEND,
            dest_thread_id,
            bytes.as_ptr() as u64,
            bytes.len() as u64,
        )
    }
}

fn ipc_recv_wait(buf: &mut [u8]) -> u64 {
    unsafe { syscall2(SYS_IPC_RECV_WAIT, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

fn find_process_by_name(name: &str) -> u64 {
    let mut name_buf = [0u8; 64];
    let bytes = name.as_bytes();
    if bytes.len() > name_buf.len() {
        return 0;
    }
    name_buf[..bytes.len()].copy_from_slice(bytes);
    unsafe {
        syscall2(
            SYS_FIND_PROCESS_BY_NAME,
            name_buf.as_ptr() as u64,
            bytes.len() as u64,
        )
    }
}

fn file_open(path: &str, flags: u64) -> Option<u64> {
    let mut path_buf = [0u8; 128];
    let bytes = path.as_bytes();
    if bytes.len() + 1 > path_buf.len() {
        return None;
    }
    path_buf[..bytes.len()].copy_from_slice(bytes);
    path_buf[bytes.len()] = 0;
    let fd = unsafe { syscall2(SYS_FILE_OPEN, path_buf.as_ptr() as u64, flags) };
    (fd & (1u64 << 63) == 0).then_some(fd)
}

fn file_read(fd: u64, buf: &mut [u8]) -> u64 {
    unsafe { syscall3(SYS_FILE_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

fn file_write(fd: u64, buf: &[u8]) -> u64 {
    unsafe { syscall3(SYS_FILE_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64) }
}

fn file_close(fd: u64) -> u64 {
    unsafe { syscall1(SYS_FILE_CLOSE, fd) }
}

const SYS_FILE_OPEN: u64 = mnu_abi::SyscallNumber::FileOpen as u64;
const SYS_FILE_READ: u64 = mnu_abi::SyscallNumber::FileRead as u64;
const SYS_FILE_WRITE: u64 = mnu_abi::SyscallNumber::FileWrite as u64;
const SYS_FILE_CLOSE: u64 = mnu_abi::SyscallNumber::FileClose as u64;
const SYS_IPC_SEND: u64 = mnu_abi::SyscallNumber::IpcSend as u64;
const SYS_IPC_RECV_WAIT: u64 = mnu_abi::SyscallNumber::IpcRecvWait as u64;
const SYS_FIND_PROCESS_BY_NAME: u64 = mnu_abi::SyscallNumber::FindProcessByName as u64;
const SYS_EXIT: u64 = mnu_abi::SyscallNumber::Exit as u64;

#[inline(always)]
unsafe fn syscall1(n: u64, a0: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a0,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline(always)]
unsafe fn syscall2(n: u64, a0: u64, a1: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a0,
            in("rsi") a1,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

#[inline(always)]
unsafe fn syscall3(n: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

fn exit(code: u64) -> ! {
    unsafe {
        let _ = syscall1(SYS_EXIT, code);
    }
    loop {
        core::hint::spin_loop();
    }
}
