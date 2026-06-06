use crate::ipc_proto::*;

#[cfg(all(target_os = "linux", target_env = "musl"))]
use crate::mochios::{
    ipc::{ipc_recv, ipc_send},
    task::{find_process_by_name, yield_now},
    user_space::looks_like_user_mapping,
};

#[cfg(all(target_os = "linux", target_env = "musl"))]
const KAGAMI_PROCESS_CANDIDATES: [&str; 3] =
    ["/applications/Kagami.app/entry.elf", "Kagami.app", "entry.elf"];

#[cfg(all(target_os = "linux", target_env = "musl"))]
const IPC_BUF_SIZE: usize = crate::mochios::ipc::MAX_MSG_SIZE;

/// A small Kagami window client for UI apps.
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
            let shared = match setup_shared_surface(kagami_tid, window_id, width, height) {
                Ok(s) => {
                    println!(
                        "[ViewKit] shared mapped: window={} addr=0x{:x} bytes={}",
                        window_id, s.virt_addr, s.mapped_bytes
                    );
                    Some(s)
                }
                Err(e) => {
                    println!("[ViewKit] shared disabled: {}", e);
                    None
                }
            };
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
            // Prefer shared-memory present when available; fall back to chunked IPC.
            if let Some(shared) = self.shared.as_mut() {
                // If shared mapping looks wrong, disable it and fall back to chunked IPC.
                if !crate::mochios::user_space::looks_like_user_mapping(
                    shared.virt_addr,
                    shared.mapped_bytes,
                ) {
                    self.shared = None;
                } else {
                    blit_shared(shared, pixels);
                    return present_shared(self.kagami_tid, self.window_id);
                }
            } else {
                // continue below
            }
            flush_window_chunked(
                self.kagami_tid,
                self.window_id,
                self.width,
                self.height,
                pixels,
            )
        }
        #[cfg(not(all(target_os = "linux", target_env = "musl")))]
        {
            let _ = pixels;
            Err("Window is only supported on mochiOS target")
        }
    }
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
struct SharedSurface {
    virt_addr: u64,
    mapped_bytes: usize,
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
    Err("create_window timeout")
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
fn setup_shared_surface(
    kagami_tid: u64,
    window_id: u32,
    width: u16,
    height: u16,
) -> Result<SharedSurface, &'static str> {
    let mut req = [0u8; 12];
    req[0..4].copy_from_slice(&OP_REQ_ATTACH_SHARED.to_le_bytes());
    req[4..8].copy_from_slice(&window_id.to_le_bytes());
    req[8..10].copy_from_slice(&width.to_le_bytes());
    req[10..12].copy_from_slice(&height.to_le_bytes());
    if (ipc_send(kagami_tid, &req) as i64) < 0 {
        return Err("send attach_shared failed");
    }

    // Wait for both:
    // - Kernel map header delivering the mapped address (magic+map_start+total).
    // - Kagami's explicit attach ack so we know it recorded the mapping.
    //
    // These can arrive in either order, so accept both while polling.
    let mut map_start: Option<u64> = None;
    let mut mapped_bytes: Option<usize> = None;
    let mut saw_ack = false;

    let mut recv = [0u8; 64];
    for _ in 0..768 {
        let (sender, len) = ipc_recv(&mut recv);
        if sender == 0 || len == 0 {
            yield_now();
            continue;
        }

        // MAP_HEADER_MAGIC: [magic:u32][map_start:u64][total:u64] (20 bytes)
        if sender == kagami_tid && (len as usize) == 20 {
            let magic = u32::from_le_bytes([recv[0], recv[1], recv[2], recv[3]]);
            if magic == crate::mochios::ipc::MAP_HEADER_MAGIC {
                map_start = Some(u64::from_le_bytes([
                    recv[4], recv[5], recv[6], recv[7], recv[8], recv[9], recv[10], recv[11],
                ]));
                mapped_bytes = Some(u64::from_le_bytes([
                    recv[12], recv[13], recv[14], recv[15], recv[16], recv[17], recv[18], recv[19],
                ]) as usize);
            }
        }

        // OP_RES_SHARED_ATTACHED: [op:u32][window_id:u32]
        if sender == kagami_tid && (len as usize) >= 8 {
            let op = u32::from_le_bytes([recv[0], recv[1], recv[2], recv[3]]);
            if op == OP_RES_SHARED_ATTACHED {
                let ack_window =
                    u32::from_le_bytes([recv[4], recv[5], recv[6], recv[7]]);
                if ack_window == window_id {
                    saw_ack = true;
                }
            }
        }

        if saw_ack {
            if let (Some(addr), Some(total)) = (map_start, mapped_bytes) {
                if looks_like_user_mapping(addr, total) {
                    return Ok(SharedSurface {
                        virt_addr: addr,
                        mapped_bytes: total,
                    });
                }
                return Err("shared mapping address invalid");
            }
        }
        yield_now();
    }
    Err("shared attach timeout")
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
fn blit_shared(shared: &mut SharedSurface, pixels: &[u32]) {
    let max = shared.mapped_bytes / 4;
    let count = max.min(pixels.len());
    if count == 0 {
        return;
    }
    unsafe {
        let dst = core::slice::from_raw_parts_mut(shared.virt_addr as *mut u32, count);
        dst.copy_from_slice(&pixels[..count]);
    }
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
fn present_shared(kagami_tid: u64, window_id: u32) -> Result<(), &'static str> {
    let mut req = [0u8; 8];
    req[0..4].copy_from_slice(&OP_REQ_PRESENT_SHARED.to_le_bytes());
    req[4..8].copy_from_slice(&window_id.to_le_bytes());
    if (ipc_send(kagami_tid, &req) as i64) < 0 {
        return Err("send present_shared failed");
    }
    Ok(())
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

    // Keep IPC messages under Kagami's fixed buffer size.
    let chunk_header = 20usize;
    let max_chunk_pixels = (IPC_BUF_SIZE - chunk_header) / 4;
    let w = width as usize;
    let h = height as usize;
    let chunk_w = w.min(96).max(1);
    let chunk_h = (max_chunk_pixels / chunk_w).max(1);

    let mut y0 = 0usize;
    while y0 < h {
        let hh = (h - y0).min(chunk_h);
        let mut x0 = 0usize;
        while x0 < w {
            let ww = (w - x0).min(chunk_w);

            let mut msg = vec![0u8; chunk_header + ww * hh * 4];
            msg[0..4].copy_from_slice(&OP_REQ_FLUSH_CHUNK.to_le_bytes());
            msg[4..8].copy_from_slice(&window_id.to_le_bytes());
            msg[8..10].copy_from_slice(&width.to_le_bytes());
            msg[10..12].copy_from_slice(&height.to_le_bytes());
            msg[12..14].copy_from_slice(&(x0 as u16).to_le_bytes());
            msg[14..16].copy_from_slice(&(y0 as u16).to_le_bytes());
            msg[16..18].copy_from_slice(&(ww as u16).to_le_bytes());
            msg[18..20].copy_from_slice(&(hh as u16).to_le_bytes());

            for row in 0..hh {
                let src_row = (y0 + row) * w;
                let dst_row = chunk_header + row * ww * 4;
                for col in 0..ww {
                    let p = pixels[src_row + x0 + col].to_le_bytes();
                    let off = dst_row + col * 4;
                    msg[off..off + 4].copy_from_slice(&p);
                }
            }

            if (ipc_send(kagami_tid, &msg) as i64) < 0 {
                return Err("send flush_chunk failed");
            }
            x0 += ww;
        }
        y0 += hh;
    }
    Ok(())
}
