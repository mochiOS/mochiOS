//! IMAP UID receive, POP3 UIDL receive, and authenticated SMTP submission.
//! Commands are never logged; errors contain no server text or credentials.
use crate::backend::{Error, Result};
use crate::transport::{MAX_MESSAGE, MAX_SESSION, command, line};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::io::{BufRead, Write};
use zeroize::Zeroizing;

pub struct Incoming {
    pub key: String,
    pub raw: Vec<u8>,
    pub unread: bool,
    pub flagged: bool,
}
fn ascii(line: &[u8]) -> Result<&str> {
    std::str::from_utf8(line).map_err(|_| Error::Protocol)
}
pub fn single(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 4096 || value.bytes().any(|b| b.is_ascii_control()) {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn quoted(value: &str) -> Result<Zeroizing<String>> {
    single(value)?;
    let mut out = Zeroizing::new(String::from("\""));
    for c in value.chars() {
        if matches!(c, '\\' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    Ok(out)
}
struct Response {
    lines: Vec<String>,
    literals: Vec<Vec<u8>>,
    completion: String,
}
/// Literals are read by length before recognizing tagged responses: message text
/// cannot masquerade as a server completion or inject another response.
fn imap<S: BufRead + Write>(s: &mut S, tag: &str, cmd: &str) -> Result<Response> {
    let tagged = Zeroizing::new(format!("{tag} {cmd}"));
    command(s, &tagged)?;
    let mut response = Response {
        lines: vec![],
        literals: vec![],
        completion: String::new(),
    };
    let mut total = 0;
    loop {
        let bytes = line(s)?;
        total += bytes.len();
        if total > MAX_SESSION || response.lines.len() > 10000 {
            return Err(Error::Limit);
        }
        let text = ascii(&bytes)?;
        if let Some(rest) = text.strip_prefix(&format!("{tag} ")) {
            if !rest
                .split_whitespace()
                .next()
                .is_some_and(|x| x.eq_ignore_ascii_case("OK"))
            {
                return Err(Error::Rejected);
            }
            response.completion = rest.to_owned();
            return Ok(response);
        }
        if !text.starts_with('*') && !text.starts_with(')') && !text.is_empty() {
            return Err(Error::Protocol);
        }
        if text.starts_with("* BYE") {
            return Err(Error::Rejected);
        }
        if text.ends_with('}') {
            let n = text
                .rsplit_once('{')
                .ok_or(Error::Protocol)?
                .1
                .trim_end_matches('}')
                .parse::<usize>()
                .map_err(|_| Error::Protocol)?;
            if n > MAX_MESSAGE || total.checked_add(n).is_none_or(|n| n > MAX_SESSION) {
                return Err(Error::Limit);
            }
            total += n;
            let mut data = vec![0; n];
            s.read_exact(&mut data).map_err(|_| Error::Network)?;
            response.literals.push(data);
        }
        response.lines.push(text.to_owned());
    }
}
pub fn imap_greeting<S: BufRead>(s: &mut S) -> Result<()> {
    if !ascii(&line(s)?)?.starts_with("* OK") {
        return Err(Error::Protocol);
    }
    Ok(())
}
pub fn imap_starttls<S: BufRead + Write>(s: &mut S) -> Result<()> {
    imap(s, "T0", "STARTTLS").map(|_| ())
}
fn imap_login<S: BufRead + Write>(s: &mut S, user: &str, password: &str) -> Result<()> {
    let login = Zeroizing::new(format!(
        "LOGIN {} {}",
        quoted(user)?.as_str(),
        quoted(password)?.as_str()
    ));
    imap(s, "A1", &login).map_err(|_| Error::Authentication)?;
    Ok(())
}
fn uid_validity(response: &Response) -> Result<u32> {
    let mut value = None;
    for line in &response.lines {
        let upper = line.to_ascii_uppercase();
        if let Some(rest) = upper.strip_prefix("* OK [UIDVALIDITY ") {
            let (number, _) = rest.split_once(']').ok_or(Error::Protocol)?;
            let parsed = number
                .parse::<u32>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or(Error::Protocol)?;
            if value.replace(parsed).is_some() {
                return Err(Error::Protocol);
            }
        }
    }
    value.ok_or(Error::Protocol)
}
/// A message identifier is scoped to INBOX and one account, never a sequence number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImapMessageId {
    pub validity: u32,
    pub uid: u32,
}
impl ImapMessageId {
    pub fn from_key(key: &str) -> Option<Self> {
        let mut parts = key.split(':');
        if parts.next() != Some("imap") {
            return None;
        }
        let validity = parts.next()?.parse::<u32>().ok().filter(|n| *n > 0)?;
        let uid = parts.next()?.parse::<u32>().ok().filter(|n| *n > 0)?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self { validity, uid })
    }
    pub fn key(self) -> String {
        format!("imap:{}:{}", self.validity, self.uid)
    }
}
/// Only supported system flags can be written; arbitrary command text is not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImapFlag {
    Seen,
    Flagged,
}
impl ImapFlag {
    fn wire(self) -> &'static str {
        match self {
            Self::Seen => "\\Seen",
            Self::Flagged => "\\Flagged",
        }
    }
    fn unknown(self) -> Error {
        match self {
            Self::Seen => Error::ReadStatusUnknown,
            Self::Flagged => Error::FlagStatusUnknown,
        }
    }
}
fn check_permanent_flag(response: &Response, flag: ImapFlag) -> Result<()> {
    // Missing PERMANENTFLAGS means all flags are permanent for older servers.
    // The \* permission covers new keywords, not the predefined \Flagged flag.
    if flag != ImapFlag::Flagged {
        return Ok(());
    }
    let mut allowed = None;
    for line in &response.lines {
        let upper = line.to_ascii_uppercase();
        if let Some(rest) = upper.strip_prefix("* OK [PERMANENTFLAGS ") {
            let list = rest
                .strip_prefix('(')
                .and_then(|r| r.split_once(")]").map(|(l, _)| l))
                .ok_or(Error::Protocol)?;
            if list.contains(['(', ')']) {
                return Err(Error::Protocol);
            }
            let supports = list
                .split_whitespace()
                .any(|atom| atom.eq_ignore_ascii_case(flag.wire()));
            if allowed.replace(supports).is_some() {
                return Err(Error::Protocol);
            }
        }
    }
    if allowed == Some(false) {
        return Err(Error::FlagNotPermanent);
    }
    Ok(())
}
fn fetch_flag(response: &Response, expected: u32, target: ImapFlag) -> Result<bool> {
    let mut value = None;
    if !response.literals.is_empty() {
        return Err(Error::Protocol);
    }
    for line in &response.lines {
        let upper = line.to_ascii_uppercase();
        let Some((prefix, fields)) = upper.split_once(" FETCH (") else {
            continue;
        };
        let mut prefix = prefix.split_whitespace();
        if prefix.next() != Some("*")
            || prefix
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|n| *n > 0)
                .is_none()
            || prefix.next().is_some()
        {
            return Err(Error::Protocol);
        }
        // FETCH here requests only UID and FLAGS. Reject malformed/duplicate
        // attributes instead of accepting UID-like text inside an extension.
        let fields = fields.strip_suffix(')').ok_or(Error::Protocol)?;
        let mut tokens = fields.split_whitespace();
        let mut uid = None;
        let mut flags = None;
        while let Some(token) = tokens.next() {
            match token {
                "UID" => {
                    if uid.is_some() {
                        return Err(Error::Protocol);
                    }
                    uid = Some(
                        tokens
                            .next()
                            .and_then(|n| n.parse::<u32>().ok())
                            .filter(|n| *n > 0)
                            .ok_or(Error::Protocol)?,
                    );
                }
                "FLAGS" => {
                    if flags.is_some() {
                        return Err(Error::Protocol);
                    }
                    let first = tokens.next().ok_or(Error::Protocol)?;
                    if !first.starts_with('(') {
                        return Err(Error::Protocol);
                    }
                    let mut flag = first.strip_prefix('(').unwrap();
                    let mut has_flag = false;
                    loop {
                        let end = flag.ends_with(')');
                        let atom = if end { &flag[..flag.len() - 1] } else { flag };
                        if atom.contains(['(', ')']) {
                            return Err(Error::Protocol);
                        }
                        has_flag |= atom.eq_ignore_ascii_case(target.wire());
                        if end {
                            break;
                        }
                        flag = tokens.next().ok_or(Error::Protocol)?;
                    }
                    flags = Some(has_flag);
                }
                _ => return Err(Error::Protocol),
            }
        }
        if uid == Some(expected) {
            value = Some(flags.ok_or(Error::Protocol)?);
        }
    }
    value.ok_or(Error::MessageGone)
}
/// Add/remove one system flag without replacing other flags. UID FETCH confirms the
/// actual state; UID STORE may succeed without modifying an expunged message.
pub fn imap_set_flag<S: BufRead + Write>(
    s: &mut S,
    user: &str,
    password: &str,
    message: ImapMessageId,
    flag: ImapFlag,
    enabled: bool,
) -> Result<()> {
    if message.validity == 0 || message.uid == 0 {
        return Err(Error::Invalid);
    }
    imap_login(s, user, password)?;
    let selected = imap(s, "R1", "SELECT INBOX")?;
    if uid_validity(&selected)? != message.validity {
        return Err(Error::MailboxChanged);
    }
    if selected
        .completion
        .to_ascii_uppercase()
        .starts_with("OK [READ-ONLY]")
    {
        return Err(Error::ReadOnly);
    }
    check_permanent_flag(&selected, flag)?;
    // Fetch first as well: a missing or mismatched UID must cause no mutation.
    let current = imap(s, "R2", &format!("UID FETCH {} (UID FLAGS)", message.uid))?;
    check_permanent_flag(&current, flag)?;
    if fetch_flag(&current, message.uid, flag)? == enabled {
        return Ok(());
    }
    let result = imap(
        s,
        "R3",
        &format!(
            "UID STORE {} {}FLAGS.SILENT ({})",
            message.uid,
            if enabled { "+" } else { "-" },
            flag.wire()
        ),
    );
    let stored = result.map_err(|e| match e {
        Error::Rejected => e,
        _ => flag.unknown(),
    })?;
    check_permanent_flag(&stored, flag)?;
    let verified = imap(s, "R4", &format!("UID FETCH {} (UID FLAGS)", message.uid))
        .map_err(|_| flag.unknown())?;
    check_permanent_flag(&verified, flag)?;
    if fetch_flag(&verified, message.uid, flag).map_err(|_| flag.unknown())? != enabled {
        return Err(flag.unknown());
    }
    Ok(())
}
// Keep the existing read operation as a thin wrapper over the shared engine.
pub fn imap_set_seen<S: BufRead + Write>(
    s: &mut S,
    user: &str,
    password: &str,
    message: ImapMessageId,
    seen: bool,
) -> Result<()> {
    imap_set_flag(s, user, password, message, ImapFlag::Seen, seen)
}
pub fn imap_receive<S: BufRead + Write>(
    s: &mut S,
    user: &str,
    password: &str,
) -> Result<Vec<Incoming>> {
    imap_login(s, user, password)?;
    let selected = imap(s, "A2", "EXAMINE INBOX")?;
    let validity = uid_validity(&selected)?;
    let search = imap(s, "A3", "UID SEARCH ALL")?;
    let mut uids = Vec::new();
    for text in search
        .lines
        .iter()
        .filter_map(|s| s.strip_prefix("* SEARCH"))
    {
        for uid in text.split_whitespace() {
            let n = uid.parse::<u32>().map_err(|_| Error::Protocol)?;
            if n == 0 {
                return Err(Error::Protocol);
            }
            uids.push(n);
            if uids.len() > 100000 {
                return Err(Error::Limit);
            }
        }
    }
    uids.sort_unstable();
    if uids.windows(2).any(|w| w[0] == w[1]) {
        return Err(Error::Protocol);
    }
    let mut out = vec![];
    let mut total = 0;
    // Newest 100 messages, bounded memory, no Seen side effect (BODY.PEEK).
    for uid in uids.into_iter().rev().take(100) {
        let fetched = imap(s, "A4", &format!("UID FETCH {uid} (UID FLAGS BODY.PEEK[])"))?;
        if fetched.literals.is_empty() {
            continue;
        } // expunged between SEARCH and FETCH
        if fetched.literals.len() != 1 {
            return Err(Error::Protocol);
        }
        let info = fetched
            .lines
            .iter()
            .find(|l| l.to_ascii_uppercase().contains(" FETCH "))
            .ok_or(Error::Protocol)?;
        let upper = info.to_ascii_uppercase();
        let returned_uid = upper
            .split_once("UID ")
            .and_then(|(_, x)| x.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|x| x.parse::<u32>().ok());
        if returned_uid != Some(uid) {
            return Err(Error::Protocol);
        }
        let flag_text = upper
            .match_indices("FLAGS (")
            .find(|(i, _)| *i == 0 || matches!(upper.as_bytes()[i - 1], b' ' | b'('))
            .and_then(|(i, _)| upper[i + 7..].split_once(')').map(|(flags, _)| flags))
            .ok_or(Error::Protocol)?;
        if flag_text.contains('(') {
            return Err(Error::Protocol);
        }
        let is_set = |flag: &str| flag_text.split_whitespace().any(|atom| atom == flag);
        let raw = fetched.literals.into_iter().next().unwrap();
        total += raw.len();
        if total > MAX_SESSION {
            return Err(Error::Limit);
        }
        out.push(Incoming {
            key: format!("imap:{validity}:{uid}"),
            raw,
            unread: !is_set("\\SEEN"),
            flagged: is_set("\\FLAGGED"),
        });
    }
    Ok(out)
}
fn pop_ok<S: BufRead>(s: &mut S) -> Result<()> {
    let bytes = line(s)?;
    if bytes == b"+OK" || bytes.starts_with(b"+OK ") {
        Ok(())
    } else {
        Err(Error::Rejected)
    }
}
pub fn pop_greeting<S: BufRead>(s: &mut S) -> Result<()> {
    pop_ok(s)
}
pub fn pop_starttls<S: BufRead + Write>(s: &mut S) -> Result<()> {
    command(s, "STLS")?;
    pop_ok(s)
}
fn multiline<S: BufRead>(s: &mut S, limit: usize) -> Result<Vec<u8>> {
    let mut out = vec![];
    loop {
        let bytes = line(s)?;
        if bytes == b"." {
            return Ok(out);
        }
        let bytes = if bytes.starts_with(b"..") {
            &bytes[1..]
        } else {
            &bytes[..]
        };
        if out.len() + bytes.len() + 2 > limit {
            return Err(Error::Limit);
        }
        out.extend_from_slice(bytes);
        out.extend_from_slice(b"\r\n");
    }
}
pub fn pop_receive<S: BufRead + Write>(
    s: &mut S,
    user: &str,
    password: &str,
) -> Result<Vec<Incoming>> {
    single(user)?;
    single(password)?;
    command(s, &format!("USER {user}"))?;
    pop_ok(s).map_err(|_| Error::Authentication)?;
    let pass = Zeroizing::new(format!("PASS {password}"));
    command(s, &pass)?;
    pop_ok(s).map_err(|_| Error::Authentication)?;
    command(s, "UIDL")?;
    pop_ok(s)?;
    let list = multiline(s, 4 * 1024 * 1024)?;
    let mut ids = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for entry in ascii(&list)?.lines() {
        let mut parts = entry.split_whitespace();
        let n = parts
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|n| *n > 0)
            .ok_or(Error::Protocol)?;
        let id = parts.next().ok_or(Error::Protocol)?;
        if parts.next().is_some()
            || id.len() > 70
            || id.bytes().any(|b| !(33..=126).contains(&b))
            || !seen.insert(id.to_owned())
        {
            return Err(Error::Protocol);
        }
        ids.push((n, id.to_owned()));
    }
    ids.sort_by_key(|(n, _)| *n);
    if ids.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(Error::Protocol);
    }
    let mut out = vec![];
    let mut total = 0;
    for (n, id) in ids.into_iter().rev().take(100) {
        command(s, &format!("RETR {n}"))?;
        pop_ok(s)?;
        let raw = multiline(s, MAX_MESSAGE)?;
        total += raw.len();
        if total > MAX_SESSION {
            return Err(Error::Limit);
        }
        out.push(Incoming {
            key: format!("pop:{id}"),
            raw,
            unread: true,
            flagged: false,
        });
    }
    // No DELE, even when a message is moved to the local Trash.
    command(s, "QUIT")?;
    pop_ok(s)?;
    Ok(out)
}
pub fn smtp_reply<S: BufRead>(s: &mut S, expected: u16) -> Result<Vec<String>> {
    let mut out = vec![];
    loop {
        let bytes = line(s)?;
        if bytes.len() < 4 || !bytes[..3].iter().all(u8::is_ascii_digit) {
            return Err(Error::Protocol);
        }
        if ascii(&bytes[..3])?.parse::<u16>().ok() != Some(expected) {
            return Err(Error::Rejected);
        }
        let sep = bytes[3];
        if sep != b'-' && sep != b' ' {
            return Err(Error::Protocol);
        }
        out.push(String::from_utf8_lossy(&bytes[4..]).into_owned());
        if out.len() > 100 {
            return Err(Error::Limit);
        }
        if sep == b' ' {
            return Ok(out);
        }
    }
}
pub fn smtp_hello<S: BufRead + Write>(s: &mut S) -> Result<Vec<String>> {
    command(s, "EHLO mochios.invalid")?;
    smtp_reply(s, 250)
}
pub fn smtp_starttls<S: BufRead + Write>(s: &mut S) -> Result<()> {
    let caps = smtp_hello(s)?;
    if !caps.iter().any(|s| s.eq_ignore_ascii_case("STARTTLS")) {
        return Err(Error::Tls);
    }
    command(s, "STARTTLS")?;
    smtp_reply(s, 220).map(|_| ())
}
pub fn smtp_send<S: BufRead + Write>(
    s: &mut S,
    user: &str,
    password: &str,
    from: &str,
    recipients: &[String],
    message: &[u8],
) -> Result<()> {
    if recipients.is_empty() || recipients.len() > 50 || message.len() > MAX_MESSAGE {
        return Err(Error::Invalid);
    }
    crate::mime::address(from)?;
    for to in recipients {
        crate::mime::address(to)?;
    }
    smtp_auth(s, user, password)?;
    command(s, &format!("MAIL FROM:<{from}>"))?;
    smtp_reply(s, 250)?;
    for to in recipients {
        command(s, &format!("RCPT TO:<{to}>"))?;
        smtp_reply(s, 250)?;
    }
    command(s, "DATA")?;
    smtp_reply(s, 354)?;
    // Message generation guarantees CRLF; dot-stuff every line.
    for row in message.split_inclusive(|b| *b == b'\n') {
        if row.starts_with(b".") {
            s.write_all(b".").map_err(|_| Error::DeliveryUnknown)?;
        }
        s.write_all(row).map_err(|_| Error::DeliveryUnknown)?;
    }
    if !message.ends_with(b"\r\n") {
        s.write_all(b"\r\n").map_err(|_| Error::DeliveryUnknown)?;
    }
    s.write_all(b".\r\n")
        .and_then(|_| s.flush())
        .map_err(|_| Error::DeliveryUnknown)?;
    // A timeout here may mean delivery succeeded. UI must not auto-retry.
    smtp_reply(s, 250).map_err(|_| Error::DeliveryUnknown)?;
    Ok(())
}

