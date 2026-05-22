use swiftlib::{time, vga};
use viewkit::{render_component_to_pixmap, VComponent};

const CARD_TEMPLATE: &str = r#"
<style>
    .screen {
        width: CONTENT_W;
        height: CONTENT_H;
        display: flex;
        justify-content: center;
        align-items: center;
        background-color: #1d2330;
    }
    .card {
        width: 360px;
        height: 200px;
        border-radius: 18px;
        background-color: #f8f9fb;
        display: flex;
        flex-direction: column;
        padding: 18px;
        gap: 12px;
    }
    .accent {
        height: 48px;
        border-radius: 14px;
        background-color: #4f46e5;
    }
    .body {
        flex: 1;
        border-radius: 14px;
        background-color: #e5e7eb;
    }
</style>
<div class="screen">
    <div class="card">
        <div class="body">
            <Children />
        </div>
    </div>
</div>
"#;

const BODY_TEMPLATE: &str = r#"
<style>
    .sample {
        width: 100%;
        height: 100%;
        border-radius: 10px;
        background-color: #d1d5db;
    }
</style>
<div class="sample">
</div>
"#;

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

    println!("[TestClient] building ViewKit card");
    let card = VComponent::from_str(CARD_TEMPLATE)
        .width(info.width)
        .height(info.height)
        .child(VComponent::from_str(BODY_TEMPLATE));

    println!("[TestClient] rendering ViewKit card");
    let pixels = render_component_to_pixmap(&card, info.width, info.height);
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

    println!("[TestClient] rendered ViewKit card");
    time::sleep_ms(300);
}
