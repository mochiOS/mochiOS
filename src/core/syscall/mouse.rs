use crate::syscall::{ENODATA, EPERM};

/// マウス入力監視 API を呼び出せるか確認する
///
/// Service または Core 権限のみ許可する。
fn caller_has_mouse_read_privilege() -> bool {
    crate::task::current_thread_id()
        .and_then(|tid| crate::task::with_thread(tid, |t| t.process_id()))
        .and_then(|pid| {
            crate::task::with_process(pid, |p| {
                matches!(
                    p.privilege(),
                    crate::task::PrivilegeLevel::Core | crate::task::PrivilegeLevel::Service
                )
            })
        })
        .unwrap_or(false)
}

/// PS/2 マウスパケットを 1 つ読み取る（非ブロッキング）
///
/// 返り値は `b0 | (b1 << 8) | (b2 << 16)` 形式。
/// キューが空なら ENODATA。
pub fn read_packet() -> u64 {
    if !caller_has_mouse_read_privilege() {
        return EPERM;
    }
    match crate::util::ps2mouse::pop_packet() {
        Some(packet) => packet as u64,
        None => ENODATA,
    }
}
