use viewkit::components::VComponent;
use viewkit::components_list;
use viewkit::AppBuilder;
use std::sync::Arc;

components_list! {
    button,
    card,
    text,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    Detail,
}

#[cfg(unix)]
fn main() -> Result<(), String> {
    const WIDTH: u32 = 960;
    const HEIGHT: u32 = 540;

    // 画面状態を管理
    let screen_state: Arc<viewkit::State<Screen>> = Arc::new(viewkit::State::new(Screen::Home));

    AppBuilder::new(WIDTH, HEIGHT)
        .children({
            let state = screen_state.clone();
            move || {
                match state.get() {
                    Screen::Home => {
                    let state = state.clone();
                    card()
                        .label("Home Screen - Click to Detail")
                        .on_click(move || {
                            state.set(Screen::Detail);
                            println!("Navigated to detail screen");
                        })
                    }
                    Screen::Detail => {
                    let state = state.clone();
                    card()
                        .label("Detail Screen - Click to Home")
                        .on_click(move || {
                            state.set(Screen::Home);
                            println!("Navigated back to home");
                        })
                    }
                }
            }
        })?
        .build()?
        .run()
}

#[cfg(not(unix))]
fn main() {
    eprintln!("stateful_ui requires a unix host with Wayland.");
}
