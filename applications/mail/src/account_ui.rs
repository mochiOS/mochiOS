//! Native account setup; credentials only leave the form for a Keychain worker.
use crate::backend::{Config, Credential, Error, Protocol, Result, Security};
use appcore::prelude::*;
use appcore::viewkit::event::{EventContext, EventResult, ViewEvent};
use appcore::viewkit::view::PaintContext;
use std::cell::Cell;

pub struct AccountForm {
    pub open: Cell<bool>,
    pub protocol: Cell<Protocol>,
    pub incoming_security: Cell<Security>,
    pub smtp_security: Cell<Security>,
    states: Vec<TextFieldInteractionState>,
    fields: Vec<TextField>,
}
const LABELS: [&str; 7] = [
    "メールアドレス",
    "ユーザー名",
    "パスワード",
    "受信サーバー",
    "受信ポート",
    "SMTPサーバー",
    "SMTPポート",
];
impl AccountForm {
    pub fn new() -> Self {
        let states: Vec<_> = (0..7).map(|_| TextFieldInteractionState::new()).collect();
        let fields = states
            .iter()
            .enumerate()
            .map(|(i, s)| {
                TextField::with_interaction(s.clone())
                    .secure(i == 2)
                    .placeholder(LABELS[i])
            })
            .collect();
        let form = Self {
            open: Cell::new(false),
            protocol: Cell::new(Protocol::Imap),
            incoming_security: Cell::new(Security::Tls),
            smtp_security: Cell::new(Security::StartTls),
            states,
            fields,
        };
        form.states[4].set_value("993");
        form.states[6].set_value("587");
        form
    }
    pub fn preset(&self, kind: u8) {
        self.protocol.set(if kind == 2 {
            Protocol::Pop3
        } else {
            Protocol::Imap
        });
        self.states[4].set_value(if kind == 2 { "995" } else { "993" });
        self.incoming_security.set(Security::Tls);
        if kind == 0 {
            self.states[3].set_value("imap.gmail.com");
            self.states[5].set_value("smtp.gmail.com");
            self.states[6].set_value("587");
            self.smtp_security.set(Security::StartTls);
        } else {
            self.states[3].set_value("");
            self.states[5].set_value("");
        }
    }
    pub fn clear_secret(&self) {
        self.states[2].clear();
    }
    pub fn credential(&self) -> Result<Credential> {
        let email = self.states[0].value().trim().to_owned();
        let username = self.states[1].value().trim().to_owned();
        let config = Config {
            email: email.clone(),
            username: if username.is_empty() { email } else { username },
            incoming_host: self.states[3].value().trim().to_owned(),
            incoming_port: self.states[4].value().parse().map_err(|_| Error::Invalid)?,
            incoming_security: self.incoming_security.get(),
            protocol: self.protocol.get(),
            smtp_host: self.states[5].value().trim().to_owned(),
            smtp_port: self.states[6].value().parse().map_err(|_| Error::Invalid)?,
            smtp_security: self.smtp_security.get(),
        };
        config.validate()?;
        let mut password = self.states[2].take_value();
        // Google displays app passwords as groups of four characters.
        if config.incoming_host == "imap.gmail.com" {
            password.retain(|c| c != ' ');
        }
        Ok(Credential {
            config,
            password: zeroize::Zeroizing::new(password),
        })
    }
    pub fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        Text::styled("メールアカウントを追加", TextRole::TitleSmall).paint(
            Rect::new(
                b.origin.x + 16.0,
                b.origin.y + 12.0,
                b.size.width - 32.0,
                28.0,
            ),
            c,
        );
        Text::caption(if self.protocol.get() == Protocol::Imap {
            "受信方式：IMAP"
        } else {
            "受信方式：POP3"
        })
        .color(c.theme.colors.text_secondary)
        .paint(
            Rect::new(
                b.origin.x + 16.0,
                b.origin.y + 84.0,
                b.size.width - 32.0,
                18.0,
            ),
            c,
        );
        for i in 0..7 {
            let y = b.origin.y + 104.0 + i as f32 * 43.0;
            Text::caption(LABELS[i]).paint(Rect::new(b.origin.x + 16.0, y + 6.0, 112.0, 24.0), c);
            self.fields[i].paint(
                Rect::new(
                    b.origin.x + 132.0,
                    y,
                    (b.size.width - 148.0).max(60.0),
                    32.0,
                ),
                c,
            );
        }
        Text::caption("Gmailはアプリパスワードを使用します。Googleでログインは後日対応。\n受信・SMTPの両方を確認してからKeychainへ保存します。\nメールの整理・下書きはこの起動中のみ。POP3はサーバーに残します。")
            .color(c.theme.colors.text_secondary).paint(Rect::new(b.origin.x+16.0,b.origin.y+443.0,b.size.width-32.0,68.0),c);
    }
    pub fn event(&self, b: Rect, e: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        let mut result = EventResult::Ignored;
        for (i, field) in self.fields.iter().enumerate() {
            result = result.merge(field.handle_event(
                Rect::new(
                    b.origin.x + 132.0,
                    b.origin.y + 104.0 + i as f32 * 43.0,
                    (b.size.width - 148.0).max(60.0),
                    32.0,
                ),
                e,
                c,
            ));
        }
        result
    }
}
