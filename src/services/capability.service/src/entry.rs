use mochi_syscall::ipc;
use mochi_syscall::task;

mod db;
mod policy;
mod protocol;
mod registry;

use db::AllowDb;
use protocol::{AlignedBuf, CapabilityRequestMsg, CapabilityResponseMsg, SubjectType, OP_NOTIFY_READY};
use registry::CapabilityRegistry;

const CORE_SERVICE_PID_FALLBACK: u64 = 2;

fn find_service_pid(candidates: &[&str]) -> Option<u64> {
    for name in candidates {
        if let Some(pid) = task::find_process_by_name(name) {
            if pid != 0 {
                return Some(pid);
            }
        }
    }
    None
}

fn core_ready_tid_from_args() -> Option<u64> {
    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--ready-tid=") {
            if let Ok(tid) = value.parse::<u64>() {
                if tid != 0 {
                    return Some(tid);
                }
            }
        }
    }
    None
}

/// core.service に準備完了を通知する
fn notify_ready_to_core() {
    let core_pid = core_ready_tid_from_args().or_else(|| {
        find_service_pid(&["core.service", "/system/services/core.service", "core"])
    })
        .unwrap_or_else(|| {
            println!(
                "[CAP] WARNING: core.service lookup failed, using fallback PID={}",
                CORE_SERVICE_PID_FALLBACK
            );
            CORE_SERVICE_PID_FALLBACK
        });

    let op_bytes = OP_NOTIFY_READY.to_le_bytes();
    let _ = ipc::ipc_send(core_pid, &op_bytes);
}

fn read_str_from_msg(msg: &CapabilityRequestMsg, offset: usize, len: usize) -> Option<String> {
    if offset.checked_add(len)? > msg.data.len() {
        return None;
    }
    let bytes = &msg.data[offset..offset + len];
    core::str::from_utf8(bytes).ok().map(|s| s.to_string())
}

fn split_nul_list(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for part in bytes.split(|b| *b == 0) {
        if part.is_empty() {
            continue;
        }
        if let Ok(s) = core::str::from_utf8(part) {
            out.push(s.to_string());
        }
    }
    out
}

fn is_trusted_grant_caller(sender_tid: u64) -> bool {
    // 権限昇格の入口になるため、呼び出し元を信頼済みプロセスに限定する。
    // ここを緩めると、任意プロセスが `unsandboxed` 等を要求して自己昇格できる。
    find_service_pid(&["core.service", "/system/services/core.service", "core"])
        .filter(|pid| *pid == sender_tid)
        .is_some()
        || find_service_pid(&[
            "process.service",
            "/system/services/process.service",
            "process",
        ])
            .filter(|pid| *pid == sender_tid)
            .is_some()
}

