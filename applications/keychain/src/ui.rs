use appcore::viewkit::{
    event::{EventContext, EventResult, ViewEvent},
    platform::Key,
    view::{Constraints, MeasureContext, PaintContext},
};
use appcore::{
    keychain::{Application, Error, Item, ItemId, Keychain, Secret, Status},
    prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Refresh,
    Lock,
    New,
    Select(usize),
    Cancel,
    Submit,
    Reveal,
    Hide,
    Delete,
    Grant(usize),
    Revoke(usize),
    ChangePassword,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Main,
    Initialize,
    Unlock,
    New,
    Reveal,
    Delete,
    Grant(usize),
    Revoke(usize),
    ChangePassword,
}
struct Snapshot {
    status: Status,
    items: Vec<Item>,
    apps: Vec<Application>,
    revealed: Option<Secret>,
}
enum Job {
    Refresh,
    Lock,
    Initialize(Secret),
    Unlock(Secret),
    Put(String, String, String, Secret),
    Reveal(ItemId, Secret),
    Delete(ItemId, Secret),
    Grant(ItemId, Application, Secret, bool),
    ChangePassword(Secret, Secret),
}
fn execute(job: Job) -> Result<Snapshot, Error> {
    let k = Keychain::connect()?;
    let manager = k.manager();
    let mut revealed = None;
    match job {
        Job::Refresh => {}
        Job::Lock => manager.lock()?,
        Job::Initialize(p) => manager.initialize(p)?,
        Job::Unlock(p) => manager.unlock(p)?,
        Job::Put(service, account, label, secret) => {
            k.put(&service, &account, &label, secret)?;
        }
        Job::Reveal(id, p) => revealed = Some(manager.reveal(id, p)?),
        Job::Delete(id, p) => manager.delete(id, p)?,
        Job::Grant(id, a, p, revoke) => {
            if revoke {
                manager.revoke(id, a, p)?;
            } else {
                manager.grant(id, a, p)?;
            }
        }
        Job::ChangePassword(old, new) => manager.change_password(old, new)?,
    }
    let status = k.status()?;
    let (items, apps) = if status.unlocked {
        (manager.items()?, manager.applications()?)
    } else {
        (vec![], vec![])
    };
    Ok(Snapshot {
        status,
        items,
        apps,
        revealed,
    })
}
struct Field {
    state: TextFieldInteractionState,
    view: TextField,
}
impl Field {
    fn new(placeholder: &str, secure: bool, pending: Rc<Cell<Option<Action>>>) -> Self {
        let state = TextFieldInteractionState::new();
        let view = TextField::with_interaction(state.clone())
            .placeholder(placeholder)
            .secure(secure)
            .on_submit(move || pending.set(Some(Action::Submit)));
        Self { state, view }
    }
    fn secret(&self) -> Secret {
        let mut value = self.state.value();
        let secret = Secret::new(value.as_bytes().to_vec());
        use_wipe(&mut value);
        self.state.clear();
        secret
    }
}
fn use_wipe(value: &mut String) {
    // Minimize the UI-owned temporary copy before it is released.
    use appcore::keychain::Secret;
    let bytes = std::mem::take(value).into_bytes();
    drop(Secret::new(bytes));
}
struct Model {
    status: Option<Status>,
    items: Vec<Item>,
    apps: Vec<Application>,
    selected: Option<ItemId>,
    form: Form,
    error: Option<Error>,
    revealed: Option<Secret>,
    hide_at: Option<Instant>,
    last_refresh: Instant,
}
struct Core {
    model: RefCell<Model>,
    pending: Rc<Cell<Option<Action>>>,
    buttons: RefCell<BTreeMap<Action, ButtonInteractionState>>,
    worker: RefCell<Option<Receiver<Result<Snapshot, Error>>>>,
    focused: Cell<bool>,
    exposure_generation: Cell<u64>,
    worker_generation: Cell<u64>,
    query: Field,
    password: Field,
    confirm: Field,
    service: Field,
    account: Field,
    label: Field,
    value: Field,
    scroll: ScrollState,
    detail_scroll: ScrollState,
    grant_picker: RefCell<Picker>,
    form_scroll: ScrollState,
}
#[derive(Clone)]
pub struct KeychainView {
    core: Rc<Core>,
}
fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w.max(0.0), h.max(0.0))
}
fn fill(b: Rect, color: Color, c: &mut PaintContext<'_>) {
    Rectangle::new()
        .color(RectangleColor::Custom(color))
        .paint(b, c);
}
impl Core {
    fn button(&self, label: &str, a: Action) -> Button {
        let state = self.buttons.borrow_mut().entry(a).or_default().clone();
        let pending = self.pending.clone();
        Button::with_interaction_and_label(state, label)
            .size(ButtonSize::Small)
            .on_click(move || pending.set(Some(a)))
    }
    fn start(&self, job: Job) {
        if self.worker.borrow().is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        *self.worker.borrow_mut() = Some(rx);
        self.worker_generation.set(self.exposure_generation.get());
        let mut m = self.model.borrow_mut();
        m.error = None;
        m.revealed = None;
        m.hide_at = None;
        drop(m);
        std::thread::spawn(move || {
            let _ = tx.send(execute(job));
        });
    }
    fn poll(&self) {
        let result = self.worker.borrow().as_ref().map(|rx| rx.try_recv());
        match result {
            Some(Ok(reply)) => {
                self.worker.borrow_mut().take();
                let mut m = self.model.borrow_mut();
                match reply {
                    Ok(mut s) => {
                        if !self.focused.get()
                            || self.worker_generation.get() != self.exposure_generation.get()
                        {
                            // A completion queued before blur must not reveal a
                            // secret, even if focus has returned in the meantime.
                            s.revealed = None;
                        }
                        m.status = Some(s.status);
                        m.items = s.items;
                        m.apps = s.apps;
                        if m.selected
                            .is_none_or(|id| !m.items.iter().any(|i| i.id == id))
                        {
                            m.selected = m.items.first().map(|i| i.id);
                        }
                        m.form = if !s.status.initialized {
                            Form::Initialize
                        } else if !s.status.unlocked {
                            Form::Unlock
                        } else {
                            Form::Main
                        };
                        m.hide_at = s
                            .revealed
                            .as_ref()
                            .map(|_| Instant::now() + Duration::from_secs(10));
                        m.revealed = s.revealed;
                    }
                    Err(e) => {
                        m.error = Some(e);
                        if matches!(
                            e,
                            Error::Locked
                                | Error::Denied
                                | Error::Unavailable
                                | Error::Storage
                                | Error::Corrupt
                        ) {
                            m.items.clear();
                            m.revealed = None;
                            if matches!(e, Error::Storage | Error::Corrupt | Error::Locked) {
                                m.status = Some(Status {
                                    initialized: true,
                                    unlocked: false,
                                });
                                m.form = Form::Unlock;
                            } else {
                                m.status = None;
                            }
                        }
                    }
                }
                m.last_refresh = Instant::now();
                drop(m);
                self.refresh_picker();
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.worker.borrow_mut().take();
                self.model.borrow_mut().error = Some(Error::Unavailable);
            }
            _ => {}
        }
    }
    fn refresh_picker(&self) {
        let m = self.model.borrow();
        let item = m
            .selected
            .and_then(|id| m.items.iter().find(|i| i.id == id));
        let mut picker = Picker::new("アプリに許可…");
        if let Some(item) = item {
            for (n, a) in m.apps.iter().enumerate() {
                if *a == item.owner || item.grants.contains(a) {
                    continue;
                }
                let pending = self.pending.clone();
                picker = picker.option(format!("{} [{}]", a.package, signer(a)), move || {
                    pending.set(Some(Action::Grant(n)))
                });
            }
        }
        *self.grant_picker.borrow_mut() = picker;
    }
    fn clear_fields(&self) {
        for f in [
            &self.password,
            &self.confirm,
            &self.service,
            &self.account,
            &self.label,
            &self.value,
        ] {
            f.state.clear();
        }
    }
    fn action(&self, a: Action) {
        if self.worker.borrow().is_some() {
            return;
        }
        match a {
            Action::Refresh => self.start(Job::Refresh),
            Action::Lock => self.start(Job::Lock),
            Action::Hide => {
                let mut m = self.model.borrow_mut();
                m.revealed = None;
                m.hide_at = None;
            }
            Action::Select(n) => {
                let mut m = self.model.borrow_mut();
                if let Some(i) = m.items.get(n) {
                    m.selected = Some(i.id);
                }
                m.revealed = None;
                m.hide_at = None;
                drop(m);
                self.detail_scroll.reset();
                self.refresh_picker();
            }
            Action::Cancel => {
                self.clear_fields();
                self.form_scroll.reset();
                let mut m = self.model.borrow_mut();
                m.form = if m.status.is_some_and(|s| s.unlocked) {
                    Form::Main
                } else if m.status.is_some_and(|s| s.initialized) {
                    Form::Unlock
                } else {
                    Form::Initialize
                };
                m.error = None;
                m.revealed = None;
                m.hide_at = None;
            }
            Action::New
            | Action::Reveal
            | Action::Delete
            | Action::Grant(_)
            | Action::Revoke(_)
            | Action::ChangePassword => {
                self.clear_fields();
                self.form_scroll.reset();
                let mut m = self.model.borrow_mut();
                m.revealed = None;
                m.hide_at = None;
                m.form = match a {
                    Action::New => Form::New,
                    Action::Reveal => Form::Reveal,
                    Action::Delete => Form::Delete,
                    Action::Grant(n) => Form::Grant(n),
                    Action::Revoke(n) => Form::Revoke(n),
                    _ => Form::ChangePassword,
                };
                m.error = None;
            }
            Action::Submit => {
                let m = self.model.borrow();
                let form = m.form;
                let id = m.selected;
                let target = match form {
                    Form::Grant(n) => m.apps.get(n).cloned(),
                    Form::Revoke(n) => id
                        .and_then(|id| m.items.iter().find(|i| i.id == id))
                        .and_then(|i| i.grants.get(n))
                        .cloned(),
                    _ => None,
                };
                drop(m);
                if matches!(form, Form::Initialize | Form::ChangePassword) {
                    let first = if form == Form::Initialize {
                        self.password.state.value()
                    } else {
                        self.value.state.value()
                    };
                    let mut check = self.confirm.state.value();
                    let equal = first == check;
                    let mut first = first;
                    use_wipe(&mut first);
                    use_wipe(&mut check);
                    if !equal {
                        self.model.borrow_mut().error = Some(Error::Invalid);
                        return;
                    }
                }
                let job = match form {
                    Form::Initialize => Job::Initialize(self.password.secret()),
                    Form::Unlock => Job::Unlock(self.password.secret()),
                    Form::New => Job::Put(
                        self.service.state.value(),
                        self.account.state.value(),
                        self.label.state.value(),
                        self.value.secret(),
                    ),
                    Form::Reveal => {
                        let Some(id) = id else { return };
                        Job::Reveal(id, self.password.secret())
                    }
                    Form::Delete => {
                        let Some(id) = id else { return };
                        Job::Delete(id, self.password.secret())
                    }
                    Form::Grant(_) | Form::Revoke(_) => {
                        let (Some(id), Some(target)) = (id, target) else {
                            return;
                        };
                        Job::Grant(
                            id,
                            target,
                            self.password.secret(),
                            matches!(form, Form::Revoke(_)),
                        )
                    }
                    Form::ChangePassword => {
                        Job::ChangePassword(self.password.secret(), self.value.secret())
                    }
                    Form::Main => return,
                };
                self.clear_fields();
                self.start(job);
            }
        }
    }
}
impl KeychainView {
    pub fn new() -> Self {
        let pending = Rc::new(Cell::new(None));
        let core = Rc::new(Core {
            model: RefCell::new(Model {
                status: None,
                items: vec![],
                apps: vec![],
                selected: None,
                form: Form::Main,
                error: None,
                revealed: None,
                hide_at: None,
                last_refresh: Instant::now(),
            }),
            pending: pending.clone(),
            buttons: RefCell::new(BTreeMap::new()),
            worker: RefCell::new(None),
            focused: Cell::new(true),
            exposure_generation: Cell::new(0),
            worker_generation: Cell::new(0),
            query: Field::new("ラベル・サービス・アカウントを検索", false, pending.clone()),
            password: Field::new("保管庫のパスワード", true, pending.clone()),
            confirm: Field::new("もう一度入力", true, pending.clone()),
            service: Field::new("サービス名", false, pending.clone()),
            account: Field::new("アカウント名", false, pending.clone()),
            label: Field::new("表示名", false, pending.clone()),
            value: Field::new("秘密情報", true, pending.clone()),
            scroll: ScrollState::new(),
            detail_scroll: ScrollState::new(),
            grant_picker: RefCell::new(Picker::new("アプリに許可…")),
            form_scroll: ScrollState::new(),
        });
        core.start(Job::Refresh);
        Self { core }
    }
    fn sidebar(b: Rect) -> Rect {
        r(
            b.origin.x,
            b.origin.y + 54.0,
            (b.size.width * 0.38).clamp(220.0, 340.0).min(b.size.width),
            b.size.height - 54.0,
        )
    }
    fn form_frames(&self, b: Rect) -> (Rect, Vec<(&Field, &'static str, Rect)>, Rect, Rect) {
        let form = self.core.model.borrow().form;
        let w = (b.size.width - 48.0).clamp(0.0, 440.0);
        let x = b.origin.x + (b.size.width - w) / 2.0;
        let y = b.origin.y + 64.0_f32.min((b.size.height * 0.15).max(8.0));
        let mut fields = vec![];
        let intro = if matches!(form, Form::Grant(_) | Form::Revoke(_)) {
            240.0
        } else {
            160.0
        };
        let specs: Vec<(&Field, &str)> = match form {
            Form::New => vec![
                (&self.core.label, "表示名"),
                (&self.core.service, "サービス"),
                (&self.core.account, "アカウント"),
                (&self.core.value, "秘密情報"),
            ],
            Form::Initialize => vec![
                (&self.core.password, "パスワード（12文字以上）"),
                (&self.core.confirm, "確認"),
            ],
            Form::ChangePassword => vec![
                (&self.core.password, "現在のパスワード"),
                (&self.core.value, "新しいパスワード（12文字以上）"),
                (&self.core.confirm, "確認"),
            ],
            _ => vec![(&self.core.password, "保管庫のパスワード")],
        };
        for (i, (f, label)) in specs.into_iter().enumerate() {
            fields.push((f, label, r(x, y + intro + i as f32 * 66.0, w, 30.0)));
        }
        let bottom = fields
            .last()
            .map(|(_, _, r)| r.origin.y + 48.0)
            .unwrap_or(y + 116.0);
        (
            r(x, y, w, intro - 30.0),
            fields,
            r(x + w - 100.0, bottom, 100.0, 30.0),
            r(x, bottom, 80.0, 30.0),
        )
    }
    fn paint_form(&self, b: Rect, c: &mut PaintContext<'_>) {
        let m = self.core.model.borrow();
        let (title, description, submit) = match m.form {
            Form::Initialize => (
                "保管庫を作成",
                "この端末だけに暗号化して保存します。パスワードを忘れると復元できません。ログインとは別のパスワードです。",
                "作成",
            ),
            Form::Unlock => (
                "保管庫を解錠",
                "アプリに許可した秘密情報を利用できるようにします。",
                "解錠",
            ),
            Form::New => (
                "項目を追加",
                "この項目の所有者はKeychainです。ほかのアプリには明示的に許可できます。",
                "保存",
            ),
            Form::Reveal => (
                "秘密情報を表示",
                "表示には保管庫のパスワードが必要です。10秒後に自動で隠れます。",
                "表示",
            ),
            Form::Delete => (
                "項目を削除",
                "この項目を保管庫から削除します。元には戻せません。",
                "削除",
            ),
            Form::Grant(_) => (
                "アプリへの利用を許可",
                "選択した署名のアプリに、この項目の取得を許可します。",
                "許可",
            ),
            Form::Revoke(_) => (
                "アプリの許可を取り消す",
                "取り消し後の取得を拒否します。すでにアプリへ渡した値は回収できません。",
                "取り消す",
            ),
            Form::ChangePassword => (
                "パスワードを変更",
                "保管庫を新しいパスワードで暗号化し直します。",
                "変更",
            ),
            Form::Main => return,
        };
        let target = match m.form {
            Form::Grant(n) => m.apps.get(n),
            Form::Revoke(n) => m
                .selected
                .and_then(|id| m.items.iter().find(|i| i.id == id))
                .and_then(|i| i.grants.get(n)),
            _ => None,
        };
        let description = if let Some(a) = target {
            format!(
                "{}\n許可先：{}\n署名：{}",
                description,
                a.package,
                a.signer
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            )
        } else {
            description.to_owned()
        };
        let (head, fields, submit_frame, cancel) = self.form_frames(b);
        Text::styled(title, TextRole::TitleSmall)
            .paint(r(head.origin.x, head.origin.y, head.size.width, 32.0), c);
        Text::body(description)
            .color(c.theme.colors.text_secondary)
            .paint(
                r(
                    head.origin.x,
                    head.origin.y + 42.0,
                    head.size.width,
                    head.size.height - 42.0,
                ),
                c,
            );
        for (f, label, frame) in fields {
            Text::caption(label)
                .color(c.theme.colors.text_secondary)
                .paint(
                    r(
                        frame.origin.x,
                        frame.origin.y - 22.0,
                        frame.size.width,
                        20.0,
                    ),
                    c,
                );
            f.view.paint(frame, c);
        }
        let busy = self.core.worker.borrow().is_some();
        self.core
            .button(if busy { "処理中…" } else { submit }, Action::Submit)
            .style(if m.form == Form::Delete {
                ButtonStyle::Danger
            } else {
                ButtonStyle::Accent
            })
            .enabled(!busy)
            .paint(submit_frame, c);
        if !matches!(m.form, Form::Initialize | Form::Unlock) {
            self.core
                .button("キャンセル", Action::Cancel)
                .style(ButtonStyle::Ghost)
                .enabled(!busy)
                .paint(cancel, c);
        }
    }
}
impl View for KeychainView {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        c.constrain(c.maximum)
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        self.core.poll();
        let now = Instant::now();
        {
            let mut m = self.core.model.borrow_mut();
            if m.hide_at.is_some_and(|t| now >= t) {
                m.revealed = None;
                m.hide_at = None;
            }
            if let Some(t) = m.hide_at {
                c.request_redraw_at(t);
            }
        }
        if self.core.worker.borrow().is_some() {
            c.request_redraw_at(now + Duration::from_millis(32));
        }
        fill(b, c.theme.colors.background, c);
        let m = self.core.model.borrow();
        let form = m.form;
        let status = m.status;
        let error = m.error;
        drop(m);
        if status.is_none() {
            Text::styled("Keychain", TextRole::TitleLarge).paint(
                r(
                    b.origin.x + 32.0,
                    b.origin.y + 40.0,
                    b.size.width - 64.0,
                    40.0,
                ),
                c,
            );
            Text::body(if self.core.worker.borrow().is_some() {
                "保管庫を確認しています…"
            } else {
                "キーチェーンサービスに接続できません。"
            })
            .color(c.theme.colors.text_secondary)
            .paint(
                r(
                    b.origin.x + 32.0,
                    b.origin.y + 96.0,
                    b.size.width - 64.0,
                    60.0,
                ),
                c,
            );
            self.core
                .button("再試行", Action::Refresh)
                .enabled(self.core.worker.borrow().is_none())
                .paint(r(b.origin.x + 32.0, b.origin.y + 174.0, 100.0, 30.0), c);
        } else if form != Form::Main {
            Scroll::new(self.core.form_scroll.clone())
                .content(FormContents(self.core.clone()))
                .paint(
                    r(b.origin.x, b.origin.y, b.size.width, b.size.height - 36.0),
                    c,
                );
        } else {
            let side = Self::sidebar(b);
            self.core.query.view.paint(
                r(
                    b.origin.x + 12.0,
                    b.origin.y + 12.0,
                    (side.size.width - 24.0).max(0.0),
                    30.0,
                ),
                c,
            );
            for (label, a, x, w) in toolbar_buttons(b, side) {
                self.core
                    .button(label, a)
                    .enabled(self.core.worker.borrow().is_none())
                    .paint(r(b.origin.x + x, b.origin.y + 12.0, w, 30.0), c);
            }
            Scroll::new(self.core.scroll.clone())
                .content(ItemList(self.core.clone()))
                .paint(side, c);
            fill(
                r(
                    side.origin.x + side.size.width,
                    side.origin.y,
                    1.0,
                    side.size.height,
                ),
                c.theme.colors.border,
                c,
            );
            let detail = r(
                side.origin.x + side.size.width + 1.0,
                b.origin.y + 54.0,
                b.size.width - side.size.width - 1.0,
                b.size.height - 82.0,
            );
            Scroll::new(self.core.detail_scroll.clone())
                .content(Details(self.core.clone()))
                .paint(detail, c);
        }
        if let Some(e) = error {
            Text::caption(error_text(e))
                .color(c.theme.colors.destructive)
                .paint(
                    r(
                        b.origin.x + 16.0,
                        b.origin.y + b.size.height - 28.0,
                        b.size.width - 32.0,
                        24.0,
                    ),
                    c,
                );
        }
    }
    fn handle_event(&self, b: Rect, event: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        if let ViewEvent::FocusChanged { focused } = event {
            self.core.focused.set(*focused);
        }
        if matches!(event, ViewEvent::FocusChanged { focused: false }) {
            self.core
                .exposure_generation
                .set(self.core.exposure_generation.get().wrapping_add(1));
            for field in [&self.core.password, &self.core.confirm, &self.core.value] {
                field.state.clear();
            }
            let mut m = self.core.model.borrow_mut();
            m.revealed = None;
            m.hide_at = None;
            c.request_redraw_in(b);
        }
        if self.core.worker.borrow().is_some() {
            return EventResult::Ignored;
        }

        if let ViewEvent::KeyPressed { key, .. } = event {
            if *key == Key::Escape {
                self.core.action(Action::Cancel);
                c.request_redraw_in(b);
                return EventResult::Consumed;
            }
        }
        let m = self.core.model.borrow();
        let form = m.form;
        let status = m.status;
        drop(m);
        let mut result = EventResult::Ignored;
        if status.is_none() {
            result = result.merge(self.core.button("再試行", Action::Refresh).handle_event(
                r(b.origin.x + 32.0, b.origin.y + 174.0, 100.0, 30.0),
                event,
                c,
            ));
        } else if form != Form::Main {
            result = result.merge(
                Scroll::new(self.core.form_scroll.clone())
                    .content(FormContents(self.core.clone()))
                    .handle_event(
                        r(b.origin.x, b.origin.y, b.size.width, b.size.height - 36.0),
                        event,
                        c,
                    ),
            );
        } else {
            let side = Self::sidebar(b);
            result = result.merge(self.core.query.view.handle_event(
                r(
                    b.origin.x + 12.0,
                    b.origin.y + 12.0,
                    side.size.width - 24.0,
                    30.0,
                ),
                event,
                c,
            ));
            for (label, a, x, w) in toolbar_buttons(b, side) {
                result = result.merge(
                    self.core
                        .button(label, a)
                        .enabled(self.core.worker.borrow().is_none())
                        .handle_event(r(b.origin.x + x, b.origin.y + 12.0, w, 30.0), event, c),
                );
            }
            result = result.merge(
                Scroll::new(self.core.scroll.clone())
                    .content(ItemList(self.core.clone()))
                    .handle_event(side, event, c),
            );
            let detail = r(
                side.origin.x + side.size.width + 1.0,
                b.origin.y + 54.0,
                b.size.width - side.size.width - 1.0,
                b.size.height - 82.0,
            );
            result = result.merge(
                Scroll::new(self.core.detail_scroll.clone())
                    .content(Details(self.core.clone()))
                    .handle_event(detail, event, c),
            );
        }
        if let Some(a) = self.core.pending.take() {
            self.core.action(a);
            c.request_redraw_in(b);
            result = EventResult::Consumed;
        }
        result
    }
}
struct FormContents(Rc<Core>);
impl View for FormContents {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        let form = self.0.model.borrow().form;
        let height = match form {
            Form::New => 520.0,
            Form::ChangePassword => 455.0,
            Form::Grant(_) | Form::Revoke(_) => 450.0,
            _ => 390.0,
        };
        c.constrain(Size::new(c.maximum.width, height))
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        KeychainView {
            core: self.0.clone(),
        }
        .paint_form(b, c)
    }
    fn handle_event(&self, b: Rect, event: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        let view = KeychainView {
            core: self.0.clone(),
        };
        let form = self.0.model.borrow().form;
        let mut result = EventResult::Ignored;
        let (_, fields, submit, cancel) = view.form_frames(b);
        for (f, _, frame) in fields {
            result = result.merge(f.view.handle_event(frame, event, c));
        }
        result = result.merge(
            self.0
                .button("実行", Action::Submit)
                .enabled(self.0.worker.borrow().is_none())
                .handle_event(submit, event, c),
        );
        if !matches!(form, Form::Initialize | Form::Unlock) {
            result = result.merge(
                self.0
                    .button("キャンセル", Action::Cancel)
                    .handle_event(cancel, event, c),
            );
        }
        result
    }
}
struct Details(Rc<Core>);
impl Details {
    fn content(&self) -> Padding<VStack> {
        let m = self.0.model.borrow();
        let mut stack = VStack::new()
            .alignment(StackAlignment::Stretch)
            .gap(StackGap::Medium);
        let Some(item) = m
            .selected
            .and_then(|id| m.items.iter().find(|i| i.id == id))
        else {
            return Padding::all(24.0)
                .content(stack.child(Text::body("項目を追加すると、ここに詳細を表示します。")));
        };
        stack = stack.child(Text::styled(&item.label, TextRole::TitleSmall));
        for (label, value) in [
            ("サービス", item.service.as_str()),
            ("アカウント", item.account.as_str()),
            ("所有者", item.owner.package.as_str()),
        ] {
            stack = stack.child(
                VStack::new()
                    .alignment(StackAlignment::Stretch)
                    .gap(StackGap::ExtraSmall)
                    .child(Text::caption(label))
                    .child(Text::body(value)),
            );
        }
        stack = stack
            .child(Text::caption(format!(
                "署名 {} · {}",
                item.owner
                    .signer
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                origin(item.owner.provenance)
            )))
            .child(
                Text::body(
                    m.revealed
                        .as_ref()
                        .map(|s| {
                            std::str::from_utf8(s.expose())
                                .map(str::to_owned)
                                .unwrap_or_else(|_| {
                                    format!("バイナリの秘密情報（{}バイト）", s.expose().len())
                                })
                        })
                        .unwrap_or_else(|| "••••••••••••".into()),
                )
                .cache_layout(false),
            )
            .child(
                HStack::new()
                    .gap(StackGap::Small)
                    .child(self.0.button(
                        if m.revealed.is_some() {
                            "隠す"
                        } else {
                            "表示"
                        },
                        if m.revealed.is_some() {
                            Action::Hide
                        } else {
                            Action::Reveal
                        },
                    ))
                    .child(
                        self.0
                            .button("削除", Action::Delete)
                            .style(ButtonStyle::Danger),
                    ),
            )
            .child(Divider::new())
            .child(Text::caption("利用を許可したアプリ"));
        for (n, a) in item.grants.iter().enumerate() {
            stack = stack.child(
                self.0
                    .button(
                        &format!("{} [{}] の許可を取り消す", a.package, signer(a)),
                        Action::Revoke(n),
                    )
                    .style(ButtonStyle::Ghost),
            );
        }
        stack = stack.child(CachedPicker(self.0.clone()));
        Padding::all(24.0).content(stack)
    }
}
impl View for Details {
    fn measure(&self, c: Constraints, ctx: &mut MeasureContext<'_>) -> Size {
        self.content().measure(c, ctx)
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        self.content().paint(b, c);
    }
    fn handle_event(&self, b: Rect, e: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        self.content().handle_event(b, e, c)
    }
}
struct CachedPicker(Rc<Core>);
impl View for CachedPicker {
    fn measure(&self, c: Constraints, ctx: &mut MeasureContext<'_>) -> Size {
        self.0.grant_picker.borrow().measure(c, ctx)
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        self.0.grant_picker.borrow().paint(b, c)
    }
    fn handle_event(&self, b: Rect, e: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        self.0.grant_picker.borrow().handle_event(b, e, c)
    }
}

