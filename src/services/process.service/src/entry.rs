use swiftlib::ipc;
use swiftlib::process;
use swiftlib::task;
use swiftlib::time;

/// READY通知OPコード
const OP_NOTIFY_READY: u64 = 0xFF;

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
                collecting_required = true;
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

fn main() {
    println!("[PROC] process.service started");
    notify_ready_to_core();

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
        if swiftlib::capability::check_thread_capability(sender, "process.spawn")
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

                let manifest_path = format!("{}/manifest.toml", path.trim_end_matches('/'));
                let manifest_text = match std::fs::read_to_string(&manifest_path) {
                    Ok(t) => t,
                    Err(_) => {
                        resp.status = -2;
                        let resp_slice = unsafe {
                            core::slice::from_raw_parts(
                                &resp as *const _ as *const u8,
                                core::mem::size_of::<ProcessResponseMsg>(),
                            )
                        };
                        let _ = ipc::ipc_send(sender, resp_slice);
                        continue;
                    }
                };

                let Some((app_id, entry, requested)) = parse_app_manifest(&manifest_text) else {
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

                let Some(cap_pid) = find_capability_service_pid() else {
                    resp.status = -5;
                    let resp_slice = unsafe {
                        core::slice::from_raw_parts(
                            &resp as *const _ as *const u8,
                            core::mem::size_of::<ProcessResponseMsg>(),
                        )
                    };
                    let _ = ipc::ipc_send(sender, resp_slice);
                    continue;
                };

                let granted = request_grant_for_app(cap_pid, &app_id, &requested).unwrap_or_default();
                let granted_refs = granted.iter().map(|s| s.as_str()).collect::<Vec<_>>();
                match process::exec_with_capabilities(&entry, &[], &granted_refs) {
                    Ok(pid) => {
                        record_granted_for_pid(cap_pid, pid, &granted);
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
