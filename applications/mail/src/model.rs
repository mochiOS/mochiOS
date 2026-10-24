//! In-memory sample mailbox. No account credentials, network or delivery are used.
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
            Self::Inbox => "受信トレイ",
            Self::Unread => "未読",
            Self::Flagged => "フラグ付き",
            Self::Drafts => "下書き",
            Self::Archive => "アーカイブ",
            Self::Sent => "送信済み",
            Self::Trash => "ゴミ箱",
        }
    }
}
#[derive(Clone)]
pub struct Message {
    pub sender: &'static str,
    pub address: &'static str,
    pub date: &'static str,
    pub body: &'static str,
}
#[derive(Clone)]
pub struct Thread {
    pub id: usize,
    pub account: usize,
    pub subject: &'static str,
    pub snippet: &'static str,
    pub time: &'static str,
    pub unread: bool,
    pub flagged: bool,
    pub folder: Folder,
    pub messages: Vec<Message>,
    pub attachment: Option<&'static str>,
}
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
                messages.push(Message { sender:"高橋 悠",address:"yu@example.test",date:"10月23日 16:30",body:"次のリリースについて、確認用の資料を作成しています。\n準備ができたら、こちらの会話で共有します。" });
            }
            messages.push(Message {
                sender,
                address,
                date: time,
                body,
            });
            threads.push(Thread {
                id: i + 1,
                account,
                subject,
                snippet,
                time,
                unread: i == 1 || i == 3,
                flagged: i == 1 || i == 4,
                folder: Folder::Inbox,
                messages,
                attachment,
            });
        }
        Self {
            threads,
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
                        t.messages.iter().map(|m| m.body).collect::<String>()
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
    pub fn open(&mut self, id: usize) {
        if let Some(t) = self.threads.iter_mut().find(|t| t.id == id) {
            t.unread = false;
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
