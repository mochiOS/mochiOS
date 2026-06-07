use super::{BackendError, FramebufferBackend, FramebufferInfo, PixelFormat};
use async_trait::async_trait;

/// ダミーバックエンド
pub struct CustomFramebufferBackend {
    info: FramebufferInfo,
    fb_data: Vec<u8>,
}

impl CustomFramebufferBackend {
    pub fn new() -> Self {
        Self {
            info: FramebufferInfo {
                width: 0,
                height: 0,
                stride: 0,
                format: PixelFormat::XRGB8888,
                phys_addr: 0,
            },
            fb_data: Vec::new(),
        }
    }
}

impl Default for CustomFramebufferBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FramebufferBackend for CustomFramebufferBackend {
    async fn init(&mut self) -> Result<FramebufferInfo, BackendError> {
        Err(BackendError::InitializationFailed(
            "Custom backend is not implemented. Provide your own backend implementation.".to_string(),
        ))
    }

    fn framebuffer(&self) -> &[u8] {
        &self.fb_data
    }

    fn framebuffer_mut(&mut self) -> &mut [u8] {
        &mut self.fb_data
    }

    fn info(&self) -> FramebufferInfo {
        self.info
    }

    fn name(&self) -> &'static str {
        "Custom (unimplemented)"
    }
}

