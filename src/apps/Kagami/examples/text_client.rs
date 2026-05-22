use swiftlib::{time, vga};

fn main() {
    println!("[TextClient] starting");

    // MVP: 直接フレームバッファへ簡単な描画を行う（将来的にはKagami IPCへ移行する想定）
    let info = match vga::get_info() {
        Some(i) => i,
        None => {
            println!("[TextClient] failed to get framebuffer info");
            return;
        }
    };
    let fb_ptr = match vga::map_framebuffer() {
        Some(p) => p,
        None => {
            println!("[TextClient] failed to map framebuffer");
            return;
        }
    };

    // 画面左上に色付き矩形を描画して動作確認
    let w = core::cmp::min(240u32, info.width);
    let h = core::cmp::min(120u32, info.height);
    let stride = info.stride as usize;
    unsafe {
        let fb = core::slice::from_raw_parts_mut(fb_ptr, stride.saturating_mul(info.height as usize));
        for y in 0..h as usize {
            let row = y.saturating_mul(stride);
            for x in 0..w as usize {
                let idx = row + x;
                if idx < fb.len() {
                    core::ptr::write_volatile(&mut fb[idx], 0xFF00_66FF); // ARGB
                }
            }
        }
    }

    println!("[TextClient] drew a rectangle; exiting soon");
    time::sleep_ms(300);
}
