//! All blocking network/Keychain operations execute on a worker, never in paint.
use crate::{protocol, transport::Connection};
use appcore::keychain::{ItemId, Keychain, Secret};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Network,
    Tls,
    Protocol,
    Rejected,
    Authentication,
    Limit,
    Message,
    Keychain,
    Duplicate,
    DeliveryUnknown,
}
impl Error {
    pub fn label(self) -> &'static str {
        match self {
            Self::Invalid => {
                "入力内容を確認してください。宛先はメールアドレスをカンマ区切りで指定します。"
            }
            Self::Network => {
                "サーバーに接続できません。ホスト名・ポート・ネットワークを確認してください。"
            }
            Self::Tls => {
                "安全なTLS接続を確立できません。証明書・日時・TLS 1.3対応を確認してください。"
            }
            Self::Protocol => "メールサーバーから未対応の応答を受信しました。",
            Self::Rejected => {
                "サーバーが操作を拒否しました。設定とサービスの利用状況を確認してください。"
            }
            Self::Authentication => {
                "認証に失敗しました。ユーザー名・パスワードを確認してください。Gmailにはアプリパスワードが必要です。"
            }
            Self::Limit => "メールまたは応答が上限を超えています（1通8MiB、1回32MiB、最新100件）。",
            Self::Message => "メールの形式を解析できませんでした。",
            Self::Keychain => {
                "Keychainを利用できません。ログイン状態と保管庫のロックを確認してください。"
            }
            Self::Duplicate => "このメールアカウントは登録済みです。",
            Self::DeliveryUnknown => {
                "送信結果を確認できません。重複送信を防ぐため、送信先を確認してから再送してください。"
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Imap,
    Pop3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Security {
    Tls,
    StartTls,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub email: String,
    pub username: String,
    pub incoming_host: String,
    pub incoming_port: u16,
    pub incoming_security: Security,
    pub protocol: Protocol,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: Security,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        crate::mime::address(&self.email)?;
        if self.email.len() > 128 || self.username.len() > 256 {
            return Err(Error::Invalid);
        }
        protocol::single(&self.username)?;
        for host in [&self.incoming_host, &self.smtp_host] {
            if host.len() > 253
                || host.is_empty()
                || host.split('.').any(|s| {
                    s.is_empty()
                        || s.len() > 63
                        || s.starts_with('-')
                        || s.ends_with('-')
                        || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err(Error::Invalid);
            }
        }
        if self.incoming_port == 0 || self.smtp_port == 0 {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct Account {
    pub id: ItemId,
    pub config: Config,
}
#[derive(Serialize, Deserialize)]
pub struct Credential {
    pub config: Config,
    pub password: Zeroizing<String>,
}
const SERVICE: &str = "org.mochios.mail.account.v1";
fn vault() -> Result<Keychain> {
    let keychain = Keychain::connect().map_err(|_| Error::Keychain)?;
    if !keychain.status().map_err(|_| Error::Keychain)?.unlocked {
        return Err(Error::Keychain);
    }
    Ok(keychain)
}
fn read(keychain: &Keychain, id: ItemId) -> Result<Credential> {
    let secret = keychain.get(id).map_err(|_| Error::Keychain)?;
    let credential: Credential =
        serde_json::from_slice(secret.expose()).map_err(|_| Error::Keychain)?;
    credential.config.validate()?;
    protocol::single(&credential.password)?;
    Ok(credential)
}
pub fn accounts() -> Result<Vec<Account>> {
    let keychain = vault()?;
    let mut accounts = vec![];
    for item in keychain.items().map_err(|_| Error::Keychain)? {
        if item.service == SERVICE {
            let c = read(&keychain, item.id)?;
            accounts.push(Account {
                id: item.id,
                config: c.config.clone(),
            });
        }
    }
    accounts.sort_by(|a, b| a.config.email.cmp(&b.config.email));
    Ok(accounts)
}
pub fn add(credential: Credential) -> Result<Account> {
    credential.config.validate()?;
    protocol::single(&credential.password)?;
    let existing = accounts()?;
    if existing.len() >= 16 {
        return Err(Error::Limit);
    }
    if existing.iter().any(|a| {
        a.config
            .email
            .eq_ignore_ascii_case(&credential.config.email)
    }) {
        return Err(Error::Duplicate);
    }
    // Authenticate both services before persisting; no message is sent.
    let mut incoming = incoming(&credential)?;
    match credential.config.protocol {
        Protocol::Imap => {
            let _ = protocol::imap_receive(
                &mut incoming,
                &credential.config.username,
                &credential.password,
            )?;
        }
        Protocol::Pop3 => {
            let _ = protocol::pop_receive(
                &mut incoming,
                &credential.config.username,
                &credential.password,
            )?;
        }
    }
    // Outgoing credentials are checked by a separate AUTH-only exchange.
    let mut outgoing = outgoing(&credential)?;
    protocol::smtp_auth(
        &mut outgoing,
        &credential.config.username,
        &credential.password,
    )?;
    let bytes = Zeroizing::new(serde_json::to_vec(&credential).map_err(|_| Error::Invalid)?);
    if bytes.len() > 4096 {
        return Err(Error::Limit);
    }
    let config = credential.config.clone();
    let keychain = vault()?;
    let id = keychain
        .put(
            SERVICE,
            &config.email,
            &config.email,
            Secret::new(bytes.to_vec()),
        )
        .map_err(|_| Error::Keychain)?;
    Ok(Account { id, config })
}
pub fn remove(id: ItemId) -> Result<()> {
    vault()?.delete(id).map_err(|_| Error::Keychain)
}
fn incoming(c: &Credential) -> Result<Connection> {
    let cfg = &c.config;
    let mut s = Connection::connect(&cfg.incoming_host, cfg.incoming_port, cfg.incoming_security)?;
    match cfg.protocol {
        Protocol::Imap => protocol::imap_greeting(&mut s)?,
        Protocol::Pop3 => protocol::pop_greeting(&mut s)?,
    }
    if cfg.incoming_security == Security::StartTls {
        match cfg.protocol {
            Protocol::Imap => protocol::imap_starttls(&mut s)?,
            Protocol::Pop3 => protocol::pop_starttls(&mut s)?,
        }
        s.upgrade(&cfg.incoming_host)?;
    }
    Ok(s)
}
fn outgoing(c: &Credential) -> Result<Connection> {
    let cfg = &c.config;
    let mut s = Connection::connect(&cfg.smtp_host, cfg.smtp_port, cfg.smtp_security)?;
    protocol::smtp_reply(&mut s, 220)?;
    if cfg.smtp_security == Security::StartTls {
        protocol::smtp_starttls(&mut s)?;
        s.upgrade(&cfg.smtp_host)?;
    }
    Ok(s)
}
pub fn receive(account: &Account) -> Result<Vec<protocol::Incoming>> {
    let c = read(&vault()?, account.id)?;
    let mut s = incoming(&c)?;
    match c.config.protocol {
        Protocol::Imap => protocol::imap_receive(&mut s, &c.config.username, &c.password),
        Protocol::Pop3 => protocol::pop_receive(&mut s, &c.config.username, &c.password),
    }
}
pub fn send(account: &Account, to: &str, subject: &str, body: &str) -> Result<()> {
    let recipients = crate::mime::recipients(to)?;
    let raw = crate::mime::encode(&account.config.email, &recipients, subject, body)?;
    let c = read(&vault()?, account.id)?;
    let mut s = outgoing(&c)?;
    protocol::smtp_send(
        &mut s,
        &c.config.username,
        &c.password,
        &c.config.email,
        &recipients,
        &raw,
    )
}
