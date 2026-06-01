use super::{BackendError, FramebufferBackend, FramebufferInfo, PixelFormat};
use async_trait::async_trait;

/// mochiOS VGA/フレームバッファ（`mochi_syscall::vga`）バックエンド
///
/// - mochiOS: syscall 経由で VRAM をマップし、`flush()` で転送
/// - Linux host: `backend-mochios-vga-hosted` を有効にすると `mochi_syscall/hosted-vga` で模擬
pub struct MochiVgaBackend {
    fb_addr: usize,
    fb_len_u32: usize,
    backbuffer: Vec<u32>,
    info: FramebufferInfo,
}

impl MochiVgaBackend {
    pub fn new() -> Self {
        Self {
            fb_addr: 0,
            fb_len_u32: 0,
            backbuffer: Vec::new(),
            info: FramebufferInfo {
                width: 0,
                height: 0,
                stride: 0,
                format: PixelFormat::XRGB8888,
                phys_addr: 0,
            },
        }
    }
}

impl Default for MochiVgaBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FramebufferBackend for MochiVgaBackend {
    async fn init(&mut self) -> Result<FramebufferInfo, BackendError> {
        #[cfg(feature = "backend-mochios-vga-hosted")]
        let fb_info = {
            let mut fb_info = mochi_syscall::vga::get_info();
            if fb_info.is_none() {
                let width = std::env::var("VGA_WIDTH")
                    .unwrap_or_else(|_| "640".to_string())
                    .parse::<u32>()
                    .unwrap_or(640);
                let height = std::env::var("VGA_HEIGHT")
                    .unwrap_or_else(|_| "480".to_string())
                    .parse::<u32>()
                    .unwrap_or(480);
                let _ = mochi_syscall::vga::host_init_framebuffer(width, height);
                fb_info = mochi_syscall::vga::get_info();
            }
            fb_info
        };

        #[cfg(not(feature = "backend-mochios-vga-hosted"))]
        let fb_info = mochi_syscall::vga::get_info();

        let fb_info = fb_info.ok_or_else(|| {
            BackendError::InitializationFailed("mochi_syscall::vga::get_info failed".to_string())
        })?;

        let fb_ptr = mochi_syscall::vga::map_framebuffer().ok_or_else(|| {
            BackendError::MemoryMapFailed("mochi_syscall::vga::map_framebuffer failed".to_string())
        })?;

        let len_u32 = (fb_info.stride as usize).saturating_mul(fb_info.height as usize);
        self.backbuffer.resize(len_u32, 0);
        self.fb_addr = fb_ptr as usize;
        self.fb_len_u32 = len_u32;

        self.info = FramebufferInfo {
            width: fb_info.width,
            height: fb_info.height,
            stride: fb_info.stride.saturating_mul(4),
            format: PixelFormat::XRGB8888,
            phys_addr: 0,
        };

        Ok(self.info)
    }

    fn framebuffer(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.backbuffer.as_ptr() as *const u8,
                self.backbuffer.len().saturating_mul(4),
            )
        }
    }

    fn framebuffer_mut(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.backbuffer.as_mut_ptr() as *mut u8,
                self.backbuffer.len().saturating_mul(4),
            )
        }
    }

    fn info(&self) -> FramebufferInfo {
        self.info
    }

    async fn flush(&mut self) -> Result<(), BackendError> {
        if self.fb_addr == 0 || self.fb_len_u32 == 0 {
            return Err(BackendError::WriteFailed("framebuffer is not mapped".to_string()));
        }
        if self.backbuffer.len() != self.fb_len_u32 {
            return Err(BackendError::WriteFailed("backbuffer size mismatch".to_string()));
        }

        unsafe {
            let dst = core::slice::from_raw_parts_mut(self.fb_addr as *mut u32, self.fb_len_u32);
            dst.copy_from_slice(&self.backbuffer);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "mochiOS VGA (mochi_syscall::vga)"
    }
}
