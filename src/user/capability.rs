//! capability（権限）照会 syscall（ユーザー側）
//!
//! 各サービスが IPC の sender（スレッドID）を元に、操作の許可をカーネルへ照会する。
//! capability の付与・ポリシー決定は capability.service 側が担当し、ここは照会のみを提供する。

use super::sys::{syscall3, SyscallNumber, EINVAL};

/// 指定スレッドが capability を持つか確認する
///
/// 戻り値:
/// - `Ok(true)`  = 許可
/// - `Ok(false)` = 不許可
/// - `Err(errno)` = 引数不正など
pub fn check_thread_capability(thread_id: u64, capability: &str) -> Result<bool, i64> {
    let bytes = capability.as_bytes();
    if thread_id == 0 || bytes.is_empty() || bytes.len() > 128 {
        return Err(EINVAL as i64);
    }
    let ret = syscall3(
        SyscallNumber::CheckThreadCapability as u64,
        thread_id,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
    );

    // エラーは負の i64 を u64 にキャストした値として返る
    let ret_i64 = ret as i64;
    if ret_i64 < 0 {
        Err(ret_i64)
    } else {
        Ok(ret != 0)
    }
}

