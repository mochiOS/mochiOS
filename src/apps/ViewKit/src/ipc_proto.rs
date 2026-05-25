//! Kagami IPC protocol constants used by ViewKit clients.

// Window server ops.
pub const OP_REQ_CREATE_WINDOW: u32 = 1;
pub const OP_RES_WINDOW_CREATED: u32 = 2;
pub const OP_REQ_FLUSH_CHUNK: u32 = 4;
pub const OP_REQ_ATTACH_SHARED: u32 = 5;
pub const OP_REQ_PRESENT_SHARED: u32 = 6;
pub const OP_RES_SHARED_ATTACHED: u32 = 7;

// Layering (smaller = further back).
pub const LAYER_DESKTOP: u8 = 0;
pub const LAYER_APP: u8 = 2;
pub const LAYER_STATUS: u8 = 3; // Dock etc.

