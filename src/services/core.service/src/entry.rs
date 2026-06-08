use mochi_syscall::ipc;
use mochi_syscall::process;
use mochi_syscall::task;

/// READY通知OPコード
const OP_NOTIFY_READY: u64 = 0xFF;
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

/// サービス定義
struct ServiceDef {
    name: &'static str,
    path: &'static str,
}

const CRITICAL_SERVICES: &[ServiceDef] = &[ServiceDef {
    name: "capability.service",
    path: "/system/services/capability.service",
}];

const BACKGROUND_SERVICES: &[ServiceDef] = &[ServiceDef {
    name: "driver.service",
    path: "/system/services/driver.service",
}];

#[cfg(feature = "run_tests")]
const TEST_PATH: &str = "tests";

fn start_service(service: &ServiceDef) -> Option<u64> {
    println!("[CORE] Starting service: {}", service.name);
    let exec_result = if service.name == "capability.service" {
        let ready_tid = task::gettid();
        let arg = format!("--ready-tid={}", ready_tid);
        // capability.service は起動直後に設定ファイルとレジストリを読むので、
        // manifest と同じ bootstrap capability を渡す。
        let caps = ["ipc.server", "fs.read.all", "system.info.read"];
        process::exec_with_capabilities(service.path, &[&arg], &caps).map_err(|_| -1)
    } else {
        process::exec(service.path).map_err(|_| -1)
    };
    match exec_result {
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

fn parse_service_id_and_required_caps(manifest_text: &str) -> Option<(String, Vec<String>)> {
    // 依存クレートを増やさず、最低限の TOML 風パースを行う。
    // 期待する形式:
    // [service]
    // id = "service.name"
    // ...
    //
    // [capabilities]
    // required = [
    //   "ipc.server",
    //   ...
    // ]

    let mut in_service = false;
    let mut in_caps = false;
    let mut collecting_required = false;
    let mut service_id: Option<String> = None;
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
            in_service = sec == "service";
            in_caps = sec == "capabilities";
            collecting_required = false;
            continue;
        }

        if in_service {
            if let Some(rest) = line.strip_prefix("id") {
                if let Some((_, rhs)) = rest.split_once('=') {
                    let v = rhs.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        service_id = Some(v.to_string());
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
                let closes_inline = line
                    .rfind(']')
                    .is_some_and(|end| line.find('[').is_some_and(|start| end > start + 1));
                if closes_inline {
                    push_caps_from_inline_list(&mut required, line);
                } else {
                    collecting_required = true;
                }
            }
        }
    }

    let id = service_id?;
    Some((id, required))
}

fn fallback_required_caps_for_service(service_base: &str) -> Option<Vec<String>> {
    // manifest が読めない場合のフォールバック（ブート継続のため）。
    // ここでのリストは plan.md の推奨 / 既存 manifest と一致させる。
    let list: &[&str] = match service_base {
        "capability.service" => &["ipc.server", "system.info.read"],
        "driver.service" => &[
            "ipc.server",
            "process.spawn",
            "fs.read.all",
            "device.storage",
            "device.net",
            "device.input",
        ],
        "disk.service" => &["ipc.server", "device.storage"],
        "process.service" => &[
            "ipc.server",
            "process.spawn",
            "process.inspect",
            "process.kill",
            "fs.read.all",
        ],
        "device.service" => &["ipc.server"],
        "net.service" => &["ipc.server", "device.net", "net.raw"],
        "window.service" => &[
            "ipc.server",
            "display.read",
            "display.capture",
            "input.pointer.global",
            "input.keyboard.global",
        ],
        "shell.service" => &[
            "ipc.server",
            "display.read",
            "input.keyboard",
            "input.pointer",
            "fs.read.all",
            "window.create",
        ],
        _ => return None,
    };
    Some(list.iter().map(|s| s.to_string()).collect())
}

fn fast_boot_caps_for_service(path: &str) -> Option<Vec<String>> {
    let service_base = service_name_from_path(path);
    match service_base {
        "capability.service" | "driver.service" | "disk.service" | "process.service"
        | "device.service" | "net.service" | "window.service" | "shell.service" => {
            fallback_required_caps_for_service(service_base)
        }
        _ => None,
    }
}

fn find_capability_service_pid() -> Option<u64> {
    task::find_process_by_name("capability.service")
}

fn request_grant_for_service(
    cap_pid: u64,
    service_id: &str,
    requested: &[String],
) -> Option<Vec<String>> {
    // 以前のリクエストに対するレスポンスがキューに残っていると、
    // 次の grant の待受で「別リクエストのレスポンス」を誤って拾ってしまう。
    // ブート直後は 500ms タイムアウトに引っかかりやすいので、送信前に古い分を排出する。
    {
        let mut drain_buf = [0u8; 576];
        for _ in 0..16 {
            let (sender, len) = ipc::ipc_recv(&mut drain_buf);
            // ipc_recv はメッセージ無し/エラー時に (0, 0) を返す
            if sender == 0 || len == 0 {
                break;
            }
        }
    }

    // subject_id と requested を NUL 区切りで詰める
    let mut msg = CapabilityRequestMsg {
        op: OP_CAP_GRANT_FOR_EXEC,
        arg0: 2, // Service
        arg1: task::gettid(),
        len0: service_id.as_bytes().len() as u64,
        len1: 0,
        data: [0u8; 512],
    };

    let mut pos = 0usize;
    let sid = service_id.as_bytes();
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
    loop {
        let (sender, len) = ipc::ipc_recv_wait(&mut buf);
        if sender == 0 || len == 0 {
            continue;
        }
        if sender != cap_pid || (len as usize) < core::mem::size_of::<CapabilityResponseMsg>() {
            continue;
        }
        let resp: CapabilityResponseMsg =
            unsafe { core::ptr::read(buf.as_ptr() as *const CapabilityResponseMsg) };
        // NOTE: 返ってくるレスポンスは他OPと混ざることがあるため op でフィルタする
        if resp.op != OP_CAP_GRANT_FOR_EXEC {
            // 他の応答（RecordGranted 等）が混ざることがあるため、目的の op 以外は捨てる
            continue;
        }
        if resp.status != 0 {
            return None;
        }
        let blob_len = resp.len as usize;
        if blob_len == 0 {
            return Some(Vec::new());
        }
        let blob_len = core::cmp::min(blob_len, resp.data.len());
        let granted = resp.data[..blob_len]
            .split(|b| *b == 0)
            .filter_map(|part| core::str::from_utf8(part).ok())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        // デバッグ観測: 期待した required cap が返っているか確認する
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

fn start_background_service(service: &ServiceDef) -> Option<u64> {
    println!("[CORE] Starting background service: {}", service.name);
    match exec_file_via_fs_service(service.path) {
        Ok(pid) => {
            println!("[CORE] {} started (PID={})", service.name, pid);
            Some(pid)
        }
        Err(errno) => {
            println!(
                "[CORE] exec failed for {}: errno={}, falling back",
                service.name, errno
            );
            start_service(service)
        }
    }
}

fn wait_for_ready(expected_pids: &[u64]) -> bool {
    let mut pending: Vec<u64> = expected_pids
        .iter()
        .copied()
        .filter(|pid| *pid != 0)
        .collect();

    if pending.is_empty() {
        println!("[CORE] WARNING: no critical services to wait for");
        return true;
    }

    let total = pending.len();
    let mut recv_buf = [0u8; 64];
    let timeout = std::time::Duration::from_secs(20);
    let start = std::time::Instant::now();

    println!(
        "[CORE] Waiting for {} critical service(s) to be ready...",
        total
    );

    while !pending.is_empty() {
        if start.elapsed() >= timeout {
            println!("[CORE] ERROR: timed out waiting for critical services");
            return false;
        }
        let (sender, len) = ipc::ipc_recv_wait(&mut recv_buf);
        if sender == 0 && len == 0 {
            continue;
        }

        if sender != 0 && (len as usize) >= 8 {
            // OP コードだけ読む
            let op = u64::from_le_bytes(recv_buf[..8].try_into().unwrap_or([0; 8]));
            if op == OP_NOTIFY_READY {
                if let Some(pos) = pending.iter().position(|pid| *pid == sender) {
                    pending.swap_remove(pos);
                    let ready_count = total - pending.len();
                    println!(
                        "[CORE] Critical service ready (PID={}, {}/{})",
                        sender, ready_count, total
                    );
                    if pending.is_empty() {
                        return true;
                    }
                }
            }
        }
    }

    true
}

fn exec_file_via_fs_service(path: &str) -> Result<u64, i64> {
    match process::exec(path) {
        Ok(pid) => Ok(pid),
        Err(_) => {
            let fallback = path.rsplit('/').next().unwrap_or(path);
            process::exec(fallback).map_err(|_| -2)
        }
    }
}

fn service_name_from_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn is_allowed_service_path(path: &str) -> bool {
    if path.is_empty() || path.contains("..") {
        return false;
    }
    path.starts_with("/system/services/")
        || path.starts_with("/bin/")
        || path.starts_with("system/services/")
        || path.starts_with("bin/")
}

fn service_already_running(path: &str) -> bool {
    let name = service_name_from_path(path);
    task::find_process_by_name(path).is_some()
        || task::find_process_by_name(name).is_some()
        || task::find_process_by_name(&format!("/system/services/{}", name)).is_some()
}

fn running_service_pid(path: &str) -> Option<u64> {
    let name = service_name_from_path(path);
    task::find_process_by_name(path)
        .or_else(|| task::find_process_by_name(name))
        .or_else(|| task::find_process_by_name(&format!("/system/services/{}", name)))
}

fn fs_open_read_lines(path: &str) -> Result<Vec<String>, i64> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let mut lines = Vec::new();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                lines.push(line.to_string());
            }
            Ok(lines)
        }
        Err(_) => Err(-2),
    }
}

