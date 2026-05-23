use swiftlib::{keyboard, process, time, vga};

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

    // 背景色で塗りつぶし（ARGB, 常に不透明で書き込む）
    let background: u32 = 0xFF1F_1F1F;
    let pixel_count = (info.stride as usize).saturating_mul(info.height as usize);
    unsafe {
        let fb = core::slice::from_raw_parts_mut(fb_ptr, pixel_count);
        for px in fb.iter_mut() {
            core::ptr::write_volatile(px, background);
        }
    }

    // Launch Binder (desktop shell) automatically.
    let binder_path = "/applications/Binder.app/entry.elf";
    match process::exec(binder_path) {
        Ok(pid) => println!("[Kagami] launched Binder pid={}", pid),
        Err(()) => println!("[Kagami] failed to exec {}", binder_path),
    }

    println!("[Kagami] ready (press 'e' to launch test_client)");

    let mut e_down = false;
    loop {
        time::sleep_ms(10);

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
