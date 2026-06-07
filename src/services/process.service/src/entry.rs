use mochi_syscall::ipc;
use mochi_syscall::process;
use mochi_syscall::task;
use mochi_syscall::time;

/// READY通知OPコード
const OP_NOTIFY_READY: u64 = 0xFF;
const APPS_CONFIG_PATH: &str = "/config/autostart.list";

/// capability.service の GrantForExec
const OP_CAP_GRANT_FOR_EXEC: u64 = 3;
const OP_CAP_RECORD_GRANTED: u64 = 5;

#[repr(C)]
#[derive(Clone, Copy)]
struct CapabilityRequestMsg {
    op: u64,
    arg0: u64,
    arg1: u64,
    len0: u64,
    len1: u64,
    data: [u8; 512],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CapabilityResponseMsg {
    op: u64,
    status: i64,
    len: u64,
    data: [u8; 512],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcessRequestMsg {
    op: u64,
    len0: u64,
    data: [u8; 512],
}

impl ProcessRequestMsg {
    const OP_EXEC_APP: u64 = 1;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcessResponseMsg {
    status: i64,
    pid: u64,
}

fn notify_ready_to_core() {
    let core_pid = match task::find_process_by_name("core.service") {
        Some(pid) => pid,
        None => {
            println!("[PROC] WARNING: core.service not found, skipping READY notify");
            return;
        }
    };

    let op_bytes = OP_NOTIFY_READY.to_le_bytes();
    let _ = ipc::ipc_send(core_pid, &op_bytes);
}

fn find_capability_service_pid() -> Option<u64> {
    task::find_process_by_name("capability.service")
}

fn normalize_app_entry(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    if let Some((_, path)) = line.split_once('=') {
        let path = path.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
        return None;
    }
    Some(line.to_string())
}

fn parse_app_manifest(manifest_text: &str) -> Option<(String, String, Vec<String>)> {
    // 期待形式:
    // [app]
    // id = "dev.taso.editor"
    // entry = "/applications/Editor.app/entry.elf"
    //
    // [capabilities]
    // required = [ ... ]
    let mut in_app = false;
    let mut in_caps = false;
    let mut collecting_required = false;
    let mut app_id: Option<String> = None;
    let mut entry: Option<String> = None;
    let mut required: Vec<String> = Vec::new();

    fn push_caps_from_inline_list(target: &mut Vec<String>, rhs: &str) {
        let trimmed = rhs.trim();
        let Some(start) = trimmed.find('[') else {
            return;
        };
        let Some(end) = trimmed.rfind(']') else {
            return;
        };
        if end <= start {
            return;
        }
        for item in trimmed[start + 1..end].split(',') {
            let cap = item.trim().trim_matches('"').trim_matches('\'');
            if !cap.is_empty() {
                target.push(cap.to_string());
            }
        }
    }

    for raw in manifest_text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let sec = &line[1..line.len() - 1];
            in_app = sec == "app";
            in_caps = sec == "capabilities";
            collecting_required = false;
            continue;
        }

        if in_app {
            if let Some(rest) = line.strip_prefix("id") {
                if let Some((_, rhs)) = rest.split_once('=') {
                    let v = rhs.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        app_id = Some(v.to_string());
                    }
                }
            } else if let Some(rest) = line.strip_prefix("entry") {
                if let Some((_, rhs)) = rest.split_once('=') {
                    let v = rhs.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        entry = Some(v.to_string());
                    }
                }
            }
        }

        if in_caps {
            if collecting_required {
                if line.starts_with(']') {
                    collecting_required = false;
                    continue;
                }
                let v = line
                    .trim_end_matches(',')
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !v.is_empty() {
                    required.push(v.to_string());
                }
            } else if line.starts_with("required") && line.contains('[') {
                let closes_inline = line.rfind(']').is_some_and(|end| {
                    line.find('[').is_some_and(|start| end > start + 1)
                });
                if closes_inline {
                    push_caps_from_inline_list(&mut required, line);
                } else {
                    collecting_required = true;
                }
            }
        }
    }

    Some((app_id?, entry?, required))
}

