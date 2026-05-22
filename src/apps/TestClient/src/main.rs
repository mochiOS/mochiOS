use swiftlib::{time, vga};

fn main() {
    println!("[TestClient] starting");

    let info = match vga::get_info() {
        Some(i) => i,
        None => {
            println!("[TestClient] failed to get framebuffer info");
            return;
        }
    };
    let fb_ptr = match vga::map_framebuffer() {
        Some(p) => p,
        None => {
            println!("[TestClient] failed to map framebuffer");
            return;
        }
    };

    // 画面右下に矩形を描画して動作確認
    let w = core::cmp::min(240u32, info.width);
    let h = core::cmp::min(120u32, info.height);
    let x0 = info.width.saturating_sub(w);
    let y0 = info.height.saturating_sub(h);
    let stride = info.stride as usize;
    unsafe {
        let fb = core::slice::from_raw_parts_mut(fb_ptr, stride.saturating_mul(info.height as usize));
        for y in 0..h as usize {
            let row = (y0 as usize + y).saturating_mul(stride);
            for x in 0..w as usize {
                let idx = row + (x0 as usize + x);
                if idx < fb.len() {
                    core::ptr::write_volatile(&mut fb[idx], 0xFFFF_CC00); // ARGB
                }
            }
        }
    }

    println!("[TestClient] drew a rectangle; exiting soon");
    time::sleep_ms(300);
}
