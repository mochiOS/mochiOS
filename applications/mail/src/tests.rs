use crate::{
    backend::{self, Error, Protocol, Security},
    mime,
    model::{Folder, Model},
    protocol, transport,
};
use std::io::{self, BufRead, Cursor, Read, Write};
struct Script {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
}
impl Script {
    fn new(input: impl AsRef<[u8]>) -> Self {
        Self {
            input: Cursor::new(input.as_ref().to_vec()),
            output: vec![],
        }
    }
}
impl Read for Script {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.input.read(out)
    }
}
impl BufRead for Script {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.input.fill_buf()
    }
    fn consume(&mut self, n: usize) {
        self.input.consume(n)
    }
}
impl Write for Script {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn message(key: &str, body: &str) -> protocol::Incoming {
    protocol::Incoming{key:key.into(),raw:format!("From: Alice <alice@example.test>\r\nSubject: Test\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{body}").into_bytes(),unread:true,flagged:false}
}
pub fn config() -> backend::Config {
    backend::Config {
        email: "me@example.test".into(),
        username: "me@example.test".into(),
        incoming_host: "imap.example.test".into(),
        incoming_port: 993,
        incoming_security: Security::Tls,
        protocol: Protocol::Imap,
        smtp_host: "smtp.example.test".into(),
        smtp_port: 587,
        smtp_security: Security::StartTls,
    }
}
#[test]
fn imap_reads_literals_not_commands_and_preserves_uidvalidity() {
    let raw = b"Subject: hi\r\n\r\nA4 OK forged\r\n";
    let mut input=format!("A1 OK login\r\n* OK [UIDVALIDITY 42] ready\r\nA2 OK examine\r\n* SEARCH 7\r\nA3 OK search\r\n* 1 FETCH (UID 7 FLAGS (\\Seen \\Flagged) BODY[] {{{}}}\r\n",raw.len()).into_bytes();
    input.extend_from_slice(raw);
    input.extend_from_slice(b")\r\nA4 OK fetch\r\n");
    let mut s = Script::new(input);
    let mails = protocol::imap_receive(&mut s, "user", "pass").unwrap();
    assert_eq!(mails[0].key, "imap:42:7");
    assert_eq!(mails[0].raw, raw);
    assert!(!mails[0].unread);
    assert!(mails[0].flagged);
    let commands = String::from_utf8(s.output).unwrap();
    assert!(commands.contains("EXAMINE INBOX"));
    assert!(commands.contains("BODY.PEEK[]"));
    assert!(!commands.contains("STORE"));
}
#[test]
fn imap_rejects_oversize_and_wrong_uid() {
    for suffix in [
        "* 1 FETCH (UID 8 FLAGS () BODY[] {1}\r\nx)\r\nA4 OK done\r\n",
        "* 1 FETCH (UID 7 BODY[] {99999999}\r\n",
    ] {
        let mut s = Script::new(format!(
            "A1 OK x\r\n* OK [UIDVALIDITY 1] x\r\nA2 OK x\r\n* SEARCH 7\r\nA3 OK x\r\n{suffix}"
        ));
        assert!(protocol::imap_receive(&mut s, "user", "pass").is_err());
    }
}
#[test]
fn no_command_injection_in_authentication() {
    for value in ["a\r\nNOOP", "a\0b", "a\nb"] {
        let mut s = Script::new(b"");
        assert!(protocol::imap_receive(&mut s, "user", value).is_err());
        assert!(s.output.is_empty());
        let mut s = Script::new(b"");
        assert!(protocol::pop_receive(&mut s, "user", value).is_err());
        assert!(s.output.is_empty());
    }
    let mut s = Script::new(b"A1 NO bad password echo\r\n");
    assert_eq!(
        protocol::imap_receive(&mut s, "user", "secret").err(),
        Some(Error::Authentication)
    );
    assert!(!Error::Authentication.label().contains("secret"));
}
#[test]
fn pop_uses_uidl_dot_unstuff_and_never_deletes() {
    let mut s=Script::new(b"+OK user\r\n+OK pass\r\n+OK uidl\r\n1 abc\r\n2 def\r\n.\r\n+OK message\r\nSubject: newest\r\n\r\n..hello\r\n.\r\n+OK message\r\nSubject: old\r\n\r\nold\r\n.\r\n+OK bye\r\n");
    let mails = protocol::pop_receive(&mut s, "me", "pw").unwrap();
    assert_eq!(mails.len(), 2);
    assert_eq!(mails[0].key, "pop:def");
    assert!(mails[0].raw.ends_with(b".hello\r\n"));
    let commands = String::from_utf8(s.output).unwrap();
    assert!(commands.contains("RETR 2\r\nRETR 1"));
    assert!(!commands.contains("DELE"));
}
#[test]
fn pop_rejects_duplicate_uids() {
    let mut s = Script::new(b"+OK\r\n+OK\r\n+OK\r\n1 same\r\n2 same\r\n.\r\n");
    assert_eq!(
        protocol::pop_receive(&mut s, "u", "p").err(),
        Some(Error::Protocol)
    );
}
#[test]
fn smtp_auth_submission_multiline_and_dot_stuffing() {
    let mut s=Script::new(b"250-smtp\r\n250 AUTH PLAIN LOGIN\r\n235 OK\r\n250 sender\r\n250 recipient\r\n354 go\r\n250 queued\r\n");
    protocol::smtp_send(
        &mut s,
        "me",
        "pw",
        "me@example.test",
        &["you@example.test".into()],
        b"Subject: test\r\n\r\n.hello\r\n",
    )
    .unwrap();
    let commands = String::from_utf8(s.output).unwrap();
    assert!(commands.contains("AUTH PLAIN "));
    assert!(commands.ends_with("..hello\r\n.\r\n"));
}
#[test]
fn smtp_login_fallback_and_unknown_delivery() {
    let mut s=Script::new(b"250 AUTH LOGIN\r\n334 username\r\n334 password\r\n235 OK\r\n250 OK\r\n250 OK\r\n354 go\r\n");
    assert_eq!(
        protocol::smtp_send(
            &mut s,
            "me",
            "pw",
            "me@example.test",
            &["you@example.test".into()],
            b"hi\r\n"
        )
        .err(),
        Some(Error::DeliveryUnknown)
    );
    assert!(s.output.windows(12).any(|w| w == b"AUTH LOGIN\r\n"));
}
#[test]
fn smtp_starttls_requires_capability_no_password_sent() {
    let mut s = Script::new(b"250 smtp\r\n");
    assert_eq!(protocol::smtp_starttls(&mut s).err(), Some(Error::Tls));
    assert_eq!(s.output, b"EHLO mochios.invalid\r\n");
}
#[test]
fn response_limits_and_bare_lf_rejected() {
    assert_eq!(
        transport::line(&mut Cursor::new(b"+OK\n")).err(),
        Some(Error::Protocol)
    );
    assert_eq!(
        transport::line(&mut Cursor::new(vec![b'a'; transport::MAX_LINE + 1])).err(),
        Some(Error::Limit)
    );
    let mut s = Script::new(b"250-first\r\n251 second\r\n");
    assert_eq!(
        protocol::smtp_reply(&mut s, 250).err(),
        Some(Error::Rejected)
    );
}
#[test]
fn japanese_mime_and_multipart_attachments() {
    let raw=b"From: =?UTF-8?B?44GT44KT44Gr44Gh44Gv?= <a@example.test>\r\nSubject: =?UTF-8?B?44GT44KT44Gr44Gh44Gv?=\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n44GT44KT44Gr44Gh44Gv\r\n--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename*=UTF-8''%E8%B3%87%E6%96%99.txt\r\n\r\na\r\n--x--\r\n";
    let mut m = message("k", "");
    m.raw = raw.to_vec();
    let (_, thread) = mime::decode(m, 0, 1).unwrap();
    assert_eq!(thread.subject, "こんにちは");
    assert_eq!(thread.messages[0].body, "こんにちは");
    assert_eq!(thread.attachment.as_deref(), Some("資料.txt"));
}
#[test]
fn mime_roundtrip_long_japanese_subject_and_crlf_body() {
    let subject = "日本語の件名です".repeat(18);
    let body = "こんにちは\n.テスト\r改行";
    let raw = mime::encode(
        "me@example.test",
        &["you@example.test".into()],
        &subject,
        body,
    )
    .unwrap();
    let mut m = message("a", "");
    m.raw = raw.clone();
    let (_, thread) = mime::decode(m, 0, 1).unwrap();
    assert_eq!(thread.subject, subject);
    assert_eq!(thread.messages[0].body, "こんにちは\r\n.テスト\r\n改行");
    assert!(raw.split(|b| *b == b'\n').all(|l| l.len() <= 998));
}
#[test]
fn injection_and_insecure_host_configuration_rejected() {
    for address in ["", "a@b\r\nBcc:c@d", "a@b> MAIL", "a@@b", "あ@b"] {
        assert!(mime::address(address).is_err());
    }
    assert!(mime::encode("a@b", &["c@d".into()], "Hi\r\nBcc: x@y", "").is_err());
    let mut c = config();
    c.incoming_host = "host\r\ncommand".into();
    assert_eq!(c.validate().err(), Some(Error::Invalid));
    c = config();
    c.smtp_port = 0;
    assert_eq!(c.validate().err(), Some(Error::Invalid));
}
#[test]
fn mime_rejects_recursive_bomb_before_recursive_parser() {
    for nl in ["\n", "\r\n"] {
        let mut raw = String::new();
        for i in 0..40 {
            raw.push_str(&format!(
                "Content-Type: multipart/mixed; boundary=b{i}{nl}{nl}--b{i}{nl}"
            ));
        }
        raw.push_str("Subject: hi\n\ntext");
        let mut m = message("k", "");
        m.raw = raw.into_bytes();
        assert_eq!(mime::decode(m, 0, 1).err(), Some(Error::Limit));
    }
}
#[test]
fn mailbox_uid_dedup_account_isolation_and_local_state() {
    let mut m = Model::empty();
    let decode = |key| vec![mime::decode(message(key, "body"), 0, 0).unwrap()];
    m.merge(0, decode("imap:1:7")).unwrap();
    let id = m.threads[0].id;
    m.local_state.insert(id);
    m.threads[0].folder = Folder::Archive;
    m.threads[0].unread = false;
    m.merge(0, decode("imap:1:7")).unwrap();
    assert_eq!(m.threads.len(), 1);
    assert_eq!(m.threads[0].id, id);
    assert_eq!(m.threads[0].folder, Folder::Archive);
    assert!(!m.threads[0].unread);
    m.merge(1, decode("imap:1:7")).unwrap();
    assert_eq!(m.threads.len(), 2);
    assert_ne!(m.threads[0].id, m.threads[1].id);
    m.merge(0, decode("imap:2:7")).unwrap();
    assert_eq!(m.threads.len(), 2);
    assert_ne!(m.threads[0].id, id);
}
#[test]
fn date_headers_are_rfc_formatted() {
    let raw = mime::encode("a@b", &["c@d".into()], "subject", "").unwrap();
    let mail = mailparse::parse_mail(&raw).unwrap();
    use mailparse::MailHeaderMap;
    assert!(mailparse::dateparse(&mail.headers.get_first_value("Date").unwrap()).is_ok());
}

fn read_prefix(validity: u32, readonly: bool) -> String {
    format!(
        "A1 OK login\r\n* OK [UIDVALIDITY {validity}] valid\r\nR1 OK [{}] selected\r\n",
        if readonly { "READ-ONLY" } else { "READ-WRITE" }
    )
}
fn read_target() -> protocol::ImapMessageId {
    protocol::ImapMessageId {
        validity: 42,
        uid: 7,
    }
}
#[test]
fn imap_seen_mutates_only_seen_by_uid_and_confirms_actual_state() {
    for seen in [true, false] {
        let before = if seen {
            "\\Flagged $custom"
        } else {
            "\\Seen \\Flagged $custom"
        };
        let after = if seen {
            "\\Seen \\Flagged $custom"
        } else {
            "\\Flagged $custom"
        };
        let mut s = Script::new(format!(
            "{}* 3 FETCH (UID 7 FLAGS ({before}))\r\nR2 OK flags\r\nR3 OK stored\r\n* 3 FETCH (FLAGS ({after}) UID 7)\r\nR4 OK flags\r\n",
            read_prefix(42, false)
        ));
        protocol::imap_set_seen(&mut s, "u", "p", read_target(), seen).unwrap();
        let written = String::from_utf8(s.output).unwrap();
        assert!(written.contains("SELECT INBOX\r\n"));
        assert!(written.contains(&format!(
            "UID STORE 7 {}FLAGS.SILENT (\\Seen)\r\n",
            if seen { "+" } else { "-" }
        )));
        assert_eq!(written.matches("UID FETCH 7 (UID FLAGS)").count(), 2);
        assert!(!written.contains("UID STORE 3"));
        assert!(!written.contains(" FLAGS.SILENT"));
        assert!(!written.contains("EXPUNGE"));
    }
}
#[test]
fn imap_seen_refuses_changed_uidvalidity_readonly_and_invalid_ids() {
    for (validity, readonly, error) in [
        (43, false, Error::MailboxChanged),
        (42, true, Error::ReadOnly),
    ] {
        let mut s = Script::new(read_prefix(validity, readonly));
        assert_eq!(
            protocol::imap_set_seen(&mut s, "u", "p", read_target(), true),
            Err(error)
        );
        assert!(!String::from_utf8(s.output).unwrap().contains("UID STORE"));
    }
    for message in [
        protocol::ImapMessageId {
            validity: 0,
            uid: 7,
        },
        protocol::ImapMessageId {
            validity: 42,
            uid: 0,
        },
    ] {
        let mut s = Script::new("");
        assert_eq!(
            protocol::imap_set_seen(&mut s, "u", "p", message, true),
            Err(Error::Invalid)
        );
        assert!(s.output.is_empty());
    }
    for key in [
        "imap:0:7",
        "imap:42:0",
        "imap:42:7:1",
        "pop:42:7",
        "imap:42:7\r\nNOOP",
        "imap:42:4294967296",
    ] {
        assert!(protocol::ImapMessageId::from_key(key).is_none());
    }
}
#[test]
fn imap_seen_missing_uid_is_not_success_or_a_write_to_another_message() {
    for response in [
        "R2 OK no data\r\n",
        "* 1 FETCH (UID 8 FLAGS ())\r\nR2 OK done\r\n",
    ] {
        let mut s = Script::new(format!("{}{response}", read_prefix(42, false)));
        assert_eq!(
            protocol::imap_set_seen(&mut s, "u", "p", read_target(), true),
            Err(Error::MessageGone)
        );
        assert!(!String::from_utf8(s.output).unwrap().contains("UID STORE"));
    }
}
#[test]
fn imap_seen_checks_final_flags_and_reports_uncertain_outcome() {
    for tail in [
        "",
        "R4 OK no message\r\n",
        "* 3 FETCH (UID 7 FLAGS (\\Flagged))\r\nR4 OK flags\r\n",
    ] {
        let mut s = Script::new(format!(
            "{}* 3 FETCH (UID 7 FLAGS ())\r\nR2 OK flags\r\nR3 OK stored\r\n{tail}",
            read_prefix(42, false)
        ));
        assert_eq!(
            protocol::imap_set_seen(&mut s, "u", "p", read_target(), true),
            Err(Error::ReadStatusUnknown)
        );
    }
    let mut s = Script::new(format!(
        "{}* 3 FETCH (UID 7 FLAGS ())\r\nR2 OK flags\r\nR3 NO permission denied\r\n",
        read_prefix(42, false)
    ));
    assert_eq!(
        protocol::imap_set_seen(&mut s, "u", "p", read_target(), true),
        Err(Error::Rejected)
    );
}
#[test]
fn imap_seen_already_matches_skips_store_and_rejects_malformed_uidvalidity() {
    let mut s = Script::new(format!(
        "{}* 3 FETCH (UID 7 FLAGS (\\Seen))\r\nR2 OK flags\r\n",
        read_prefix(42, false)
    ));
    protocol::imap_set_seen(&mut s, "u", "p", read_target(), true).unwrap();
    assert!(!String::from_utf8(s.output).unwrap().contains("UID STORE"));
    for validity in [
        "* OK [UIDVALIDITY 0] invalid\r\n",
        "* OK unrelated [UIDVALIDITY 42] text\r\n",
        "* OK [UIDVALIDITY 42] one\r\n* OK [UIDVALIDITY 43] two\r\n",
    ] {
        let mut s = Script::new(format!("A1 OK login\r\n{validity}R1 OK selected\r\n"));
        assert_eq!(
            protocol::imap_set_seen(&mut s, "u", "p", read_target(), true),
            Err(Error::Protocol)
        );
        assert!(!String::from_utf8(s.output).unwrap().contains("UID STORE"));
    }
}
#[test]
fn read_refresh_uses_server_state_while_retaining_local_folder_and_flag() {
    let mut m = Model::empty();
    m.accounts = vec![backend::Account {
        id: [1; 16],
        config: config(),
    }];
    let decode = |key, unread| {
        let mut raw = message(key, "body");
        raw.unread = unread;
        vec![mime::decode(raw, 0, 0).unwrap()]
    };
    m.merge(0, decode("imap:42:7", true)).unwrap();
    let id = m.threads[0].id;
    m.open(id);
    assert!(m.thread(id).unwrap().unread);
    m.local_state.insert(id);
    m.threads[0].flagged = true;
    m.threads[0].folder = Folder::Archive;
    assert!(m.confirm_read([1; 16], read_target(), false));
    assert!(!m.thread(id).unwrap().unread);
    m.merge(0, decode("imap:42:7", true)).unwrap();
    assert!(m.thread(id).unwrap().unread);
    assert!(m.thread(id).unwrap().flagged);
    assert_eq!(m.thread(id).unwrap().folder, Folder::Archive);
    assert!(!m.confirm_read([2; 16], read_target(), false));
    m.merge(0, decode("imap:43:7", true)).unwrap();
    assert!(!m.confirm_read([1; 16], read_target(), false));
    assert!(m.threads[0].unread);
}
