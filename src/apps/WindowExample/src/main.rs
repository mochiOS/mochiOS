use swiftlib::{
    ipc::{ipc_recv, ipc_send},
    task::{find_process_by_name, yield_now},
    vga,
};

const IPC_BUF_SIZE: usize = 4128;
const KAGAMI_PROCESS_CANDIDATES: [&str; 3] =
    ["/applications/Kagami.app/entry.elf", "Kagami.app", "entry.elf"];

const OP_REQ_CREATE_WINDOW: u32 = 1;
const OP_RES_WINDOW_CREATED: u32 = 2;
const OP_REQ_FLUSH_CHUNK: u32 = 4;
const LAYER_APP: u8 = 2;

fn main() {
    println!("[WindowExample] start");

    let kagami_tid = match find_kagami_tid() {
        Some(tid) => tid,
        None => {
            println!("[WindowExample] Kagami not found");
            return;
        }
    };

    let (screen_w, screen_h) = match vga::get_info() {
        Some(info) => (info.width, info.height),
        None => (800, 600),
    };

    let win_w = screen_w.min(640).max(200);
    let win_h = screen_h.min(420).max(160);

    let window_id = match create_window(kagami_tid, win_w as u16, win_h as u16) {
        Ok(id) => id,
        Err(e) => {
            println!("[WindowExample] create_window failed: {}", e);
            return;
        }
    };
    println!("[WindowExample] window_id={}", window_id);

    let pixels = render_demo_pixels(win_w as usize, win_h as usize);
    if let Err(e) = flush_window_chunked(
        kagami_tid,
        window_id,
        win_w as u16,
        win_h as u16,
        &pixels,
    ) {
        println!("[WindowExample] flush failed: {}", e);
        return;
    }

    println!("[WindowExample] rendered");
    loop {
        yield_now();
    }
}

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
        return Err("ipc_send create_window failed");
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

fn render_demo_pixels(width: usize, height: usize) -> Vec<u32> {
    let mut out = vec![0u32; width * height];
    for y in 0..height {
        for x in 0..width {
            let r = (30 + (x * 160 / width)) as u32;
            let g = (40 + (y * 160 / height)) as u32;
            let b = 220u32;
            out[y * width + x] = 0xFF00_0000 | (r << 16) | (g << 8) | b;
        }
    }
    // simple border
    for x in 0..width {
        out[x] = 0xFFFFFFFF;
        out[(height - 1) * width + x] = 0xFFFFFFFF;
    }
    for y in 0..height {
        out[y * width] = 0xFFFFFFFF;
        out[y * width + (width - 1)] = 0xFFFFFFFF;
    }
    out
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
                return Err("ipc_send flush chunk failed");
            }
            x0 += w;
        }
        y0 += h;
    }
    Ok(())
}

