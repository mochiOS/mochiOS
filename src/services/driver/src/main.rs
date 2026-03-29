use std::vec::Vec;

use swiftlib::time;
use swiftlib::task;
use swiftlib::fs;

const OP_NOTIFY_READY: u64 = 0xFF;
const DRIVER_CONFIG_PATH: &str = "Config/drivers.list";
const DEFAULT_DRIVERS: &[&str] = &["Binaries/drivers/usb.elf"];

fn fs_exec(fs_tid: u64, path: &str) -> Result<u64, i64> {
    // swiftlib::fs handles finding fs.service and IPC details
    fs::exec_via_fs(path)
}

fn fs_open(_fs_tid: u64, path: &str) -> Result<u64, i64> {
    fs::open_via_fs(path)
}

fn fs_read(_fs_tid: u64, fd: u64, out: &mut [u8]) -> Result<usize, i64> {
    fs::read_via_fs(fd, out)
}

fn fs_close(_fs_tid: u64, fd: u64) {
    fs::close_via_fs(fd)
}

fn load_driver_list(_fs_tid: u64) -> Vec<String> {
    let mut drivers = Vec::new();

    match swiftlib::fs::read_file_via_fs(DRIVER_CONFIG_PATH, 4096) {
        Some(bytes) => {
            if let Ok(text) = core::str::from_utf8(&bytes) {
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    drivers.push(line.to_string());
                }
            }
        }
        None => {
            println!(
                "[DRIVER] Failed to open {} via fs.service (using defaults)",
                DRIVER_CONFIG_PATH
            );
        }
    }

    if drivers.is_empty() {
        for path in DEFAULT_DRIVERS {
            drivers.push((*path).to_string());
        }
    }

    drivers
}

fn start_driver(fs_tid: u64, path: &str) {
    println!("[DRIVER] Starting {}", path);
    match fs_exec(fs_tid, path) {
        Ok(pid) => println!("[DRIVER] Started {} (PID={})", path, pid),
        Err(errno) => println!("[DRIVER] Failed to start {} (errno={})", path, errno),
    }
}

fn notify_ready_to_core() {
    let core_pid = match task::find_process_by_name("core.service") {
        Some(pid) => pid,
        None => {
            println!("[DRIVER] WARNING: core.service not found, skipping READY notify");
            return;
        }
    };

    let op_bytes = OP_NOTIFY_READY.to_le_bytes();
    if ipc::ipc_send(core_pid, &op_bytes) == 0 {
        println!("[DRIVER] Sent READY to core.service (PID={})", core_pid);
    } else {
        println!("[DRIVER] Failed to send READY to core.service");
    }
}

fn main() {
    println!("[DRIVER] Driver service started");

    let fs_tid = match task::find_process_by_name("fs.service") {
        Some(pid) => pid,
        None => {
            println!("[DRIVER] fs.service not found");
            loop {
                time::sleep_ms(1000);
            }
        }
    };

    let drivers = load_driver_list(fs_tid);
    for path in &drivers {
        start_driver(fs_tid, path);
    }

    notify_ready_to_core();

    println!("[DRIVER] Entering monitoring loop...");
    loop {
        time::sleep_ms(1000);
    }
}
