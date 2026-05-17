use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use viewkit::{AppBuilder, VComponent};

#[allow(unused)]
pub struct WindowComponent {
    title: String,
    height: i32,
    width: i32,
}

impl From<WindowComponent> for VComponent {
    fn from(window: WindowComponent) -> Self {
        let component = VComponent::new(VComponent::from_str(include_str!("components/window.html")));
        component.height(window.height as u32).width(window.width as u32)

    }
}

fn main() -> Result<(), String> {
    const WIDTH: u32 = 1280;
    const HEIGHT: u32 = 800;

    let app = AppBuilder::new(WIDTH, HEIGHT)
        .children(|| {
            let window = WindowComponent { title: "ExampleWindow".into(), height: 250, width: 400 };
            VComponent::new(VComponent::from(window))
        })?
        .build()?;

    let _frame_done = Arc::new(AtomicBool::new(false));
    let mut frame_count = 0_u32;

    loop {
        let _ui = app().render();

        frame_count += 1;
        if frame_count % 60 == 0 {
            println!("[Binder] Rendered frame {}", frame_count);
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}