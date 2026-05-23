use crate::backend::RawOSEvent;
use crate::backend::ViewKitBackend;
use crate::ui::UIElement;

pub struct ViewKitApp {
    pub backend: Box<dyn ViewKitBackend>,
    pub width: u32,
    pub height: u32,
    pub screen_buffer: Vec<u32>,
    pub key_tap_callback: Option<extern "C" fn(key_code: u32)>,
}

impl ViewKitApp {
    pub fn new(backend: Box<dyn ViewKitBackend>) -> Self {
        let w = 800;
        let h = 600;

        Self {
            backend,
            width: w,
            height: h,
            screen_buffer: vec![0xFF000000; (w * h) as usize],
            key_tap_callback: None,
        }
    }

    /// Kome側の `window.onKeyTap` からコールバックを登録するためのFFI用関数
    pub fn set_key_tap_callback(&mut self, cb: extern "C" fn(u32)) {
        self.key_tap_callback = Some(cb);
    }

    /// メインループを実行する（Kome側のランタイムループと同期する）
    pub fn run_loop(&mut self) {
        loop {
            while let Some(event) = self.backend.poll_os_event() {
                match event {
                    RawOSEvent::Key {
                        scan_code,
                        pressed: true,
                    } => {
                        if let Some(callback) = self.key_tap_callback {
                            callback(scan_code);
                        }
                    }
                    RawOSEvent::Quit => return,
                    _ => {}
                }
            }

            self.backend
                .swap_buffers(&self.screen_buffer, self.width, self.height);
        }
    }
}

/// High-level app runner for mochiOS clients.
///
/// Client code should not need to touch framebuffer pointers or `unsafe`.
pub struct App {
    ui: UIElement,
    asset_root: Option<String>,
}

impl App {
    pub fn new(ui: UIElement) -> Self {
        let asset_root = std::env::args()
            .next()
            .and_then(|p| p.rsplit_once("/entry.elf").map(|(d, _)| d.to_string()));
        Self { ui, asset_root }
    }

    pub fn asset_root(mut self, root: impl Into<String>) -> Self {
        self.asset_root = Some(root.into());
        self
    }

    pub fn run(self) {
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        {
            use swiftlib::{time, vga};

            let info = match vga::get_info() {
                Some(i) => i,
                None => return,
            };
            let fb_ptr = match vga::map_framebuffer() {
                Some(p) => p,
                None => return,
            };

            let pixels = crate::render_ui_element_to_pixmap_with_asset_root(
                &self.ui,
                info.width,
                info.height,
                self.asset_root.as_deref(),
            );

            let stride = info.stride as usize;
            unsafe {
                let fb = core::slice::from_raw_parts_mut(
                    fb_ptr,
                    stride.saturating_mul(info.height as usize),
                );
                for y in 0..info.height as usize {
                    let row = y * stride;
                    let src_row = y * info.width as usize;
                    for x in 0..info.width as usize {
                        let idx = row + x;
                        if idx < fb.len() {
                            core::ptr::write_volatile(&mut fb[idx], pixels[src_row + x]);
                        }
                    }
                }
            }

            // Keep the frame visible briefly for smoke tests.
            time::sleep_ms(300);
        }
        #[cfg(not(all(target_os = "linux", target_env = "musl")))]
        {
            // Host-side runner is not provided yet.
            let _ = self;
        }
    }
}
