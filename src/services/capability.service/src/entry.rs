use swiftlib::ipc;
use swiftlib::task;

/// READY通知OPコード
const OP_NOTIFY_READY: u64 = 0xFF;

/// IPC リクエスト
///
/// 固定長バッファでやり取りする（このOSのIPCは生バイト転送が基本のため）。
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

impl CapabilityRequestMsg {
    const OP_RESOLVE: u64 = 1;
    const OP_CHECK: u64 = 2;
    const OP_GRANT_FOR_EXEC: u64 = 3;
    const OP_LIST_GRANTED: u64 = 4;
    const OP_RECORD_GRANTED: u64 = 5;
}

/// IPC レスポンス
#[repr(C)]
#[derive(Clone, Copy)]
struct CapabilityResponseMsg {
    status: i64,
    len: u64,
    data: [u8; 512],
}

#[repr(align(8))]
struct AlignedBuf([u8; 576]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubjectType {
    App = 1,
    Service = 2,
}

impl SubjectType {
    fn from_u64(v: u64) -> Option<Self> {
        match v {
            1 => Some(Self::App),
            2 => Some(Self::Service),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CapabilityLevel {
    Normal,
    Sensitive,
    Privileged,
    Dangerous,
}

/// capability の危険度を分類する
fn classify(cap: &str) -> CapabilityLevel {
    match cap {
        // Normal
        "fs.read.user.documents"
        | "fs.write.user.documents"
        | "window.create"
        | "audio.playback"
        | "notification.send"
        | "system.time.read"
        | "system.info.read"
        | "account.self.read" => CapabilityLevel::Normal,

        // Sensitive
        "clipboard.read"
        | "audio.record"
        | "camera.access"
        | "microphone.access"
        | "location.access"
        | "display.capture"
        | "window.capture"
        | "input.keyboard.global"
        | "input.pointer.global"
        | "fs.read.user"
        | "fs.write.user"
        | "net.listen" => CapabilityLevel::Sensitive,

        // Privileged
        "fs.read.all"
        | "fs.write.all"
        | "net.raw"
        | "process.kill"
        | "service.register"
        | "service.control"
        | "package.install"
        | "package.remove"
        | "package.update"
        | "device.storage"
        | "device.gpu"
        | "device.input"
        | "device.audio"
        | "device.net"
        | "system.time.set"
        | "system.logs.read" => CapabilityLevel::Privileged,

        // Dangerous
        "kernel.module.load" | "kernel.debug" | "unsandboxed" | "developer.debug"
        | "developer.tracing" => CapabilityLevel::Dangerous,

        // 未分類は保守的に Sensitive 扱い（deny-by-default に倒す）
        _ => CapabilityLevel::Sensitive,
    }
}

fn is_bootstrap_trusted_service(id: &str) -> bool {
    matches!(id, "core.service" | "capability.service" | "fs.service")
}

fn dev_allow_sensitive() -> bool {
    // UI 未実装のため、Sensitive は原則 deny-by-default。
    // 開発ビルド時のみ、明示設定があれば仮許可できるようにする。
    // ここでは簡易にファイルの存在でスイッチする。
    std::fs::read_to_string("/config/allow_sensitive_caps")
        .ok()
        .map(|s| s.trim() == "1" || s.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn dev_allow_dangerous() -> bool {
    std::fs::read_to_string("/config/allow_dangerous_caps")
        .ok()
        .map(|s| s.trim() == "1" || s.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// registry: capabilities.toml に定義されているかを確認する
fn load_registry_set() -> std::collections::BTreeSet<String> {
    let text = include_str!("../resources/capabilities.toml");
    let mut set = std::collections::BTreeSet::new();

    // 依存クレートを増やさないため、TOML を完全には解析せず、
    // セクションヘッダの `[capabilities.<name>]` だけを拾う。
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('[') || !line.ends_with(']') {
            continue;
        }
        let inside = &line[1..line.len() - 1];
        let Some(rest) = inside.strip_prefix("capabilities.") else {
            continue;
        };
        // rest は `fs.read.user.documents` のような名前になる
        if !rest.is_empty() {
            set.insert(rest.to_string());
        }
    }
    set
}

/// core.service に準備完了を通知する
fn notify_ready_to_core() {
    let core_pid = match task::find_process_by_name("core.service") {
        Some(pid) => pid,
        None => {
            println!("[CAP] WARNING: core.service not found, skipping READY notify");
            return;
        }
    };

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

/// 許可DB（subject_id ごとの許可された capability）
///
/// ファイル形式（簡易）:
/// - `service:<id>=cap1,cap2,...`
/// - `app:<id>=cap1,cap2,...`
///
/// 例:
/// `service:net.service=net.raw,device.net`
fn load_allow_db() -> (
    std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) {
    let mut services = std::collections::BTreeMap::new();
    let mut apps = std::collections::BTreeMap::new();

    let text = match std::fs::read_to_string("/config/capabilities.db") {
        Ok(t) => t,
        Err(_) => return (services, apps),
    };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((lhs, rhs)) = line.split_once('=') else {
            continue;
        };
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        let (kind, id) = if let Some(rest) = lhs.strip_prefix("service:") {
            ("service", rest.trim())
        } else if let Some(rest) = lhs.strip_prefix("app:") {
            ("app", rest.trim())
        } else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let mut set = std::collections::BTreeSet::new();
        for cap in rhs.split(',') {
            let cap = cap.trim();
            if !cap.is_empty() {
                set.insert(cap.to_string());
            }
        }
        if kind == "service" {
            services.insert(id.to_string(), set);
        } else {
            apps.insert(id.to_string(), set);
        }
    }

    (services, apps)
}

fn db_allows(
    subject_type: SubjectType,
    subject_id: &str,
    cap: &str,
    svc_db: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    app_db: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) -> bool {
    match subject_type {
        SubjectType::Service => svc_db
            .get(subject_id)
            .map(|s| s.contains(cap))
            .unwrap_or(false),
        SubjectType::App => app_db
            .get(subject_id)
            .map(|s| s.contains(cap))
            .unwrap_or(false),
    }
}

fn main() {
    println!("[CAP] capability.service started");

    // registry をロード（未知 capability の typo を弾くため）
    let registry = load_registry_set();
    println!("[CAP] registry loaded: {} entries", registry.len());

    // allow DB をロード（ユーザー許可/UI の代替）
    let (svc_allow_db, app_allow_db) = load_allow_db();
    println!(
        "[CAP] allow db loaded: services={} apps={}",
        svc_allow_db.len(),
        app_allow_db.len()
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

                match swiftlib::capability::check_thread_capability(thread_id, &cap) {
                    Ok(ok) => {
                        resp.status = if ok { 1 } else { 0 };
                    }
                    Err(errno) => resp.status = errno,
                }
            }
            CapabilityRequestMsg::OP_GRANT_FOR_EXEC => {
                // 権限昇格の入口になるため、呼び出し元を信頼済みプロセスに限定する。
                // ここを緩めると、任意プロセスが `unsandboxed` 等を要求して自己昇格できる。
                let caller_name = task::find_process_by_name("core.service")
                    .filter(|pid| *pid == sender)
                    .map(|_| "core.service")
                    .or_else(|| {
                        task::find_process_by_name("process.service")
                            .filter(|pid| *pid == sender)
                            .map(|_| "process.service")
                    });
                if caller_name.is_none() {
                    resp.status = -1;
                    let resp_slice = unsafe {
                        core::slice::from_raw_parts(
                            &resp as *const _ as *const u8,
                            core::mem::size_of::<CapabilityResponseMsg>(),
                        )
                    };
                    let _ = ipc::ipc_send(sender, resp_slice);
                    continue;
                }

                // arg0 = subject_type, len0 = subject_id_len, len1 = requested_blob_len
                let Some(subject_type) = SubjectType::from_u64(req.arg0) else {
                    resp.status = -22;
                    let _ = ipc::ipc_send(sender, unsafe {
                        core::slice::from_raw_parts(
                            &resp as *const _ as *const u8,
                            core::mem::size_of::<CapabilityResponseMsg>(),
                        )
                    });
                    continue;
                };

                let subject_id_len = req.len0 as usize;
                let requested_len = req.len1 as usize;
                let Some(subject_id) = read_str_from_msg(&req, 0, subject_id_len) else {
                    resp.status = -22;
                    let _ = ipc::ipc_send(sender, unsafe {
                        core::slice::from_raw_parts(
                            &resp as *const _ as *const u8,
                            core::mem::size_of::<CapabilityResponseMsg>(),
                        )
                    });
                    continue;
                };

                let requested_off = subject_id_len;
                if requested_off + requested_len > req.data.len() {
                    resp.status = -22;
                    let _ = ipc::ipc_send(sender, unsafe {
                        core::slice::from_raw_parts(
                            &resp as *const _ as *const u8,
                            core::mem::size_of::<CapabilityResponseMsg>(),
                        )
                    });
                    continue;
                }
                let requested_blob = &req.data[requested_off..requested_off + requested_len];
                let requested = split_nul_list(requested_blob);

                let mut granted: Vec<String> = Vec::new();
                for cap in requested {
                    if !registry.contains(&cap) {
                        continue;
                    }
                    let lvl = classify(&cap);
                    let allow = match lvl {
                        CapabilityLevel::Normal => true,
                        CapabilityLevel::Sensitive => {
                            // UI 未実装: 基本 deny。allow DB または開発フラグでのみ許可。
                            db_allows(subject_type, &subject_id, &cap, &svc_allow_db, &app_allow_db)
                                || dev_allow_sensitive()
                        }
                        CapabilityLevel::Privileged => {
                            // 署名/検証未実装のため、現状は Service + allow DB のみ許可。
                            // ただし bootstrap 例外はコード上で明示的に許可してよい。
                            (subject_type == SubjectType::Service
                                && is_bootstrap_trusted_service(&subject_id))
                                || db_allows(
                                    subject_type,
                                    &subject_id,
                                    &cap,
                                    &svc_allow_db,
                                    &app_allow_db,
                                )
                        }
                        CapabilityLevel::Dangerous => {
                            // Dangerous は debug/developer mode 相当の設定が必要。
                            // ここでは allow DB + allow_dangerous_caps の両方を要求する。
                            subject_type == SubjectType::Service
                                && db_allows(
                                    subject_type,
                                    &subject_id,
                                    &cap,
                                    &svc_allow_db,
                                    &app_allow_db,
                                )
                                && dev_allow_dangerous()
                        }
                    };
                    if allow {
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
                let caller_ok = task::find_process_by_name("core.service")
                    .filter(|pid| *pid == sender)
                    .is_some()
                    || task::find_process_by_name("process.service")
                        .filter(|pid| *pid == sender)
                        .is_some();
                if !caller_ok {
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
