mod model;
mod ui;
use appcore::prelude::*;
struct DiskUtilityApp {
    view: ui::DiskView,
}
impl App for DiskUtilityApp {
    type Body = ui::DiskView;
    fn new() -> Self {
        Self {
            view: ui::DiskView::new(),
        }
    }
    fn window(&self) -> WindowOptions {
        WindowOptions::new("Disk Utility")
            .size(960.0, 640.0)
            .resizable(true)
    }
    fn body(&self, _: &ViewContext) -> Self::Body {
        self.view.clone()
    }
}
fn main() -> Result<(), ViewKitError> {
    run::<DiskUtilityApp>()
}
