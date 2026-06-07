//! Built-in ViewKit components generated at build time.
//!
//! Usage:
//! - `use viewkit::components;`
//! - `let ui = components::card().children([ ... ]).into_elem();`
//!
//! The list of components is derived automatically from
//! `resources/components/*.html` in this crate.

include!(concat!(env!("OUT_DIR"), "/viewkit_builtin_components.rs"));
