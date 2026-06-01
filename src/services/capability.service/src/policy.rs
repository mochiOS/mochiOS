use crate::db::AllowDb;
use crate::protocol::SubjectType;

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
        "ipc.client"
        | "ipc.server"
        | "process.spawn"
        | "process.inspect"
        | "fs.read.user.documents"
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
    // 署名/検証が未実装でも、OS の起動に必須で明示的に信頼できるものだけ例外にする。
    matches!(
        id,
        "core.service"
            | "capability.service"
            | "driver.service"
            | "disk.service"
            | "process.service"
            | "device.service"
            | "net.service"
            | "window.service"
            | "shell.service"
    )
}

fn dev_allow_sensitive() -> bool {
    // UI 未実装のため、Sensitive は原則 deny-by-default。
    // 開発時のみ、明示設定があれば仮許可できるようにする（存在でスイッチ）。
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

/// 指定 capability を付与してよいか（deny-by-default）
pub fn should_grant(subject_type: SubjectType, subject_id: &str, cap: &str, db: &AllowDb) -> bool {
    let lvl = classify(cap);
    match lvl {
        CapabilityLevel::Normal => true,
        CapabilityLevel::Sensitive => db.allows(subject_type, subject_id, cap) || dev_allow_sensitive(),
        CapabilityLevel::Privileged => {
            (subject_type == SubjectType::Service && is_bootstrap_trusted_service(subject_id))
                || db.allows(subject_type, subject_id, cap)
        }
        CapabilityLevel::Dangerous => {
            subject_type == SubjectType::Service
                && db.allows(subject_type, subject_id, cap)
                && dev_allow_dangerous()
        }
    }
}
