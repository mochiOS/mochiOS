mod editing;
mod engine;
mod history;
mod number;
mod preferences;
mod ui;
mod units;
use appcore::prelude::*;
struct CalculatorApp {
    view: ui::CalculatorView,
}
impl App for CalculatorApp {
    type Body = ui::CalculatorView;
    fn new() -> Self {
        Self {
            view: ui::CalculatorView::new(),
        }
    }
    fn window(&self) -> WindowOptions {
        WindowOptions::new("Calculator")
            .size(360.0, 540.0)
            .resizable(true)
    }
    fn window_for(&self, window: appcore::viewkit::app::WindowId) -> WindowOptions {
        if window == appcore::viewkit::app::WindowId::PRIMARY {
            self.window()
        } else {
            WindowOptions::new("Calculator History")
                .size(340.0, 500.0)
                .resizable(true)
        }
    }
    fn body_for(&self, window: appcore::viewkit::app::WindowId, _: &ViewContext) -> Self::Body {
        if window == appcore::viewkit::app::WindowId::PRIMARY {
            self.view.clone()
        } else {
            self.view.history_view()
        }
    }
    fn close_requested_for_window(&mut self, window: appcore::viewkit::app::WindowId) -> bool {
        self.view.closing(window);
        true
    }
    fn body(&self, _: &ViewContext) -> Self::Body {
        self.view.clone()
    }
}
fn main() -> Result<(), ViewKitError> {
    run::<CalculatorApp>()
}
