//! capability 関連の syscall
//!
//! カーネルはプロセスに紐づく `CapabilitySet` を保持し、各サービスが caller を検査できるように
//! 最低限の照会 API を提供する。
//!
//! policy 判定（危険度分類、ユーザー許可 UI、manifest 解析など）は capability.service 側へ寄せる。

extern crate alloc;

use alloc::vec::Vec;

use crate::capability::Capability;
use crate::syscall::copy_from_user;
use crate::syscall::types::{EFAULT, EINVAL};

/// 指定スレッドが capability を持つか確認する
///
/// - `thread_id`: 照会対象のスレッドID（IPCの sender をそのまま渡す想定）
/// - `cap_ptr` / `cap_len`: UTF-8 の capability 名（例: `fs.read.user.documents`）
///
/// 戻り値:
/// - `1` = 許可
/// - `0` = 不許可
/// - `EINVAL/EFAULT` = 不正な引数
pub fn check_thread_capability(thread_id: u64, cap_ptr: u64, cap_len: u64) -> u64 {
    // 過剰なコピーを避けるため、ここでは短い上限を設ける。
    // capability 名は固定の識別子であり、長大な文字列である必要がない。
    let max_cap_name_len = crate::config::kernel().capability.max_name_len;

    if thread_id == 0 || cap_ptr == 0 || cap_len == 0 {
        return EINVAL;
    }
    let Ok(cap_len_usize) = usize::try_from(cap_len) else {
        return EINVAL;
    };
    if cap_len_usize > max_cap_name_len {
        return EINVAL;
    }

    let mut buf = Vec::with_capacity(cap_len_usize);
    buf.resize(cap_len_usize, 0u8);
    if copy_from_user(cap_ptr, &mut buf).is_err() {
        return EFAULT;
    }

    let Ok(name) = core::str::from_utf8(&buf) else {
        return EINVAL;
    };
    let Some(cap) = Capability::from_str(name) else {
        return EINVAL;
    };

    let Some(pid) = crate::task::thread_to_process_id(thread_id) else {
        return 0;
    };
    if crate::task::process::process_has_capability(pid, cap) {
        1
    } else {
        0
    }
}