fn main() {
    println!("[CORE] Service Manager Started");

    let mut critical_pids = [0u64; CRITICAL_SERVICES.len()];
    for (idx, service) in CRITICAL_SERVICES.iter().enumerate() {
        let Some(pid) = start_service(service) else {
            println!(
                "[CORE] ERROR: failed to start critical service {}, aborting startup",
                service.name
            );
            return;
        };
        critical_pids[idx] = pid;
    }

    if !wait_for_ready(&critical_pids) {
        println!("[CORE] Critical services readiness failed; aborting startup");
        return;
    }

    // Try to read /config/services.list and start listed services from rootfs.
    match fs_open_read_lines("/config/services.list") {
        Ok(lines) => {
            println!("[CORE] Found services.list with {} entries", lines.len());
            // NOTE:
            // disk.service が起動すると、カーネル側の初期ローダ（kmod::fs）が使う
            // デバイス状態を変更してしまい、以降の /system/services/* の読み込みが
            // 失敗することがある（"file not found" で起動不能になる）。
            //
            // そのため、まず他のサービスを起動し、最後に disk.service を起動する。
            let mut paths = lines;
            paths.sort_by_key(|p| {
                if p.ends_with("/disk.service") || p == "/system/services/disk.service" {
                    1u8
                } else {
                    0u8
                }
            });

            for p in paths {
                if !is_allowed_service_path(&p) {
                    println!("[CORE] Skipping disallowed service path: {}", p);
                    continue;
                }
                if service_already_running(&p) {
                    println!(
                        "[CORE] Skipping {} ({} already running)",
                        p,
                        service_name_from_path(&p)
                    );
                    continue;
                }
                println!("[CORE] Requesting exec for {}", p);
                let service_base = service_name_from_path(&p);
                let cap_pid = find_capability_service_pid();
                let maybe_granted = if let Some(req) = fast_boot_caps_for_service(&p) {
                    Some(req)
                } else {
                    let manifest_path = format!(
                        "/system/services/{}.service.manifest.toml",
                        service_name_from_path(&p).trim_end_matches(".service")
                    );
                    let manifest_text = std::fs::read_to_string(&manifest_path).ok();

                    if service_base == "capability.service" {
                        manifest_text
                            .as_ref()
                            .and_then(|text| parse_service_id_and_required_caps(text))
                            .map(|(_, requested)| requested)
                    } else {
                        let requested = manifest_text
                            .as_ref()
                            .and_then(|text| parse_service_id_and_required_caps(text))
                            .map(|(sid, req)| (sid, req))
                            .or_else(|| {
                                fallback_required_caps_for_service(service_base)
                                    .map(|req| (service_base.to_string(), req))
                            });

                        if let (Some(cap_pid), Some((sid, req))) = (cap_pid, requested) {
                            request_grant_for_service(cap_pid, &sid, &req)
                        } else {
                            None
                        }
                    }
                };

                let launch = if let Some(granted) = maybe_granted {
                    let granted_refs = granted.iter().map(|s| s.as_str()).collect::<Vec<_>>();
                    match process::exec_with_capabilities(&p, &[], &granted_refs) {
                        Ok(pid) => {
                            if let Some(cap_pid) = find_capability_service_pid() {
                                record_granted_for_pid(cap_pid, pid, &granted);
                            }
                            Ok(pid)
                        }
                        Err(errno) => Err(errno),
                    }
                } else {
                    exec_file_via_fs_service(&p)
                };

                match launch {
                    Ok(pid) => println!("[CORE] {} started (PID={})", p, pid),
                    Err(errno) => println!("[CORE] Failed to exec {}: errno={}", p, errno),
                }
            }
        }
        Err(errno) => {
            println!(
                "[CORE] No services.list (errno={}), falling back to background list",
                errno
            );
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
        task::yield_now();
    }

    println!("[CORE] Entering monitoring loop...");
    loop {
        task::yield_now();
    }
}
