use crate::db::AllowDb;
use crate::protocol::SubjectType;
use crate::registry::{CapabilityRegistry, CapabilityRisk};

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
pub fn should_grant(
    subject_type: SubjectType,
    subject_id: &str,
    cap: &str,
    db: &AllowDb,
    registry: &CapabilityRegistry,
) -> bool {
    let lvl = registry.risk_of(cap);
    match lvl {
        CapabilityRisk::Normal => true,
        CapabilityRisk::Sensitive => db.allows(subject_type, subject_id, cap) || dev_allow_sensitive(),
        CapabilityRisk::Privileged => {
            (subject_type == SubjectType::Service && registry.is_bootstrap_trusted_service(subject_id))
                || db.allows(subject_type, subject_id, cap)
        }
        CapabilityRisk::Dangerous => {
            subject_type == SubjectType::Service
                && db.allows(subject_type, subject_id, cap)
                && dev_allow_dangerous()
        }
    }
}