fn toolbar_buttons(b: Rect, side: Rect) -> Vec<(&'static str, Action, f32, f32)> {
    let mut buttons = vec![
        ("追加", Action::New, side.size.width + 12.0, 64.0),
        ("施錠", Action::Lock, b.size.width - 148.0, 64.0),
        ("変更", Action::ChangePassword, b.size.width - 72.0, 60.0),
    ];
    if b.size.width - side.size.width > 340.0 {
        buttons.push(("更新", Action::Refresh, b.size.width - 224.0, 64.0));
    }
    buttons
}
fn signer(a: &Application) -> String {
    a.signer[..6].iter().map(|b| format!("{b:02x}")).collect()
}
fn origin(value: u8) -> &'static str {
    match value {
        1 => "組み込みアプリ",
        2 => "署名済みアプリ",
        _ => "開発用アプリ",
    }
}
fn error_text(e: Error) -> &'static str {
    match e {
        Error::Authentication => "パスワードが違うか、保管庫を認証できません。",
        Error::Invalid => {
            "入力内容を確認してください。新しいパスワードは12文字以上で、確認欄と一致させてください。"
        }
        Error::Locked => "保管庫は施錠されています。",
        Error::Denied => "この操作は許可されていません。",
        Error::Unavailable => "サービスに接続できません。",
        Error::Storage => "保存に失敗しました。保管庫を解錠し直してください。",
        _ => "処理を完了できません。更新して状態を確認してください。",
    }
}
struct ItemList(Rc<Core>);
impl View for ItemList {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        c.constrain(Size::new(
            c.maximum.width,
            self.0.model.borrow().items.len() as f32 * 68.0,
        ))
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        let m = self.0.model.borrow();
        let q = self.0.query.state.value().to_lowercase();
        let mut visual = 0;
        for (n, i) in m.items.iter().enumerate() {
            if !format!("{} {} {}", i.label, i.service, i.account)
                .to_lowercase()
                .contains(&q)
            {
                continue;
            }
            let selected = m.selected == Some(i.id);
            self.0
                .button(&i.label, Action::Select(n))
                .style(ButtonStyle::Custom {
                    background: if selected {
                        c.theme.list.selected_background
                    } else {
                        c.theme.list.background
                    },
                    hovered_background: c.theme.list.hovered_background,
                    border: Color::TRANSPARENT,
                    hovered_border: Color::TRANSPARENT,
                    foreground: c.theme.list.foreground,
                })
                .radius(CornerRadius::None)
                .content(
                    VStack::new()
                        .alignment(StackAlignment::Start)
                        .gap(StackGap::Small)
                        .child(Text::label(&i.label))
                        .child(Text::caption(&i.account).color(c.theme.colors.text_secondary)),
                )
                .paint(
                    r(
                        b.origin.x,
                        b.origin.y + visual as f32 * 68.0,
                        b.size.width,
                        68.0,
                    ),
                    c,
                );
            visual += 1;
        }
    }
    fn handle_event(&self, b: Rect, event: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        let m = self.0.model.borrow();
        let q = self.0.query.state.value().to_lowercase();
        let mut visual = 0;
        let mut result = EventResult::Ignored;
        for (n, i) in m.items.iter().enumerate() {
            if !format!("{} {} {}", i.label, i.service, i.account)
                .to_lowercase()
                .contains(&q)
            {
                continue;
            }
            result = result.merge(self.0.button(&i.label, Action::Select(n)).handle_event(
                r(
                    b.origin.x,
                    b.origin.y + visual as f32 * 68.0,
                    b.size.width,
                    68.0,
                ),
                event,
                c,
            ));
            visual += 1;
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use appcore::viewkit::{
        draw_command::DisplayList, event::ContextMenuRequest, platform::PointerButton,
        typography::TextMeasurer,
    };
    fn application(name: &str, n: u8) -> Application {
        Application {
            package: name.into(),
            signer: [n; 32],
            provenance: 1,
        }
    }
    fn fixture() -> KeychainView {
        let v = KeychainView::new();
        v.core.worker.borrow_mut().take();
        let owner = application("org.mochios.mail", 1);
        let other = application("org.mochios.terra", 2);
        let item = Item {
            id: [1; 16],
            owner: owner.clone(),
            service: "google.oauth".into(),
            account: "person@example.test".into(),
            label: "Gmail".into(),
            grants: vec![],
        };
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(Snapshot {
            status: Status {
                initialized: true,
                unlocked: true,
            },
            items: vec![item],
            apps: vec![owner, other],
            revealed: None,
        }))
        .unwrap();
        *v.core.worker.borrow_mut() = Some(rx);
        v.core.poll();
        v
    }
    fn bounds() -> Rect {
        r(0.0, 0.0, 880.0, 620.0)
    }
    fn paint(v: &KeychainView) -> Vec<AccessibilityNode> {
        let theme = Theme::current();
        let mut text = TextMeasurer::new();
        let mut list = DisplayList::new();
        let mut nodes = vec![];
        v.paint(
            bounds(),
            &mut PaintContext::new(&mut list, &theme, &theme.typography, &mut text)
                .with_accessibility_nodes(&mut nodes),
        );
        nodes
    }
    fn event(v: &KeychainView, e: ViewEvent) -> Option<ContextMenuRequest> {
        let theme = Theme::current();
        let mut text = TextMeasurer::new();
        let mut c = EventContext::new(&theme, &theme.typography, &mut text);
        v.handle_event(bounds(), &e, &mut c);
        c.take_context_menu_request()
    }
    #[test]
    fn native_grant_picker_survives_paints_and_requires_reauthentication() {
        let v = fixture();
        let nodes = paint(&v);
        let node = nodes
            .iter()
            .find(|n| n.label.as_deref() == Some("アプリに許可…"))
            .expect("grant picker");
        let point = Point::new(
            node.bounds.origin.x + node.bounds.size.width / 2.0,
            node.bounds.origin.y + node.bounds.size.height / 2.0,
        );
        event(
            &v,
            ViewEvent::PointerPressed {
                position: point,
                button: PointerButton::Primary,
            },
        );
        let menu = event(
            &v,
            ViewEvent::PointerReleased {
                position: point,
                button: PointerButton::Primary,
            },
        )
        .unwrap();
        paint(&v);
        event(
            &v,
            ViewEvent::ContextMenuResult {
                request_id: menu.request_id,
                command_id: Some(1),
            },
        );
        assert!(matches!(v.core.model.borrow().form, Form::Grant(1)));
        assert!(v.core.worker.borrow().is_none());
        assert!(v.core.password.state.value().is_empty());
        paint(&v);
    }
    #[test]
    fn secret_is_hidden_on_expiry_focus_loss_and_cancel() {
        let v = fixture();
        {
            let mut m = v.core.model.borrow_mut();
            m.revealed = Some(Secret::new(b"display-once".to_vec()));
            m.hide_at = Some(Instant::now() - Duration::from_secs(1));
        }
        paint(&v);
        assert!(v.core.model.borrow().revealed.is_none());
        v.core.model.borrow_mut().revealed = Some(Secret::new(b"display-once".to_vec()));
        event(&v, ViewEvent::FocusChanged { focused: false });
        assert!(v.core.model.borrow().revealed.is_none());
        v.core.password.state.set_value("secret input");
        v.core.action(Action::Cancel);
        assert!(v.core.password.state.value().is_empty());
    }
    #[test]
    fn late_reveal_cannot_restore_secret_after_losing_focus() {
        let v = fixture();
        let (tx, rx) = mpsc::channel();
        *v.core.worker.borrow_mut() = Some(rx);
        v.core.password.state.set_value("private input");
        event(&v, ViewEvent::FocusChanged { focused: false });
        event(&v, ViewEvent::FocusChanged { focused: true });
        tx.send(Ok(Snapshot {
            status: Status {
                initialized: true,
                unlocked: true,
            },
            items: v.core.model.borrow().items.clone(),
            apps: vec![],
            revealed: Some(Secret::new(b"late secret".to_vec())),
        }))
        .unwrap();
        v.core.poll();
        assert!(v.core.model.borrow().revealed.is_none());
        assert!(v.core.password.state.value().is_empty());
    }
    #[test]
    fn storage_failure_removes_stale_unlocked_state() {
        let v = fixture();
        let (tx, rx) = mpsc::channel();
        tx.send(Err(Error::Storage)).unwrap();
        *v.core.worker.borrow_mut() = Some(rx);
        v.core.poll();
        let m = v.core.model.borrow();
        assert!(m.items.is_empty());
        assert!(!m.status.unwrap().unlocked);
        assert!(m.form == Form::Unlock);
    }
    #[test]
    fn toolbar_controls_do_not_overlap_at_supported_widths() {
        for width in [480.0, 640.0, 880.0, 1200.0] {
            let b = r(0.0, 0.0, width, 620.0);
            let side = KeychainView::sidebar(b);
            let buttons = toolbar_buttons(b, side);
            for (i, (_, _, x, w)) in buttons.iter().enumerate() {
                assert!(*x >= side.size.width && x + w <= width);
                for (_, _, other, ow) in buttons.iter().skip(i + 1) {
                    assert!(x + w <= *other || other + ow <= *x);
                }
            }
        }
    }
}