fn main() {
    println!("[CAP] capability.service started");

    let registry = CapabilityRegistry::load();
    println!(
        "[CAP] registry loaded: {} entries, bootstrap_trusted={}",
        registry.len(),
        registry.trusted_services_len()
    );

    let allow_db = AllowDb::load_from_config();
    println!(
        "[CAP] allow db loaded: services={} apps={}",
        allow_db.services_len(),
        allow_db.apps_len()
    );

    notify_ready_to_core();

    // pid -> granted list（ListGranted 用、再起動で消える）
    let mut granted_by_pid: std::collections::BTreeMap<u64, Vec<String>> =
        std::collections::BTreeMap::new();

    let mut recv = AlignedBuf([0u8; 576]);
    loop {
        let (sender, len) = ipc::ipc_recv(&mut recv.0);

        // EAGAIN
        if sender == 0xFFFFFFFF || len == 0xFFFFFFFD {
            task::yield_now();
            continue;
        }

        if sender == 0 || (len as usize) < core::mem::size_of::<CapabilityRequestMsg>() {
            continue;
        }

        let req: CapabilityRequestMsg =
            unsafe { core::ptr::read(recv.0.as_ptr() as *const CapabilityRequestMsg) };

        let mut resp = CapabilityResponseMsg {
            op: req.op,
            status: -1,
            len: 0,
            data: [0u8; 512],
        };

        match req.op {
            CapabilityRequestMsg::OP_RESOLVE => {
                let name_len = req.len0 as usize;
                if let Some(name) = read_str_from_msg(&req, 0, name_len) {
                    resp.status = if registry.contains(&name) { 0 } else { -22 };
                } else {
                    resp.status = -22;
                }
            }
            CapabilityRequestMsg::OP_CHECK => {
                // arg0 = thread_id, data[0..len0] = capability
                let thread_id = req.arg0;
                let cap_len = req.len0 as usize;
                let Some(cap) = read_str_from_msg(&req, 0, cap_len) else {
                    resp.status = -22;
                    let _ = ipc::ipc_send(sender, unsafe {
                        core::slice::from_raw_parts(
                            &resp as *const _ as *const u8,
                            core::mem::size_of::<CapabilityResponseMsg>(),
                        )
                    });
                    continue;
                };

                match mochi_syscall::capability::check_thread_capability(thread_id, &cap) {
                    Ok(ok) => {
                        resp.status = if ok { 1 } else { 0 };
                    }
                    Err(errno) => resp.status = errno,
                }
            }
            CapabilityRequestMsg::OP_GRANT_FOR_EXEC => {
                if !is_trusted_grant_caller(sender) {
                    resp.status = -1;
                } else {
                    // arg0 = subject_type, len0 = subject_id_len, len1 = requested_blob_len
                    let Some(subject_type) = SubjectType::from_u64(req.arg0) else {
                        resp.status = -22;
                        let resp_slice = unsafe {
                            core::slice::from_raw_parts(
                                &resp as *const _ as *const u8,
                                core::mem::size_of::<CapabilityResponseMsg>(),
                            )
                        };
                        let _ = ipc::ipc_send(sender, resp_slice);
                        continue;
                    };

                    let subject_id_len = req.len0 as usize;
                    let requested_len = req.len1 as usize;
                    let Some(subject_id) = read_str_from_msg(&req, 0, subject_id_len) else {
                        resp.status = -22;
                        let resp_slice = unsafe {
                            core::slice::from_raw_parts(
                                &resp as *const _ as *const u8,
                                core::mem::size_of::<CapabilityResponseMsg>(),
                            )
                        };
                        let _ = ipc::ipc_send(sender, resp_slice);
                        continue;
                    };

                    let requested_off = subject_id_len;
                    if requested_off + requested_len > req.data.len() {
                        resp.status = -22;
                        let resp_slice = unsafe {
                            core::slice::from_raw_parts(
                                &resp as *const _ as *const u8,
                                core::mem::size_of::<CapabilityResponseMsg>(),
                            )
                        };
                        let _ = ipc::ipc_send(sender, resp_slice);
                        continue;
                    }

                    let requested_blob =
                        &req.data[requested_off..requested_off + requested_len];
                    let requested = split_nul_list(requested_blob);

                    let mut granted: Vec<String> = Vec::new();
                    for cap in requested {
                        if !registry.contains(&cap) {
                            continue;
                        }
                        if policy::should_grant(
                            subject_type,
                            &subject_id,
                            &cap,
                            &allow_db,
                            &registry,
                        ) {
                            granted.push(cap);
                        }
                    }

                    // レスポンスは NUL 区切りで返す
                    let mut out = Vec::new();
                    for s in granted {
                        out.extend_from_slice(s.as_bytes());
                        out.push(0);
                        if out.len() >= resp.data.len() {
                            break;
                        }
                    }
                    let n = core::cmp::min(out.len(), resp.data.len());
                    resp.data[..n].copy_from_slice(&out[..n]);
                    resp.len = n as u64;
                    resp.status = 0;
                }
            }
            CapabilityRequestMsg::OP_LIST_GRANTED => {
                // arg0 = pid
                let pid = req.arg0;
                let list = granted_by_pid.get(&pid).cloned().unwrap_or_default();
                let mut out = Vec::new();
                for s in list {
                    out.extend_from_slice(s.as_bytes());
                    out.push(0);
                    if out.len() >= resp.data.len() {
                        break;
                    }
                }
                let n = core::cmp::min(out.len(), resp.data.len());
                resp.data[..n].copy_from_slice(&out[..n]);
                resp.len = n as u64;
                resp.status = 0;
            }
            CapabilityRequestMsg::OP_RECORD_GRANTED => {
                // caller を信頼済みプロセスに限定
                if !is_trusted_grant_caller(sender) {
                    resp.status = -1;
                } else {
                    // arg0 = pid, data[0..len0] = granted NUL list
                    let pid = req.arg0;
                    let blob_len = core::cmp::min(req.len0 as usize, req.data.len());
                    let list = split_nul_list(&req.data[..blob_len]);
                    granted_by_pid.insert(pid, list);
                    resp.status = 0;
                }
            }
            _ => {
                resp.status = -38; // ENOSYS
            }
        }

        let resp_slice = unsafe {
            core::slice::from_raw_parts(
                &resp as *const _ as *const u8,
                core::mem::size_of::<CapabilityResponseMsg>(),
            )
        };
        let _ = ipc::ipc_send(sender, resp_slice);
    }
}
