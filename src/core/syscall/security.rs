use crate::capability::Capability;
use crate::task::{self, ids::ProcessId, PrivilegeLevel};

#[inline]
pub fn current_thread_id() -> Option<crate::task::ThreadId> {
    task::current_thread_id()
}

#[inline]
pub fn current_process_id() -> Option<ProcessId> {
    current_thread_id().and_then(|tid| task::with_thread(tid, |t| t.process_id()))
}

#[inline]
pub fn current_process_privilege() -> Option<PrivilegeLevel> {
    current_process_id().and_then(|pid| task::with_process(pid, |p| p.privilege()))
}

#[inline]
pub fn process_has_any_capability(pid: ProcessId, caps: &[Capability]) -> bool {
    caps.iter()
        .copied()
        .any(|cap| task::process::process_has_capability(pid, cap))
}

#[inline]
pub fn caller_has_any_capability(caps: &[Capability]) -> bool {
    current_process_id()
        .map(|pid| process_has_any_capability(pid, caps))
        .unwrap_or(false)
}

#[inline]
pub fn caller_has_privilege(levels: &[PrivilegeLevel]) -> bool {
    current_process_privilege()
        .map(|privilege| levels.iter().any(|level| *level == privilege))
        .unwrap_or(false)
}

#[inline]
pub fn caller_is_core_or_service() -> bool {
    caller_has_privilege(&[PrivilegeLevel::Core, PrivilegeLevel::Service])
}
