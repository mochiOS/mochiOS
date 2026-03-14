use std::vec::Vec;

use swiftlib::ipc;
use swiftlib::process;
use swiftlib::task;
use swiftlib::time;

const OP_NOTIFY_READY: u64 = 0xFF;
const DRIVER_CONFIG_PATH: &str = "Config/drivers.list";
const DEFAULT_DRIVERS: &[&str] = &["Binaries/drivers/usb.elf"];

fn load_driver_list() -> Vec<String> {
    let mut drivers = Vec::new();

    match std::fs::read_to_string(DRIVER_CONFIG_PATH) {
        Ok(text) => {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                drivers.push(line.to_string());
            }
        }
        Err(e) => {
            println!(
                "[DRIVER] Failed to read {}: {} (using defaults)",
                DRIVER_CONFIG_PATH, e
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

fn start_driver(path: &str) {
    println!("[DRIVER] Starting {}", path);
    match process::exec(path) {
        Ok(pid) => println!("[DRIVER] Started {} (PID={})", path, pid),
        Err(_) => println!("[DRIVER] Failed to start {}", path),
    }
    time::sleep_ms(50);
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

    let drivers = load_driver_list();
    for path in &drivers {
        start_driver(path);
    }

    notify_ready_to_core();

    println!("[DRIVER] Entering monitoring loop...");
    loop {
        time::sleep_ms(1000);
    }
}
