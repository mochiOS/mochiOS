use crate::ipc_proto::*;

#[cfg(all(target_os = "linux", target_env = "musl"))]
use swiftlib::{
    ipc::{ipc_recv, ipc_send},
    privileged,
    task::{find_process_by_name, yield_now},
};

#[cfg(all(target_os = "linux", target_env = "musl"))]
const KAGAMI_PROCESS_CANDIDATES: [&str; 3] =
    ["/applications/Kagami.app/entry.elf", "Kagami.app", "entry.elf"];

#[cfg(all(target_os = "linux", target_env = "musl"))]
struct SharedSurface {
    virt_addr: u64,
    page_count: u64,
    total_pixels: usize,
}

/// A simple window client for Kagami.
///
/// Intended for UI apps like Dock that just want to create a window and present ARGB pixels,
/// without reimplementing IPC.
pub struct Window {
    #[cfg(all(target_os = "linux", target_env = "musl"))]
    kagami_tid: u64,
    #[cfg(all(target_os = "linux", target_env = "musl"))]
    window_id: u32,
    #[cfg(all(target_os = "linux", target_env = "musl"))]
    width: u16,
    #[cfg(all(target_os = "linux", target_env = "musl"))]
    height: u16,
    #[cfg(all(target_os = "linux", target_env = "musl"))]
    shared: Option<SharedSurface>,
}

impl Window {
    pub fn new(width: u16, height: u16, layer: u8) -> Result<Self, &'static str> {
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        {
            let kagami_tid = find_kagami_tid().ok_or("Kagami not found")?;
            let window_id = create_window(kagami_tid, width, height, layer)?;
            let shared = setup_shared_surface(kagami_tid, window_id, width, height).ok();
            Ok(Self {
                kagami_tid,
                window_id,
                width,
                height,
                shared,
            })
        }
        #[cfg(not(all(target_os = "linux", target_env = "musl")))]
        {
            let _ = (width, height, layer);
            Err("Window is only supported on mochiOS target")
        }
    }

    pub fn id(&self) -> u32 {
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        {
            self.window_id
        }
        #[cfg(not(all(target_os = "linux", target_env = "musl")))]
        {
            0
        }
    }

    pub fn present(&mut self, pixels: &[u32]) -> Result<(), &'static str> {
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        {
            let total = self.width as usize * self.height as usize;
            if pixels.len() < total {
                return Err("pixel buffer too small");
            }
            if let Some(shared) = &self.shared {
                blit_shared(shared, pixels);
                present_shared(self.kagami_tid, self.window_id)
            } else {
                flush_window_chunked(
                    self.kagami_tid,
                    self.window_id,
                    self.width,
                    self.height,
                    pixels,
                )
            }
        }
        #[cfg(not(all(target_os = "linux", target_env = "musl")))]
        {
            let _ = pixels;
            Err("Window is only supported on mochiOS target")
        }
    }
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
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

#[cfg(all(target_os = "linux", target_env = "musl"))]
fn create_window(kagami_tid: u64, width: u16, height: u16, layer: u8) -> Result<u32, &'static str> {
    let mut req = [0u8; 9];
    req[0..4].copy_from_slice(&OP_REQ_CREATE_WINDOW.to_le_bytes());
    req[4..6].copy_from_slice(&width.to_le_bytes());
    req[6..8].copy_from_slice(&height.to_le_bytes());
    req[8] = layer;
    if (ipc_send(kagami_tid, &req) as i64) < 0 {
        return Err("send create_window failed");
    }
    let mut recv = [0u8; IPC_BUF_SIZE];
    for _ in 0..256 {
        let (sender, len) = ipc_recv(&mut recv);
        if sender != kagami_tid || len < 8 {
            yield_now();
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

#[cfg(all(target_os = "linux", target_env = "musl"))]
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
    let virt_addr =
        unsafe { privileged::alloc_shared_pages(page_count as u64, Some(phys_pages.as_mut_slice()), 0) };
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
    let send_pages_ret = unsafe { privileged::ipc_send_pages(kagami_tid, phys_pages.as_slice(), 0) };
    if (send_pages_ret as i64) < 0 {
        return Err("failed to send shared pages");
    }
    wait_shared_attach_ack(kagami_tid, window_id)?;

    Ok(SharedSurface {
        virt_addr,
        page_count: page_count as u64,
        total_pixels: total,
    })
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
fn wait_shared_attach_ack(kagami_tid: u64, window_id: u32) -> Result<(), &'static str> {
    let mut recv = [0u8; IPC_BUF_SIZE];
    for _ in 0..256 {
        let (sender, len) = ipc_recv(&mut recv);
        if sender != kagami_tid || len < 8 {
            yield_now();
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

#[cfg(all(target_os = "linux", target_env = "musl"))]
fn present_shared(kagami_tid: u64, window_id: u32) -> Result<(), &'static str> {
    let mut present = [0u8; 8];
    present[0..4].copy_from_slice(&OP_REQ_PRESENT_SHARED.to_le_bytes());
    present[4..8].copy_from_slice(&window_id.to_le_bytes());
    if (ipc_send(kagami_tid, &present) as i64) < 0 {
        return Err("failed to send shared present");
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
fn blit_shared(shared: &SharedSurface, pixels: &[u32]) {
    let count = shared.total_pixels.min(pixels.len());
    let mapped_pixels = (shared.page_count as usize).saturating_mul(4096) / 4;
    let count = count.min(mapped_pixels);
    unsafe {
        let dst = core::slice::from_raw_parts_mut(shared.virt_addr as *mut u32, count);
        for (d, s) in dst.iter_mut().zip(pixels.iter().take(count)) {
            *d = *s | 0xFF00_0000;
        }
    }
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
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
                    msg[off..off + 4]
                        .copy_from_slice(&(pixels[src_row + x0 + col] | 0xFF00_0000).to_le_bytes());
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

