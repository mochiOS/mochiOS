/// capability.service IPC プロトコル定義
///
/// このOSのIPCは「固定長の生バイト転送」が基本なので、ここでも固定長メッセージを使う。

/// READY通知OPコード
pub const OP_NOTIFY_READY: u64 = 0xFF;

/// IPC リクエスト（固定長）
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CapabilityRequestMsg {
    pub op: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub len0: u64,
    pub len1: u64,
    pub data: [u8; 512],
}

impl CapabilityRequestMsg {
    pub const OP_RESOLVE: u64 = 1;
    pub const OP_CHECK: u64 = 2;
    pub const OP_GRANT_FOR_EXEC: u64 = 3;
    pub const OP_LIST_GRANTED: u64 = 4;
    pub const OP_RECORD_GRANTED: u64 = 5;
}

/// IPC レスポンス（固定長）
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CapabilityResponseMsg {
    /// 元リクエストの op（呼び出し側で相関を取るためにエコーバックする）
    pub op: u64,
    pub status: i64,
    pub len: u64,
    pub data: [u8; 512],
}

#[repr(align(8))]
pub struct AlignedBuf(pub [u8; 576]);

/// 対象種別
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubjectType {
    App = 1,
    Service = 2,
}

impl SubjectType {
    pub fn from_u64(v: u64) -> Option<Self> {
        match v {
            1 => Some(Self::App),
            2 => Some(Self::Service),
            _ => None,
        }
    }
}
