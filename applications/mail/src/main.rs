mod layout;
mod model;
mod ui;
use appcore::prelude::*;
struct MailApp {
    view: ui::MailView,
}
impl App for MailApp {
    type Body = ui::MailView;
    fn new() -> Self {
        Self {
            view: ui::MailView::new(),
        }
    }
    fn window(&self) -> WindowOptions {
        WindowOptions::new("Mail")
            .size(1080.0, 720.0)
            .resizable(true)
    }
    fn body(&self, _: &ViewContext) -> Self::Body {
        self.view.clone()
    }
}
fn main() -> Result<(), ViewKitError> {
    run::<MailApp>()
}
