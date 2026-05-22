#[cfg(feature = "wayland-host")]
pub mod backend;
#[cfg(feature = "wayland-host")]
pub mod protocol;
#[cfg(feature = "wayland-host")]
pub mod compositor;
#[cfg(feature = "wayland-host")]
pub mod surface;
#[cfg(feature = "wayland-host")]
pub mod client;
#[cfg(feature = "wayland-host")]
pub mod error;

#[cfg(feature = "wayland-host")]
pub use compositor::Compositor;
#[cfg(feature = "wayland-host")]
pub use error::{CompositorError, Result};
#[cfg(feature = "wayland-host")]
pub use surface::Surface;

#[cfg(feature = "wayland-host")]
pub use backend::FramebufferBackend;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert!(true);
    }
}