pub fn smtp_auth<S: BufRead + Write>(s: &mut S, user: &str, password: &str) -> Result<()> {
    single(user)?;
    single(password)?;
    let caps = smtp_hello(s)?;
    let caps: Vec<_> = caps.iter().map(|s| s.to_ascii_uppercase()).collect();
    let auth: Vec<_> = caps
        .iter()
        .filter_map(|s| s.strip_prefix("AUTH "))
        .flat_map(str::split_whitespace)
        .collect();
    if auth.iter().any(|s| s.eq_ignore_ascii_case("PLAIN")) {
        let plain = Zeroizing::new(format!("\0{user}\0{password}"));
        let b64 = Zeroizing::new(STANDARD.encode(plain.as_bytes()));
        let encoded = Zeroizing::new(format!("AUTH PLAIN {}", b64.as_str()));
        command(s, &encoded)?;
        smtp_reply(s, 235).map_err(|_| Error::Authentication)?;
    } else if auth.iter().any(|s| s.eq_ignore_ascii_case("LOGIN")) {
        command(s, "AUTH LOGIN")?;
        smtp_reply(s, 334).map_err(|_| Error::Authentication)?;
        let encoded = Zeroizing::new(STANDARD.encode(user));
        command(s, &encoded)?;
        smtp_reply(s, 334).map_err(|_| Error::Authentication)?;
        let encoded = Zeroizing::new(STANDARD.encode(password));
        command(s, &encoded)?;
        smtp_reply(s, 235).map_err(|_| Error::Authentication)?;
    } else {
        return Err(Error::Authentication);
    }
    Ok(())
}
