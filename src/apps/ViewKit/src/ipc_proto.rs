//! Minimal IPC protocol constants shared by mochiOS UI clients.

/// Desktop/background layer (Binder).
pub const LAYER_DESKTOP: u8 = 0;
/// Status layer (Dock, overlays).
pub const LAYER_STATUS: u8 = 1;
/// Normal application windows.
pub const LAYER_APP: u8 = 2;

pub const OP_REQ_CREATE_WINDOW: u32 = 1;
pub const OP_RES_WINDOW_CREATED: u32 = 2;
pub const OP_REQ_FLUSH_CHUNK: u32 = 4;
pub const OP_REQ_ATTACH_SHARED: u32 = 5;
pub const OP_REQ_PRESENT_SHARED: u32 = 6;
pub const OP_RES_SHARED_ATTACHED: u32 = 7;

pub const IPC_BUF_SIZE: usize = 4128;

