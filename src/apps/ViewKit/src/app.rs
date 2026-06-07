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
    title: String,
    width: Option<u32>,
    height: Option<u32>,
    decoration: bool,
}

impl App {
    pub fn new(ui: UIElement) -> Self {
        let asset_root = std::env::args()
            .next()
            .and_then(|p| p.rsplit_once("/entry.elf").map(|(d, _)| d.to_string()));
        Self {
            ui,
            asset_root,
            title: "Window".to_string(),
            width: None,
            height: None,
            decoration: true,
        }
    }

    pub fn asset_root(mut self, root: impl Into<String>) -> Self {
        self.asset_root = Some(root.into());
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    pub fn decoration(mut self, enable: bool) -> Self {
        self.decoration = enable;
        self
    }

    pub fn run(self) {
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        {
            kagami_present_loop(self);
        }
        #[cfg(not(all(target_os = "linux", target_env = "musl")))]
        {
            // Host-side runner is not provided yet.
            let _ = self;
        }
    }
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
fn kagami_present_loop(app: App) {
    use crate::mochios::{
<<<<<<< HEAD
        ipc::{ipc_recv, ipc_recv_wait, ipc_send},
=======
        ipc::{ipc_recv, ipc_send},
>>>>>>> master
        privileged,
        task::{find_process_by_name, yield_now},
        time, vga,
    };

    const IPC_BUF_SIZE: usize = crate::mochios::ipc::MAX_MSG_SIZE;
    const KAGAMI_PROCESS_CANDIDATES: [&str; 3] =
        ["/applications/Kagami.app/entry.elf", "Kagami.app", "entry.elf"];

    const OP_REQ_CREATE_WINDOW: u32 = 1;
    const OP_RES_WINDOW_CREATED: u32 = 2;
    const OP_REQ_FLUSH_CHUNK: u32 = 4;
    const OP_REQ_ATTACH_SHARED: u32 = 5;
    const OP_REQ_PRESENT_SHARED: u32 = 6;
    const OP_RES_SHARED_ATTACHED: u32 = 7;
    const LAYER_APP: u8 = 2;

    fn find_kagami_tid() -> Option<u64> {
        for name in KAGAMI_PROCESS_CANDIDATES {
            if let Some(pid) = find_process_by_name(name) {
                if pid != 0 {
                    return Some(pid);
                }
            }
        }
        None
    }

    fn create_window(kagami_tid: u64, width: u16, height: u16) -> Result<u32, &'static str> {
        let mut req = [0u8; 9];
        req[0..4].copy_from_slice(&OP_REQ_CREATE_WINDOW.to_le_bytes());
        req[4..6].copy_from_slice(&width.to_le_bytes());
        req[6..8].copy_from_slice(&height.to_le_bytes());
        req[8] = LAYER_APP;
        if (ipc_send(kagami_tid, &req) as i64) < 0 {
            return Err("send create_window failed");
        }
        let mut recv = [0u8; IPC_BUF_SIZE];
        for _ in 0..256 {
<<<<<<< HEAD
            let (sender, len) = ipc_recv_wait(&mut recv);
            if sender != kagami_tid || len < 8 {
=======
            let (sender, len) = ipc_recv(&mut recv);
            if sender != kagami_tid || len < 8 {
                yield_now();
>>>>>>> master
                continue;
            }
            let op = u32::from_le_bytes([recv[0], recv[1], recv[2], recv[3]]);
            if op != OP_RES_WINDOW_CREATED {
                continue;
            }
            return Ok(u32::from_le_bytes([recv[4], recv[5], recv[6], recv[7]]));
        }
        Err("window create timeout")
    }

    struct SharedSurface {
        virt_addr: u64,
        total_pixels: usize,
    }

    fn wait_shared_attach_ack(kagami_tid: u64, window_id: u32) -> Result<(), &'static str> {
        let mut recv = [0u8; IPC_BUF_SIZE];
        for _ in 0..256 {
<<<<<<< HEAD
            let (sender, len) = ipc_recv_wait(&mut recv);
            if sender != kagami_tid || len < 8 {
=======
            let (sender, len) = ipc_recv(&mut recv);
            if sender != kagami_tid || len < 8 {
                yield_now();
>>>>>>> master
                continue;
            }
            let op = u32::from_le_bytes([recv[0], recv[1], recv[2], recv[3]]);
            if op != OP_RES_SHARED_ATTACHED {
                continue;
            }
            let ack_window = u32::from_le_bytes([recv[4], recv[5], recv[6], recv[7]]);
            if ack_window == window_id {
                return Ok(());
            }
        }
        Err("shared attach ack timeout")
    }

    fn setup_shared_surface(
        kagami_tid: u64,
        window_id: u32,
        width: u16,
        height: u16,
    ) -> Result<SharedSurface, &'static str> {
        let total = width as usize * height as usize;
        let total_bytes = total.checked_mul(4).ok_or("size overflow")?;
        let page_count = total_bytes.div_ceil(4096);
        if page_count == 0 {
            return Err("shared surface page count out of range");
        }

        let mut phys_pages = vec![0u64; page_count];
        let virt_addr = unsafe {
            privileged::alloc_shared_pages(page_count as u64, Some(phys_pages.as_mut_slice()), 0)
        };
        if (virt_addr as i64) < 0 || virt_addr == 0 {
            return Err("alloc_shared_pages failed");
        }
        if phys_pages.iter().all(|&x| x == 0) {
            return Err("alloc_shared_pages returned zeroed phys pages");
        }

        let mut attach = [0u8; 12];
        attach[0..4].copy_from_slice(&OP_REQ_ATTACH_SHARED.to_le_bytes());
        attach[4..8].copy_from_slice(&window_id.to_le_bytes());
        attach[8..10].copy_from_slice(&width.to_le_bytes());
        attach[10..12].copy_from_slice(&height.to_le_bytes());
        if (ipc_send(kagami_tid, &attach) as i64) < 0 {
            return Err("failed to send shared attach");
        }
        let send_pages_ret =
            unsafe { privileged::ipc_send_pages(kagami_tid, phys_pages.as_slice(), 0) };
        if (send_pages_ret as i64) < 0 {
            return Err("failed to send shared pages");
        }
        wait_shared_attach_ack(kagami_tid, window_id)?;
        Ok(SharedSurface {
            virt_addr,
            total_pixels: total,
        })
    }

    fn blit_shared_surface(shared: &SharedSurface, pixels: &[u32]) {
        let total = shared.total_pixels.min(pixels.len());
        unsafe {
            let dst = core::slice::from_raw_parts_mut(shared.virt_addr as *mut u32, total);
            for i in 0..total {
                core::ptr::write_volatile(&mut dst[i], pixels[i] | 0xFF00_0000);
            }
        }
    }

    fn present_shared(kagami_tid: u64, window_id: u32) -> Result<(), &'static str> {
        let mut present = [0u8; 8];
        present[0..4].copy_from_slice(&OP_REQ_PRESENT_SHARED.to_le_bytes());
        present[4..8].copy_from_slice(&window_id.to_le_bytes());
        if (ipc_send(kagami_tid, &present) as i64) < 0 {
            return Err("send present_shared failed");
        }
        Ok(())
    }

