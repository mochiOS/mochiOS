use crate::syscall::{EINVAL, ENODATA, EPERM, SUCCESS};

/// 入力監視 API（tap）を呼び出せるか確認する
///
/// Service または Core 権限のみ許可する。
fn caller_has_input_privilege() -> bool {
    crate::task::current_thread_id()
        .and_then(|tid| crate::task::with_thread(tid, |t| t.process_id()))
        .and_then(|pid| {
            crate::task::with_process(pid, |p| {
                matches!(
                    p.privilege(),
                    crate::task::PrivilegeLevel::Core
                        | crate::task::PrivilegeLevel::Service
                        | crate::task::PrivilegeLevel::User
                )
            })
        })
        .unwrap_or(false)
}

/// PS/2 キーボードから rawスキャンコードを1バイト読み取り
/// バッファが空なら ENODATA を返す（変換はユーザー空間で行う）
pub fn read_char() -> u64 {
    match crate::util::ps2kbd::pop_scancode() {
        Some(sc) => sc as u64,
        None => ENODATA,
    }
}

/// ドライバ監視用キューから rawスキャンコードを1バイト読み取る（非破壊 tap）
pub fn read_char_tap() -> u64 {
    if !caller_has_input_privilege() {
        return EPERM;
    }
    match crate::util::ps2kbd::pop_tap_scancode() {
        Some(sc) => sc as u64,
        None => ENODATA,
    }
}

/// raw スキャンコードを通常入力キューへ注入する（Service/Core専用）
pub fn inject_scancode(scancode: u64) -> u64 {
    if !caller_has_input_privilege() {
        return EPERM;
    }
    if scancode > 0xFF {
        return EINVAL;
    }
    crate::util::ps2kbd::push_scancode(scancode as u8);
    SUCCESS
}

/// PS/2 キーボードから rawスキャンコードを1バイト読み取る（ブロッキング版）
///
/// バッファが空であれば、スキャンコードが届くまでスレッドをスリープして待機する。
/// IPC recv_blocking と同じ「登録→再確認→眠る」パターンで競合を回避する。
pub fn read_char_blocking() -> u8 {
    let tid = match crate::task::current_thread_id() {
        Some(id) => id,
        // カーネルスレッドからの呼び出し（通常は起きない）: スピンで待つ
        None => loop {
            if let Some(sc) = crate::util::ps2kbd::pop_scancode() {
                return sc;
            }
            crate::task::yield_now();
        },
    };

    loop {
        // waiter を登録してから pop を再試行することで、登録後に届いたスキャンコードを見逃さない
        crate::util::ps2kbd::register_waiter(tid.as_u64());

        if let Some(sc) = crate::util::ps2kbd::pop_scancode() {
            // データがあった → 起床不要なので waiter をクリアして返す
            crate::util::ps2kbd::unregister_waiter(tid.as_u64());
            return sc;
        }

        // データなし → pending_wakeup がなければスリープして yield
        if crate::task::sleep_thread_unless_woken(tid) {
            crate::task::yield_now();
            // 起床後にループしてデータを再確認
        }
        // pending_wakeup で即起床した場合もループして再確認
    }
}
