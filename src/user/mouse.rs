//! マウス系システムコール（ユーザー側）

use super::sys::{syscall0, SyscallNumber, ENODATA};

/// PS/2 3バイトパケットを展開した入力イベント
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MousePacket {
    /// ボタンビット（bit0=Left, bit1=Right, bit2=Middle）
    pub buttons: u8,
    /// X移動量（符号付き）
    pub dx: i8,
    /// Y移動量（符号付き、PS/2生値）
    pub dy: i8,
}

impl MousePacket {
    #[inline]
    pub fn left(&self) -> bool {
        (self.buttons & 0x01) != 0
    }

    #[inline]
    pub fn right(&self) -> bool {
        (self.buttons & 0x02) != 0
    }

    #[inline]
    pub fn middle(&self) -> bool {
        (self.buttons & 0x04) != 0
    }
}

/// PS/2 マウスパケットを1件読み取る（非ブロッキング）
pub fn read_packet() -> Result<Option<MousePacket>, u64> {
    let ret = syscall0(SyscallNumber::MouseRead as u64);
    if ret == ENODATA {
        return Ok(None);
    }
    if (ret as i64) < 0 {
        return Err(ret);
    }

    let b0 = (ret & 0xFF) as u8;
    let b1 = ((ret >> 8) & 0xFF) as u8;
    let b2 = ((ret >> 16) & 0xFF) as u8;
    Ok(Some(MousePacket {
        buttons: b0 & 0x07,
        dx: b1 as i8,
        dy: b2 as i8,
    }))
}