    fn flush_window_chunked(
        kagami_tid: u64,
        window_id: u32,
        width: u16,
        height: u16,
        pixels: &[u32],
    ) -> Result<(), &'static str> {
        let total = width as usize * height as usize;
        if pixels.len() < total {
            return Err("pixel buffer too small");
        }
        let chunk_header = 20usize;
        let max_chunk_pixels = (IPC_BUF_SIZE - chunk_header) / 4;
        let width_usize = width as usize;
        let height_usize = height as usize;
        let chunk_w = width_usize.min(96).max(1);
        let chunk_h = (max_chunk_pixels / chunk_w).max(1);

        let mut y0 = 0usize;
        while y0 < height_usize {
            let h = (height_usize - y0).min(chunk_h);
            let mut x0 = 0usize;
            while x0 < width_usize {
                let w = (width_usize - x0).min(chunk_w);
                let mut msg = vec![0u8; chunk_header + (w * h * 4)];
                msg[0..4].copy_from_slice(&OP_REQ_FLUSH_CHUNK.to_le_bytes());
                msg[4..8].copy_from_slice(&window_id.to_le_bytes());
                msg[8..10].copy_from_slice(&width.to_le_bytes());
                msg[10..12].copy_from_slice(&height.to_le_bytes());
                msg[12..14].copy_from_slice(&(x0 as u16).to_le_bytes());
                msg[14..16].copy_from_slice(&(y0 as u16).to_le_bytes());
                msg[16..18].copy_from_slice(&(w as u16).to_le_bytes());
                msg[18..20].copy_from_slice(&(h as u16).to_le_bytes());
                let mut off = chunk_header;
                for row in 0..h {
                    let src_row = (y0 + row) * width_usize;
                    for col in 0..w {
                        msg[off..off + 4].copy_from_slice(
                            &(pixels[src_row + x0 + col] | 0xFF00_0000).to_le_bytes(),
                        );
                        off += 4;
                    }
                }
                if (ipc_send(kagami_tid, &msg) as i64) < 0 {
                    return Err("send flush chunk failed");
                }
                x0 += w;
            }
            y0 += h;
        }
        Ok(())
    }

    let kagami_tid = match find_kagami_tid() {
        Some(tid) => tid,
        None => return,
    };

    let (screen_w, screen_h) = match vga::get_info() {
        Some(info) => (info.width, info.height),
        None => (800, 600),
    };
    let w = app.width.unwrap_or(screen_w.min(720).max(200));
    let h = app.height.unwrap_or(screen_h.min(520).max(160));

    let window_id = match create_window(kagami_tid, w as u16, h as u16) {
        Ok(id) => id,
        Err(_) => return,
    };

    let shared = setup_shared_surface(kagami_tid, window_id, w as u16, h as u16).ok();

    // Render once (for now). When input/events are wired, this can be re-rendered on demand.
    let pixels = crate::render_ui_element_to_pixmap_with_asset_root(
        &app.ui,
        w,
        h,
        app.asset_root.as_deref(),
    );

    if let Some(shared) = &shared {
        blit_shared_surface(shared, &pixels);
        let _ = present_shared(kagami_tid, window_id);
    } else {
        let _ = flush_window_chunked(kagami_tid, window_id, w as u16, h as u16, &pixels);
    }

    // Keep process alive so the window remains visible.
    loop {
<<<<<<< HEAD
        yield_now();
=======
        time::sleep_ms(16);
>>>>>>> master
    }
}
