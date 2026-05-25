use crate::ipc_proto::*;

#[cfg(all(target_os = "linux", target_env = "musl"))]
use swiftlib::{
    ipc::{ipc_recv, ipc_send},
    task::{find_process_by_name, yield_now},
};

#[cfg(all(target_os = "linux", target_env = "musl"))]
const KAGAMI_PROCESS_CANDIDATES: [&str; 3] =
    ["/applications/Kagami.app/entry.elf", "Kagami.app", "entry.elf"];

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
}

impl Window {
    pub fn new(width: u16, height: u16, layer: u8) -> Result<Self, &'static str> {
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        {
            let kagami_tid = find_kagami_tid().ok_or("Kagami not found")?;
            let window_id = create_window(kagami_tid, width, height, layer)?;
            Ok(Self {
                kagami_tid,
                window_id,
                width,
                height,
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
    let mut recv = [0u8; 4096];
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
fn flush_window_chunked(
    kagami_tid: u64,
    window_id: u32,
    width: u16,
    height: u16,
    pixels: &[u32],
) -> Result<(), &'static str> {
    // Use modest chunks to keep IPC messages under size limit.
    let chunk_w = 64usize;
    let chunk_h = 64usize;
    let w = width as usize;
    let h = height as usize;
    let mut y0 = 0usize;
    while y0 < h {
        let hh = (h - y0).min(chunk_h);
        let mut x0 = 0usize;
        while x0 < w {
            let ww = (w - x0).min(chunk_w);

            let mut msg = vec![0u8; 20 + ww * hh * 4];
            msg[0..4].copy_from_slice(&OP_REQ_FLUSH_CHUNK.to_le_bytes());
            msg[4..8].copy_from_slice(&window_id.to_le_bytes());
            msg[12..14].copy_from_slice(&(x0 as u16).to_le_bytes());
            msg[14..16].copy_from_slice(&(y0 as u16).to_le_bytes());
            msg[16..18].copy_from_slice(&(ww as u16).to_le_bytes());
            msg[18..20].copy_from_slice(&(hh as u16).to_le_bytes());

            for row in 0..hh {
                let src_row = (y0 + row) * w;
                let dst_row = 20 + row * ww * 4;
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
