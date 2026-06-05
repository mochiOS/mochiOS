use mochi_syscall::{ipc::ipc_recv, keyboard, privileged, process, task, time, vga};

const IPC_BUF_SIZE: usize = mochi_syscall::ipc::MAX_MSG_SIZE;
const OP_REQ_CREATE_WINDOW: u32 = 1;
const OP_RES_WINDOW_CREATED: u32 = 2;
const OP_REQ_FLUSH_CHUNK: u32 = 4;
const OP_REQ_ATTACH_SHARED: u32 = 5;
const OP_REQ_PRESENT_SHARED: u32 = 6;
const OP_RES_SHARED_ATTACHED: u32 = 7;

struct Window {
    id: u32,
    x: i32,
    y: i32,
    width: u16,
    height: u16,
    layer: u8,
    pixels: Vec<u32>,
    shared_addr: Option<u64>,
    shared_bytes: usize,
}

fn is_e_make(scancode: u8) -> bool {
    // PS/2 Set 1 make code: 'e' = 0x12
    scancode == 0x12
}

fn main() {
    println!("[Kagami] kagamid starting");

    let info = match vga::get_info() {
        Some(i) => i,
        None => {
            println!("[Kagami] failed to get framebuffer info");
            loop {
                time::sleep_ms(1000);
            }
        }
    };
    let fb_ptr = match vga::map_framebuffer() {
        Some(p) => p,
        None => {
            println!("[Kagami] failed to map framebuffer");
            loop {
                time::sleep_ms(1000);
            }
        }
    };

    println!(
        "[Kagami] fb info: width={} height={} stride={} fb_ptr={:p}",
        info.width, info.height, info.stride, fb_ptr
    );
    println!("[Kagami] tid={}", task::gettid());

    // 背景色で塗りつぶし（ARGB, 常に不透明で書き込む）
    let background: u32 = 0xFF1F_1F1F;
    let pixel_count = (info.stride as usize).saturating_mul(info.height as usize);
    unsafe {
        let fb = core::slice::from_raw_parts_mut(fb_ptr, pixel_count);
        for px in fb.iter_mut() {
            core::ptr::write_volatile(px, background);
        }
    }

    let binder_bundle = "/applications/Binder.app";
    match process::exec_app_via_process_service(binder_bundle) {
        Ok(pid) => println!("[Kagami] launched Binder pid={}", pid),
        Err(errno) => println!(
            "[Kagami] failed to launch {} via process.service: errno={}",
            binder_bundle, errno
        ),
    }

    println!("[Kagami] ready (press 'e' to launch test_client)");

    let mut next_window_id: u32 = 1;
    let mut windows: Vec<Window> = Vec::new();
    let mut pending_shared: Option<(u64, u32, u16, u16)> = None; // (sender_tid, window_id, w, h)

    let mut e_down = false;
    loop {
        time::sleep_ms(10);

        // Handle window server IPC.
        let mut recv = [0u8; IPC_BUF_SIZE];
        loop {
            let (sender, len) = ipc_recv(&mut recv);
            if sender == 0 || len == 0 {
                break;
            }
            let len = len as usize;
            // Handle shared-page map header (kernel format).
            if len == 20 {
                let magic = u32::from_le_bytes([recv[0], recv[1], recv[2], recv[3]]);
                if magic == mochi_syscall::ipc::MAP_HEADER_MAGIC {
                    let map_start = u64::from_le_bytes([
                        recv[4], recv[5], recv[6], recv[7], recv[8], recv[9], recv[10], recv[11],
                    ]);
                    let total = u64::from_le_bytes([
                        recv[12], recv[13], recv[14], recv[15], recv[16], recv[17], recv[18],
                        recv[19],
                    ]);
                    if let Some((psender, window_id, w, h)) = pending_shared.take() {
                        if psender == sender {
                            if let Some(win) = windows.iter_mut().find(|w0| w0.id == window_id) {
                                win.shared_addr = Some(map_start);
                                win.shared_bytes = total as usize;
                                win.width = w;
                                win.height = h;
                                // Ack attach.
                                let mut res = [0u8; 8];
                                res[0..4].copy_from_slice(&OP_RES_SHARED_ATTACHED.to_le_bytes());
                                res[4..8].copy_from_slice(&window_id.to_le_bytes());
                                let _ = mochi_syscall::ipc::ipc_send(sender, &res);
                            }
                        }
                    }
                    continue;
                }
            }

            if len < 4 {
                continue;
            }
            let op = u32::from_le_bytes([recv[0], recv[1], recv[2], recv[3]]);
            match op {
                OP_REQ_CREATE_WINDOW => {
                    if len < 9 {
                        continue;
                    }
                    let width = u16::from_le_bytes([recv[4], recv[5]]);
                    let height = u16::from_le_bytes([recv[6], recv[7]]);
                    let layer = recv[8];
                    let id = next_window_id;
                    next_window_id = next_window_id.wrapping_add(1).max(1);

                    // Placement:
                    // - Fullscreen clients (Binder desktop) go to (0, 0).
                    //   Some clients may use framebuffer stride as width, so accept that too.
                    // - Status layer (Dock) goes bottom-center.
                    // - Others: simple cascade.
                    let is_fullscreen = (width as u32 == info.width || width as u32 == info.stride)
                        && height as u32 == info.height;
                    let is_dock = layer >= 3 || (layer >= 2 && height == 75);
                    let (x, y) = if is_fullscreen {
                        (0, 0)
                    } else if is_dock {
                        let cx = ((info.width as i32 - width as i32) / 2).max(0);
                        let cy = (info.height as i32 - height as i32 - 16).max(0);
                        (cx, cy)
                    } else {
                        (
                            40 + (id as i32 * 24) % 200,
                            60 + (id as i32 * 18) % 160,
                        )
                    };
                    println!(
                        "[Kagami] create_window id={} w={} h={} layer={} -> ({},{})",
                        id, width, height, layer, x, y
                    );
                    let pixels = vec![0u32; (width as usize).saturating_mul(height as usize)];
                    windows.push(Window {
                        id,
                        x,
                        y,
                        width,
                        height,
                        layer,
                        pixels,
                        shared_addr: None,
                        shared_bytes: 0,
                    });

                    let mut res = [0u8; 8];
                    res[0..4].copy_from_slice(&OP_RES_WINDOW_CREATED.to_le_bytes());
                    res[4..8].copy_from_slice(&id.to_le_bytes());
                    let _ = mochi_syscall::ipc::ipc_send(sender, &res);
                }
                OP_REQ_FLUSH_CHUNK => {
                    if len < 20 {
                        continue;
                    }
                    let window_id = u32::from_le_bytes([recv[4], recv[5], recv[6], recv[7]]);
                    let x0 = u16::from_le_bytes([recv[12], recv[13]]) as usize;
                    let y0 = u16::from_le_bytes([recv[14], recv[15]]) as usize;
                    let w0 = u16::from_le_bytes([recv[16], recv[17]]) as usize;
                    let h0 = u16::from_le_bytes([recv[18], recv[19]]) as usize;
                    let payload = &recv[20..len];
                    if payload.len() < w0.saturating_mul(h0).saturating_mul(4) {
                        continue;
                    }

                    if let Some(win) = windows.iter_mut().find(|w| w.id == window_id) {
                        let ww = win.width as usize;
                        let hh = win.height as usize;
                        for row in 0..h0 {
                            let dy = y0 + row;
                            if dy >= hh {
                                continue;
                            }
                            for col in 0..w0 {
                                let dx = x0 + col;
                                if dx >= ww {
                                    continue;
                                }
                                let off = (row * w0 + col) * 4;
                                let argb = u32::from_le_bytes([
                                    payload[off],
                                    payload[off + 1],
                                    payload[off + 2],
                                    payload[off + 3],
                                ]);
                                win.pixels[dy * ww + dx] = argb | 0xFF00_0000;
                            }
                        }

                        // Composite whole scene (simple + slow, fine for now).
                        unsafe {
                            let fb = core::slice::from_raw_parts_mut(fb_ptr, pixel_count);
                            for px in fb.iter_mut() {
                                core::ptr::write_volatile(px, background);
                            }
                        }
                        windows.sort_by_key(|w| w.layer);
                        for w in windows.iter() {
                            blit_window(
                                fb_ptr,
                                info.stride as usize,
                                info.height as usize,
                                w.x,
                                w.y,
                                w.width as usize,
                                w.height as usize,
                                window_pixels(w),
                            );
                        }
                    }
                }
                OP_REQ_ATTACH_SHARED => {
                    if len < 12 {
                        continue;
                    }
                    let window_id = u32::from_le_bytes([recv[4], recv[5], recv[6], recv[7]]);
                    let w = u16::from_le_bytes([recv[8], recv[9]]);
                    let h = u16::from_le_bytes([recv[10], recv[11]]);
                    // Preferred path (Wayland-like): Kagami allocates the shared pages and sends
                    // them to the client; kernel maps them into the client and delivers a 20-byte
                    // MAP_HEADER_MAGIC message with the mapped address.
                    let total_bytes = (w as usize)
                        .saturating_mul(h as usize)
                        .saturating_mul(4);
                    let page_count = total_bytes.div_ceil(4096).max(1);

                    let mut attached = false;
                    if let Some(win) = windows.iter_mut().find(|ww| ww.id == window_id) {
                        let mut phys_pages = vec![0u64; page_count];
                        let virt_addr = unsafe {
                            privileged::alloc_shared_pages(page_count as u64, Some(phys_pages.as_mut_slice()), 0)
                        };
                        if (virt_addr as i64) >= 0 && virt_addr != 0 {
                            let send_ret =
                                unsafe { privileged::ipc_send_pages(sender, phys_pages.as_slice(), 0) };
                            if (send_ret as i64) >= 0 {
                                win.shared_addr = Some(virt_addr);
                                win.shared_bytes = page_count * 4096;
                                win.width = w;
                                win.height = h;
                                attached = true;
                                let mut res = [0u8; 8];
                                res[0..4].copy_from_slice(&OP_RES_SHARED_ATTACHED.to_le_bytes());
                                res[4..8].copy_from_slice(&window_id.to_le_bytes());
                                let _ = mochi_syscall::ipc::ipc_send(sender, &res);
                            }
                        }
                    }

                    // Fallback: allow privileged clients (Binder) to allocate and send pages.
                    if !attached {
                        pending_shared = Some((sender, window_id, w, h));
                    }
                }
                OP_REQ_PRESENT_SHARED => {
                    if len < 8 {
                        continue;
                    }
                    let window_id = u32::from_le_bytes([recv[4], recv[5], recv[6], recv[7]]);
                    if windows.iter().any(|w| w.id == window_id) {
                        unsafe {
                            let fb = core::slice::from_raw_parts_mut(fb_ptr, pixel_count);
                            for px in fb.iter_mut() {
                                core::ptr::write_volatile(px, background);
                            }
                        }
                        windows.sort_by_key(|w| w.layer);
                        for w in windows.iter() {
                            blit_window(
                                fb_ptr,
                                info.stride as usize,
                                info.height as usize,
                                w.x,
                                w.y,
                                w.width as usize,
                                w.height as usize,
                                window_pixels(w),
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        let sc = match keyboard::read_scancode_tap() {
            Ok(Some(s)) => s,
            Ok(None) => continue,
            Err(_) => continue,
        };

        // break code is make|0x80 for set1
        if sc == 0x92 {
            e_down = false;
            continue;
        }

        if is_e_make(sc) {
            if e_down {
                continue;
            }
            e_down = true;

            let path = "/applications/TestClient.app/entry.elf";
            match process::exec(path) {
                Ok(pid) => println!("[Kagami] launched TestClient pid={}", pid),
                Err(()) => println!("[Kagami] failed to exec {}", path),
            }
        }
    }
}

fn window_pixels(win: &Window) -> &[u32] {
    if let Some(addr) = win.shared_addr {
        let needed = (win.width as usize).saturating_mul(win.height as usize).saturating_mul(4);
        if needed == 0 || needed > win.shared_bytes {
            return &win.pixels;
        }
        // Safety: kernel mapped shared pages into our address space.
        unsafe {
            core::slice::from_raw_parts(addr as *const u32, needed / 4)
        }
    } else {
        &win.pixels
    }
}

fn blit_window(
    fb_ptr: *mut u32,
    stride: usize,
    fb_h: usize,
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    pixels: &[u32],
) {
    if fb_ptr.is_null() {
        return;
    }
    unsafe {
        let fb = core::slice::from_raw_parts_mut(fb_ptr, stride.saturating_mul(fb_h));
        for sy in 0..h {
            let dy = y + sy as i32;
            if dy < 0 {
                continue;
            }
            let dy = dy as usize;
            if dy >= fb_h {
                continue;
            }
            for sx in 0..w {
                let dx = x + sx as i32;
                if dx < 0 {
                    continue;
                }
                let dx = dx as usize;
                if dx >= stride {
                    continue;
                }
                let src_idx = sy * w + sx;
                if src_idx >= pixels.len() {
                    continue;
                }
                let dst_idx = dy * stride + dx;
                if dst_idx < fb.len() {
                    let spx = pixels[src_idx];
                    let a = ((spx >> 24) & 0xFF) as u32;
                    if a == 0 {
                        continue;
                    }
                    if a >= 255 {
                        core::ptr::write_volatile(&mut fb[dst_idx], spx);
                        continue;
                    }
                    // Alpha blend over current framebuffer pixel.
                    // Note: tiny-skia outputs premultiplied alpha, so RGB is already scaled by A.
                    let dpx = fb[dst_idx];
                    let inv = 255u32 - a;
                    let sr = ((spx >> 16) & 0xFF) as u32;
                    let sg = ((spx >> 8) & 0xFF) as u32;
                    let sb = (spx & 0xFF) as u32;
                    let dr = ((dpx >> 16) & 0xFF) as u32;
                    let dg = ((dpx >> 8) & 0xFF) as u32;
                    let db = (dpx & 0xFF) as u32;
                    let r = (sr + (dr * inv) / 255).min(255);
                    let g = (sg + (dg * inv) / 255).min(255);
                    let b = (sb + (db * inv) / 255).min(255);
                    core::ptr::write_volatile(&mut fb[dst_idx], 0xFF00_0000 | (r << 16) | (g << 8) | b);
                }
            }
        }
    }
}