fn request_grant_for_app(
    cap_pid: u64,
    app_id: &str,
    requested: &[String],
) -> Option<Vec<String>> {
    // subject_id と requested を NUL 区切りで詰める
    let mut msg = CapabilityRequestMsg {
        op: OP_CAP_GRANT_FOR_EXEC,
        arg0: 1, // App
        arg1: task::gettid(),
        len0: app_id.as_bytes().len() as u64,
        len1: 0,
        data: [0u8; 512],
    };

    let mut pos = 0usize;
    let sid = app_id.as_bytes();
    if sid.len() > msg.data.len() {
        return None;
    }
    msg.data[..sid.len()].copy_from_slice(sid);
    pos += sid.len();

    let mut req_blob: Vec<u8> = Vec::new();
    for s in requested {
        req_blob.extend_from_slice(s.as_bytes());
        req_blob.push(0);
    }
    msg.len1 = req_blob.len() as u64;
    if pos + req_blob.len() > msg.data.len() {
        return None;
    }
    msg.data[pos..pos + req_blob.len()].copy_from_slice(&req_blob);

    let req_slice = unsafe {
        core::slice::from_raw_parts(
            &msg as *const _ as *const u8,
            core::mem::size_of::<CapabilityRequestMsg>(),
        )
    };
    let _ = ipc::ipc_send(cap_pid, req_slice);

    let mut buf = [0u8; 576];
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(800);
    loop {
        if std::time::Instant::now() > deadline {
            return None;
        }
        let (sender, len) = ipc::ipc_recv(&mut buf);
        if sender == 0xFFFFFFFF || len == 0xFFFFFFFD {
            time::sleep_ms(0);
            continue;
        }
        if sender != cap_pid || (len as usize) < core::mem::size_of::<CapabilityResponseMsg>() {
            continue;
        }
        let resp: CapabilityResponseMsg =
            unsafe { core::ptr::read(buf.as_ptr() as *const CapabilityResponseMsg) };
        if resp.op != OP_CAP_GRANT_FOR_EXEC {
            continue;
        }
        if resp.status != 0 {
            return None;
        }
        let blob_len = resp.len as usize;
        let blob_len = core::cmp::min(blob_len, resp.data.len());
        let granted = resp.data[..blob_len]
            .split(|b| *b == 0)
            .filter_map(|part| core::str::from_utf8(part).ok())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        return Some(granted);
    }
}

fn record_granted_for_pid(cap_pid: u64, pid: u64, granted: &[String]) {
    let mut msg = CapabilityRequestMsg {
        op: OP_CAP_RECORD_GRANTED,
        arg0: pid,
        arg1: 0,
        len0: 0,
        len1: 0,
        data: [0u8; 512],
    };
    let mut out = Vec::new();
    for s in granted {
        out.extend_from_slice(s.as_bytes());
        out.push(0);
        if out.len() >= msg.data.len() {
            break;
        }
    }
    let n = core::cmp::min(out.len(), msg.data.len());
    msg.data[..n].copy_from_slice(&out[..n]);
    msg.len0 = n as u64;

    let req_slice = unsafe {
        core::slice::from_raw_parts(
            &msg as *const _ as *const u8,
            core::mem::size_of::<CapabilityRequestMsg>(),
        )
    };
    let _ = ipc::ipc_send(cap_pid, req_slice);
}

fn launch_app_bundle(path: &str) -> Result<u64, i64> {
    let manifest_path = format!("{}/manifest.toml", path.trim_end_matches('/'));
    let manifest_text = match std::fs::read_to_string(&manifest_path) {
        Ok(t) => t,
        Err(_) => return Err(-2),
    };

    let Some((app_id, entry, requested)) = parse_app_manifest(&manifest_text) else {
        return Err(-22);
    };

    let Some(cap_pid) = find_capability_service_pid() else {
        println!(
            "[PROC] ERROR: capability.service unavailable; refusing to launch {}",
            app_id
        );
        return Err(-5);
    };
    let Some(granted) = request_grant_for_app(cap_pid, &app_id, &requested) else {
        println!(
            "[PROC] ERROR: capability.service grant failed for {}; refusing launch",
            app_id
        );
        return Err(-5);
    };
    let granted_refs = granted.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    match process::exec_with_capabilities(&entry, &[], &granted_refs) {
        Ok(pid) => {
            record_granted_for_pid(cap_pid, pid, &granted);
            Ok(pid)
        }
        Err(errno) => Err(errno),
    }
}

