//! ファイルシステムIPC定数（カーネル・サービス・ユーザー空間共通）

/// ファイルパスの最大長
pub const FS_PATH_MAX: usize = 512;

/// 1回のFS応答で送信可能なデータの最大サイズ
pub const FS_DATA_MAX: usize = 4096;

/// IPCメッセージの最大サイズ（kernel ipc.rs の MAX_MSG_SIZE と一致）
pub const IPC_MAX_MSG_SIZE: usize = 4128;

/// stat(2) のファイル種別マスク
pub const S_IFMT: u64 = 0o170000;
/// ディレクトリ
pub const S_IFDIR: u64 = 0o040000;
/// 通常ファイル
pub const S_IFREG: u64 = 0o100000;
