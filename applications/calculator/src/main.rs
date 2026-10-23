mod engine;
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
            .size(600.0, 690.0)
            .resizable(true)
    }
    fn body(&self, _: &ViewContext) -> Self::Body {
        self.view.clone()
    }
}
fn main() -> Result<(), ViewKitError> {
    run::<CalculatorApp>()
}