fn wait_for_desktop_services() {
    let required = ["window.service", "shell.service"];
    for _ in 0..500 {
        let ready = required
            .iter()
            .all(|name| task::find_process_by_name(name).is_some());
        if ready {
            return;
        }
        task::yield_now();
    }
    println!("[PROC] desktop services not ready; launching apps anyway");
}

fn launch_autostart_apps() {
    wait_for_desktop_services();

    let text = match std::fs::read_to_string(APPS_CONFIG_PATH) {
        Ok(t) => t,
        Err(_) => {
            println!("[PROC] No autostart.list at {}", APPS_CONFIG_PATH);
            return;
        }
    };

    let mut launched = 0usize;
    for raw in text.lines() {
        let Some(path) = normalize_app_entry(raw) else {
            continue;
        };
        match launch_app_bundle(&path) {
            Ok(pid) => {
                launched += 1;
                println!("[PROC] autostarted {} (PID={})", path, pid);
            }
            Err(errno) => {
                println!("[PROC] failed to autostart {}: errno={}", path, errno);
            }
        }
    }
    println!("[PROC] autostart apps done: {}", launched);
}

fn main() {
    println!("[PROC] process.service started");
    notify_ready_to_core();
    launch_autostart_apps();

    let mut recv = [0u8; 520];
    loop {
        let (sender, len) = ipc::ipc_recv(&mut recv);
        if sender == 0xFFFFFFFF || len == 0xFFFFFFFD {
            task::yield_now();
            continue;
        }
        if sender == 0 || (len as usize) < core::mem::size_of::<ProcessRequestMsg>() {
            continue;
        }

        // spawn は process.spawn を要求する
        if mochi_syscall::capability::check_thread_capability(sender, "process.spawn")
            .ok()
            .unwrap_or(false)
            == false
        {
            let resp = ProcessResponseMsg { status: -13, pid: 0 };
            let resp_slice = unsafe {
                core::slice::from_raw_parts(
                    &resp as *const _ as *const u8,
                    core::mem::size_of::<ProcessResponseMsg>(),
                )
            };
            let _ = ipc::ipc_send(sender, resp_slice);
            continue;
        }

        let req: ProcessRequestMsg =
            unsafe { core::ptr::read(recv.as_ptr() as *const ProcessRequestMsg) };

        let mut resp = ProcessResponseMsg { status: -1, pid: 0 };

        match req.op {
            ProcessRequestMsg::OP_EXEC_APP => {
                let n = core::cmp::min(req.len0 as usize, req.data.len());
                let Ok(path) = core::str::from_utf8(&req.data[..n]) else {
                    resp.status = -22;
                    let resp_slice = unsafe {
                        core::slice::from_raw_parts(
                            &resp as *const _ as *const u8,
                            core::mem::size_of::<ProcessResponseMsg>(),
                        )
                    };
                    let _ = ipc::ipc_send(sender, resp_slice);
                    continue;
                };
                match launch_app_bundle(path) {
                    Ok(pid) => {
                        resp.status = 0;
                        resp.pid = pid;
                    }
                    Err(errno) => {
                        resp.status = errno;
                    }
                }
            }
            _ => {
                resp.status = -38;
            }
        }

        let resp_slice = unsafe {
            core::slice::from_raw_parts(
                &resp as *const _ as *const u8,
                core::mem::size_of::<ProcessResponseMsg>(),
            )
        };
        let _ = ipc::ipc_send(sender, resp_slice);
    }
}
