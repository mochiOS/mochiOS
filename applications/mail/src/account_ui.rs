//! Account setup uses the same field geometry for painting and event delivery.
use crate::backend::{Config, Credential, Error, Protocol, Result, Security};
use appcore::prelude::*;
use appcore::viewkit::event::{EventContext, EventResult, ViewEvent};
use appcore::viewkit::view::PaintContext;
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AccountPage {
    List,
    Choose,
    Details,
    Manage(usize),
    Remove(usize),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Gmail,
    Imap,
    Pop3,
}
impl Provider {
    pub fn title(self) -> &'static str {
        match self {
            Self::Gmail => "Gmail",
            Self::Imap => "IMAP",
            Self::Pop3 => "POP3",
        }
    }
}
pub struct AccountForm {
    pub open: Cell<bool>,
    pub page: Cell<AccountPage>,
    pub provider: Cell<Provider>,
    pub advanced: Cell<bool>,
    pub protocol: Cell<Protocol>,
    pub incoming_security: Cell<Security>,
    pub smtp_security: Cell<Security>,
    pub feedback: RefCell<String>,
    invalid: Cell<Option<usize>>,
    states: Vec<TextFieldInteractionState>,
}
const LABELS: [&str; 7] = [
    "Email address",
    "Username",
    "Password",
    "Incoming server",
    "Port",
    "Outgoing server",
    "Port",
];
fn rect(b: Rect, x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(b.origin.x + x, b.origin.y + y, w.max(0.0), h)
}
impl AccountForm {
    fn field(&self, index: usize) -> TextField {
        TextField::with_interaction(self.states[index].clone())
            .secure(index == 2)
            .invalid(self.invalid.get() == Some(index))
            .placeholder(match index {
                0 => "name@example.com",
                1 => "Same as email address",
                2 => {
                    if self.provider.get() == Provider::Gmail {
                        "Google app password"
                    } else {
                        "Password"
                    }
                }
                3 => {
                    if self.provider.get() == Provider::Pop3 {
                        "pop.example.com"
                    } else {
                        "imap.example.com"
                    }
                }
                5 => "smtp.example.com",
                _ => LABELS[index],
            })
    }
    pub fn new() -> Self {
        let states: Vec<_> = (0..7).map(|_| TextFieldInteractionState::new()).collect();
        let form = Self {
            open: Cell::new(false),
            page: Cell::new(AccountPage::Choose),
            provider: Cell::new(Provider::Imap),
            advanced: Cell::new(false),
            protocol: Cell::new(Protocol::Imap),
            incoming_security: Cell::new(Security::Tls),
            smtp_security: Cell::new(Security::StartTls),
            feedback: RefCell::new(String::new()),
            invalid: Cell::new(None),
            states,
        };
        form.states[4].set_value("993");
        form.states[6].set_value("587");
        form
    }
    pub fn preset(&self, kind: u8) {
        self.clear_secret();
        self.invalid.set(None);
        self.feedback.borrow_mut().clear();
        self.advanced.set(false);
        self.provider.set(match kind {
            0 => Provider::Gmail,
            2 => Provider::Pop3,
            _ => Provider::Imap,
        });
        self.protocol.set(if kind == 2 {
            Protocol::Pop3
        } else {
            Protocol::Imap
        });
        self.states[1].set_value("");
        self.states[4].set_value(if kind == 2 { "995" } else { "993" });
        self.states[6].set_value("587");
        self.incoming_security.set(Security::Tls);
        self.smtp_security.set(Security::StartTls);
        self.states[3].set_value(if kind == 0 { "imap.gmail.com" } else { "" });
        self.states[5].set_value(if kind == 0 { "smtp.gmail.com" } else { "" });
        self.page.set(AccountPage::Details);
    }
    pub fn set_incoming_port(&self, port: u16) {
        self.states[4].set_value(port.to_string());
    }
    pub fn set_smtp_port(&self, port: u16) {
        self.states[6].set_value(port.to_string());
    }
    pub fn clear_secret(&self) {
        self.states[2].clear();
    }
    pub fn expanded(&self) -> bool {
        self.provider.get() != Provider::Gmail || self.advanced.get()
    }
    pub fn submit_y(&self) -> f32 {
        if !self.expanded() {
            392.0
        } else if self.advanced.get() {
            862.0
        } else {
            762.0
        }
    }
    pub fn height(&self) -> f32 {
        self.submit_y() + 180.0
    }
    pub fn fields(&self, b: Rect) -> Vec<(usize, Rect, &'static str)> {
        let w = (b.size.width - 48.0).max(0.0);
        let mut fields = vec![
            (0, rect(b, 24.0, 144.0, w, 32.0), "Email address"),
            (
                2,
                rect(b, 24.0, 220.0, w, 32.0),
                if self.provider.get() == Provider::Gmail {
                    "App password"
                } else {
                    "Password"
                },
            ),
        ];
        if self.expanded() {
            fields.extend([
                (3, rect(b, 24.0, 412.0, w, 32.0), "Server address"),
                (4, rect(b, 24.0, 478.0, 96.0, 32.0), "Port"),
                (5, rect(b, 24.0, 582.0, w, 32.0), "Server address"),
                (6, rect(b, 24.0, 648.0, 96.0, 32.0), "Port"),
            ]);
            if self.advanced.get() {
                fields.push((1, rect(b, 24.0, 764.0, w, 32.0), "Username"));
            }
        }
        for (_, frame, _) in &mut fields {
            frame.origin.y += 32.0;
        }
        fields
    }
    fn invalid(&self, index: usize, message: &str) -> Error {
        self.invalid.set(Some(index));
        *self.feedback.borrow_mut() = message.into();
        for (i, s) in self.states.iter().enumerate() {
            s.set_focused(i == index);
        }
        Error::Invalid
    }
    pub fn credential(&self) -> Result<Credential> {
        self.invalid.set(None);
        self.feedback.borrow_mut().clear();
        let email = self.states[0].value().trim().to_owned();
        if crate::mime::address(&email).is_err() {
            return Err(self.invalid(0, "Enter a valid email address."));
        }
        let username = self.states[1].value().trim().to_owned();
        let incoming_port = self.states[4]
            .value()
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or_else(|| self.invalid(4, "Enter an incoming server port from 1 to 65535."))?;
        let smtp_port = self.states[6]
            .value()
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or_else(|| self.invalid(6, "Enter an outgoing server port from 1 to 65535."))?;
        let incoming_host = self.states[3].value().trim().to_owned();
        let smtp_host = self.states[5].value().trim().to_owned();
        for (i, host) in [(3, &incoming_host), (5, &smtp_host)] {
            if crate::backend::validate_server(host).is_err() {
                return Err(self.invalid(
                    i,
                    "Enter the server address provided by your email service.",
                ));
            }
        }
        let config = Config {
            email: email.clone(),
            username: if username.is_empty() { email } else { username },
            incoming_host,
            incoming_port,
            incoming_security: self.incoming_security.get(),
            protocol: self.protocol.get(),
            smtp_host,
            smtp_port,
            smtp_security: self.smtp_security.get(),
        };
        if config.validate().is_err() {
            return Err(self.invalid(
                1,
                "Check the email address and username in Advanced Settings.",
            ));
        }
        let mut password = zeroize::Zeroizing::new(self.states[2].take_value());
        if self.provider.get() == Provider::Gmail {
            password.retain(|c| c != ' ');
        }
        if password.is_empty() {
            return Err(self.invalid(
                2,
                if self.provider.get() == Provider::Gmail {
                    "Enter a Google app password."
                } else {
                    "Enter your account password."
                },
            ));
        }
        Ok(Credential { config, password })
    }
    pub fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        for (i, frame, label) in self.fields(b) {
            Text::caption(label)
                .color(c.theme.colors.text_secondary)
                .paint(
                    rect(
                        b,
                        frame.origin.x - b.origin.x,
                        frame.origin.y - b.origin.y - 22.0,
                        frame.size.width,
                        20.0,
                    ),
                    c,
                );
            self.field(i).paint(frame, c);
        }
        if self.provider.get() == Provider::Gmail {
            Text::caption("Use an app password from your Google Account.\nYour regular Google password will not work.").color(c.theme.colors.text_secondary).paint(rect(b,24.0,296.0,b.size.width-48.0,44.0),c);
        }
        if self.expanded() {
            Text::label("Incoming Mail").paint(rect(b, 24.0, 388.0, b.size.width - 48.0, 24.0), c);
            Text::caption(self.provider.get().title())
                .color(c.theme.colors.text_secondary)
                .alignment(TextAlignment::End)
                .paint(rect(b, b.size.width - 100.0, 388.0, 76.0, 24.0), c);
            Text::caption("Connection security")
                .color(c.theme.colors.text_secondary)
                .paint(rect(b, 140.0, 488.0, b.size.width - 164.0, 20.0), c);
            Rectangle::new()
                .color(RectangleColor::Custom(c.theme.colors.border))
                .paint(rect(b, 24.0, 544.0, b.size.width - 48.0, 1.0), c);
            Text::label("Outgoing Mail").paint(rect(b, 24.0, 558.0, b.size.width - 48.0, 24.0), c);
            Text::caption("SMTP")
                .color(c.theme.colors.text_secondary)
                .alignment(TextAlignment::End)
                .paint(rect(b, b.size.width - 100.0, 558.0, 76.0, 24.0), c);
            Text::caption("Connection security")
                .color(c.theme.colors.text_secondary)
                .paint(rect(b, 140.0, 658.0, b.size.width - 164.0, 20.0), c);
        }
    }
    pub fn event(&self, b: Rect, e: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        let mut result = EventResult::Ignored;
        for (i, frame, _) in self.fields(b) {
            result = result.merge(self.field(i).handle_event(frame, e, c));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gmail_only_requests_credentials_until_advanced_is_opened() {
        let f = AccountForm::new();
        f.preset(0);
        let b = Rect::new(20.0, 10.0, 320.0, 1000.0);
        assert_eq!(
            f.fields(b).iter().map(|(i, _, _)| *i).collect::<Vec<_>>(),
            vec![0, 2]
        );
        f.advanced.set(true);
        assert_eq!(f.fields(b).len(), 7);
        for kind in [1, 2] {
            f.preset(kind);
            assert_eq!(f.fields(b).len(), 6);
        }
    }
    #[test]
    fn invalid_fields_keep_password_until_a_valid_submission() {
        let f = AccountForm::new();
        f.preset(0);
        f.states[0].set_value("bad");
        f.states[2].set_value("abcd efgh ijkl mnop");
        assert!(f.credential().is_err());
        assert_eq!(f.invalid.get(), Some(0));
        assert!(!f.states[2].value().is_empty());
        f.states[0].set_value("person@gmail.com");
        let credential = f.credential().unwrap();
        assert_eq!(credential.password.as_str(), "abcdefghijklmnop");
        assert!(f.states[2].value().is_empty());
        assert_eq!(credential.config.incoming_host, "imap.gmail.com");
    }
    #[test]
    fn fields_fit_narrow_windows_and_never_overlap() {
        for width in [320.0, 400.0, 560.0] {
            for kind in [0, 1, 2] {
                for advanced in [false, true] {
                    let f = AccountForm::new();
                    f.preset(kind);
                    f.advanced.set(advanced);
                    let b = Rect::new(32.0, 18.0, width, 1200.0);
                    let fields = f.fields(b);
                    for (_, frame, _) in &fields {
                        assert!(frame.origin.x >= b.origin.x);
                        assert!(frame.origin.x + frame.size.width <= b.origin.x + width);
                        assert!(frame.origin.y + frame.size.height < b.origin.y + f.submit_y());
                    }
                    for (i, (_, a, _)) in fields.iter().enumerate() {
                        for (_, z, _) in fields.iter().skip(i + 1) {
                            assert!(
                                a.origin.y + a.size.height <= z.origin.y
                                    || z.origin.y + z.size.height <= a.origin.y
                                    || a.origin.x + a.size.width <= z.origin.x
                                    || z.origin.x + z.size.width <= a.origin.x
                            );
                        }
                    }
                }
            }
        }
    }
}
