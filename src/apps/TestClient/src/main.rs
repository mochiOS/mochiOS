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
        justify-content: center;
        padding: 24px;
    }
    .title {
        text-align: center;
        color: #111827;
    }
    .body {
        text-align: center;
        color: #4b5563;
        margin-left: 0px;
    }
</style>
<div class="screen">
    <div class="card">
        <div class="title">
            <Content type="String" />
        </div>
        <div class="body">
            <Children />
        </div>
    </div>
</div>
"#;

const BODY_TEMPLATE: &str = r#"
<style>
    .copy {
        color: #4b5563;
        text-align: center;
    }
</style>
<div class="copy">
    <Content type="String" />
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

    let card = VComponent::from_str(CARD_TEMPLATE)
        .width(info.width)
        .height(info.height)
        .label("ViewKit HTML Card".to_string())
        .child(
            VComponent::from_str(BODY_TEMPLATE)
                .label("Rendered from TestClient using ViewKit.".to_string()),
        );

    let pixels = render_component_to_pixmap(&card, info.width, info.height);
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
