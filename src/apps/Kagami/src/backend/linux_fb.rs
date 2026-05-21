use super::{BackendError, FramebufferBackend, FramebufferInfo, PixelFormat};
use async_trait::async_trait;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::ptr;

const FB_DEVICE: &str = "/dev/fb0";
const MMAP_OFFSET: u64 = 0;

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
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(FB_DEVICE)
            .map_err(|e| BackendError::InitializationFailed(
                format!("Failed to open {}: {}", FB_DEVICE, e),
            ))?;

        // メモリマップ
        let mmap_size = self.info.total_size();

        unsafe {
            let mmap_addr = libc::mmap(
                ptr::null_mut(),
                mmap_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                MMAP_OFFSET as libc::off_t,
            );

            if mmap_addr == libc::MAP_FAILED {
                return Err(BackendError::MemoryMapFailed(
                    "Failed to mmap framebuffer".to_string(),
                ));
            }

            let mapped_data = std::slice::from_raw_parts(mmap_addr as *const u8, mmap_size);
            self.fb_data = mapped_data.to_vec();

            // アンマップ（Vec が管理するようになったので）
            libc::munmap(mmap_addr, mmap_size);
        }

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
        let mut backend = LinuxFramebufferBackend::default();
        let info = backend.init().await.expect("Failed to initialize Linux framebuffer");
        assert_eq!(info.width, 1024);
        assert_eq!(info.height, 768);
    }
}