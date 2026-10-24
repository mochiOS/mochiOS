//! Session mailbox and locally organized message state.
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Folder {
    Inbox,
    Unread,
    Flagged,
    Drafts,
    Archive,
    Sent,
    Trash,
}
impl Folder {
    pub fn label(self) -> &'static str {
        match self {
            Self::Inbox => "Inbox",
            Self::Unread => "Unread",
            Self::Flagged => "Flagged",
            Self::Drafts => "Drafts",
            Self::Archive => "Archive",
            Self::Sent => "Sent",
            Self::Trash => "Trash",
        }
    }
}
#[derive(Clone)]
pub struct Message {
    pub sender: String,
    pub address: String,
    pub recipient: String,
    pub date: String,
    pub body: String,
}
#[derive(Clone)]
pub struct Thread {
    pub id: usize,
    pub account: usize,
    pub subject: String,
    pub snippet: String,
    pub time: String,
    pub unread: bool,
    pub flagged: bool,
    pub folder: Folder,
    pub messages: Vec<Message>,
    pub attachment: Option<String>,
}
#[cfg(test)]
pub const ACCOUNTS: [&str; 2] = ["個人用", "仕事用"];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    List,
    Reader,
    Compose,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mailbox {
    pub account: Option<usize>,
    pub folder: Folder,
}
impl Default for Folder {
    fn default() -> Self {
        Self::Inbox
    }
}
pub struct Model {
    pub threads: Vec<Thread>,
    pub accounts: Vec<crate::backend::Account>,
    pub remote_ids: std::collections::BTreeMap<(usize, String), usize>,
    pub local_state: std::collections::BTreeSet<usize>,
    pub mailbox: Mailbox,
    pub selected: Option<usize>,
    pub page: Page,
    pub expanded: BTreeSet<(usize, usize)>,
    pub folders_open: bool,
    pub sidebar_hidden: bool,
    pub sidebar_width: f32,
    pub list_width: f32,
    pub previous_page: Page,
}
impl Model {
    #[cfg(test)]
    pub fn sample() -> Self {
        let mut threads = vec![];
        let samples = [
            (
                0,
                "来週のランチ、どうする？",
                "駅前のお店、予約できそうです。土曜日の12時はどうかな。",
                "10:42",
                "佐藤 はるか",
                "haruka@example.test",
                "おはよう！\n\nこの前話していた駅前のお店、土曜日なら予約できそうです。12時に待ち合わせはどうかな？\n\n予定が合えば、あとでお店の場所も送ります。\n楽しみにしています。",
                None,
            ),
            (
                1,
                "次のリリースに向けた確認",
                "レビューありがとうございます。変更点をまとめました。",
                "09:18",
                "高橋 悠",
                "yu@example.test",
                "お疲れさまです。\n\n先日のレビュー内容を反映しました。今回は以下の3点を中心に確認をお願いします。\n\n・初回起動時の案内\n・ウィンドウを小さくしたときの表示\n・キーボードでの操作\n\n細かい内容は添付の資料にまとめています。\n気になる点があれば、このメールに返信してください。\n\nよろしくお願いします。",
                Some("リリース確認.pdf · 248 KB"),
            ),
            (
                0,
                "ご予約ありがとうございます",
                "ご来店をお待ちしております。予約内容をご確認ください。",
                "昨日",
                "喫茶 こもれび",
                "cafe@example.test",
                "ご予約ありがとうございます。\n\n日時：10月31日（土）12:00\n人数：2名\n\n窓際のお席をご用意しています。変更がありましたらご連絡ください。\n\n喫茶 こもれび",
                None,
            ),
            (
                1,
                "デザイン資料を共有しました",
                "余白と文字組みを中心に更新しています。",
                "昨日",
                "田中 みな",
                "mina@example.test",
                "こんにちは。\n\n画面構成の資料を更新しました。\n今回のテーマは、情報を探しやすくすることと、本文を落ち着いて読めることです。\n\n狭いウィンドウでも操作に迷わないか、一緒に確認できればと思います。\n\nよろしくお願いします。",
                None,
            ),
            (
                0,
                "週末の写真",
                "よく撮れていた写真をまとめて送ります。",
                "火曜日",
                "山本 りん",
                "rin@example.test",
                "先週はありがとう！\n\n散歩の写真をまとめました。夕方の光がとてもきれいだったね。\nまた天気がいい日に出かけましょう。",
                Some("散歩の写真.zip · 8.2 MB"),
            ),
            (
                1,
                "打ち合わせのまとめ",
                "決定事項と次回までの確認項目です。",
                "火曜日",
                "鈴木 健",
                "ken@example.test",
                "本日はありがとうございました。\n\n話し合った内容をまとめます。\n\nまずは受信画面で、一覧から本文への移動が自然か確認します。次に、画面のサイズを変えたときも作業が続けられるようにします。\n\n次回は動くものを見ながら相談しましょう。",
                None,
            ),
            (
                0,
                "今月のお知らせ",
                "新しく入った本を紹介します。",
                "月曜日",
                "まちの図書館",
                "library@example.test",
                "こんにちは。\n\n今月も新しい本が届きました。読書の秋に、気になる一冊を見つけてみませんか。\n\n開館時間は午前9時から午後7時です。\nご来館をお待ちしています。",
                None,
            ),
            (
                1,
                "資料の確認をお願いします",
                "更新版を添付しました。お時間のあるときにご確認ください。",
                "10/18",
                "伊藤 あおい",
                "aoi@example.test",
                "お疲れさまです。\n\n資料を更新しました。数字の表記と見出しの位置を整えています。\n内容に問題がなければ、この版で進めたいと思います。\n\nよろしくお願いします。",
                Some("確認資料.pdf · 126 KB"),
            ),
        ];
        for (i, (account, subject, snippet, time, sender, address, body, attachment)) in
            samples.into_iter().enumerate()
        {
            let mut messages = vec![];
            if i == 1 {
                messages.push(Message { sender:"高橋 悠".into(),address:"yu@example.test".into(),recipient:"自分".into(),date:"10月23日 16:30".into(),body:"次のリリースについて、確認用の資料を作成しています。\n準備ができたら、こちらの会話で共有します。".into() });
            }
            messages.push(Message {
                sender: sender.into(),
                address: address.into(),
                recipient: "自分".into(),
                date: time.into(),
                body: body.into(),
            });
            threads.push(Thread {
                id: i + 1,
                account,
                subject: subject.into(),
                snippet: snippet.into(),
                time: time.into(),
                unread: i == 1 || i == 3,
                flagged: i == 1 || i == 4,
                folder: Folder::Inbox,
                messages,
                attachment: attachment.map(str::to_owned),
            });
        }
        Self {
            threads,
            accounts: Vec::new(),
            remote_ids: Default::default(),
            local_state: Default::default(),
            mailbox: Mailbox::default(),
            selected: Some(1),
            page: Page::List,
            expanded: BTreeSet::new(),
            folders_open: false,
            sidebar_hidden: false,
            sidebar_width: 188.0,
            list_width: 300.0,
            previous_page: Page::List,
        }
    }
    /// Disposable Linux UI data. These identities never authenticate to a service.
    #[cfg(target_os = "linux")]
    pub fn preview() -> Self {
        use crate::backend::{Account, Config, Protocol, Security};
        let mut m = Self::empty();
        for (i, email) in ["alex@example.test", "alex@work.example.test"]
            .into_iter()
            .enumerate()
        {
            m.accounts.push(Account {
                id: [(i + 1) as u8; 16],
                config: Config {
                    email: email.into(),
                    username: email.into(),
                    incoming_host: "imap.example.test".into(),
                    incoming_port: 993,
                    incoming_security: Security::Tls,
                    protocol: Protocol::Imap,
                    smtp_host: "smtp.example.test".into(),
                    smtp_port: 587,
                    smtp_security: Security::StartTls,
                },
            });
        }
        for (i,(account,sender,address,subject,body,time)) in [
            (0,"Jamie Lee","jamie@example.test","Lunch on Saturday?","Hi Alex,\n\nWould you like to meet for lunch on Saturday? I found a small cafe near the station.\n\nLet me know what time works for you.\nJamie","10:42"),
            (1,"Morgan Chen","morgan@work.example.test","Design review notes","Hi Alex,\n\nThanks for the review. I have updated the spacing and keyboard navigation.\n\nCould you take another look before our next meeting?\nMorgan","09:18"),
            (0,"The Bookshop","books@example.test","Your order is ready","Your order is ready to collect.\n\nWe are open from 10 am to 6 pm. See you soon!","Yesterday"),
            (1,"Taylor Reed","taylor@work.example.test","Plans for next week","Hello,\n\nHere is the plan for next week. We will start with the release checklist on Monday.\n\nHave a great weekend!","Yesterday"),
            (0,"Sam Park","sam@example.test","Weekend photos","Hi Alex,\n\nIt was great to catch up last weekend. Shall we plan another walk soon?\n\nSam","Tuesday"),
        ].into_iter().enumerate(){
            m.threads.push(Thread{id:i+1,account,subject:subject.into(),snippet:body.split_whitespace().collect::<Vec<_>>().join(" "),time:time.into(),unread:i==1||i==2,flagged:i==1,folder:Folder::Inbox,messages:vec![Message{sender:sender.into(),address:address.into(),recipient:m.accounts[account].config.email.clone(),date:time.into(),body:body.into()}],attachment:None});
        }
        m.selected = Some(1);
        m
    }
    pub fn empty() -> Self {
        Self {
            threads: vec![],
            accounts: vec![],
            remote_ids: Default::default(),
            local_state: Default::default(),
            mailbox: Mailbox::default(),
            selected: None,
            page: Page::List,
            expanded: BTreeSet::new(),
            folders_open: false,
            sidebar_hidden: false,
            sidebar_width: 188.0,
            list_width: 300.0,
            previous_page: Page::List,
        }
    }
    pub fn account_label(&self, index: usize) -> &str {
        #[cfg(test)]
        if self.accounts.is_empty() {
            return ACCOUNTS.get(index).copied().unwrap_or("");
        }
        self.accounts
            .get(index)
            .map_or("", |a| a.config.email.as_str())
    }
    pub fn merge(
        &mut self,
        account: usize,
        incoming: Vec<(String, Thread)>,
    ) -> crate::backend::Result<usize> {
        let mut decoded = vec![];
        let mut next = self.threads.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let mut keys = std::collections::BTreeMap::new();
        for (key, mut thread) in incoming {
            thread.account = account;
            thread.id = self
                .remote_ids
                .get(&(account, key.clone()))
                .copied()
                .unwrap_or_else(|| {
                    let id = next;
                    next += 1;
                    id
                });
            if self.local_state.contains(&thread.id) {
                if let Some(old) = self.thread(thread.id) {
                    // IMAP Seen is authoritative on the server even when
                    // archive/folder/flag organization is still session-local.
                    if self
                        .accounts
                        .get(account)
                        .is_none_or(|a| a.config.protocol != crate::backend::Protocol::Imap)
                    {
                        thread.unread = old.unread;
                    }
                    thread.flagged = old.flagged;
                    thread.folder = old.folder;
                }
            }
            keys.insert((account, key), thread.id);
            decoded.push(thread);
        }
        // Bound the entire live mailbox as well as each network response.
        let retained = self
            .threads
            .iter()
            .filter(|t| t.account != account || !self.remote_ids.values().any(|id| *id == t.id));
        let bytes = decoded
            .iter()
            .chain(retained.clone())
            .map(|t| {
                t.messages.iter().map(|m| m.body.len()).sum::<usize>()
                    + t.subject.len()
                    + t.snippet.len()
            })
            .sum::<usize>();
        if bytes > 64 * 1024 * 1024 || decoded.len() + retained.count() > 2000 {
            return Err(crate::backend::Error::Limit);
        }
        let count = decoded.len();
        self.threads
            .retain(|t| t.account != account || !self.remote_ids.values().any(|id| *id == t.id));
        decoded.append(&mut self.threads);
        self.threads = decoded;
        self.remote_ids.retain(|(a, _), _| *a != account);
        self.remote_ids.extend(keys);
        self.local_state
            .retain(|id| self.threads.iter().any(|t| t.id == *id));
        if self.selected.is_some_and(|id| self.thread(id).is_none()) {
            self.selected = None;
        }
        Ok(count)
    }
    pub fn visible(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        self.threads
            .iter()
            .filter(|t| self.mailbox.account.is_none_or(|a| a == t.account))
            .filter(|t| match self.mailbox.folder {
                Folder::Unread => t.folder == Folder::Inbox && t.unread,
                Folder::Flagged => !matches!(t.folder, Folder::Trash | Folder::Drafts) && t.flagged,
                folder => t.folder == folder,
            })
            .filter(|t| {
                query.is_empty()
                    || format!(
                        "{} {} {} {}",
                        t.subject,
                        t.snippet,
                        t.messages.last().unwrap().sender,
                        t.messages
                            .iter()
                            .map(|m| m.body.as_str())
                            .collect::<String>()
                    )
                    .to_lowercase()
                    .contains(&query)
            })
            .map(|t| t.id)
            .collect()
    }
    pub fn thread(&self, id: usize) -> Option<&Thread> {
        self.threads.iter().find(|t| t.id == id)
    }
    pub fn imap_identity(
        &self,
        id: usize,
    ) -> Option<(crate::backend::Account, crate::protocol::ImapMessageId)> {
        let thread = self.thread(id)?;
        let account = self.accounts.get(thread.account)?;
        if account.config.protocol != crate::backend::Protocol::Imap {
            return None;
        }
        let key = self
            .remote_ids
            .iter()
            .find(|((a, _), value)| *a == thread.account && **value == id)?
            .0
            .1
            .as_str();
        Some((
            account.clone(),
            crate::protocol::ImapMessageId::from_key(key)?,
        ))
    }
    pub fn confirm_read(
        &mut self,
        account_id: [u8; 16],
        message: crate::protocol::ImapMessageId,
        unread: bool,
    ) -> bool {
        let Some(account) = self.accounts.iter().position(|a| a.id == account_id) else {
            return false;
        };
        let Some(id) = self.remote_ids.get(&(account, message.key())).copied() else {
            return false;
        };
        let Some(thread) = self.threads.iter_mut().find(|t| t.id == id) else {
            return false;
        };
        thread.unread = unread;
        true
    }
    pub fn open(&mut self, id: usize) {
        let remote = self.imap_identity(id).is_some();
        if let Some(t) = self.threads.iter_mut().find(|t| t.id == id) {
            if !remote {
                t.unread = false;
            }
            self.selected = Some(id);
            self.page = Page::Reader;
        }
    }
    pub fn switch(&mut self, mailbox: Mailbox) {
        self.mailbox = mailbox;
        let ids = self.visible("");
        if self.selected.is_some_and(|id| !ids.contains(&id)) {
            self.selected = None;
        }
        self.folders_open = false;
        self.page = Page::List;
    }
    pub fn unread(&self, account: Option<usize>) -> usize {
        self.threads
            .iter()
            .filter(|t| {
                t.unread && t.folder == Folder::Inbox && account.is_none_or(|a| t.account == a)
            })
            .count()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_filtering_and_account_selection_do_not_mix_folders() {
        let mut m = Model::sample();
        assert_eq!(m.visible("").len(), 8);
        assert_eq!(m.visible("リリース"), vec![2]);
        m.switch(Mailbox {
            account: Some(1),
            folder: Folder::Inbox,
        });
        assert_eq!(m.visible("").len(), 4);
        m.switch(Mailbox {
            account: None,
            folder: Folder::Unread,
        });
        assert_eq!(m.visible("").len(), 2);
        m.open(2);
        assert_eq!(m.visible(""), vec![4]);
        assert_eq!(m.selected, Some(2));
        m.switch(Mailbox {
            account: None,
            folder: Folder::Archive,
        });
        assert!(m.visible("").is_empty());
    }
}
