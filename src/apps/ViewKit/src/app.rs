use crate::backend::RawOSEvent;
use crate::backend::ViewKitBackend;

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
