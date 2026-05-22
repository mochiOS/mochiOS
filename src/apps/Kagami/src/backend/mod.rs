use async_trait::async_trait;
use std::fmt;

/// フレームバッファのピクセルフォーマット
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// ARGB 8888
    ARGB8888,
    /// XRGB 8888
    XRGB8888,
    /// RGBA 8888
    RGBA8888,
    /// RGB 565
    RGB565,
    /// Custom format
    Custom(u32),
}

impl PixelFormat {
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::ARGB8888 | PixelFormat::XRGB8888 | PixelFormat::RGBA8888 => 4,
            PixelFormat::RGB565 => 2,
            PixelFormat::Custom(_) => 4,
        }
    }
}

/// フレームバッファの情報
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub phys_addr: u64,
}

impl FramebufferInfo {
    pub fn total_size(&self) -> usize {
        (self.stride * self.height) as usize
    }

    pub fn pixel_size(&self) -> usize {
        ((self.width * self.format.bytes_per_pixel() as u32) * self.height) as usize
    }
}

/// バックエンド実装が返すエラー型
#[derive(Debug, Clone)]
pub enum BackendError {
    /// 初期化失敗
    InitializationFailed(String),
    /// メモリマッピング失敗
    MemoryMapFailed(String),
    /// 書き込み失敗
    WriteFailed(String),
    /// デバイスロック失敗
    LockFailed(String),
    /// その他エラー
    Other(String),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendError::InitializationFailed(msg) => write!(f, "Initialization failed: {}", msg),
            BackendError::MemoryMapFailed(msg) => write!(f, "Memory map failed: {}", msg),
            BackendError::WriteFailed(msg) => write!(f, "Write failed: {}", msg),
            BackendError::LockFailed(msg) => write!(f, "Lock failed: {}", msg),
            BackendError::Other(msg) => write!(f, "Backend error: {}", msg),
        }
    }
}

impl std::error::Error for BackendError {}

/// バックエンド実装の共通トレイト
#[async_trait]
pub trait FramebufferBackend: Send + Sync {
    /// バックエンドの初期化
    async fn init(&mut self) -> Result<FramebufferInfo, BackendError>;

    /// フレームバッファへの参照を取得（読み取り専用）
    /// 実装側でロック機構を提供可能
    fn framebuffer(&self) -> &[u8];

    /// フレームバッファへの可変参照を取得（書き込み用）
    /// 内部的にはロック機構で保護されている可能性がある
    fn framebuffer_mut(&mut self) -> &mut [u8];

    /// 指定領域に描画データを書き込み（最適化用）
    /// デフォルト実装：framebuffer_mut を使用
    fn write_region(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<(), BackendError> {
        let fb_info = self.info();
        let fb = self.framebuffer_mut();

        let bytes_per_pixel = fb_info.format.bytes_per_pixel();
        let stride = fb_info.stride as usize;

        let start_offset = (y * fb_info.stride + x as u32 * bytes_per_pixel as u32) as usize;

        for row in 0..height {
            let src_offset = (row * width as u32 * bytes_per_pixel as u32) as usize;
            let dst_offset = start_offset + row as usize * stride;

            let row_bytes = (width as u32 * bytes_per_pixel as u32) as usize;
            if src_offset + row_bytes <= data.len() && dst_offset + row_bytes <= fb.len() {
                fb[dst_offset..dst_offset + row_bytes]
                    .copy_from_slice(&data[src_offset..src_offset + row_bytes]);
            }
        }

        Ok(())
    }

    /// フレームバッファの情報を取得
    fn info(&self) -> FramebufferInfo;

    /// フレームバッファをクリア
    fn clear(&mut self, color: u32) -> Result<(), BackendError> {
        let info = self.info();
        let fb = self.framebuffer_mut();

        let bytes_per_pixel = info.format.bytes_per_pixel();

        match bytes_per_pixel {
            4 => {
                let color_bytes = color.to_le_bytes();
                for chunk in fb.chunks_exact_mut(4) {
                    chunk.copy_from_slice(&color_bytes);
                }
            }
            2 => {
                let color_u16 = (color & 0xFFFF) as u16;
                for chunk in fb.chunks_exact_mut(2) {
                    chunk.copy_from_slice(&color_u16.to_le_bytes());
                }
            }
            _ => {
                return Err(BackendError::WriteFailed(
                    format!("Unsupported pixel size: {}", bytes_per_pixel),
                ));
            }
        }

        Ok(())
    }

    /// フレームバッファの画面更新（同期）
    /// 必要に応じて実装（例：フロントバッファとバックバッファのスワップ）
    async fn flush(&mut self) -> Result<(), BackendError> {
        Ok(())
    }

    /// バックエンド名の取得
    fn name(&self) -> &'static str;
}

#[async_trait]
impl<T: FramebufferBackend + ?Sized> FramebufferBackend for Box<T> {
    async fn init(&mut self) -> Result<FramebufferInfo, BackendError> {
        (**self).init().await
    }

    fn framebuffer(&self) -> &[u8] {
        (**self).framebuffer()
    }

    fn framebuffer_mut(&mut self) -> &mut [u8] {
        (**self).framebuffer_mut()
    }
    fn write_region(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<(), BackendError> {
        (**self).write_region(x, y, width, height, data)
    }

    fn info(&self) -> FramebufferInfo {
        (**self).info()
    }

    fn clear(&mut self, color: u32) -> Result<(), BackendError> {
        (**self).clear(color)
    }

    async fn flush(&mut self) -> Result<(), BackendError> {
        (**self).flush().await
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }
}

#[cfg(feature = "backend-linux-fb")]
pub mod linux_fb;

#[cfg(feature = "backend-mochios-vga")]
pub mod vga;

#[cfg(any(feature = "backend-generic-memory", test))]
pub mod memory;

#[cfg(feature = "backend-custom")]
pub mod custom;

#[cfg(feature = "backend-linux-fb")]
pub use linux_fb::LinuxFramebufferBackend;

#[cfg(all(feature = "backend-mochios-vga", not(feature = "backend-linux-fb")))]
pub use vga::MochiVgaBackend;

#[cfg(all(
    any(feature = "backend-generic-memory", test),
    not(any(feature = "backend-linux-fb", feature = "backend-mochios-vga"))
))]
pub use memory::MemoryFramebufferBackend;

#[cfg(all(
    feature = "backend-custom",
    not(any(
        feature = "backend-linux-fb",
        feature = "backend-mochios-vga",
        feature = "backend-generic-memory"
    ))
))]
pub use custom::CustomFramebufferBackend;
