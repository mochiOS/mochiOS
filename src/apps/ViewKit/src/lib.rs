//! viewKit - UI library for Rust / Kome

pub mod app;
pub mod backend;
mod ffi;
pub mod pipeline;
pub mod ui;
pub mod vcomponent;
// re-export proc-macro so crate users can call `components!{ ... }` without prefix
pub use viewkit_macros::components;

pub use vcomponent::{
    render_component_to_pixmap, render_component_to_pixmap_with_asset_root, VComponent,
};
