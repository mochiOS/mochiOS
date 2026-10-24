use crate::account_ui::{AccountForm, AccountPage, Provider};
use crate::backend::{self, Account, Error, Result};
use crate::layout::{Layout, Width};
use crate::model::{Folder, Mailbox, Model, Page};
use crate::protocol::ImapFlag;
use appcore::prelude::*;
use appcore::viewkit::draw_command::DrawCommand;
use appcore::viewkit::event::{
    ContextMenuItem, ContextMenuRequest, EventContext, EventResult, ViewEvent,
};
use appcore::viewkit::platform::{CursorIcon, Key, PointerButton};
use appcore::viewkit::view::{Constraints, MeasureContext, PaintContext};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Accounts,
    CloseAccounts,
    AddAccount,
    RemoveAccount(usize),
    ManageAccount(usize),
    ConfirmRemove(usize),
    ChooseProvider,
    AccountBack,
    Advanced,
    IncomingSecurityValue(backend::Security),
    SmtpSecurityValue(backend::Security),
    Preset(u8),
    Refresh,
    Send,
    Sender,
    Folders,
    CloseFolders,
    Box(Mailbox),
    Open(usize),
    Back,
    New,
    SaveDraft,
    ResumeDraft(usize),
    Reply,
    Forward,
    Archive,
    Delete,
    Flag,
    Unread,
    Expand(usize, usize),
    Details,
}
#[derive(Clone, Copy)]
enum Split {
    Sidebar,
    List,
}
struct Draft {
    to: TextFieldInteractionState,
    subject: TextFieldInteractionState,
    body: TextEditorInteractionState,
    to_field: TextField,
    subject_field: TextField,
    editor: TextEditor,
}
impl Draft {
    fn new() -> Self {
        let to = TextFieldInteractionState::new();
        let subject = TextFieldInteractionState::new();
        let body = TextEditorInteractionState::new();
        let to_field = TextField::with_interaction(to.clone())
            .bordered(false)
            .placeholder("Email address");
        let subject_field = TextField::with_interaction(subject.clone())
            .bordered(false)
            .placeholder("Subject");
        let editor = TextEditor::with_interaction(body.clone())
            .bordered(false)
            .placeholder("Write your message");
        Self {
            to,
            subject,
            body,
            to_field,
            subject_field,
            editor,
        }
    }
}
#[derive(Clone)]
struct DraftRecord {
    sender: usize,
    to: String,
    subject: String,
    body: String,
}
struct Core {
    model: RefCell<Model>,
    preview: Cell<bool>,
    form: AccountForm,
    worker: RefCell<Option<mpsc::Receiver<Result<Reply>>>>,
    sending: Cell<bool>,
    active_flag: RefCell<Option<FlagChange>>,
    flag_queue: RefCell<VecDeque<FlagChange>>,
    flag_error: Cell<Option<Error>>,
    status: RefCell<String>,
    accounts_loaded: Cell<bool>,
    sender: Cell<usize>,
    sender_menu: Cell<u64>,
    sender_menu_draft: Cell<Option<usize>>,
    pending: Rc<Cell<Option<Action>>>,
    buttons: RefCell<BTreeMap<Action, ButtonInteractionState>>,
    query: TextFieldInteractionState,
    search: TextField,
    list_scroll: RefCell<BTreeMap<Mailbox, ScrollState>>,
    body_scroll: RefCell<BTreeMap<usize, ScrollState>>,
    folders_scroll: ScrollState,
    account_scroll: ScrollState,
    drag: Cell<Option<Split>>,
    menu_id: Cell<u64>,
    menu_target: Cell<Option<usize>>,
    drafts: RefCell<BTreeMap<usize, DraftRecord>>,
    active_draft: Cell<Option<usize>>,
    next_draft: Cell<usize>,
    draft: Draft,
}
#[derive(Clone)]
struct FlagChange {
    account: Account,
    message: crate::protocol::ImapMessageId,
    flag: ImapFlag,
    enabled: bool,
}
impl FlagChange {
    fn same_update(&self, other: &Self) -> bool {
        self.account.id == other.account.id
            && self.message == other.message
            && self.flag == other.flag
    }
}
enum Job {
    Load,
    Flag(FlagChange),
    Add(backend::Credential),
    Remove(usize, Account),
    Sync(usize, Account),
    Send(usize, Account, DraftRecord),
}
enum Reply {
    Loaded(Vec<Account>),
    FlagChanged(FlagChange),
    Added(Account),
    Removed(usize),
    Synced(usize, Vec<(String, crate::model::Thread)>),
    Sent(usize, DraftRecord),
}
fn execute(job: Job) -> Result<Reply> {
    match job {
        Job::Load => backend::accounts().map(Reply::Loaded),
        Job::Flag(change) => {
            backend::set_flag(&change.account, change.message, change.flag, change.enabled)?;
            Ok(Reply::FlagChanged(change))
        }
        Job::Add(c) => backend::add(c).map(Reply::Added),
        Job::Remove(i, a) => backend::remove(a.id).map(|_| Reply::Removed(i)),
        Job::Sync(i, a) => {
            let decoded = backend::receive(&a)?
                .into_iter()
                .map(|m| crate::mime::decode(m, i, 0))
                .collect::<Result<Vec<_>>>()?;
            Ok(Reply::Synced(i, decoded))
        }
        Job::Send(i, a, d) => {
            backend::send(&a, &d.to, &d.subject, &d.body)?;
            Ok(Reply::Sent(i, d))
        }
    }
}
#[derive(Clone)]
pub struct MailView {
    core: Rc<Core>,
}
#[derive(Clone, Copy)]
enum Content {
    Folders,
    List,
    Reader,
}
struct Contents {
    core: Rc<Core>,
    kind: Content,
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w.max(0.0), h.max(0.0))
}
fn line(b: Rect, c: &mut PaintContext<'_>) {
    Rectangle::new()
        .color(RectangleColor::Custom(c.theme.colors.border))
        .paint(b, c);
}
fn fill(b: Rect, color: Color, c: &mut PaintContext<'_>) {
    Rectangle::new()
        .color(RectangleColor::Custom(color))
        .paint(b, c);
}
impl Core {
    fn start(&self, job: Job) {
        // Sample accounts are UI fixtures, never service/Keychain identities.
        if self.preview.get() && !matches!(&job, Job::Add(_)) {
            *self.status.borrow_mut() = "Preview · Sample messages. No email is sent.".into();
            return;
        }
        if self.worker.borrow().is_some() {
            return;
        }
        let starting_flag = matches!(&job, Job::Flag(_));
        self.sending.set(matches!(&job, Job::Send(..)));
        *self.active_flag.borrow_mut() = match &job {
            Job::Flag(c) => Some(c.clone()),
            _ => None,
        };
        let (tx, rx) = mpsc::channel();
        *self.worker.borrow_mut() = Some(rx);
        *self.status.borrow_mut() = match &job {
            Job::Flag(change) => {
                if self.flag_error.get().is_some() {
                    "Some message updates failed. Use Get Mail to refresh. Updating remaining messages…".into()
                } else {
                    match change.flag {
                        ImapFlag::Seen => "Updating read status…",
                        ImapFlag::Flagged => "Updating flag…",
                    }
                    .into()
                }
            }
            Job::Sync(_, a) => format!("Receiving mail for {}…", a.config.email),
            Job::Send(_, a, _) => format!("Sending from {}…", a.config.email),
            _ => "Connecting…".into(),
        };
        if std::thread::Builder::new()
            .name("Mail network".into())
            .spawn(move || {
                let _ = tx.send(execute(job));
            })
            .is_err()
        {
            self.worker.borrow_mut().take();
            self.active_flag.borrow_mut().take();
            self.sending.set(false);
            if starting_flag {
                self.flag_error.set(Some(Error::Network));
            }
            *self.status.borrow_mut() = Error::Network.label().into();
        }
    }
    fn flag_pending(
        &self,
        account: [u8; 16],
        message: crate::protocol::ImapMessageId,
        flag: ImapFlag,
    ) -> bool {
        self.active_flag
            .borrow()
            .as_ref()
            .is_some_and(|c| c.account.id == account && c.message == message && c.flag == flag)
            || self
                .flag_queue
                .borrow()
                .iter()
                .any(|c| c.account.id == account && c.message == message && c.flag == flag)
    }
    fn queue_flag(&self, change: FlagChange) {
        if self.preview.get() {
            return;
        }
        if let Some(queued) = self
            .flag_queue
            .borrow_mut()
            .iter_mut()
            .find(|c| c.same_update(&change))
        {
            queued.enabled = change.enabled;
            return;
        }
        if self
            .active_flag
            .borrow()
            .as_ref()
            .is_some_and(|c| c.same_update(&change) && c.enabled == change.enabled)
        {
            return;
        }
        if self.flag_queue.borrow().len() >= 2000 {
            *self.status.borrow_mut() =
                "Too many pending message updates. Wait for them to finish.".into();
            return;
        }
        if self.active_flag.borrow().is_none() && self.flag_queue.borrow().is_empty() {
            self.flag_error.set(None);
        }
        self.flag_queue.borrow_mut().push_back(change);
    }
    fn take_next_flag(&self) -> Option<FlagChange> {
        loop {
            let change = self.flag_queue.borrow_mut().pop_front()?;
            let m = self.model.borrow();
            let current = m
                .accounts
                .iter()
                .position(|a| a.id == change.account.id)
                .and_then(|i| m.remote_ids.get(&(i, change.message.key())));
            if current.is_some() {
                return Some(change);
            }
        }
    }
    fn start_next_flag(&self) {
        if self.worker.borrow().is_some() {
            return;
        }
        while let Some(change) = self.take_next_flag() {
            self.start(Job::Flag(change));
            if self.worker.borrow().is_some() {
                return;
            }
        }
    }
    fn poll(&self) {
        let result = self.worker.borrow().as_ref().map(|rx| rx.try_recv());
        let Some(result) = result else {
            return;
        };
        let result = match result {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => Err(Error::Network),
        };
        self.worker.borrow_mut().take();
        self.sending.set(false);
        let was_flag = self.active_flag.borrow_mut().take().is_some();
        let mut receive_after_add = None;
        match result {
            Err(e) => {
                if was_flag {
                    self.flag_error.set(Some(e));
                }
                *self.status.borrow_mut() = e.label().into();
                if self.form.open.get() {
                    *self.form.feedback.borrow_mut() = e.label().into();
                }
            }
            Ok(reply) => {
                let mut m = self.model.borrow_mut();
                match reply {
                    Reply::FlagChanged(change) => {
                        if m.confirm_flag(
                            change.account.id,
                            change.message,
                            change.flag,
                            change.enabled,
                        ) {
                            *self.status.borrow_mut() = self.flag_error.get().map_or_else(
                                || {
                                    match change.flag {
                                        ImapFlag::Seen => "Read status updated.",
                                        ImapFlag::Flagged => "Flag updated.",
                                    }
                                    .into()
                                },
                                |e| e.label().into(),
                            );
                        } else {
                            self.flag_error.set(Some(Error::MailboxChanged));
                            *self.status.borrow_mut() = Error::MailboxChanged.label().into();
                        }
                    }
                    Reply::Loaded(accounts) => {
                        self.accounts_loaded.set(true);
                        m.accounts = accounts;
                        *self.status.borrow_mut() = if m.accounts.is_empty() {
                            "Add an account to start using Mail."
                        } else {
                            "Choose a mailbox, then Get Mail to refresh it."
                        }
                        .into();
                    }
                    Reply::Added(a) => {
                        if self.preview.replace(false) {
                            *m = Model::empty();
                            self.drafts.borrow_mut().clear();
                            self.active_draft.set(None);
                            self.accounts_loaded.set(true);
                        }
                        let index = m.accounts.len();
                        receive_after_add = Some(Job::Sync(index, a.clone()));
                        m.accounts.push(a);
                        m.switch(Mailbox {
                            account: Some(index),
                            folder: Folder::Inbox,
                        });
                        self.form.open.set(false);
                        self.sender.set(m.accounts.len() - 1);
                        *self.status.borrow_mut() = "Account connected.".into();
                    }
                    Reply::Removed(i) => {
                        m.accounts.remove(i);
                        self.drafts.borrow_mut().retain(|_, d| d.sender != i);
                        for d in self.drafts.borrow_mut().values_mut() {
                            if d.sender > i {
                                d.sender -= 1;
                            }
                        }
                        self.active_draft.set(None);
                        self.draft.to.set_value("");
                        self.draft.subject.set_value("");
                        self.draft.body.set_value("");
                        m.threads.retain(|t| t.account != i);
                        for t in &mut m.threads {
                            if t.account > i {
                                t.account -= 1;
                            }
                        }
                        m.remote_ids = m
                            .remote_ids
                            .iter()
                            .filter(|((a, _), _)| *a != i)
                            .map(|((a, k), id)| {
                                ((if *a > i { *a - 1 } else { *a }, k.clone()), *id)
                            })
                            .collect();
                        m.selected = None;
                        m.mailbox = Mailbox::default();
                        self.form
                            .page
                            .set(if self.preview.get() || m.accounts.is_empty() {
                                AccountPage::Choose
                            } else {
                                AccountPage::List
                            });
                        self.sender.set(0);
                        *self.status.borrow_mut() =
                            "Account removed. Messages on the server are unchanged.".into();
                    }
                    Reply::Synced(i, messages) => {
                        *self.status.borrow_mut() = match m.merge(i, messages) {
                            Ok(n) => format!("{} — {n} messages received", m.account_label(i)),
                            Err(e) => e.label().into(),
                        };
                    }
                    Reply::Sent(i, d) => {
                        let id = m.threads.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                        let email = m.accounts[i].config.email.clone();
                        m.threads.insert(
                            0,
                            crate::model::Thread {
                                id,
                                account: i,
                                subject: d.subject,
                                snippet: d.body.chars().take(160).collect(),
                                time: "Sent".into(),
                                unread: false,
                                flagged: false,
                                folder: Folder::Sent,
                                messages: vec![crate::model::Message {
                                    sender: email.clone(),
                                    address: email,
                                    recipient: d.to,
                                    date: "Sent".into(),
                                    body: d.body,
                                }],
                                attachment: None,
                            },
                        );
                        if let Some(id) = self.active_draft.take() {
                            self.drafts.borrow_mut().remove(&id);
                        }
                        self.draft.to.set_value("");
                        self.draft.subject.set_value("");
                        self.draft.body.set_value("");
                        m.page = Page::List;
                        *self.status.borrow_mut() = "Message accepted by the mail server.".into();
                    }
                }
            }
        }
        if let Some(job) = receive_after_add {
            self.start(job);
        }
        self.start_next_flag();
    }
    fn save_draft(&self) {
        if let Some(id) = self.active_draft.get() {
            self.drafts.borrow_mut().insert(
                id,
                DraftRecord {
                    sender: self.sender.get(),
                    to: self.draft.to.value(),
                    subject: self.draft.subject.value(),
                    body: self.draft.body.value(),
                },
            );
        }
    }
    fn begin_draft(&self, to: String, subject: String, body: String) {
        let id = self.next_draft.get();
        self.next_draft.set(id + 1);
        self.active_draft.set(Some(id));
        self.draft.to.set_value(to);
        self.draft.subject.set_value(subject);
        self.draft.body.set_value(body);
        self.save_draft();
    }
    fn button(&self, label: &str, action: Action) -> Button {
        let state = self.buttons.borrow_mut().entry(action).or_default().clone();
        let pending = self.pending.clone();
        Button::with_interaction_and_label(state, label)
            .size(ButtonSize::Small)
            .style(ButtonStyle::Ghost)
            .accessibility_label(label)
            .on_click(move || pending.set(Some(action)))
    }
    fn new_message_button(&self, compact: bool) -> Button {
        self.button(if compact { "New" } else { "New Message" }, Action::New)
            .accessibility_label("New Message")
            .leading_symbol(SymbolName::Pencil)
            .style(ButtonStyle::Standard)
    }
    fn icon(&self, symbol: SymbolName, label: &str, action: Action) -> Button {
        self.button(label, action).content(Glyph(symbol))
    }
    fn scroll(self: &Rc<Self>, kind: Content) -> Scroll {
        let state = match kind {
            Content::Folders => self.folders_scroll.clone(),
            Content::List => {
                let key = self.model.borrow().mailbox;
                self.list_scroll
                    .borrow_mut()
                    .entry(key)
                    .or_default()
                    .clone()
            }
            Content::Reader => {
                let id = self.model.borrow().selected.unwrap_or(0);
                self.body_scroll.borrow_mut().entry(id).or_default().clone()
            }
        };
        Scroll::new(state).content(Contents {
            core: self.clone(),
            kind,
        })
    }
    fn layout(&self, b: Rect) -> Layout {
        let m = self.model.borrow();
        let content = if !self.status.borrow().is_empty() && m.page != Page::Compose {
            rect(b.origin.x, b.origin.y, b.size.width, b.size.height - 48.0)
        } else {
            b
        };
        Layout::new(
            content,
            m.page,
            m.sidebar_hidden,
            m.sidebar_width,
            m.list_width,
        )
    }
    fn ids(&self) -> Vec<usize> {
        self.model.borrow().visible(&self.query.value())
    }
    fn apply(&self, action: Action, wide: bool) {
        let mut m = self.model.borrow_mut();
        match action {
            Action::Accounts => {
                if m.accounts.is_empty()
                    && !self.accounts_loaded.get()
                    && self.worker.borrow().is_none()
                {
                    self.start(Job::Load);
                }
                self.account_scroll.reset();
                self.form.clear_secret();
                self.form.feedback.borrow_mut().clear();
                self.form
                    .page
                    .set(if self.preview.get() || m.accounts.is_empty() {
                        AccountPage::Choose
                    } else {
                        AccountPage::List
                    });
                self.form.open.set(true);
                m.folders_open = false;
            }
            Action::CloseAccounts => {
                self.form.clear_secret();
                self.form.open.set(false);
            }
            Action::ChooseProvider => {
                self.form.page.set(AccountPage::Choose);
                self.form.feedback.borrow_mut().clear();
                self.account_scroll.reset();
            }
            Action::AccountBack => {
                self.form.clear_secret();
                self.form.feedback.borrow_mut().clear();
                self.form.page.set(match self.form.page.get() {
                    AccountPage::Details => AccountPage::Choose,
                    AccountPage::Remove(i) => AccountPage::Manage(i),
                    _ => AccountPage::List,
                });
                self.account_scroll.reset();
            }
            Action::Preset(kind) => {
                self.form.preset(kind);
                self.account_scroll.reset();
            }
            Action::Advanced => self.form.advanced.set(!self.form.advanced.get()),
            Action::IncomingSecurityValue(value) => {
                self.form.incoming_security.set(value);
                self.form
                    .set_incoming_port(if value == backend::Security::Tls {
                        if self.form.protocol.get() == backend::Protocol::Pop3 {
                            995
                        } else {
                            993
                        }
                    } else if self.form.protocol.get() == backend::Protocol::Pop3 {
                        110
                    } else {
                        143
                    });
            }
            Action::SmtpSecurityValue(value) => {
                self.form.smtp_security.set(value);
                self.form.set_smtp_port(if value == backend::Security::Tls {
                    465
                } else {
                    587
                });
            }
            Action::ManageAccount(i) => {
                self.form.page.set(AccountPage::Manage(i));
                self.account_scroll.reset();
                self.form.feedback.borrow_mut().clear();
            }
            Action::ConfirmRemove(i) => {
                self.form.page.set(AccountPage::Remove(i));
                self.account_scroll.reset();
            }
            Action::AddAccount => {
                if self.worker.borrow().is_some() {
                    return;
                }
                match self.form.credential() {
                    Ok(c) => {
                        self.form.clear_secret();
                        self.start(Job::Add(c));
                    }
                    Err(e) => {
                        if self.form.feedback.borrow().is_empty() {
                            *self.form.feedback.borrow_mut() = e.label().into();
                        }
                    }
                }
            }
            Action::RemoveAccount(i) => {
                if let Some(a) = m.accounts.get(i).cloned() {
                    self.start(Job::Remove(i, a));
                }
            }
            Action::Refresh => {
                if let Some((i, a)) = m
                    .mailbox
                    .account
                    .or(Some(self.sender.get()))
                    .and_then(|i| m.accounts.get(i).cloned().map(|a| (i, a)))
                {
                    self.start(Job::Sync(i, a));
                }
            }
            Action::Sender => {}
            Action::Send => {
                if let Some(a) = m.accounts.get(self.sender.get()).cloned() {
                    let d = DraftRecord {
                        sender: self.sender.get(),
                        to: self.draft.to.value(),
                        subject: self.draft.subject.value(),
                        body: self.draft.body.value(),
                    };
                    if crate::mime::recipients(&d.to).is_err() {
                        *self.status.borrow_mut() = Error::Invalid.label().into();
                        return;
                    }
                    self.save_draft();
                    self.start(Job::Send(self.sender.get(), a, d));
                }
            }
            Action::Folders => {
                if wide {
                    m.sidebar_hidden = !m.sidebar_hidden;
                } else {
                    m.folders_open = !m.folders_open;
                }
            }
            Action::CloseFolders => m.folders_open = false,
            Action::Box(mailbox) => m.switch(mailbox),
            Action::Open(id) => {
                let remote = m.imap_identity(id);
                let unread = m.thread(id).is_some_and(|t| t.unread);
                m.open(id);
                if let Some((account, message)) = remote {
                    // Opening again after a queued mark-unread means read it.
                    let desired = self
                        .flag_queue
                        .borrow()
                        .iter()
                        .rev()
                        .find(|c| {
                            c.account.id == account.id
                                && c.message == message
                                && c.flag == ImapFlag::Seen
                        })
                        .map(|c| !c.enabled)
                        .or_else(|| {
                            self.active_flag
                                .borrow()
                                .as_ref()
                                .filter(|c| {
                                    c.account.id == account.id
                                        && c.message == message
                                        && c.flag == ImapFlag::Seen
                                })
                                .map(|c| !c.enabled)
                        })
                        .unwrap_or(unread);
                    if desired {
                        self.queue_flag(FlagChange {
                            account,
                            message,
                            flag: ImapFlag::Seen,
                            enabled: true,
                        });
                    }
                } else {
                    m.local_state.insert(id);
                }
                self.query.set_focused(false);
            }
            Action::Back => {
                m.page = Page::List;
            }
            Action::New => {
                self.save_draft();
                if let Some(account) = m.mailbox.account {
                    self.sender.set(account);
                }
                if m.page != Page::Compose {
                    m.previous_page = m.page;
                }
                self.begin_draft(String::new(), String::new(), String::new());
                m.page = Page::Compose;
                m.folders_open = false;
                self.query.set_focused(false);
            }
            Action::ResumeDraft(id) => {
                self.save_draft();
                if let Some(d) = self.drafts.borrow().get(&id).cloned() {
                    self.sender.set(d.sender);
                    self.draft.to.set_value(d.to);
                    self.draft.subject.set_value(d.subject);
                    self.draft.body.set_value(d.body);
                    self.active_draft.set(Some(id));
                    m.previous_page = Page::List;
                    m.page = Page::Compose;
                }
            }
            Action::SaveDraft => {
                self.save_draft();
                m.page = m.previous_page;
            }
            Action::Reply | Action::Forward => {
                self.save_draft();
                if let Some(t) = m.selected.and_then(|id| m.thread(id)) {
                    self.sender.set(t.account);
                    let to = if action == Action::Reply {
                        t.messages.last().unwrap().address.to_owned()
                    } else {
                        String::new()
                    };
                    let subject = format!(
                        "{}: {}",
                        if action == Action::Reply { "Re" } else { "Fwd" },
                        t.subject
                    );
                    let body = if action == Action::Forward {
                        format!(
                            "\n\n---------- Forwarded message ----------\n{}\n\n{}",
                            &t.subject,
                            t.messages.last().unwrap().body
                        )
                    } else {
                        String::new()
                    };
                    self.begin_draft(to, subject, body);
                    m.previous_page = Page::Reader;
                    m.page = Page::Compose;
                    self.query.set_focused(false);
                }
            }
            Action::Unread => {
                if let Some(id) = m.selected {
                    if let Some((account, message)) = m.imap_identity(id) {
                        if !self.flag_pending(account.id, message, ImapFlag::Seen) {
                            let unread = !m.thread(id).unwrap().unread;
                            self.queue_flag(FlagChange {
                                account,
                                message,
                                flag: ImapFlag::Seen,
                                enabled: !unread,
                            });
                        }
                    } else if let Some(t) = m.threads.iter_mut().find(|t| t.id == id) {
                        t.unread = !t.unread;
                        m.local_state.insert(id);
                    }
                }
            }
            Action::Flag => {
                if let Some(id) = m.selected {
                    if let Some((account, message)) = m.imap_identity(id) {
                        if !self.flag_pending(account.id, message, ImapFlag::Flagged) {
                            let enabled = !m.thread(id).unwrap().flagged;
                            self.queue_flag(FlagChange {
                                account,
                                message,
                                flag: ImapFlag::Flagged,
                                enabled,
                            });
                        }
                    } else if let Some(t) = m.threads.iter_mut().find(|t| t.id == id) {
                        t.flagged = !t.flagged;
                        m.local_state.insert(id);
                    }
                }
            }
            Action::Archive | Action::Delete => {
                if let Some(id) = m.selected {
                    m.local_state.insert(id);
                    if let Some(t) = m.threads.iter_mut().find(|t| t.id == id) {
                        match action {
                            Action::Archive => t.folder = Folder::Archive,
                            Action::Delete => t.folder = Folder::Trash,
                            _ => {}
                        }
                        if matches!(action, Action::Archive | Action::Delete) {
                            m.selected = None;
                            m.page = Page::List;
                        }
                    }
                }
            }
            Action::Expand(id, n) => {
                if !m.expanded.remove(&(id, n)) {
                    m.expanded.insert((id, n));
                }
            }
            Action::Details => {}
        }
        drop(m);
        self.start_next_flag();
    }
}
impl MailView {
    pub fn new() -> Self {
        #[cfg(test)]
        let model = Model::sample();
        #[cfg(not(test))]
        let model = Model::empty();
        #[cfg(all(target_os = "linux", not(test)))]
        if std::env::var("MOCHIOS_PREVIEW_MAIL_DEMO").as_deref() == Ok("1") {
            return Self::with_model(Model::preview(), true);
        }
        let view = Self::with_model(model, false);
        #[cfg(not(test))]
        view.core.start(Job::Load);
        view
    }
    fn with_model(model: Model, preview: bool) -> Self {
        let query = TextFieldInteractionState::new();
        let search = TextField::with_interaction(query.clone())
            .placeholder("Search mail")
            .leading_symbol(SymbolName::Search);
        let view = Self {
            core: Rc::new(Core {
                model: RefCell::new(model),
                preview: Cell::new(preview),
                form: AccountForm::new(),
                worker: RefCell::new(None),
                sending: Cell::new(false),
                active_flag: RefCell::new(None),
                flag_queue: RefCell::new(VecDeque::new()),
                flag_error: Cell::new(None),
                status: RefCell::new(if preview {
                    "Preview · Sample messages. No email is sent.".into()
                } else {
                    String::new()
                }),
                accounts_loaded: Cell::new(preview),
                sender: Cell::new(0),
                sender_menu: Cell::new(0x4d41494c10000000),
                sender_menu_draft: Cell::new(None),
                pending: Rc::new(Cell::new(None)),
                buttons: RefCell::new(BTreeMap::new()),
                query,
                search,
                list_scroll: RefCell::new(BTreeMap::new()),
                body_scroll: RefCell::new(BTreeMap::new()),
                folders_scroll: ScrollState::new(),
                account_scroll: ScrollState::new(),
                drag: Cell::new(None),
                menu_id: Cell::new(0x4d41494c00000000),
                menu_target: Cell::new(None),
                drafts: RefCell::new(BTreeMap::new()),
                active_draft: Cell::new(None),
                next_draft: Cell::new(1),
                draft: Draft::new(),
            }),
        };
        view
    }
    fn account_bounds(b: Rect) -> Rect {
        rect(
            b.origin.x + ((b.size.width - 560.0) / 2.0).max(0.0),
            b.origin.y,
            b.size.width.min(560.0),
            b.size.height,
        )
    }
    fn account_height(&self) -> f32 {
        match self.core.form.page.get() {
            AccountPage::Details => self.core.form.height(),
            AccountPage::List => 320.0 + self.core.model.borrow().accounts.len() as f32 * 64.0,
            _ => 560.0,
        }
    }
    fn account_buttons(&self, b: Rect) -> Vec<(Rect, String, Action)> {
        let f = &self.core.form;
        let w = b.size.width;
        let m = self.core.model.borrow();
        let mut buttons = vec![(
            rect(b.origin.x + w - 88.0, b.origin.y + 20.0, 64.0, 28.0),
            "Done".into(),
            Action::CloseAccounts,
        )];
        if f.page.get() != AccountPage::List
            && !(f.page.get() == AccountPage::Choose
                && (self.core.preview.get() || m.accounts.is_empty()))
        {
            buttons.push((
                rect(b.origin.x + 24.0, b.origin.y + 20.0, 72.0, 28.0),
                "Back".into(),
                Action::AccountBack,
            ));
        }
        match f.page.get() {
            AccountPage::List => {
                for (i, a) in m.accounts.iter().enumerate() {
                    buttons.push((
                        rect(
                            b.origin.x + 24.0,
                            b.origin.y + 148.0 + i as f32 * 64.0,
                            w - 48.0,
                            48.0,
                        ),
                        a.config.email.clone(),
                        Action::ManageAccount(i),
                    ));
                }
                buttons.push((
                    rect(
                        b.origin.x + 24.0,
                        b.origin.y + 140.0 + m.accounts.len() as f32 * 64.0,
                        144.0,
                        30.0,
                    ),
                    "Add Account…".into(),
                    Action::ChooseProvider,
                ));
            }
            AccountPage::Choose => {
                for (i, (label, kind)) in [
                    ("Gmail", 0),
                    ("Other Account · IMAP", 1),
                    ("Other Account · POP3", 2),
                ]
                .into_iter()
                .enumerate()
                {
                    buttons.push((
                        rect(
                            b.origin.x + 24.0,
                            b.origin.y + 152.0 + i as f32 * 72.0,
                            w - 48.0,
                            48.0,
                        ),
                        label.into(),
                        Action::Preset(kind),
                    ));
                }
            }
            AccountPage::Details => {
                buttons.push((
                    rect(b.origin.x + 24.0, b.origin.y + 348.0, 180.0, 28.0),
                    if f.advanced.get() {
                        "Hide Advanced Settings"
                    } else {
                        "Advanced Settings…"
                    }
                    .into(),
                    Action::Advanced,
                ));
                if f.expanded() {
                    for (y, selected, actions) in [
                        (
                            510.0,
                            f.incoming_security.get(),
                            [
                                Action::IncomingSecurityValue(backend::Security::Tls),
                                Action::IncomingSecurityValue(backend::Security::StartTls),
                            ],
                        ),
                        (
                            680.0,
                            f.smtp_security.get(),
                            [
                                Action::SmtpSecurityValue(backend::Security::Tls),
                                Action::SmtpSecurityValue(backend::Security::StartTls),
                            ],
                        ),
                    ] {
                        let _ = selected;
                        let cell = (w - 164.0) / 2.0;
                        for (i, label) in ["TLS", "STARTTLS"].into_iter().enumerate() {
                            buttons.push((
                                rect(
                                    b.origin.x + 140.0 + i as f32 * cell,
                                    b.origin.y + y,
                                    cell - 4.0,
                                    32.0,
                                ),
                                label.into(),
                                actions[i],
                            ));
                        }
                    }
                }
                buttons.push((
                    rect(
                        b.origin.x + w - 148.0,
                        b.origin.y + f.submit_y(),
                        124.0,
                        32.0,
                    ),
                    if self.core.worker.borrow().is_some() {
                        "Connecting…"
                    } else {
                        "Connect"
                    }
                    .into(),
                    Action::AddAccount,
                ));
            }
            AccountPage::Manage(i) => {
                if m.accounts.get(i).is_some() {
                    buttons.push((
                        rect(b.origin.x + 24.0, b.origin.y + 346.0, 152.0, 30.0),
                        "Remove Account…".into(),
                        Action::ConfirmRemove(i),
                    ));
                }
            }
            AccountPage::Remove(i) => {
                buttons.push((
                    rect(b.origin.x + 24.0, b.origin.y + 270.0, 96.0, 32.0),
                    "Cancel".into(),
                    Action::AccountBack,
                ));
                buttons.push((
                    rect(b.origin.x + w - 176.0, b.origin.y + 270.0, 152.0, 32.0),
                    "Remove Account".into(),
                    Action::RemoveAccount(i),
                ));
            }
        }
        buttons
    }
    fn account_button(&self, label: &str, action: Action) -> Button {
        let f = &self.core.form;
        let style = match action {
            Action::AddAccount => ButtonStyle::Accent,
            Action::RemoveAccount(_) => ButtonStyle::Danger,
            Action::Preset(_) | Action::ManageAccount(_) => ButtonStyle::Standard,
            _ => ButtonStyle::Ghost,
        };
        let selected = match action {
            Action::IncomingSecurityValue(v) => f.incoming_security.get() == v,
            Action::SmtpSecurityValue(v) => f.smtp_security.get() == v,
            _ => false,
        };
        let button = self
            .core
            .button(label, action)
            .style(if selected {
                ButtonStyle::Standard
            } else {
                style
            })
            .accessibility_selected(selected)
            .enabled(self.core.worker.borrow().is_none());
        if matches!(action, Action::Preset(_) | Action::ManageAccount(_)) {
            button.content(
                HStack::new()
                    .alignment(StackAlignment::Center)
                    .gap(StackGap::Small)
                    .child(Text::label(label).layout().flex_grow(1.0))
                    .child(Glyph(SymbolName::ChevronRight)),
            )
        } else {
            button
        }
    }
    fn account_panel(&self, b: Rect, c: &mut PaintContext<'_>) {
        Scroll::new(self.core.account_scroll.clone())
            .content(AccountContents {
                core: self.core.clone(),
            })
            .paint(b, c);
    }
    fn account_event(&self, b: Rect, e: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        Scroll::new(self.core.account_scroll.clone())
            .content(AccountContents {
                core: self.core.clone(),
            })
            .handle_event(b, e, c)
    }
    fn account_content(&self, b: Rect, c: &mut PaintContext<'_>) {
        let b = Self::account_bounds(b);
        let f = &self.core.form;
        let m = self.core.model.borrow();
        let mut scope = AccessibilityNode::new(AccessibilityRole::Dialog, b);
        scope.focus_scope = true;
        scope.label = Some("Mail Accounts".into());
        c.record_accessibility(scope);
        let (title, subtitle) = match f.page.get() {
            AccountPage::List => (
                "Mail Accounts".to_owned(),
                "Manage the accounts you use with Mail.",
            ),
            AccountPage::Choose => (
                "Add an Account".into(),
                "Choose your email service to get started.",
            ),
            AccountPage::Details => (
                format!("Connect {}", f.provider.get().title()),
                if f.provider.get() == Provider::Gmail {
                    "Enter your email address and Google app password."
                } else {
                    "Use the server settings from your email provider."
                },
            ),
            AccountPage::Manage(_) => (
                "Account Details".into(),
                "Connection settings are stored in Keychain.",
            ),
            AccountPage::Remove(_) => (
                "Remove this account?".into(),
                "Messages on the server will not be deleted.",
            ),
        };
        Text::styled(title, TextRole::TitleSmall).paint(
            rect(
                b.origin.x + 24.0,
                b.origin.y + 68.0,
                b.size.width - 48.0,
                28.0,
            ),
            c,
        );
        Text::caption(subtitle)
            .color(c.theme.colors.text_secondary)
            .paint(
                rect(
                    b.origin.x + 24.0,
                    b.origin.y + 100.0,
                    b.size.width - 48.0,
                    36.0,
                ),
                c,
            );
        match f.page.get() {
            AccountPage::Choose => {
                Text::caption("Gmail uses an app password. Google sign-in is not available yet.")
                    .color(c.theme.colors.text_secondary)
                    .paint(
                        rect(
                            b.origin.x + 24.0,
                            b.origin.y + 382.0,
                            b.size.width - 48.0,
                            52.0,
                        ),
                        c,
                    );
            }
            AccountPage::Details => {
                f.paint(b, c);
                Text::caption("Your password is saved securely in Keychain.")
                    .color(c.theme.colors.text_secondary)
                    .paint(
                        rect(
                            b.origin.x + 24.0,
                            b.origin.y + f.submit_y() + 46.0,
                            b.size.width - 48.0,
                            28.0,
                        ),
                        c,
                    );
            }
            AccountPage::Manage(i) => {
                if let Some(a) = m.accounts.get(i) {
                    for (n, (label, value)) in [
                        ("Email", a.config.email.clone()),
                        (
                            "Incoming",
                            format!(
                                "{}:{} · {:?}",
                                a.config.incoming_host,
                                a.config.incoming_port,
                                a.config.incoming_security.label()
                            ),
                        ),
                        (
                            "Outgoing",
                            format!(
                                "{}:{} · {:?}",
                                a.config.smtp_host,
                                a.config.smtp_port,
                                a.config.smtp_security.label()
                            ),
                        ),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        Text::caption(label)
                            .color(c.theme.colors.text_secondary)
                            .paint(
                                rect(
                                    b.origin.x + 24.0,
                                    b.origin.y + 146.0 + n as f32 * 62.0,
                                    b.size.width - 48.0,
                                    20.0,
                                ),
                                c,
                            );
                        single(
                            &value,
                            TextRole::Body,
                            rect(
                                b.origin.x + 24.0,
                                b.origin.y + 168.0 + n as f32 * 62.0,
                                b.size.width - 48.0,
                                24.0,
                            ),
                            c,
                            c.theme.colors.text_primary,
                            false,
                        );
                    }
                }
            }
            AccountPage::Remove(i) => {
                if let Some(a) = m.accounts.get(i) {
                    Text::body(&a.config.email).paint(
                        rect(
                            b.origin.x + 24.0,
                            b.origin.y + 158.0,
                            b.size.width - 48.0,
                            28.0,
                        ),
                        c,
                    );
                    Text::caption(
                        "This removes its saved credentials and downloaded messages from Mail.",
                    )
                    .color(c.theme.colors.text_secondary)
                    .paint(
                        rect(
                            b.origin.x + 24.0,
                            b.origin.y + 198.0,
                            b.size.width - 48.0,
                            60.0,
                        ),
                        c,
                    );
                }
            }
            _ => {}
        }
        drop(m);
        for (frame, label, action) in self.account_buttons(b) {
            self.account_button(&label, action).paint(frame, c);
        }
        let feedback = if self.core.worker.borrow().is_some() {
            self.core.status.borrow().clone()
        } else {
            f.feedback.borrow().clone()
        };
        let y = if f.page.get() == AccountPage::Details {
            f.submit_y() + 84.0
        } else {
            self.account_height() - 100.0
        };
        Text::caption(feedback)
            .color(c.theme.colors.text_secondary)
            .paint(
                rect(b.origin.x + 24.0, b.origin.y + y, b.size.width - 48.0, 88.0),
                c,
            );
    }
    fn account_content_event(
        &self,
        b: Rect,
        e: &ViewEvent,
        c: &mut EventContext<'_>,
    ) -> EventResult {
        let b = Self::account_bounds(b);
        if self.core.worker.borrow().is_some() {
            return EventResult::Consumed;
        }
        let mut result = if self.core.form.page.get() == AccountPage::Details {
            self.core.form.event(b, e, c)
        } else {
            EventResult::Ignored
        };
        if matches!(
            e,
            ViewEvent::KeyPressed {
                key: Key::Escape,
                ..
            }
        ) {
            self.core.apply(Action::CloseAccounts, false);
            c.request_redraw_in(b);
            return EventResult::Consumed;
        }
        for (frame, label, action) in self.account_buttons(b) {
            result = result.merge(
                self.account_button(&label, action)
                    .handle_event(frame, e, c),
            );
        }
        if let Some(a) = self.core.pending.take() {
            self.core.apply(a, false);
        }
        c.request_redraw_in(b);
        result
    }
    fn folder_panel(&self, b: Rect, c: &mut PaintContext<'_>) {
        fill(b, c.theme.colors.surface_subtle, c);
        Text::styled("Mailboxes", TextRole::TitleSmall).paint(
            rect(
                b.origin.x + 16.0,
                b.origin.y + 14.0,
                b.size.width - 60.0,
                28.0,
            ),
            c,
        );
        if self.core.model.borrow().folders_open {
            self.core
                .icon(SymbolName::X, "Close", Action::CloseFolders)
                .paint(
                    rect(
                        b.origin.x + b.size.width - 40.0,
                        b.origin.y + 12.0,
                        28.0,
                        28.0,
                    ),
                    c,
                );
        }
        self.core.new_message_button(false).paint(
            rect(
                b.origin.x + 12.0,
                b.origin.y + 52.0,
                b.size.width - 24.0,
                30.0,
            ),
            c,
        );
        self.core
            .scroll(Content::Folders)
            .paint(Layout::content(b, 96.0), c);
    }
    fn list_header(&self, l: &Layout, b: Rect, c: &mut PaintContext<'_>) {
        let m = self.core.model.borrow();
        let title = m.mailbox.folder.label();
        for (i, (label, action)) in [
            ("Get Mail", Action::Refresh),
            ("Accounts", Action::Accounts),
        ]
        .into_iter()
        .enumerate()
        {
            self.core
                .button(label, action)
                .enabled(self.core.worker.borrow().is_none())
                .paint(
                    rect(
                        b.origin.x + b.size.width - 180.0 + i as f32 * 74.0,
                        b.origin.y + 7.0,
                        72.0,
                        30.0,
                    ),
                    c,
                );
        }
        let compact = l.width == Width::Compact;
        self.core
            .icon(SymbolName::List, "Mailboxes", Action::Folders)
            .paint(rect(b.origin.x + 8.0, b.origin.y + 7.0, 30.0, 30.0), c);
        if !compact {
            Text::label(title).paint(
                rect(
                    b.origin.x + 46.0,
                    b.origin.y + 11.0,
                    (b.size.width - 232.0).max(0.0),
                    24.0,
                ),
                c,
            );
        } else {
            Text::styled(title, TextRole::TitleLarge).paint(
                rect(
                    b.origin.x + 20.0,
                    b.origin.y + 49.0,
                    b.size.width - 40.0,
                    38.0,
                ),
                c,
            );
            let unread = m.unread(m.mailbox.account);
            Text::caption(format!("{unread} unread"))
                .color(c.theme.colors.text_secondary)
                .paint(
                    rect(
                        b.origin.x + 20.0,
                        b.origin.y + 87.0,
                        b.size.width - 40.0,
                        20.0,
                    ),
                    c,
                );
        }
        let y = b.origin.y + if compact { 116.0 } else { 50.0 };
        self.core
            .search
            .paint(rect(b.origin.x + 12.0, y, b.size.width - 24.0, 30.0), c);
        if !compact {
            let count = if m.mailbox.folder == Folder::Drafts {
                self.core.drafts.borrow().len()
            } else {
                self.core.ids().len()
            };
            let target = m.account_label(m.mailbox.account.unwrap_or(self.core.sender.get()));
            let summary = if target.is_empty() {
                format!("{count} messages")
            } else {
                format!("{count} messages · {target}")
            };
            single(
                &summary,
                TextRole::Caption,
                rect(
                    b.origin.x + 16.0,
                    b.origin.y + 87.0,
                    b.size.width - 32.0,
                    20.0,
                ),
                c,
                c.theme.colors.text_secondary,
                false,
            );
        }
    }
    fn reader_toolbar(&self, l: &Layout, b: Rect, c: &mut PaintContext<'_>) {
        let active = self.core.model.borrow().selected.is_some();
        let x = b.origin.x;
        if l.width == Width::Compact {
            self.core
                .icon(SymbolName::ChevronLeft, "Back to Messages", Action::Back)
                .paint(rect(x + 8.0, b.origin.y + 7.0, 30.0, 30.0), c);
        }
        self.core
            .button("Reply", Action::Reply)
            .enabled(active)
            .paint(
                rect(
                    x + if l.width == Width::Compact {
                        46.0
                    } else {
                        16.0
                    },
                    b.origin.y + 7.0,
                    60.0,
                    30.0,
                ),
                c,
            );
        self.core
            .button("Forward", Action::Forward)
            .enabled(active)
            .paint(
                rect(
                    x + if l.width == Width::Compact {
                        108.0
                    } else {
                        78.0
                    },
                    b.origin.y + 7.0,
                    72.0,
                    30.0,
                ),
                c,
            );
        self.core
            .button("More", Action::Details)
            .enabled(active)
            .paint(
                rect(x + b.size.width - 102.0, b.origin.y + 7.0, 56.0, 30.0),
                c,
            );
        self.core
            .icon(SymbolName::Trash, "Move to Trash", Action::Delete)
            .enabled(active)
            .paint(
                rect(x + b.size.width - 40.0, b.origin.y + 7.0, 30.0, 30.0),
                c,
            );
        line(rect(x, b.origin.y + 43.0, b.size.width, 1.0), c);
    }
    fn composer(&self, b: Rect, c: &mut PaintContext<'_>) {
        let frames = ComposeFrames::new(b);
        self.core
            .button("Save Draft", Action::SaveDraft)
            .paint(frames.close, c);
        self.core
            .button("Send", Action::Send)
            .style(ButtonStyle::Accent)
            .enabled(
                !self.core.preview.get()
                    && !self.core.model.borrow().accounts.is_empty()
                    && self.core.worker.borrow().is_none(),
            )
            .paint(frames.send, c);
        if b.size.width >= 600.0 {
            Text::label("New Message")
                .alignment(TextAlignment::Center)
                .paint(
                    rect(
                        b.origin.x + 128.0,
                        b.origin.y + 12.0,
                        b.size.width - 240.0,
                        24.0,
                    ),
                    c,
                );
        }
        line(rect(b.origin.x, b.origin.y + 44.0, b.size.width, 1.0), c);
        for (label, frame) in [
            ("From", frames.from),
            ("To", frames.to),
            ("Subject", frames.subject),
        ] {
            Text::caption(label)
                .color(c.theme.colors.text_secondary)
                .paint(rect(b.origin.x + 16.0, frame.origin.y + 6.0, 56.0, 24.0), c);
            line(
                rect(
                    b.origin.x + 16.0,
                    frame.origin.y + 38.0,
                    b.size.width - 32.0,
                    1.0,
                ),
                c,
            );
        }
        self.sender_button()
            .enabled(self.core.worker.borrow().is_none())
            .paint(frames.from, c);
        self.core.draft.to_field.paint(frames.to, c);
        self.core.draft.subject_field.paint(frames.subject, c);
        self.core.draft.editor.paint(frames.body, c);
        Text::caption(self.core.status.borrow().clone())
            .color(c.theme.colors.text_secondary)
            .paint(
                rect(
                    b.origin.x + 16.0,
                    b.origin.y + b.size.height - 52.0,
                    b.size.width - 32.0,
                    48.0,
                ),
                c,
            );
    }
    fn sender_button(&self) -> Button {
        let label = self
            .core
            .model
            .borrow()
            .account_label(self.core.sender.get())
            .to_owned();
        self.core
            .button(&label, Action::Sender)
            .accessibility_label(format!("From: {label}. Choose sending account"))
            .content(
                HStack::new()
                    .alignment(StackAlignment::Center)
                    .gap(StackGap::Small)
                    .child(Text::caption(label).layout().flex_grow(1.0))
                    .child(Glyph(SymbolName::ChevronDown)),
            )
    }
    fn needs_account(&self) -> bool {
        let m = self.core.model.borrow();
        m.accounts.is_empty() && m.threads.is_empty()
    }
    fn welcome_button(b: Rect) -> Rect {
        rect(
            b.origin.x + (b.size.width - 152.0) / 2.0,
            b.origin.y + b.size.height / 2.0 + 20.0,
            152.0,
            32.0,
        )
    }
    fn welcome(&self, b: Rect, c: &mut PaintContext<'_>) {
        let x = b.origin.x + ((b.size.width - 400.0) / 2.0).max(0.0);
        let w = b.size.width.min(400.0);
        let y = b.origin.y + b.size.height / 2.0;
        Text::styled("Welcome to Mail", TextRole::TitleSmall)
            .alignment(TextAlignment::Center)
            .paint(rect(x, y - 76.0, w, 30.0), c);
        Text::body("Add your email account to read and send messages.")
            .alignment(TextAlignment::Center)
            .color(c.theme.colors.text_secondary)
            .paint(rect(x + 20.0, y - 34.0, w - 40.0, 44.0), c);
        self.core
            .button("Add Account…", Action::Accounts)
            .style(ButtonStyle::Accent)
            .enabled(self.core.worker.borrow().is_none())
            .paint(Self::welcome_button(b), c);
        if !self.core.status.borrow().is_empty() {
            Text::caption(self.core.status.borrow().clone())
                .color(c.theme.colors.text_secondary)
                .paint(
                    rect(
                        b.origin.x + 20.0,
                        b.origin.y + b.size.height - 84.0,
                        b.size.width - 40.0,
                        76.0,
                    ),
                    c,
                );
        }
    }
}
struct ComposeFrames {
    close: Rect,
    send: Rect,
    from: Rect,
    to: Rect,
    subject: Rect,
    body: Rect,
}
impl ComposeFrames {
    fn new(b: Rect) -> Self {
        let field = |y| rect(b.origin.x + 80.0, b.origin.y + y, b.size.width - 96.0, 32.0);
        Self {
            close: rect(b.origin.x + 12.0, b.origin.y + 8.0, 100.0, 30.0),
            send: rect(
                b.origin.x + b.size.width - 76.0,
                b.origin.y + 8.0,
                64.0,
                30.0,
            ),
            from: field(54.0),
            to: field(96.0),
            subject: field(138.0),
            body: rect(
                b.origin.x + 16.0,
                b.origin.y + 190.0,
                b.size.width - 32.0,
                b.size.height - 250.0,
            ),
        }
    }
}
struct AccountContents {
    core: Rc<Core>,
}
impl View for AccountContents {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        Size::new(
            c.maximum.width,
            MailView {
                core: self.core.clone(),
            }
            .account_height(),
        )
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        MailView {
            core: self.core.clone(),
        }
        .account_content(b, c);
    }
    fn handle_event(&self, b: Rect, e: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        MailView {
            core: self.core.clone(),
        }
        .account_content_event(b, e, c)
    }
}
impl View for MailView {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        c.constrain(c.maximum)
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        self.core.poll();
        if self.core.worker.borrow().is_some() {
            c.request_redraw_at(Instant::now() + Duration::from_millis(50));
        }
        fill(b, c.theme.colors.background, c);
        if self.core.form.open.get() {
            self.account_panel(b, c);
            return;
        }
        if self.needs_account() {
            self.welcome(b, c);
            return;
        }
        let l = self.core.layout(b);
        fill(b, c.theme.colors.background, c);
        if l.composer {
            self.composer(b, c);
            return;
        }
        if let Some(side) = l.sidebar {
            self.folder_panel(side, c);
            line(
                rect(
                    side.origin.x + side.size.width,
                    side.origin.y,
                    1.0,
                    side.size.height,
                ),
                c,
            );
        }
        if let Some(list) = l.list {
            self.list_header(&l, list, c);
            self.core
                .scroll(Content::List)
                .paint(l.list_body().unwrap(), c);
            if l.reader.is_some() {
                line(
                    rect(
                        list.origin.x + list.size.width,
                        list.origin.y,
                        1.0,
                        list.size.height,
                    ),
                    c,
                );
            }
        }
        if let Some(reader) = l.reader {
            let top = 44.0;
            self.core
                .scroll(Content::Reader)
                .paint(Layout::content(reader, top), c);
            self.reader_toolbar(&l, reader, c);
        }
        if let Some(frame) = l.floating_compose() {
            self.core.new_message_button(true).paint(frame, c);
        }
        Text::caption(self.core.status.borrow().clone())
            .color(c.theme.colors.text_secondary)
            .paint(
                rect(
                    b.origin.x + 12.0,
                    b.origin.y + b.size.height - 44.0,
                    b.size.width - 24.0,
                    42.0,
                ),
                c,
            );
        if self.core.model.borrow().folders_open {
            fill(b, c.theme.colors.text_primary.with_alpha(40), c);
            let panel = rect(
                b.origin.x,
                b.origin.y,
                (b.size.width - 40.0).clamp(0.0, 280.0),
                b.size.height,
            );
            let mut scope = AccessibilityNode::new(AccessibilityRole::Dialog, panel);
            scope.focus_scope = true;
            scope.label = Some("Mailboxes".into());
            c.record_accessibility(scope);
            self.folder_panel(panel, c);
            line(
                rect(
                    panel.origin.x + panel.size.width,
                    panel.origin.y,
                    1.0,
                    panel.size.height,
                ),
                c,
            );
        }
    }
    fn handle_event(&self, b: Rect, event: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        if let ViewEvent::ContextMenuResult {
            request_id,
            command_id,
        } = event
        {
            if *request_id == self.core.sender_menu.get() {
                let same_draft = self.core.sender_menu_draft.take() == self.core.active_draft.get();
                if same_draft
                    && self.core.worker.borrow().is_none()
                    && self.core.model.borrow().page == Page::Compose
                {
                    if let Some(index) = command_id
                        .and_then(|id| id.checked_sub(1))
                        .map(|id| id as usize)
                        .filter(|i| *i < self.core.model.borrow().accounts.len())
                    {
                        self.core.sender.set(index);
                        self.core.save_draft();
                        c.request_redraw_in(b);
                    }
                }
                return EventResult::Consumed;
            }
        }
        if self.core.form.open.get() {
            return self.account_event(b, event, c);
        }
        if self.needs_account() {
            let result = self
                .core
                .button("Add Account…", Action::Accounts)
                .style(ButtonStyle::Accent)
                .enabled(self.core.worker.borrow().is_none())
                .handle_event(Self::welcome_button(b), event, c);
            if let Some(action) = self.core.pending.take() {
                self.core.apply(action, false);
                c.request_redraw_in(b);
            }
            return result;
        }
        let l = self.core.layout(b);
        if l.composer && self.core.sending.get() {
            return EventResult::Consumed;
        }
        if let ViewEvent::ContextMenuResult {
            request_id,
            command_id,
        } = event
        {
            if *request_id == self.core.menu_id.get() {
                if self.core.menu_target.take() == self.core.model.borrow().selected {
                    let action = match command_id {
                        Some(1) => Some(Action::Archive),
                        Some(2) => Some(Action::Flag),
                        Some(3) => Some(Action::Unread),
                        _ => None,
                    };
                    if let Some(action) = action {
                        self.core.apply(action, l.width == Width::Wide);
                        c.request_redraw_in(b);
                    }
                }
                return EventResult::Consumed;
            }
        }
        if let ViewEvent::KeyPressed { key, modifiers } = event {
            let action = match key {
                Key::Escape if self.core.model.borrow().folders_open => Some(Action::CloseFolders),
                Key::Escape if l.composer => Some(Action::SaveDraft),
                Key::Escape if l.width == Width::Compact && l.reader.is_some() => {
                    Some(Action::Back)
                }
                Key::Character('n' | 'N') if modifiers.shortcut() => Some(Action::New),
                Key::Character('f' | 'F') if modifiers.shortcut() && !l.composer => {
                    {
                        let mut m = self.core.model.borrow_mut();
                        m.page = Page::List;
                        m.folders_open = false;
                    }
                    self.core.query.set_focused(true);
                    self.core
                        .query
                        .set_selection(0..self.core.query.value().len());
                    c.request_redraw_in(b);
                    return EventResult::Consumed;
                }
                _ => None,
            };
            if let Some(action) = action {
                self.core.apply(action, l.width == Width::Wide);
                c.request_redraw_in(b);
                return EventResult::Consumed;
            }
        }
        // Drawer is modal: events never reach the mailbox beneath it.
        if self.core.model.borrow().folders_open {
            let panel = rect(
                b.origin.x,
                b.origin.y,
                (b.size.width - 40.0).clamp(0.0, 280.0),
                b.size.height,
            );
            if let ViewEvent::PointerPressed { position, .. } = event {
                if !panel.contains(*position) {
                    self.core.apply(Action::CloseFolders, false);
                    c.request_redraw_in(b);
                    return EventResult::Consumed;
                }
            }
            self.folder_event(panel, event, c);
            self.flush(&l, b, c);
            return EventResult::Consumed;
        }
        if let Some(list) = l.list {
            for (i, (label, action)) in [
                ("Get Mail", Action::Refresh),
                ("Accounts", Action::Accounts),
            ]
            .into_iter()
            .enumerate()
            {
                let frame = rect(
                    list.origin.x + list.size.width - 180.0 + i as f32 * 74.0,
                    list.origin.y + 7.0,
                    72.0,
                    30.0,
                );
                {
                    let r = self
                        .core
                        .button(label, action)
                        .enabled(self.core.worker.borrow().is_none())
                        .handle_event(frame, event, c);
                    if r.is_consumed() {
                        self.flush(&l, b, c);
                        return r;
                    }
                }
            }
        }
        if l.composer {
            if self.core.sending.get() {
                return EventResult::Consumed;
            }
            let frames = ComposeFrames::new(b);
            let mut result = self
                .core
                .button("Save Draft", Action::SaveDraft)
                .handle_event(frames.close, event, c);
            result = result.merge(
                self.core
                    .button("Send", Action::Send)
                    .style(ButtonStyle::Accent)
                    .enabled(
                        !self.core.preview.get() && !self.core.model.borrow().accounts.is_empty(),
                    )
                    .handle_event(frames.send, event, c),
            );
            result = result.merge(
                self.sender_button()
                    .enabled(!self.core.model.borrow().accounts.is_empty())
                    .handle_event(frames.from, event, c),
            );
            result = result.merge(self.core.draft.to_field.handle_event(frames.to, event, c));
            result = result.merge(self.core.draft.subject_field.handle_event(
                frames.subject,
                event,
                c,
            ));
            result = result.merge(self.core.draft.editor.handle_event(frames.body, event, c));
            self.flush(&l, b, c);
            return result;
        }
        if let Some(frame) = l.floating_compose() {
            if event.position().is_some_and(|p| frame.contains(p)) {
                self.core
                    .new_message_button(true)
                    .handle_event(frame, event, c);
                self.flush(&l, b, c);
                return EventResult::Consumed;
            }
        }
        if self.split_event(&l, event, c) {
            return EventResult::Consumed;
        }
        let mut result = EventResult::Ignored;
        if let Some(side) = l.sidebar {
            result = result.merge(self.folder_event(side, event, c));
        }
        if let Some(list) = l.list {
            result = result.merge(
                self.core
                    .icon(SymbolName::List, "Mailboxes", Action::Folders)
                    .handle_event(
                        rect(list.origin.x + 8.0, list.origin.y + 7.0, 30.0, 30.0),
                        event,
                        c,
                    ),
            );
            let query = self.core.query.value();
            result = result.merge(self.core.search.handle_event(
                rect(
                    list.origin.x + 12.0,
                    list.origin.y
                        + if l.width == Width::Compact {
                            116.0
                        } else {
                            50.0
                        },
                    list.size.width - 24.0,
                    30.0,
                ),
                event,
                c,
            ));
            if query != self.core.query.value() {
                self.core.scroll(Content::List).state().reset();
                let ids = self.core.ids();
                let mut m = self.core.model.borrow_mut();
                if m.selected.is_some_and(|id| !ids.contains(&id)) {
                    m.selected = None;
                }
                c.request_redraw_in(b);
            }
            result = result.merge(self.core.scroll(Content::List).handle_event(
                l.list_body().unwrap(),
                event,
                c,
            ));
        }
        if let Some(reader) = l.reader {
            let top = 44.0;
            result = result.merge(self.core.scroll(Content::Reader).handle_event(
                Layout::content(reader, top),
                event,
                c,
            ));
            if l.width == Width::Compact {
                result = result.merge(
                    self.core
                        .icon(SymbolName::ChevronLeft, "Back to Messages", Action::Back)
                        .handle_event(
                            rect(reader.origin.x + 8.0, reader.origin.y + 7.0, 30.0, 30.0),
                            event,
                            c,
                        ),
                );
            }
            let active = self.core.model.borrow().selected.is_some();
            for (label, action, x, w) in [
                (
                    "Reply",
                    Action::Reply,
                    if l.width == Width::Compact {
                        46.0
                    } else {
                        16.0
                    },
                    60.0,
                ),
                (
                    "Forward",
                    Action::Forward,
                    if l.width == Width::Compact {
                        108.0
                    } else {
                        78.0
                    },
                    72.0,
                ),
                ("More", Action::Details, reader.size.width - 102.0, 56.0),
            ] {
                result = result.merge(
                    self.core
                        .button(label, action)
                        .enabled(active)
                        .handle_event(
                            rect(reader.origin.x + x, reader.origin.y + 7.0, w, 30.0),
                            event,
                            c,
                        ),
                );
            }
            result = result.merge(
                self.core
                    .icon(SymbolName::Trash, "Move to Trash", Action::Delete)
                    .enabled(active)
                    .handle_event(
                        rect(
                            reader.origin.x + reader.size.width - 40.0,
                            reader.origin.y + 7.0,
                            30.0,
                            30.0,
                        ),
                        event,
                        c,
                    ),
            );
        }
        // Floating compose wins over the list below it, including its press capture.
        if let Some(frame) = l.floating_compose() {
            result = result.merge(
                self.core
                    .new_message_button(true)
                    .handle_event(frame, event, c),
            );
        }
        if self.flush(&l, b, c) {
            result = EventResult::Consumed;
        }
        result
    }
}
impl MailView {
    fn flush(&self, l: &Layout, b: Rect, c: &mut EventContext<'_>) -> bool {
        if let Some(a) = self.core.pending.take() {
            if a == Action::Sender {
                self.core
                    .sender_menu_draft
                    .set(self.core.active_draft.get());
                let m = self.core.model.borrow();
                self.core
                    .sender_menu
                    .set(self.core.sender_menu.get().wrapping_add(1));
                c.show_context_menu(ContextMenuRequest {
                    request_id: self.core.sender_menu.get(),
                    position: Point::new(b.origin.x + 60.0, b.origin.y + 88.0),
                    items: m
                        .accounts
                        .iter()
                        .enumerate()
                        .map(|(i, a)| ContextMenuItem {
                            command_id: i as u32 + 1,
                            label: a.config.email.clone(),
                            enabled: true,
                            checked: i == self.core.sender.get(),
                            destructive: false,
                            separator: false,
                        })
                        .collect(),
                });
            } else if a == Action::Details {
                if let Some(reader) = l.reader {
                    self.show_menu(
                        Point::new(
                            reader.origin.x + reader.size.width - 100.0,
                            reader.origin.y + 44.0,
                        ),
                        c,
                    );
                }
            } else {
                self.core.apply(a, l.width == Width::Wide);
            }
            c.request_redraw_in(b);
            true
        } else {
            false
        }
    }
    fn show_menu(&self, position: Point, c: &mut EventContext<'_>) {
        let m = self.core.model.borrow();
        let Some(t) = m.selected.and_then(|id| m.thread(id)) else {
            return;
        };
        self.core.menu_target.set(Some(t.id));
        self.core
            .menu_id
            .set(self.core.menu_id.get().wrapping_add(1));
        c.show_context_menu(ContextMenuRequest {
            request_id: self.core.menu_id.get(),
            position,
            items: vec![
                ContextMenuItem {
                    command_id: 1,
                    label: "Archive".into(),
                    enabled: t.folder != Folder::Archive,
                    checked: false,
                    destructive: false,
                    separator: false,
                },
                ContextMenuItem {
                    command_id: 2,
                    label: if t.flagged {
                        "Remove Flag"
                    } else {
                        "Flag Message"
                    }
                    .into(),
                    enabled: m.imap_identity(t.id).is_none_or(|(a, key)| {
                        !self.core.flag_pending(a.id, key, ImapFlag::Flagged)
                    }),
                    checked: t.flagged,
                    destructive: false,
                    separator: false,
                },
                ContextMenuItem {
                    command_id: 3,
                    label: if t.unread {
                        "Mark as Read"
                    } else {
                        "Mark as Unread"
                    }
                    .into(),
                    enabled: m
                        .imap_identity(t.id)
                        .is_none_or(|(a, key)| !self.core.flag_pending(a.id, key, ImapFlag::Seen)),
                    checked: false,
                    destructive: false,
                    separator: false,
                },
            ],
        });
    }
    fn folder_event(&self, b: Rect, event: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        let mut r = self.core.new_message_button(false).handle_event(
            rect(
                b.origin.x + 12.0,
                b.origin.y + 52.0,
                b.size.width - 24.0,
                30.0,
            ),
            event,
            c,
        );
        if self.core.model.borrow().folders_open {
            r = r.merge(
                self.core
                    .icon(SymbolName::X, "Close", Action::CloseFolders)
                    .handle_event(
                        rect(
                            b.origin.x + b.size.width - 40.0,
                            b.origin.y + 12.0,
                            28.0,
                            28.0,
                        ),
                        event,
                        c,
                    ),
            );
        }
        r.merge(
            self.core
                .scroll(Content::Folders)
                .handle_event(Layout::content(b, 96.0), event, c),
        )
    }
    fn split_event(&self, l: &Layout, event: &ViewEvent, c: &mut EventContext<'_>) -> bool {
        let Some(list) = l.list else { return false };
        if l.width == Width::Compact {
            return false;
        }
        let split_at = |p: Point| {
            if l.sidebar
                .is_some_and(|s| (p.x - s.origin.x - s.size.width).abs() < 4.0)
            {
                Some(Split::Sidebar)
            } else if (p.x - list.origin.x - list.size.width).abs() < 4.0 {
                Some(Split::List)
            } else {
                None
            }
        };
        match event {
            ViewEvent::PointerPressed {
                position,
                button: PointerButton::Primary,
            } => {
                if let Some(split) = split_at(*position) {
                    self.core.drag.set(Some(split));
                    c.set_cursor(CursorIcon::EwResize);
                    return true;
                }
            }
            ViewEvent::PointerMoved { position } => {
                if let Some(split) = self.core.drag.get() {
                    let mut m = self.core.model.borrow_mut();
                    match split {
                        Split::Sidebar => {
                            m.sidebar_width = (position.x - l.bounds.origin.x).clamp(160.0, 240.0)
                        }
                        Split::List => {
                            m.list_width = (position.x - list.origin.x).clamp(260.0, 400.0)
                        }
                    };
                    c.set_cursor(CursorIcon::EwResize);
                    c.request_redraw_in(l.bounds);
                    return true;
                }
                if split_at(*position).is_some() {
                    c.set_cursor(CursorIcon::EwResize);
                }
            }
            ViewEvent::PointerReleased {
                button: PointerButton::Primary,
                ..
            }
            | ViewEvent::PointerLeft => {
                if self.core.drag.take().is_some() {
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}
impl Contents {
    fn folders(&self, width: f32, theme: &Theme) -> VStack {
        let m = self.core.model.borrow();
        let mut stack = VStack::new()
            .alignment(StackAlignment::Stretch)
            .gap(StackGap::ExtraSmall);
        let mut accounts = vec![(None, "All Accounts".to_owned())];
        #[cfg(test)]
        if m.accounts.is_empty() {
            accounts.extend(
                crate::model::ACCOUNTS
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (Some(i), s.to_string())),
            );
        }
        accounts.extend(
            m.accounts
                .iter()
                .enumerate()
                .map(|(i, a)| (Some(i), a.config.email.clone())),
        );
        for (account, title) in accounts {
            stack = stack.child(
                Padding::only(12.0, 12.0, 4.0, 12.0)
                    .content(Text::caption(title).color(theme.colors.text_secondary)),
            );
            let folders: &[Folder] = if account.is_none() {
                &[
                    Folder::Inbox,
                    Folder::Unread,
                    Folder::Flagged,
                    Folder::Drafts,
                ]
            } else {
                &[Folder::Inbox, Folder::Sent, Folder::Archive, Folder::Trash]
            };
            for folder in folders {
                let mailbox = Mailbox {
                    account,
                    folder: *folder,
                };
                let selected = m.mailbox == mailbox;
                let mut row = HStack::new()
                    .alignment(StackAlignment::Center)
                    .gap(StackGap::Small);
                let symbol = match folder {
                    Folder::Inbox => Some(SymbolName::Tray),
                    Folder::Unread => Some(SymbolName::Mail),
                    Folder::Drafts => Some(SymbolName::Pencil),
                    Folder::Trash => Some(SymbolName::Trash),
                    _ => None,
                };
                if let Some(symbol) = symbol {
                    row = row.child(
                        Icon::new(symbol)
                            .size(theme.layout.compact_icon_size)
                            .color(theme.colors.text_secondary)
                            .frame(16.0, 16.0),
                    );
                } else {
                    row = row.child(
                        Rectangle::new()
                            .color(RectangleColor::Custom(Color::TRANSPARENT))
                            .frame(16.0, 16.0),
                    );
                }
                row = row.child(Text::label(folder.label()).layout().flex_grow(1.0));
                if *folder == Folder::Inbox || *folder == Folder::Unread {
                    let n = m.unread(account);
                    if n > 0 {
                        row = row
                            .child(Text::caption(n.to_string()).color(theme.colors.text_secondary));
                    }
                }
                if *folder == Folder::Drafts && !self.core.drafts.borrow().is_empty() {
                    row = row.child(Text::caption(self.core.drafts.borrow().len().to_string()));
                }
                let button = self
                    .core
                    .button(folder.label(), Action::Box(mailbox))
                    .accessibility_selected(selected)
                    .style(ButtonStyle::Custom {
                        background: if selected {
                            theme.list.selected_background
                        } else {
                            Color::TRANSPARENT
                        },
                        hovered_background: if selected {
                            theme.list.selected_background
                        } else {
                            theme.list.hovered_background
                        },
                        border: Color::TRANSPARENT,
                        hovered_border: Color::TRANSPARENT,
                        foreground: theme.list.foreground,
                    })
                    .content(row)
                    .radius(theme.button.radius);
                stack = stack.child(
                    Padding::symmetric(8.0, 0.0)
                        .content(button)
                        .frame(width, 32.0),
                );
            }
        }
        stack
    }

    fn reader(&self, width: f32, theme: &Theme) -> Padding<VStack> {
        let m = self.core.model.borrow();
        let mut stack = VStack::new()
            .alignment(StackAlignment::Stretch)
            .gap(StackGap::Medium);
        let Some(t) = m.selected.and_then(|id| m.thread(id)) else {
            return Padding::all(28.0).content(
                stack
                    .child(Text::styled("No Message Selected", TextRole::TitleSmall))
                    .child(
                        Text::body("Select a message to read it here.")
                            .color(theme.colors.text_secondary),
                    ),
            );
        };
        stack = stack.child(Text::styled(&t.subject, TextRole::TitleSmall));
        for (n, message) in t.messages.iter().enumerate() {
            if n + 1 < t.messages.len() {
                let expanded = m.expanded.contains(&(t.id, n));
                stack = stack.child(self.core.button(
                    &format!(
                        "{} · {}  {}",
                        &message.sender,
                        &message.date,
                        if expanded {
                            "Close"
                        } else {
                            "Show Earlier Message"
                        }
                    ),
                    Action::Expand(t.id, n),
                ));
                if !expanded {
                    continue;
                }
            }
            stack = stack
                .child(
                    HStack::new()
                        .alignment(StackAlignment::Center)
                        .gap(StackGap::Small)
                        .child(
                            Text::body_emphasized(&message.sender)
                                .layout()
                                .flex_grow(1.0),
                        )
                        .child(Text::caption(&message.date).color(theme.colors.text_secondary)),
                )
                .child(
                    Text::caption(format!("{}\nTo：{}", message.address, message.recipient))
                        .color(theme.colors.text_secondary),
                )
                .child(Text::body(&message.body));
        }
        if let Some(name) = &t.attachment {
            stack = stack
                .child(Divider::new())
                .child(
                    HStack::new()
                        .gap(StackGap::Small)
                        .child(Glyph(SymbolName::File))
                        .child(Text::caption(name).layout().flex_grow(1.0)),
                )
                .child(
                    Text::caption("Attachment opening and saving are not yet available.")
                        .color(theme.colors.text_secondary),
                );
        }
        stack = stack.child(Divider::new()).child(
            self.core
                .button("Reply", Action::Reply)
                .style(ButtonStyle::Standard)
                .layout()
                .width(80.0),
        );
        // A readable line length on large displays, without enclosing the message in a card.
        let margin = if width > 760.0 {
            (width - 704.0) / 2.0
        } else {
            24.0
        };
        Padding::symmetric(margin, 24.0).content(stack)
    }
    fn mail_button(&self, id: usize, theme: &Theme) -> Button {
        let m = self.core.model.borrow();
        let t = m.thread(id).unwrap();
        let selected = m.selected == Some(id);
        self.core
            .button(&t.subject, Action::Open(id))
            .radius(CornerRadius::None)
            .omochi(false)
            .accessibility_role(AccessibilityRole::ListItem)
            .accessibility_selected(selected)
            .accessibility_label(format!(
                "{}、{}、{}{}",
                &t.messages.last().unwrap().sender,
                &t.subject,
                &t.time,
                if t.unread { ", unread" } else { "" }
            ))
            .style(ButtonStyle::Custom {
                background: if selected {
                    theme.list.selected_background
                } else {
                    theme.list.background
                },
                hovered_background: if selected {
                    theme.list.selected_background
                } else {
                    theme.list.hovered_background
                },
                border: Color::TRANSPARENT,
                hovered_border: Color::TRANSPARENT,
                foreground: theme.list.foreground,
            })
            .content(MailRow {
                core: self.core.clone(),
                id,
            })
    }
}
impl View for Contents {
    fn measure(&self, c: Constraints, ctx: &mut MeasureContext<'_>) -> Size {
        match self.kind {
            Content::Folders => self.folders(c.maximum.width, ctx.theme).measure(c, ctx),
            Content::Reader => self.reader(c.maximum.width, ctx.theme).measure(c, ctx),
            Content::List => {
                let m = self.core.model.borrow();
                let rows = m.visible(&self.core.query.value()).len();
                let height = if m.mailbox.folder == Folder::Drafts {
                    (self.core.drafts.borrow().len() as f32 * 96.0).max(160.0)
                } else if rows == 0 {
                    160.0
                } else {
                    rows as f32 * 96.0
                };
                c.constrain(Size::new(c.maximum.width, height + 80.0))
            }
        }
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        match self.kind {
            Content::Folders => self.folders(b.size.width, c.theme).paint(b, c),
            Content::Reader => self.reader(b.size.width, c.theme).paint(b, c),
            Content::List => {
                let m = self.core.model.borrow();
                if m.mailbox.folder == Folder::Drafts {
                    let drafts = self.core.drafts.borrow();
                    if drafts.is_empty() {
                        Text::body("No Drafts")
                            .color(c.theme.colors.text_secondary)
                            .paint(
                                rect(
                                    b.origin.x + 20.0,
                                    b.origin.y + 30.0,
                                    b.size.width - 40.0,
                                    80.0,
                                ),
                                c,
                            );
                    }
                    for (i, (id, d)) in drafts.iter().rev().enumerate() {
                        let label = if d.subject.is_empty() {
                            "(No Subject)"
                        } else {
                            &d.subject
                        };
                        self.core
                            .button(label, Action::ResumeDraft(*id))
                            .content(
                                VStack::new()
                                    .gap(StackGap::Small)
                                    .child(Text::body_emphasized(label))
                                    .child(
                                        Text::caption(if d.to.is_empty() {
                                            "No recipient"
                                        } else {
                                            &d.to
                                        })
                                        .color(c.theme.colors.text_secondary),
                                    ),
                            )
                            .paint(
                                rect(
                                    b.origin.x + 12.0,
                                    b.origin.y + i as f32 * 96.0 + 8.0,
                                    b.size.width - 24.0,
                                    80.0,
                                ),
                                c,
                            );
                    }
                    return;
                }
                let ids = m.visible(&self.core.query.value());
                if ids.is_empty() {
                    Text::body(if self.core.query.value().is_empty() {
                        "No Messages"
                    } else {
                        "No Matching Messages"
                    })
                    .color(c.theme.colors.text_secondary)
                    .paint(
                        rect(
                            b.origin.x + 20.0,
                            b.origin.y + 30.0,
                            b.size.width - 40.0,
                            80.0,
                        ),
                        c,
                    );
                    return;
                }
                for (i, id) in ids.into_iter().enumerate() {
                    let r = rect(b.origin.x, b.origin.y + i as f32 * 96.0, b.size.width, 96.0);
                    self.mail_button(id, c.theme).paint(r, c);
                    line(
                        rect(
                            r.origin.x + 20.0,
                            r.origin.y + 95.0,
                            r.size.width - 40.0,
                            1.0,
                        ),
                        c,
                    );
                }
            }
        }
    }
    fn handle_event(&self, b: Rect, event: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        match self.kind {
            Content::Folders => self
                .folders(b.size.width, c.theme())
                .handle_event(b, event, c),
            Content::Reader => self
                .reader(b.size.width, c.theme())
                .handle_event(b, event, c),
            Content::List => {
                let m = self.core.model.borrow();
                if m.mailbox.folder == Folder::Drafts {
                    let mut result = EventResult::Ignored;
                    for (i, (id, _)) in self.core.drafts.borrow().iter().rev().enumerate() {
                        result = result.merge(
                            self.core
                                .button("Open Draft", Action::ResumeDraft(*id))
                                .handle_event(
                                    rect(
                                        b.origin.x + 12.0,
                                        b.origin.y + i as f32 * 96.0 + 8.0,
                                        b.size.width - 24.0,
                                        80.0,
                                    ),
                                    event,
                                    c,
                                ),
                        );
                    }
                    return result;
                }
                let mut result = EventResult::Ignored;
                for (i, id) in m.visible(&self.core.query.value()).into_iter().enumerate() {
                    result = result.merge(self.mail_button(id, c.theme()).handle_event(
                        rect(b.origin.x, b.origin.y + i as f32 * 96.0, b.size.width, 96.0),
                        event,
                        c,
                    ));
                }
                result
            }
        }
    }
}
struct MailRow {
    core: Rc<Core>,
    id: usize,
}
impl View for MailRow {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        c.constrain(Size::new(c.maximum.width, 96.0))
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        let m = self.core.model.borrow();
        let Some(t) = m.thread(self.id) else { return };
        let x = b.origin.x + 20.0;
        let w = b.size.width - 40.0;
        let y = b.origin.y + 13.0;
        single(
            &t.messages.last().unwrap().sender,
            TextRole::Body,
            rect(x, y, w - 64.0, 22.0),
            c,
            c.theme.colors.text_primary,
            t.unread,
        );
        Text::caption(&t.time)
            .alignment(TextAlignment::End)
            .color(c.theme.colors.text_secondary)
            .paint(rect(x + w - 60.0, y + 2.0, 60.0, 20.0), c);
        single(
            &t.subject,
            TextRole::Label,
            rect(x, y + 25.0, w, 20.0),
            c,
            c.theme.colors.text_primary,
            t.unread,
        );
        single(
            &t.snippet,
            TextRole::Caption,
            rect(x, y + 49.0, w - if t.flagged { 50.0 } else { 0.0 }, 20.0),
            c,
            c.theme.colors.text_secondary,
            false,
        );
        if t.flagged {
            Text::caption("Flagged")
                .color(c.theme.colors.text_secondary)
                .alignment(TextAlignment::End)
                .paint(rect(x + w - 48.0, y + 49.0, 48.0, 20.0), c);
        }
        if t.unread {
            Ellipse::new()
                .color(EllipseColor::Custom(c.theme.list.status_marker))
                .paint(rect(b.origin.x + 7.0, y + 7.0, 6.0, 6.0), c);
        }
        if t.messages.len() > 1 {
            Text::caption(t.messages.len().to_string())
                .color(c.theme.colors.text_secondary)
                .paint(rect(b.origin.x + 5.0, y + 29.0, 12.0, 18.0), c);
        }
    }
}
fn single(
    value: &str,
    role: TextRole,
    b: Rect,
    c: &mut PaintContext<'_>,
    color: Color,
    bold: bool,
) {
    let weight = if bold { 600 } else { 400 };
    let text = Text::styled(value, role).weight(weight).color(color);
    c.display_list.push(DrawCommand::PushClip { rect: b });
    if text.measure_unbounded(c.text_measurer).width <= b.size.width {
        text.paint(b, c);
    } else {
        let chars: Vec<_> = value
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(value.len()))
            .collect();
        let mut low = 0;
        let mut high = chars.len() - 1;
        while low < high {
            let mid = (low + high + 1) / 2;
            let label = format!("{}…", &value[..chars[mid]]);
            if Text::styled(label, role)
                .weight(weight)
                .measure_unbounded(c.text_measurer)
                .width
                <= b.size.width
            {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        Text::styled(format!("{}…", &value[..chars[low]]), role)
            .weight(weight)
            .color(color)
            .paint(b, c);
    }
    c.display_list.push(DrawCommand::PopClip);
}

struct Glyph(SymbolName);
impl View for Glyph {
    fn measure(&self, c: Constraints, ctx: &mut MeasureContext<'_>) -> Size {
        let n = ctx.theme.layout.compact_icon_size;
        c.constrain(Size::new(n, n))
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        Icon::new(self.0)
            .size(c.theme.layout.compact_icon_size)
            .color(c.theme.colors.text_secondary)
            .paint(b, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore::viewkit::draw_command::DisplayList;
    use appcore::viewkit::platform::KeyModifiers;
    use appcore::viewkit::typography::TextMeasurer;
    struct Fixture {
        view: MailView,
        bounds: Cell<Rect>,
    }
    impl Fixture {
        fn new(width: f32) -> Self {
            Self {
                view: MailView::new(),
                bounds: Cell::new(Rect::new(0.0, 0.0, width, 640.0)),
            }
        }
        fn event(&self, event: ViewEvent) -> Option<ContextMenuRequest> {
            let theme = Theme::current();
            let mut measurer = TextMeasurer::new();
            let mut ctx = EventContext::new(&theme, &theme.typography, &mut measurer);
            self.view.handle_event(self.bounds.get(), &event, &mut ctx);
            ctx.take_context_menu_request()
        }
        fn paint(&self) {
            let theme = Theme::current();
            let mut measurer = TextMeasurer::new();
            let mut list = DisplayList::new();
            self.view.paint(
                self.bounds.get(),
                &mut PaintContext::new(&mut list, &theme, &theme.typography, &mut measurer),
            );
            assert!(!list.commands().is_empty());
        }
        fn click(&self, p: Point) -> Option<ContextMenuRequest> {
            self.paint();
            self.event(ViewEvent::PointerPressed {
                position: p,
                button: PointerButton::Primary,
            });
            self.event(ViewEvent::PointerReleased {
                position: p,
                button: PointerButton::Primary,
            })
        }
        fn resize(&self, width: f32) {
            self.bounds.set(Rect::new(0.0, 0.0, width, 640.0));
            self.paint();
        }
    }
    #[test]
    fn compact_reader_back_and_window_resizing_preserve_selection_and_scroll() {
        let f = Fixture::new(400.0);
        let scroll = f.view.core.scroll(Content::List).state().clone();
        scroll.set_offset(0.0, 120.0);
        f.paint();
        f.click(Point::new(150.0, 274.0));
        assert_eq!(f.view.core.model.borrow().selected, Some(3));
        assert_eq!(f.view.core.model.borrow().page, Page::Reader);
        f.resize(1080.0);
        assert_eq!(f.view.core.model.borrow().selected, Some(3));
        f.resize(400.0);
        f.click(Point::new(23.0, 22.0));
        assert_eq!(f.view.core.model.borrow().page, Page::List);
        f.paint();
        assert_eq!(scroll.offset_y(), 120.0);
    }
    #[test]
    fn folder_drawer_blocks_the_underlying_mail_list_and_compose_wins_over_rows() {
        let f = Fixture::new(400.0);
        f.click(Point::new(23.0, 22.0));
        assert!(f.view.core.model.borrow().folders_open);
        let selected = f.view.core.model.borrow().selected;
        f.click(Point::new(390.0, 300.0));
        assert!(!f.view.core.model.borrow().folders_open);
        assert_eq!(f.view.core.model.borrow().selected, selected);
        let fab = f
            .view
            .core
            .layout(f.bounds.get())
            .floating_compose()
            .unwrap();
        f.click(Point::new(fab.origin.x + 50.0, fab.origin.y + 20.0));
        assert_eq!(f.view.core.model.borrow().page, Page::Compose);
        assert_eq!(f.view.core.model.borrow().selected, selected);
    }
    #[test]
    fn search_uses_native_input_and_account_changes_do_not_show_other_account_mail() {
        let f = Fixture::new(1080.0);
        f.event(ViewEvent::KeyPressed {
            key: Key::Character('f'),
            modifiers: KeyModifiers::from_bits(KeyModifiers::CONTROL),
        });
        f.event(ViewEvent::TextInput {
            text: "リリース".into(),
        });
        assert_eq!(f.view.core.ids(), vec![2]);
        assert_eq!(f.view.core.model.borrow().selected, None);
        f.paint();
        f.view.core.apply(
            Action::Box(Mailbox {
                account: Some(1),
                folder: Folder::Inbox,
            }),
            true,
        );
        assert!(f.view.core.model.borrow().selected.is_none());
    }
    #[test]
    fn independent_in_session_drafts_survive_navigation_and_resizing() {
        let f = Fixture::new(400.0);
        f.view.core.apply(Action::New, false);
        f.paint();
        f.click(Point::new(140.0, 112.0));
        f.event(ViewEvent::TextInput {
            text: "friend@example.test".into(),
        });
        f.click(Point::new(140.0, 154.0));
        f.event(ViewEvent::TextInput {
            text: "こんにちは".into(),
        });
        f.click(Point::new(140.0, 200.0));
        f.event(ViewEvent::TextInput {
            text: "あとで続きを書きます。".into(),
        });
        f.resize(1080.0);
        f.resize(400.0);
        assert_eq!(f.view.core.draft.body.value(), "あとで続きを書きます。");
        f.event(ViewEvent::KeyPressed {
            key: Key::Escape,
            modifiers: KeyModifiers::default(),
        });
        assert_eq!(f.view.core.drafts.borrow().len(), 1);
        f.view.core.apply(Action::Reply, false);
        assert_eq!(f.view.core.drafts.borrow().len(), 2);
        assert_eq!(f.view.core.draft.to.value(), "haruka@example.test");
        f.view.core.apply(Action::SaveDraft, false);
        f.view.core.apply(Action::ResumeDraft(1), false);
        assert_eq!(f.view.core.draft.subject.value(), "こんにちは");
        assert_eq!(f.view.core.draft.body.value(), "あとで続きを書きます。");
    }
    #[test]
    fn organization_uses_native_context_menu_and_moves_only_the_selected_sample() {
        let f = Fixture::new(1080.0);
        let reader = f.view.core.layout(f.bounds.get()).reader.unwrap();
        let menu = f
            .click(Point::new(reader.origin.x + reader.size.width - 74.0, 22.0))
            .unwrap();
        assert_eq!(menu.items.len(), 3);
        f.event(ViewEvent::ContextMenuResult {
            request_id: menu.request_id,
            command_id: Some(2),
        });
        assert!(f.view.core.model.borrow().thread(1).unwrap().flagged);
        f.view.core.apply(Action::Archive, true);
        assert_eq!(
            f.view.core.model.borrow().thread(1).unwrap().folder,
            Folder::Archive
        );
        assert_eq!(
            f.view.core.model.borrow().thread(2).unwrap().folder,
            Folder::Inbox
        );
        f.view.core.apply(
            Action::Box(Mailbox {
                account: None,
                folder: Folder::Archive,
            }),
            true,
        );
        f.paint();
        assert_eq!(f.view.core.ids(), vec![1]);
    }
    #[test]
    fn drawer_has_native_focus_scope_and_new_draft_shortcut_can_close_again() {
        let f = Fixture::new(400.0);
        f.view.core.apply(Action::Folders, false);
        let theme = Theme::current();
        let mut measurer = TextMeasurer::new();
        let mut list = DisplayList::new();
        let mut nodes = vec![];
        f.view.paint(
            f.bounds.get(),
            &mut PaintContext::new(&mut list, &theme, &theme.typography, &mut measurer)
                .with_accessibility_nodes(&mut nodes),
        );
        let scope = nodes.iter().position(|n| n.focus_scope).unwrap();
        assert!(nodes[scope + 1..].iter().any(|n| n.focusable));
        f.view.core.apply(Action::CloseFolders, false);
        f.view.core.apply(Action::New, false);
        f.event(ViewEvent::KeyPressed {
            key: Key::Character('n'),
            modifiers: KeyModifiers::from_bits(KeyModifiers::CONTROL),
        });
        f.event(ViewEvent::KeyPressed {
            key: Key::Escape,
            modifiers: KeyModifiers::default(),
        });
        assert_eq!(f.view.core.model.borrow().page, Page::List);
    }
    #[test]
    fn split_drag_keeps_the_reader_readable() {
        let f = Fixture::new(1080.0);
        f.paint();
        let l = f.view.core.layout(f.bounds.get());
        let list = l.list.unwrap();
        let x = list.origin.x + list.size.width;
        f.event(ViewEvent::PointerPressed {
            position: Point::new(x, 300.0),
            button: PointerButton::Primary,
        });
        f.event(ViewEvent::PointerMoved {
            position: Point::new(1000.0, 300.0),
        });
        f.event(ViewEvent::PointerReleased {
            position: Point::new(1000.0, 300.0),
            button: PointerButton::Primary,
        });
        f.paint();
        assert!(
            f.view
                .core
                .layout(f.bounds.get())
                .reader
                .unwrap()
                .size
                .width
                >= 400.0
        );
    }
    fn with_accounts(f: &Fixture) {
        f.view.core.accounts_loaded.set(true);
        let mut cfg = crate::tests::config();
        let first = backend::Account {
            id: [1; 16],
            config: cfg.clone(),
        };
        cfg.email = "work@example.test".into();
        f.view.core.model.borrow_mut().accounts = vec![
            first,
            backend::Account {
                id: [2; 16],
                config: cfg,
            },
        ];
    }
    fn completion(f: &Fixture, result: Result<Reply>) {
        let (tx, rx) = mpsc::channel();
        tx.send(result).ok();
        *f.view.core.worker.borrow_mut() = Some(rx);
        f.view.core.poll();
    }
    #[test]
    fn account_page_is_native_scrollable_and_modal() {
        let f = Fixture::new(360.0);
        with_accounts(&f);
        f.view.core.apply(Action::Accounts, false);
        f.paint();
        f.event(ViewEvent::KeyPressed {
            key: Key::Character('n'),
            modifiers: KeyModifiers::from_bits(KeyModifiers::CONTROL),
        });
        assert_ne!(f.view.core.model.borrow().page, Page::Compose);
        f.view.core.form.preset(0);
        assert_eq!(f.view.core.form.protocol.get(), backend::Protocol::Imap);
        f.event(ViewEvent::KeyPressed {
            key: Key::Escape,
            modifiers: KeyModifiers::default(),
        });
        assert!(!f.view.core.form.open.get());
    }
    #[test]
    fn draft_sender_and_reply_account_survive_switches() {
        let f = Fixture::new(1080.0);
        with_accounts(&f);
        f.view.core.apply(Action::New, true);
        f.view.core.draft.body.set_value("personal draft");
        let first = f.view.core.active_draft.get().unwrap();
        f.view.core.apply(Action::SaveDraft, true);
        f.view.core.apply(Action::Open(2), true);
        f.view.core.apply(Action::Reply, true);
        assert_eq!(f.view.core.sender.get(), 1);
        assert_eq!(f.view.core.drafts.borrow()[&first].sender, 0);
        f.view.core.apply(Action::ResumeDraft(first), true);
        assert_eq!(f.view.core.sender.get(), 0);
        assert_eq!(f.view.core.draft.body.value(), "personal draft");
    }
    #[test]
    fn uncertain_delivery_retains_draft_and_success_clears_only_sent_draft() {
        let f = Fixture::new(1080.0);
        with_accounts(&f);
        f.view.core.apply(Action::New, true);
        f.view.core.draft.to.set_value("you@example.test");
        f.view.core.draft.body.set_value("body");
        f.view.core.save_draft();
        let id = f.view.core.active_draft.get().unwrap();
        completion(&f, Err(Error::DeliveryUnknown));
        assert_eq!(f.view.core.model.borrow().page, Page::Compose);
        assert!(f.view.core.drafts.borrow().contains_key(&id));
        assert_eq!(f.view.core.draft.body.value(), "body");
        let draft = f.view.core.drafts.borrow()[&id].clone();
        completion(&f, Ok(Reply::Sent(0, draft)));
        assert!(!f.view.core.drafts.borrow().contains_key(&id));
        assert!(f.view.core.draft.body.value().is_empty());
        assert!(
            f.view
                .core
                .model
                .borrow()
                .threads
                .iter()
                .any(|t| t.folder == Folder::Sent && t.messages[0].body == "body")
        );
    }
    #[test]
    fn inflight_send_cannot_be_replaced_by_keyboard_navigation() {
        let f = Fixture::new(1080.0);
        with_accounts(&f);
        f.view.core.apply(Action::New, true);
        let (_tx, rx) = mpsc::channel();
        *f.view.core.worker.borrow_mut() = Some(rx);
        f.view.core.sending.set(true);
        let id = f.view.core.active_draft.get();
        for key in [Key::Escape, Key::Character('n')] {
            f.event(ViewEvent::KeyPressed {
                key,
                modifiers: KeyModifiers::from_bits(KeyModifiers::CONTROL),
            });
        }
        assert_eq!(f.view.core.active_draft.get(), id);
        assert_eq!(f.view.core.model.borrow().page, Page::Compose);
    }
    #[test]
    fn sender_menu_lists_accounts_and_updates_only_the_open_draft() {
        let f = Fixture::new(360.0);
        with_accounts(&f);
        f.view.core.apply(Action::New, false);
        let frame = ComposeFrames::new(f.bounds.get()).from;
        let request = f
            .click(Point::new(frame.origin.x + 24.0, frame.origin.y + 16.0))
            .unwrap();
        assert_eq!(request.items.len(), 2);
        assert!(request.items[0].checked);
        f.event(ViewEvent::ContextMenuResult {
            request_id: request.request_id,
            command_id: Some(2),
        });
        assert_eq!(f.view.core.sender.get(), 1);
        let request = f
            .click(Point::new(frame.origin.x + 24.0, frame.origin.y + 16.0))
            .unwrap();
        f.view.core.apply(Action::New, false);
        f.event(ViewEvent::ContextMenuResult {
            request_id: request.request_id,
            command_id: Some(1),
        });
        assert_eq!(f.view.core.sender.get(), 1);
    }
    #[test]
    fn account_navigation_separates_setup_details_and_removal() {
        for width in [320.0, 560.0, 1080.0] {
            let f = Fixture::new(width);
            with_accounts(&f);
            f.view.core.apply(Action::Accounts, false);
            assert!(f.view.core.form.page.get() == AccountPage::List);
            f.paint();
            f.view.core.apply(Action::ChooseProvider, false);
            f.paint();
            f.view.core.apply(Action::Preset(0), false);
            f.paint();
            assert!(f.view.core.form.page.get() == AccountPage::Details);
            f.view.core.apply(Action::Advanced, false);
            f.paint();
            f.view.core.apply(Action::AccountBack, false);
            assert!(f.view.core.form.page.get() == AccountPage::Choose);
            f.view.core.apply(Action::ManageAccount(1), false);
            f.paint();
            f.view.core.apply(Action::ConfirmRemove(1), false);
            f.paint();
            assert_eq!(f.view.core.model.borrow().accounts.len(), 2);
            f.view.core.apply(Action::AccountBack, false);
            assert!(f.view.core.form.page.get() == AccountPage::Manage(1));
        }
    }
    #[test]
    fn empty_mailbox_has_a_direct_add_account_action() {
        let f = Fixture::new(360.0);
        *f.view.core.model.borrow_mut() = Model::empty();
        f.view.core.accounts_loaded.set(true);
        f.paint();
        let button = MailView::welcome_button(f.bounds.get());
        f.click(Point::new(button.origin.x + 40.0, button.origin.y + 16.0));
        assert!(f.view.core.form.open.get());
        assert!(f.view.core.form.page.get() == AccountPage::Choose);
    }
    #[test]
    fn symbols_remain_compact_in_an_oversized_allocation() {
        let theme = Theme::current();
        let mut measurer = TextMeasurer::new();
        let mut list = DisplayList::new();
        Glyph(SymbolName::Mail).paint(
            Rect::new(10.0, 20.0, 300.0, 180.0),
            &mut PaintContext::new(&mut list, &theme, &theme.typography, &mut measurer),
        );
        let shapes: Vec<_> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::DrawSvg { command } => Some(command.bounds),
                _ => None,
            })
            .collect();
        assert!(!shapes.is_empty());
        for r in shapes {
            assert!(
                r.size.width <= theme.layout.compact_icon_size * 2.0
                    && r.size.height <= theme.layout.compact_icon_size * 2.0
            );
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_preview_opens_mail_without_keychain_or_account_login() {
        let f = Fixture {
            view: MailView::with_model(Model::preview(), true),
            bounds: Cell::new(Rect::new(0.0, 0.0, 360.0, 640.0)),
        };
        assert!(!f.view.needs_account());
        assert!(f.view.core.worker.borrow().is_none());
        f.paint();
        f.view.core.apply(Action::Refresh, false);
        assert!(f.view.core.worker.borrow().is_none());
        f.view.core.apply(Action::Open(2), false);
        f.paint();
        f.view.core.apply(Action::Reply, false);
        assert_eq!(f.view.core.draft.to.value(), "morgan@work.example.test");
        assert_eq!(f.view.core.sender.get(), 1);
        f.paint();
        f.view.core.apply(Action::Send, false);
        assert!(f.view.core.worker.borrow().is_none());
        f.view.core.start(Job::Load);
        assert!(f.view.core.worker.borrow().is_none());
        f.view.core.apply(Action::Accounts, false);
        assert!(f.view.core.form.page.get() == AccountPage::Choose);
        f.paint();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn preview_samples_are_disposable_and_keep_real_mail_addresses_out() {
        let model = Model::preview();
        assert!(!model.threads.is_empty());
        assert!(
            model
                .accounts
                .iter()
                .all(|a| a.config.email.ends_with(".test"))
        );
        assert!(
            model
                .threads
                .iter()
                .flat_map(|t| t.messages.iter())
                .all(|m| m.address.ends_with(".test"))
        );
        let mut changed = Model::preview();
        changed.threads.clear();
        assert!(!Model::preview().threads.is_empty());
    }
    fn remote_fixture() -> Fixture {
        let f = Fixture::new(1080.0);
        with_accounts(&f);
        {
            let mut m = f.view.core.model.borrow_mut();
            m.threads.truncate(2);
            for t in &mut m.threads {
                t.unread = true;
            }
            m.remote_ids.insert((0, "imap:42:7".into()), 1);
            m.remote_ids.insert((1, "imap:42:8".into()), 2);
        }
        f
    }
    fn hold_network(f: &Fixture) -> mpsc::Sender<Result<Reply>> {
        let (tx, rx) = mpsc::channel();
        *f.view.core.worker.borrow_mut() = Some(rx);
        tx
    }
    #[test]
    fn opening_remote_messages_queues_each_identity_and_waits_for_confirmation() {
        let f = remote_fixture();
        let _hold = hold_network(&f);
        f.view.core.apply(Action::Open(1), true);
        f.view.core.apply(Action::Open(2), true);
        assert!(f.view.core.model.borrow().thread(1).unwrap().unread);
        assert!(f.view.core.model.borrow().thread(2).unwrap().unread);
        let first = f.view.core.take_next_flag().unwrap();
        let second = f.view.core.take_next_flag().unwrap();
        assert!(f.view.core.take_next_flag().is_none());
        assert_eq!(first.account.id, [1; 16]);
        assert_eq!(first.message.uid, 7);
        assert_eq!(second.account.id, [2; 16]);
        assert_eq!(second.message.uid, 8);
        *f.view.core.active_flag.borrow_mut() = Some(first.clone());
        completion(&f, Ok(Reply::FlagChanged(first)));
        assert!(!f.view.core.model.borrow().thread(1).unwrap().unread);
        assert!(f.view.core.model.borrow().thread(2).unwrap().unread);
        assert_eq!(f.view.core.model.borrow().selected, Some(2));
        *f.view.core.active_flag.borrow_mut() = Some(second.clone());
        completion(&f, Ok(Reply::FlagChanged(second)));
        assert!(!f.view.core.model.borrow().thread(2).unwrap().unread);
    }
    #[test]
    fn failed_read_update_keeps_confirmed_state_and_manual_retry_can_mark_unread() {
        let f = remote_fixture();
        let _hold = hold_network(&f);
        f.view.core.apply(Action::Open(1), true);
        f.paint();
        let reader = f.view.core.layout(f.bounds.get()).reader.unwrap();
        let menu = f
            .click(Point::new(reader.origin.x + reader.size.width - 74.0, 22.0))
            .unwrap();
        assert!(
            !menu
                .items
                .iter()
                .find(|i| i.command_id == 3)
                .unwrap()
                .enabled
        );
        let change = f.view.core.take_next_flag().unwrap();
        *f.view.core.active_flag.borrow_mut() = Some(change.clone());
        completion(&f, Err(Error::ReadStatusUnknown));
        assert!(f.view.core.model.borrow().thread(1).unwrap().unread);
        assert!(f.view.core.active_flag.borrow().is_none());
        assert!(
            f.view
                .core
                .status
                .borrow()
                .contains("could not be confirmed")
        );
        let _hold2 = hold_network(&f);
        f.view.core.apply(Action::Unread, true);
        let retry = f.view.core.take_next_flag().unwrap();
        assert!(retry.enabled);
        *f.view.core.active_flag.borrow_mut() = Some(retry.clone());
        completion(&f, Ok(Reply::FlagChanged(retry)));
        assert!(!f.view.core.model.borrow().thread(1).unwrap().unread);
        let _hold3 = hold_network(&f);
        f.view.core.apply(Action::Unread, true);
        let unread = f.view.core.take_next_flag().unwrap();
        assert!(!unread.enabled);
        *f.view.core.active_flag.borrow_mut() = Some(unread.clone());
        completion(&f, Ok(Reply::FlagChanged(unread)));
        assert!(f.view.core.model.borrow().thread(1).unwrap().unread);
    }
    #[test]
    fn stale_flag_queue_is_discarded_and_late_ack_cannot_change_a_replaced_message() {
        let f = remote_fixture();
        let _hold = hold_network(&f);
        f.view.core.apply(Action::Open(1), true);
        {
            let mut m = f.view.core.model.borrow_mut();
            m.remote_ids.remove(&(0, "imap:42:7".into()));
            m.remote_ids.insert((0, "imap:43:7".into()), 1);
        }
        assert!(f.view.core.take_next_flag().is_none());
        let account = f.view.core.model.borrow().accounts[0].clone();
        completion(
            &f,
            Ok(Reply::FlagChanged(FlagChange {
                account,
                message: crate::protocol::ImapMessageId {
                    validity: 42,
                    uid: 7,
                },
                flag: ImapFlag::Seen,
                enabled: true,
            })),
        );
        assert!(f.view.core.model.borrow().thread(1).unwrap().unread);
        assert!(!f.view.core.status.borrow().contains("updated"));
    }
    #[test]
    fn reply_editor_stays_editable_during_a_read_update() {
        let f = remote_fixture();
        let _hold = hold_network(&f);
        f.view.core.apply(Action::Open(1), true);
        f.view.core.apply(Action::Reply, true);
        f.paint();
        let body = ComposeFrames::new(f.bounds.get()).body;
        f.click(Point::new(body.origin.x + 20.0, body.origin.y + 16.0));
        f.event(ViewEvent::TextInput {
            text: "Still typing".into(),
        });
        assert_eq!(f.view.core.draft.body.value(), "Still typing");
        f.event(ViewEvent::KeyPressed {
            key: Key::Escape,
            modifiers: KeyModifiers::default(),
        });
        assert_ne!(f.view.core.model.borrow().page, Page::Compose);
    }
    #[test]
    fn queued_read_changes_coalesce_to_the_last_intent() {
        let f = remote_fixture();
        let account = f.view.core.model.borrow().accounts[0].clone();
        let message = crate::protocol::ImapMessageId {
            validity: 42,
            uid: 7,
        };
        for unread in [false, true, false] {
            f.view.core.queue_flag(FlagChange {
                account: account.clone(),
                message,
                flag: ImapFlag::Seen,
                enabled: !unread,
            });
        }
        assert_eq!(f.view.core.flag_queue.borrow().len(), 1);
        assert!(f.view.core.take_next_flag().unwrap().enabled);
    }
    #[test]
    fn seen_and_flagged_updates_on_one_message_are_independent() {
        let f = remote_fixture();
        let _hold = hold_network(&f);
        f.view.core.model.borrow_mut().threads[0].flagged = false;
        f.view.core.apply(Action::Open(1), true);
        f.view.core.apply(Action::Flag, true);
        assert_eq!(f.view.core.flag_queue.borrow().len(), 2);
        assert!(!f.view.core.model.borrow().thread(1).unwrap().flagged);
        assert!(f.view.core.model.borrow().thread(1).unwrap().unread);
        let seen = f.view.core.take_next_flag().unwrap();
        let flag = f.view.core.take_next_flag().unwrap();
        assert_eq!(seen.flag, ImapFlag::Seen);
        assert_eq!(flag.flag, ImapFlag::Flagged);
        *f.view.core.active_flag.borrow_mut() = Some(flag.clone());
        completion(&f, Ok(Reply::FlagChanged(flag)));
        assert!(f.view.core.model.borrow().thread(1).unwrap().flagged);
        assert!(f.view.core.model.borrow().thread(1).unwrap().unread);
        *f.view.core.active_flag.borrow_mut() = Some(seen.clone());
        completion(&f, Ok(Reply::FlagChanged(seen)));
        assert!(f.view.core.model.borrow().thread(1).unwrap().flagged);
        assert!(!f.view.core.model.borrow().thread(1).unwrap().unread);
    }
    #[test]
    fn failed_flag_update_preserves_confirmed_state_and_can_be_retried() {
        let f = remote_fixture();
        {
            let mut m = f.view.core.model.borrow_mut();
            m.threads[0].unread = false;
            m.threads[0].flagged = false;
        }
        let _hold = hold_network(&f);
        f.view.core.apply(Action::Open(1), true);
        f.view.core.apply(Action::Flag, true);
        f.paint();
        let reader = f.view.core.layout(f.bounds.get()).reader.unwrap();
        let menu = f
            .click(Point::new(reader.origin.x + reader.size.width - 74.0, 22.0))
            .unwrap();
        assert!(
            !menu
                .items
                .iter()
                .find(|i| i.command_id == 2)
                .unwrap()
                .enabled
        );
        assert!(
            menu.items
                .iter()
                .find(|i| i.command_id == 3)
                .unwrap()
                .enabled
        );
        let flag = f.view.core.take_next_flag().unwrap();
        *f.view.core.active_flag.borrow_mut() = Some(flag);
        completion(&f, Err(Error::FlagStatusUnknown));
        assert!(!f.view.core.model.borrow().thread(1).unwrap().flagged);
        assert!(
            f.view
                .core
                .status
                .borrow()
                .contains("Flag status could not be confirmed")
        );
        for enabled in [true, false] {
            let _hold = hold_network(&f);
            f.view.core.apply(Action::Flag, true);
            let flag = f.view.core.take_next_flag().unwrap();
            assert_eq!(flag.flag, ImapFlag::Flagged);
            assert_eq!(flag.enabled, enabled);
            *f.view.core.active_flag.borrow_mut() = Some(flag.clone());
            completion(&f, Ok(Reply::FlagChanged(flag)));
            assert_eq!(
                f.view.core.model.borrow().thread(1).unwrap().flagged,
                enabled
            );
            assert!(!f.view.core.model.borrow().thread(1).unwrap().unread);
        }
    }
    #[test]
    fn queue_coalesces_per_flag_and_opening_ignores_pending_flagged_intent() {
        let f = remote_fixture();
        let _hold = hold_network(&f);
        let account = f.view.core.model.borrow().accounts[0].clone();
        let message = crate::protocol::ImapMessageId {
            validity: 42,
            uid: 7,
        };
        f.view.core.queue_flag(FlagChange {
            account: account.clone(),
            message,
            flag: ImapFlag::Seen,
            enabled: true,
        });
        f.view.core.queue_flag(FlagChange {
            account: account.clone(),
            message,
            flag: ImapFlag::Flagged,
            enabled: true,
        });
        f.view.core.queue_flag(FlagChange {
            account: account.clone(),
            message,
            flag: ImapFlag::Seen,
            enabled: false,
        });
        assert_eq!(f.view.core.flag_queue.borrow().len(), 2);
        let first = f.view.core.take_next_flag().unwrap();
        let second = f.view.core.take_next_flag().unwrap();
        assert_eq!(first.flag, ImapFlag::Seen);
        assert!(!first.enabled);
        assert_eq!(second.flag, ImapFlag::Flagged);
        assert!(second.enabled);
        *f.view.core.active_flag.borrow_mut() = Some(second);
        f.view.core.apply(Action::Open(1), true);
        let opened = f.view.core.take_next_flag().unwrap();
        assert_eq!(opened.flag, ImapFlag::Seen);
        assert!(opened.enabled);
    }
    #[test]
    fn stale_flagged_ack_does_not_change_a_replacement_message() {
        let f = remote_fixture();
        let _hold = hold_network(&f);
        f.view.core.model.borrow_mut().selected = Some(1);
        f.view.core.apply(Action::Flag, true);
        let change = f.view.core.take_next_flag().unwrap();
        {
            let mut m = f.view.core.model.borrow_mut();
            m.remote_ids.remove(&(0, "imap:42:7".into()));
            m.remote_ids.insert((0, "imap:43:7".into()), 1);
        }
        *f.view.core.active_flag.borrow_mut() = Some(change.clone());
        completion(&f, Ok(Reply::FlagChanged(change)));
        assert!(!f.view.core.model.borrow().thread(1).unwrap().flagged);
        assert!(f.view.core.status.borrow().contains("mailbox changed"));
    }
    #[test]
    fn pop3_flagging_stays_local_and_does_not_create_a_worker() {
        let f = remote_fixture();
        {
            let mut m = f.view.core.model.borrow_mut();
            m.accounts[0].config.protocol = backend::Protocol::Pop3;
            m.selected = Some(1);
            m.threads[0].flagged = false;
        }
        f.view.core.apply(Action::Flag, true);
        assert!(f.view.core.model.borrow().thread(1).unwrap().flagged);
        assert!(f.view.core.worker.borrow().is_none());
        assert!(f.view.core.flag_queue.borrow().is_empty());
    }
}
