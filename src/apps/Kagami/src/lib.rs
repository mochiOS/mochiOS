pub mod backend;
pub mod protocol;
pub mod compositor;
pub mod surface;
pub mod client;
pub mod error;

pub use compositor::Compositor;
pub use error::{CompositorError, Result};
pub use surface::Surface;

pub use backend::FramebufferBackend;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert!(true);
    }
}