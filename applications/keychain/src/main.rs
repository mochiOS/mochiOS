mod ui;
use appcore::prelude::*;
struct KeychainApp {
    view: ui::KeychainView,
}
impl App for KeychainApp {
    type Body = ui::KeychainView;
    fn new() -> Self {
        Self {
            view: ui::KeychainView::new(),
        }
    }
    fn window(&self) -> WindowOptions {
        WindowOptions::new("Keychain")
            .size(880.0, 620.0)
            .resizable(true)
    }
    fn body(&self, _: &ViewContext) -> Self::Body {
        self.view.clone()
    }
}
fn main() -> Result<(), ViewKitError> {
    run::<KeychainApp>()
}
