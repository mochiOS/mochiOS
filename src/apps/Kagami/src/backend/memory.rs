use super::{BackendError, FramebufferBackend, FramebufferInfo, PixelFormat};
use async_trait::async_trait;

/// 汎用メモリバックエンド
///
/// 環境変数で解像度などを指定可能：
/// - FB_WIDTH / FB_HEIGHT / FB_STRIDE / FB_FORMAT
pub struct MemoryFramebufferBackend {
    fb_data: Vec<u8>,
    info: FramebufferInfo,
}

impl MemoryFramebufferBackend {
    pub fn from_env() -> Self {
        let mut backend = Self::with_info(FramebufferInfo {
            width: 0,
            height: 0,
            stride: 0,
            format: PixelFormat::XRGB8888,
            phys_addr: 0,
        });
        backend.info = backend.get_fb_info();
        backend
    }

    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        let stride = width.saturating_mul(format.bytes_per_pixel() as u32);
        Self {
            fb_data: Vec::new(),
            info: FramebufferInfo {
                width,
                height,
                stride,
                format,
                phys_addr: 0,
            },
        }
    }

    pub fn with_info(info: FramebufferInfo) -> Self {
        Self {
            fb_data: Vec::new(),
            info,
        }
    }

    fn get_fb_info(&self) -> FramebufferInfo {
        let width = std::env::var("FB_WIDTH")
            .unwrap_or_else(|_| "1024".to_string())
            .parse::<u32>()
            .unwrap_or(1024);

        let height = std::env::var("FB_HEIGHT")
            .unwrap_or_else(|_| "768".to_string())
            .parse::<u32>()
            .unwrap_or(768);

        let default_stride = width.saturating_mul(4);
        let stride = std::env::var("FB_STRIDE")
            .unwrap_or_else(|_| default_stride.to_string())
            .parse::<u32>()
            .unwrap_or(default_stride);

        let format = match std::env::var("FB_FORMAT")
            .unwrap_or_else(|_| "0".to_string())
            .as_str()
        {
            "0" => PixelFormat::XRGB8888,
            "1" => PixelFormat::ARGB8888,
            "2" => PixelFormat::RGBA8888,
            "565" => PixelFormat::RGB565,
            other => PixelFormat::Custom(other.parse::<u32>().unwrap_or(0)),
        };

        FramebufferInfo {
            width,
            height,
            stride,
            format,
            phys_addr: 0,
        }
    }
}

impl Default for MemoryFramebufferBackend {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait]
impl FramebufferBackend for MemoryFramebufferBackend {
    async fn init(&mut self) -> Result<FramebufferInfo, BackendError> {
        if self.info.width == 0 || self.info.height == 0 || self.info.stride == 0 {
            self.info = self.get_fb_info();
        }

        let size = self.info.total_size();
        self.fb_data.resize(size, 0);
        Ok(self.info)
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
        "Generic memory framebuffer"
    }
}
