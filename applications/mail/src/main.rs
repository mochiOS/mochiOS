mod account_ui;
mod backend;
mod layout;
mod mime;
mod model;
#[cfg(target_os = "mochios")]
mod native_socket;
mod protocol;
mod transport;
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
#[cfg(target_os = "mochios")]
getrandom::register_custom_getrandom!(mochios_tls_client::platform_getrandom);

fn main() -> Result<(), ViewKitError> {
    run::<MailApp>()
}

#[cfg(test)]
mod tests;
