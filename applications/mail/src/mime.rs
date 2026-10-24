//! MIME decoding is delegated to mailparse (RFC 2047/2231, charsets, multipart).
//! Mail never loads remote images, executes HTML or interprets attachment names as paths.
use crate::backend::{Error, Result};
use crate::model::{Folder, Message, Thread};
use crate::protocol::Incoming;
use base64::{Engine, engine::general_purpose::STANDARD};
use mailparse::{MailAddr, MailHeaderMap};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn address(s: &str) -> Result<()> {
    if s.len() > 254
        || !s.is_ascii()
        || s.bytes()
            .any(|b| b <= 32 || b >= 127 || b"<>(),;:\\\"[]".contains(&b))
    {
        return Err(Error::Invalid);
    }
    let (local, domain) = s.split_once('@').ok_or(Error::Invalid)?;
    if local.is_empty()
        || domain.is_empty()
        || domain.contains('@')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
pub fn recipients(s: &str) -> Result<Vec<String>> {
    let values: Vec<_> = s.split(',').map(|s| s.trim().to_owned()).collect();
    if values.is_empty() || values.len() > 50 {
        return Err(Error::Invalid);
    }
    for to in &values {
        address(to)?;
    }
    Ok(values)
}
/// Validate the same prefix-based multipart splits as mailparse before entering
/// its recursive parser. Limits apply to LF and CRLF and malformed missing ends.
fn validate_structure(raw: &[u8]) -> Result<()> {
    let mut stack = vec![(raw, 0)];
    let mut count = 0;
    while let Some((part, depth)) = stack.pop() {
        count += 1;
        if depth > 16 || count > 256 {
            return Err(Error::Limit);
        }
        let (headers, body) = mailparse::parse_headers(part).map_err(|_| Error::Message)?;
        if body > MAX_HEADER {
            return Err(Error::Limit);
        }
        let Some(content) = headers.get_first_value("Content-Type") else {
            continue;
        };
        let ctype = mailparse::parse_content_type(&content);
        if !ctype.mimetype.starts_with("multipart/") {
            continue;
        }
        let Some(boundary) = ctype.params.get("boundary") else {
            continue;
        };
        if boundary.is_empty() || boundary.len() > 200 {
            return Err(Error::Message);
        }
        let prefix = format!("--{boundary}");
        let mut offset = body;
        let mut start = None;
        for row in part[body..].split_inclusive(|b| *b == b'\n') {
            if row.starts_with(prefix.as_bytes()) {
                if let Some(begin) = start.take() {
                    if stack.len() + count >= 256 {
                        return Err(Error::Limit);
                    }
                    let mut end = offset;
                    if end > begin && part[end - 1] == b'\n' {
                        end -= 1;
                        if end > begin && part[end - 1] == b'\r' {
                            end -= 1;
                        }
                    }
                    stack.push((&part[begin..end], depth + 1));
                }
                if row[prefix.len()..].starts_with(b"--") {
                    break;
                }
                if row.ends_with(b"\n") {
                    start = Some(offset + row.len());
                }
            }
            offset += row.len();
        }
        if let Some(begin) = start {
            stack.push((&part[begin..], depth + 1));
        }
    }
    Ok(())
}
const MAX_HEADER: usize = 64 * 1024;
pub fn decode(incoming: Incoming, account: usize, id: usize) -> Result<(String, Thread)> {
    if incoming.raw.len() > crate::transport::MAX_MESSAGE {
        return Err(Error::Limit);
    }
    validate_structure(&incoming.raw)?;
    let parsed = mailparse::parse_mail(&incoming.raw).map_err(|_| Error::Message)?;
    let subject = parsed
        .headers
        .get_first_value("Subject")
        .unwrap_or_else(|| "(No Subject)".into());
    let date = parsed.headers.get_first_value("Date").unwrap_or_default();
    let mut sender = String::new();
    let mut address = String::new();
    if let Some(header) = parsed.headers.get_first_header("From") {
        if let Ok(list) = mailparse::addrparse_header(header) {
            if let Some(MailAddr::Single(info)) = list.first() {
                sender = info
                    .display_name
                    .clone()
                    .unwrap_or_else(|| info.addr.clone());
                address = info.addr.clone();
            }
        }
    }
    let mut text = None;
    let mut attachments = vec![];
    let mut stack = vec![(&parsed, 0)];
    let mut count = 0;
    while let Some((part, depth)) = stack.pop() {
        count += 1;
        if depth > 16 || count > 256 {
            return Err(Error::Limit);
        }
        let disposition = part.get_content_disposition();
        if disposition.disposition == mailparse::DispositionType::Attachment
            || disposition.params.contains_key("filename")
        {
            let name = disposition
                .params
                .get("filename")
                .cloned()
                .or_else(|| part.ctype.params.get("name").cloned())
                .unwrap_or_else(|| "Attachment".into());
            // Display metadata only; no path or external content is opened.
            attachments.push(name.replace(['\r', '\n', '\0'], " "));
        } else if part.subparts.is_empty()
            && part.ctype.mimetype.eq_ignore_ascii_case("text/plain")
            && text.is_none()
        {
            text = Some(part.get_body().map_err(|_| Error::Message)?);
        }
        for child in part.subparts.iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    let body = text.unwrap_or_else(|| {
        "This message has no plain-text body. HTML display is not yet available.".into()
    });
    let snippet = body
        .chars()
        .take(160)
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let attachment = if attachments.is_empty() {
        None
    } else {
        Some(attachments.join("、"))
    };
    Ok((
        incoming.key,
        Thread {
            id,
            account,
            subject,
            snippet,
            time: date.clone(),
            unread: incoming.unread,
            flagged: incoming.flagged,
            folder: Folder::Inbox,
            messages: vec![Message {
                sender,
                address,
                recipient: parsed.headers.get_first_value("To").unwrap_or_default(),
                date,
                body,
            }],
            attachment,
        },
    ))
}
pub fn encode(from: &str, to: &[String], subject: &str, body: &str) -> Result<Vec<u8>> {
    address(from)?;
    for addr in to {
        address(addr)?;
    }
    if subject.len() > 512 || subject.chars().any(char::is_control) || body.len() > 4 * 1024 * 1024
    {
        return Err(Error::Invalid);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Invalid)?
        .as_secs();
    let mut random = [0u8; 16];
    getrandom::getrandom(&mut random).map_err(|_| Error::Invalid)?;
    let id: String = random.iter().map(|b| format!("{b:02x}")).collect();
    // RFC 2047 encoded words are <=75 chars; split on UTF-8 character boundaries.
    let mut chunks = vec![];
    let mut current = String::new();
    for c in subject.chars() {
        if current.len() + c.len_utf8() > 42 {
            chunks.push(current);
            current = String::new();
        }
        current.push(c);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    let subject = chunks
        .iter()
        .map(|s| format!("=?UTF-8?B?{}?=", STANDARD.encode(s)))
        .collect::<Vec<_>>()
        .join("\r\n ");
    // Format a UTC date without platform libc timezone/state dependencies.
    let date = date_utc(now);
    let mut message = format!(
        "From: <{from}>\r\nTo: {}\r\nSubject: {subject}\r\nDate: {date}\r\nMessage-ID: <{id}.{now}@mochios.invalid>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n",
        to.iter()
            .map(|a| format!("<{a}>"))
            .collect::<Vec<_>>()
            .join(",\r\n ")
    );
    let normalized = body
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n");
    let encoded = STANDARD.encode(normalized);
    for line in encoded.as_bytes().chunks(76) {
        message.push_str(std::str::from_utf8(line).unwrap());
        message.push_str("\r\n");
    }
    Ok(message.into_bytes())
}
fn date_utc(seconds: u64) -> String {
    // Civil date conversion (Gregorian eras), valid for real system times.
    let days = (seconds / 86400) as i64;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{day:02} {} {year:04} {:02}:{:02}:{:02} +0000",
        months[(month - 1) as usize],
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}
