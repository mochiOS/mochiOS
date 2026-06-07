use super::{BackendError, FramebufferBackend, FramebufferInfo, PixelFormat};
use async_trait::async_trait;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};

const FB_DEVICE: &str = "/dev/fb0";

/// Linux フレームバッファドライババックエンド
pub struct LinuxFramebufferBackend {
    fb_file: Option<std::fs::File>,
    fb_data: Vec<u8>,
    info: FramebufferInfo,
}

impl LinuxFramebufferBackend {
    pub fn new() -> Self {
        Self {
            fb_file: None,
            fb_data: Vec::new(),
            info: FramebufferInfo {
                width: 0,
                height: 0,
                stride: 0,
                format: PixelFormat::XRGB8888,
                phys_addr: 0,
            },
        }
    }

    /// /dev/fb0 から フレームバッファ情報を取得
    /// 実装簡略化のため、環境変数で指定する方法も併用
    fn get_fb_info(&self) -> Result<FramebufferInfo, BackendError> {
        // 環境変数から取得（設定方法：
        // export FB_WIDTH=1024 FB_HEIGHT=768 FB_STRIDE=4096 FB_FORMAT=0
        // ）
        let width = std::env::var("FB_WIDTH")
            .unwrap_or_else(|_| "1024".to_string())
            .parse::<u32>()
            .unwrap_or(1024);

        let height = std::env::var("FB_HEIGHT")
            .unwrap_or_else(|_| "768".to_string())
            .parse::<u32>()
            .unwrap_or(768);

        let stride = std::env::var("FB_STRIDE")
            .unwrap_or_else(|_| format!("{}", width * 4))
            .parse::<u32>()
            .unwrap_or(width * 4);

        let format = match std::env::var("FB_FORMAT")
            .unwrap_or_else(|_| "0".to_string())
            .as_str()
        {
            "0" => PixelFormat::XRGB8888,
            "1" => PixelFormat::ARGB8888,
            "2" => PixelFormat::RGBA8888,
            "565" => PixelFormat::RGB565,
            _ => PixelFormat::XRGB8888,
        };

        Ok(FramebufferInfo {
            width,
            height,
            stride,
            format,
            phys_addr: 0,
        })
    }
}

impl Default for LinuxFramebufferBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FramebufferBackend for LinuxFramebufferBackend {
    async fn init(&mut self) -> Result<FramebufferInfo, BackendError> {
        // フレームバッファ情報を取得
        self.info = self.get_fb_info()?;

        // フレームバッファファイルを開く
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(FB_DEVICE)
            .map_err(|e| BackendError::InitializationFailed(
                format!("Failed to open {}: {}", FB_DEVICE, e),
            ))?;

        let size = self.info.total_size();
        self.fb_data.resize(size, 0);

        // 初期状態を読み込める場合は読み込む
        let _ = file.seek(SeekFrom::Start(0));
        let _ = file.read_exact(&mut self.fb_data);

        self.fb_file = Some(file);

        log::info!(
            "Linux framebuffer initialized: {}x{} (stride={})",
            self.info.width,
            self.info.height,
            self.info.stride
        );

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

    async fn flush(&mut self) -> Result<(), BackendError> {
        let Some(file) = self.fb_file.as_mut() else {
            return Err(BackendError::WriteFailed(
                "Framebuffer device is not initialized".to_string(),
            ));
        };

        file.seek(SeekFrom::Start(0))
            .map_err(|e| BackendError::WriteFailed(format!("Seek failed: {}", e)))?;
        file.write_all(&self.fb_data)
            .map_err(|e| BackendError::WriteFailed(format!("Write failed: {}", e)))?;
        file.flush()
            .map_err(|e| BackendError::WriteFailed(format!("Flush failed: {}", e)))?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Linux /dev/fb0"
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::{FramebufferBackend, LinuxFramebufferBackend};

    #[tokio::test]
    async fn test_linux_fb_init() {
        if !std::path::Path::new("/dev/fb0").exists() {
            return;
        }
        let mut backend = LinuxFramebufferBackend::default();
        let info = backend
            .init()
            .await
            .expect("Failed to initialize Linux framebuffer");
        assert!(info.width > 0);
        assert!(info.height > 0);
    }
}
