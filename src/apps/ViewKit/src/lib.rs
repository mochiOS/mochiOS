//! viewKit - UI library for Rust / Kome

pub mod app;
pub mod backend;
pub mod ipc_proto;
pub mod platform;
#[cfg(all(target_os = "linux", target_env = "musl"))]
mod mochios;
mod ffi;
pub mod pipeline;
pub mod ui;
pub mod vcomponent;
pub mod window;
pub mod components;
pub use viewkit_macros::components;

pub use vcomponent::{
    render_component_to_pixmap, render_component_to_pixmap_with_asset_root,
    render_component_to_pixmap_with_asset_root_and_boxes, render_ui_element_to_pixmap,
    render_ui_element_to_pixmap_with_asset_root, measure_component_boxes, VComponent,
};

pub use app::App;
pub use window::Window;
