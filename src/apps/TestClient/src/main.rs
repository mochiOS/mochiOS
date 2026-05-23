use swiftlib::{time, vga};
use viewkit::{components, render_ui_element_to_pixmap};

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

    println!("[TestClient] building ViewKit UI");
    let ui = components::card()
        .children([
            components::text().text("Hello from ViewKit components").into_elem(),
            components::card()
                .children([components::text().label("Nested card").into_elem()])
                .into_elem(),
        ])
        .into_elem();

    println!("[TestClient] rendering ViewKit UI");
    let pixels = render_ui_element_to_pixmap(&ui, info.width, info.height);
    println!("[TestClient] blitting framebuffer");
    let stride = info.stride as usize;
    unsafe {
        let fb =
            core::slice::from_raw_parts_mut(fb_ptr, stride.saturating_mul(info.height as usize));
        for y in 0..info.height as usize {
            let row = y * stride;
            let src_row = y * info.width as usize;
            for x in 0..info.width as usize {
                let idx = row + x;
                if idx < fb.len() {
                    core::ptr::write_volatile(&mut fb[idx], pixels[src_row + x]);
                }
            }
        }
    }

    println!("[TestClient] rendered ViewKit UI");
    time::sleep_ms(300);
}
