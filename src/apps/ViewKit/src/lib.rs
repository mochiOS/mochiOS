//! viewKit - UI library for Rust / Kome

pub mod app;
pub mod backend;
mod ffi;
pub mod pipeline;
pub mod ui;
pub mod vcompon
pub mod components;
pub use viewkit_macros::components;

pub use vcomponent::{
    render_component_to_pixmap, render_component_to_pixmap_with_asset_root,
    render_component_to_pixmap_with_asset_root_and_boxes, render_ui_element_to_pixmap,
    render_ui_element_to_pixmap_with_asset_root, VComponent,
};

pub use app::App;
