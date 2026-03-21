use crate::syscall::{EINVAL, ENODATA, EPERM, SUCCESS};

/// マウス入力監視 API を呼び出せるか確認する
///
/// Service または Core 権限のみ許可する。
fn caller_has_mouse_privilege() -> bool {
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
    if !caller_has_mouse_privilege() {
        return EPERM;
    }
    match crate::util::ps2mouse::pop_packet() {
        Some(packet) => packet as u64,
        None => ENODATA,
    }
}

/// 3バイト相当のマウスパケットを通常入力キューへ注入する（Service/Core専用）
///
/// `packet` は `b0 | (b1 << 8) | (b2 << 16)` 形式。
pub fn inject_packet(packet: u64) -> u64 {
    if !caller_has_mouse_privilege() {
        return EPERM;
    }
    if packet > 0x00FF_FFFF {
        return EINVAL;
    }
    let mut b0 = (packet & 0xFF) as u8;
    let b1 = ((packet >> 8) & 0xFF) as u8;
    let b2 = ((packet >> 16) & 0xFF) as u8;
    // caller が buttons のみ渡した場合でもパケット同期できるよう補完
    if (b0 & 0x08) == 0 {
        b0 = (b0 & 0x07) | 0x08;
        if (b1 & 0x80) != 0 {
            b0 |= 1 << 4;
        }
        if (b2 & 0x80) != 0 {
            b0 |= 1 << 5;
        }
    }
    crate::util::ps2mouse::push_byte(b0);
    crate::util::ps2mouse::push_byte(b1);
    crate::util::ps2mouse::push_byte(b2);
    SUCCESS
}
