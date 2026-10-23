mod date;
mod store;
mod ui;

use appcore::prelude::*;

struct CalendarApp {
    view: ui::CalendarView,
}
impl App for CalendarApp {
    type Body = ui::CalendarView;
    fn new() -> Self {
        Self {
            view: ui::CalendarView::new(),
        }
    }
    fn window(&self) -> WindowOptions {
        WindowOptions::new("Calendar")
            .size(1100.0, 730.0)
            .resizable(true)
    }
    fn body(&self, _: &ViewContext) -> Self::Body {
        self.view.clone()
    }
}
fn main() -> Result<(), ViewKitError> {
    run::<CalendarApp>()
}
