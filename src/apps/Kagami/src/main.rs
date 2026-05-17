mod app;
mod input;
mod ipc_proto;
mod mouse;
mod renderer;

use app::KagamiApp;
use renderer::Renderer;
use swiftlib::vga;

pub fn main() {
    let info = match vga::get_info() {
        Some(i) => i,
        None => {
            eprintln!("[KAGAMI] failed: get framebuffer info");
            return;
        }
    };
    let fb_ptr = match vga::map_framebuffer() {
        Some(p) => p,
        None => {
            eprintln!("[KAGAMI] failed: map framebuffer");
            return;
        }
    };

    let renderer = Renderer::new(fb_ptr, info);
    let mut app = KagamiApp::new(renderer);
    app.run();
}
