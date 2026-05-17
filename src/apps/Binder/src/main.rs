use viewkit::{AppBuilder, VComponent, pipeline};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub struct WindowComponent {
    title: String,
}

impl VComponent for WindowComponent {
    fn render(&self) -> String {
        let content_w = 800;
        let content_h = 600;

        format!(
            r#"<div class="appwindow" style="width: {}px; height: {}px;">
                <div class="appwindow-control">
                    <div class="appwindow-spacer"></div>
                    <div class="appwindow-title">
                        {}
                    </div>
                    <div class="appwindow-control-buttons">
                        <img
                            src="components/icons/MinimizeButton.png"
                            class="appwindow-control-button"
                        />
                        <img
                            src="components/icons/FullScreenButton.png"
                            class="appwindow-control-button"
                        />
                        <img src="components/icons/CloseButton.png" class="appwindow-control-button" />
                    </div>
                </div>
                <div class="appwindow-content">
                    <Children />
                </div>
            </div>"#,
            content_w, content_h, self.title
        )
    }

    fn css(&self) -> String {
        r#"
            body {
                background-color: #000;
            }
            .appwindow {
                border-radius: 10px;
                display: flex;
                flex-direction: column;
                border: 1px solid #ccc; /* Add a border for decoration */
                box-shadow: 2px 2px 8px rgba(0, 0, 0, 0.2); /* Add a shadow */
            }

            .appwindow-control {
                border-radius: 10px 10px 0px 0px;
                background-color: #f0f0f0;
                display: flex;
                justify-content: center;
                align-items: center;
                padding: 5px;
                cursor: grab; /* Indicate draggable area */
            }
            .appwindow-spacer {
                flex: 1;
            }
            .appwindow-title {
                flex: 1;
                text-align: center;
                font-family: sans-serif;
                font-size: 14px;
                color: #333;
            }
            .appwindow-control-buttons {
                display: flex;
                justify-content: flex-end;
                flex: 1;
                margin-left: 10px;
            }
            .appwindow-control-button {
                width: 20px;
                height: 20px;
                margin: 0 2px;
                border-radius: 50%; /* Make buttons round */
                border: 1px solid #aaa;
                cursor: pointer;
            }
            .appwindow-control-button:hover {
                opacity: 0.8;
            }

            .appwindow-content {
                background-color: #afafaf;
                width: 100%;
                flex: 1;
                overflow: hidden;
                border-radius: 0px 0px 10px 10px;
            }
        "#.to_string()
    }
}

fn main() -> Result<(), String> {
    const WIDTH: u32 = 1280;
    const HEIGHT: u32 = 800;

    let ui_builder = AppBuilder::new(WIDTH, HEIGHT)
        .children(|| {
            WindowComponent {
                title: "My Awesome Window".to_string(),
            }
        })?
        .build()?;

    let frame_done = Arc::new(AtomicBool::new(false));
    let mut frame_count = 0_u32;

    // In a real scenario, Binder would manage multiple windows.
    // Here, we're just rendering one for demonstration.
    loop {
        let ui = ui_builder();
        let html = ui.render();
        let css = ui.css();

        // In a real Binder, this would draw to a specific window's buffer.
        // For now, we'll just simulate rendering.
        let rendered = pipeline::render_document(&html, &css, WIDTH, HEIGHT);
        // Here you would take 'rendered.framebuffer.pixels' and blit it to the actual display.

        frame_count += 1;
        if frame_count % 60 == 0 {
            println!("[Binder] Rendered frame {}", frame_count);
        }
        std::thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }
}
