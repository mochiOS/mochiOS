//! ユーザー空間アドレス範囲の補助定数と判定

/// カーネルが許容するユーザー空間の上限
pub const USER_SPACE_END: u64 = 0x0000_7FFF_FFFF_FFFF;
/// IPC の外部ページマッピングが配置される下限
pub const USER_MAP_FLOOR: u64 = 0x7100_0000_0000;
/// ユーザー空間ページサイズ（4KiB）
pub const PAGE_SIZE: usize = 4096;

/// IPC で受け取ったマッピングがユーザー空間らしいかを軽く検証する
pub fn looks_like_user_mapping(addr: u64, bytes: usize) -> bool {
    if addr == 0 {
        return false;
    }
    if addr < USER_MAP_FLOOR {
        return false;
    }
    if addr > USER_SPACE_END {
        return false;
    }
    if (addr & 0xFFF) != 0 {
        return false;
    }
    if bytes < PAGE_SIZE {
        return false;
    }
    let end = match addr.checked_add(bytes.saturating_sub(1) as u64) {
        Some(e) => e,
        None => return false,
    };
    end <= USER_SPACE_END
}
