//! viewKit - UI library for Rust / Kome

pub mod app;
pub mod backend;
mod ffi;
pub mod pipeline;
pub mod ui;
// re-export proc-macro so crate users can call `components!{ ... }` without prefix
pub use viewkit_macros::components;
