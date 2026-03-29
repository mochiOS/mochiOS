use swiftlib::ipc;
use swiftlib::process;
use swiftlib::time;

/// READY通知OPコード
const OP_NOTIFY_READY: u64 = 0xFF;

/// サービス定義
struct ServiceDef {
    name: &'static str,
    path: &'static str,
}

const CRITICAL_SERVICES: &[ServiceDef] = &[
    ServiceDef { name: "disk.service",   path: "disk.service"   },
    ServiceDef { name: "fs.service",     path: "fs.service"     },
];

const BACKGROUND_SERVICES: &[ServiceDef] = &[
    ServiceDef { name: "driver.service", path: "driver.service" },
];

#[cfg(feature = "run_tests")]
const TEST_PATH: &str = "tests";

fn start_service(service: &ServiceDef) -> Option<u64> {
    println!("[CORE] Starting service: {}", service.name);
    match process::exec(service.path) {
        Ok(pid) => {
            println!("[CORE] {} started (PID={})", service.name, pid);
            Some(pid)
        }
        Err(_) => {
            println!("[CORE] Failed to start {}", service.name);
            None
        }
    }
}

fn start_background_service(service: &ServiceDef) -> Option<u64> {
    println!("[CORE] Starting background service: {}", service.name);
    match exec_file_via_fs_service(service.path) {
        Ok(pid) => {
            println!("[CORE] {} started via fs.service (PID={})", service.name, pid);
            Some(pid)
        }
        Err(errno) => {
            println!("[CORE] exec via fs.service failed for {}: errno={}, falling back", service.name, errno);
            start_service(service)
        }
    }
}

fn wait_for_ready(expected_pids: &[u64]) {
    let mut pending = [0u64; CRITICAL_SERVICES.len()];
    let mut pending_len = 0usize;
    for &pid in expected_pids {
        if pid != 0 && pending_len < pending.len() {
            pending[pending_len] = pid;
            pending_len += 1;
        }
    }

    if pending_len == 0 {
        println!("[CORE] WARNING: no critical services to wait for");
        return;
    }

    let total = pending_len;
    let mut recv_buf = [0u8; 64];

    println!("[CORE] Waiting for {} critical service(s) to be ready...", total);

    while pending_len > 0 {
        let (sender, len) = ipc::ipc_recv_wait(&mut recv_buf);
        if sender == 0 && len == 0 {
            continue;
        }

        if sender != 0 && (len as usize) >= 8 {
            // OP コードだけ読む
            let op = u64::from_le_bytes(recv_buf[..8].try_into().unwrap_or([0; 8]));
            if op == OP_NOTIFY_READY {
                let mut matched = false;
                for i in 0..pending_len {
                    if pending[i] == sender {
                        pending[i] = pending[pending_len - 1];
                        pending_len -= 1;
                        matched = true;
                        break;
                    }
                }
                if matched {
                    let ready_count = total - pending_len;
                    println!(
                        "[CORE] Critical service ready (PID={}, {}/{})",
                        sender, ready_count, total
                    );
                    if pending_len == 0 {
                        return;
                    }
                }
            }
        }
    }
}

fn exec_file_via_fs_service(path: &str) -> Result<u64, i64> {
    swiftlib::fs::exec_via_fs(path)
}

fn start_shell_service() {
    // rootfs は fs.service がマウントするため、fs.service に実行を依頼する
    println!("[CORE] Loading shell.service via fs.service...");
    match exec_file_via_fs_service("Services/shell.service") {
        Ok(pid) => println!("[CORE] shell.service started (PID={})", pid),
        Err(errno) => {
            println!(
                "[CORE] Failed to exec shell.service via fs.service: errno={}",
                errno
            );
            println!("[CORE] Fallback: launching shell.service from initfs...");
            match process::exec("shell.service") {
                Ok(pid) => println!("[CORE] shell.service started (PID={})", pid),
                Err(_) => println!("[CORE] Failed to start shell.service"),
            }
        }
    }
}

fn fs_open_read_lines(path: &str) -> Result<Vec<String>, i64> {
    match swiftlib::fs::read_file_via_fs(path, 4096) {
        Some(bytes) => {
            let mut lines = Vec::new();
            if let Ok(text) = core::str::from_utf8(&bytes) {
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    lines.push(line.to_string());
                }
            }
            Ok(lines)
        }
        None => Err(-5),
    }
}

fn main() {
    println!("[CORE] Service Manager Started");

    let mut critical_pids = [0u64; CRITICAL_SERVICES.len()];
    for (idx, service) in CRITICAL_SERVICES.iter().enumerate() {
        critical_pids[idx] = start_service(service).unwrap_or(0);
    }

    wait_for_ready(&critical_pids);

    start_shell_service();

    // Try to read fs/Config/services.list via fs.service and start listed services from ATA rootfs.
    match fs_open_read_lines("Config/services.list") {
        Ok(lines) => {
            println!("[CORE] Found services.list with {} entries", lines.len());
            for p in lines {
                println!("[CORE] Requesting exec for {}", p);
                match exec_file_via_fs_service(&p) {
                    Ok(pid) => println!("[CORE] {} started (PID={})", p, pid),
                    Err(errno) => println!("[CORE] Failed to exec {} via fs.service: errno={}", p, errno),
                }
            }
        }
        Err(errno) => {
            println!("[CORE] No services.list via fs.service (errno={}), falling back to background list", errno);
            for service in BACKGROUND_SERVICES {
                let _ = start_background_service(service);
            }
        }
    }

    #[cfg(feature = "run_tests")]
    {
        println!("[CORE] Starting test application...");
        match process::exec(TEST_PATH) {
            Ok(pid) => println!("[CORE] tests started (PID={})", pid),
            Err(_) => println!("[CORE] Failed to start tests"),
        }
        time::sleep_ms(100);
    }

    println!("[CORE] Entering monitoring loop...");
    loop {
        time::sleep_ms(1000);
    }
}
