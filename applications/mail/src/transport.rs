//! Bounded, certificate-verified transport. No unencrypted authentication path.
use crate::backend::{Error, Result, Security};
#[cfg(target_os = "mochios")]
use crate::native_socket::Socket;
use mochios_tls_client::{TlsConnection, TlsEvent, TlsState};
use std::io::{self, BufRead, BufReader, Read, Write};
#[cfg(not(target_os = "mochios"))]
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(not(target_os = "mochios"))]
type Socket = TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_MESSAGE: usize = 8 * 1024 * 1024;
pub const MAX_SESSION: usize = 32 * 1024 * 1024;
pub const MAX_LINE: usize = 64 * 1024;

pub struct Wire {
    socket: Socket,
    tls: Option<TlsConnection>,
    pending: Vec<u8>,
    offset: usize,
    deadline: Instant,
}
impl Wire {
    pub fn connect(host: &str, port: u16, security: Security) -> Result<BufReader<Self>> {
        #[cfg(not(target_os = "mochios"))]
        let socket = {
            let addresses = (host, port).to_socket_addrs().map_err(|_| Error::Network)?;
            let mut socket = None;
            for address in addresses.take(4) {
                if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_secs(8)) {
                    socket = Some(stream);
                    break;
                }
            }
            let socket = socket.ok_or(Error::Network)?;
            socket
                .set_read_timeout(Some(Duration::from_secs(15)))
                .map_err(|_| Error::Network)?;
            socket
                .set_write_timeout(Some(Duration::from_secs(15)))
                .map_err(|_| Error::Network)?;
            socket
        };
        #[cfg(target_os = "mochios")]
        let socket = Socket::connect(host, port)?;
        let mut wire = Self {
            socket,
            tls: None,
            pending: Vec::new(),
            offset: 0,
            deadline: Instant::now() + Duration::from_secs(120),
        };
        if security == Security::Tls {
            wire.upgrade(host)?;
        }
        Ok(BufReader::new(wire))
    }
    pub fn upgrade(&mut self, host: &str) -> Result<()> {
        if self.tls.is_some() {
            return Err(Error::Protocol);
        }
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Tls)?
            .as_secs();
        let config = mochios_tls_client::build_client_config(
            mochios_tls_client::production_root_store(),
            Arc::new(mochios_tls_client::FixedTimeProvider::at(seconds)),
        )
        .map_err(|_| Error::Tls)?;
        self.upgrade_with_config(host, Arc::new(config))
    }
    fn upgrade_with_config(
        &mut self,
        host: &str,
        config: Arc<mochios_tls_client::rustls::ClientConfig>,
    ) -> Result<()> {
        self.tls = Some(TlsConnection::new(config, host).map_err(|_| Error::Tls)?);
        while self.tls.as_ref().unwrap().state() != TlsState::Established {
            self.drive().map_err(|_| Error::Tls)?;
        }
        Ok(())
    }
    fn drive(&mut self) -> io::Result<()> {
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "mail deadline"));
        }
        let tls = self.tls.as_mut().unwrap();
        match tls
            .next_event()
            .map_err(|_| io::Error::other("TLS verification/protocol"))?
        {
            TlsEvent::Transmit(bytes) => self.socket.write_all(&bytes)?,
            TlsEvent::NeedReceive => {
                let mut bytes = [0; 16384];
                let n = self.socket.read(&mut bytes)?;
                if n == 0 {
                    return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
                }
                tls.receive_tls(&bytes[..n])
                    .map_err(|_| io::Error::other("TLS record"))?;
            }
            TlsEvent::Plaintext(bytes) => {
                self.pending = bytes;
                self.offset = 0;
            }
            TlsEvent::Established => {}
            TlsEvent::PeerClosed | TlsEvent::Closed => {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
        }
        Ok(())
    }
}
impl Read for Wire {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.tls.is_none() {
            return self.socket.read(out);
        }
        while self.offset == self.pending.len() {
            self.drive()?;
        }
        let n = out.len().min(self.pending.len() - self.offset);
        out[..n].copy_from_slice(&self.pending[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
}
impl Write for Wire {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if Instant::now() >= self.deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        if let Some(tls) = &mut self.tls {
            let n = bytes.len().min(mochios_tls_client::MAX_TLS_PLAINTEXT);
            let encrypted = tls
                .encrypt(&bytes[..n])
                .map_err(|_| io::Error::other("TLS encrypt"))?;
            self.socket.write_all(&encrypted)?;
            Ok(n)
        } else {
            self.socket.write(bytes)
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}
/// STARTTLS must not reuse any plaintext prefetched by BufReader.
pub fn upgrade(wire: &mut BufReader<Wire>, host: &str) -> Result<()> {
    if !wire.buffer().is_empty() {
        return Err(Error::Protocol);
    }
    wire.get_mut().upgrade(host)
}
pub fn line<R: BufRead>(r: &mut R) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let buffer = r.fill_buf().map_err(|_| Error::Network)?;
        if buffer.is_empty() {
            return Err(Error::Network);
        }
        let n = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map_or(buffer.len(), |p| p + 1);
        if bytes.len() + n > MAX_LINE {
            return Err(Error::Limit);
        }
        let done = buffer[n - 1] == b'\n';
        bytes.extend_from_slice(&buffer[..n]);
        r.consume(n);
        if done {
            break;
        }
    }
    if !bytes.ends_with(b"\r\n") {
        return Err(Error::Protocol);
    }
    bytes.truncate(bytes.len() - 2);
    Ok(bytes)
}
pub fn command<W: Write>(w: &mut W, text: &str) -> Result<()> {
    w.write_all(text.as_bytes())
        .and_then(|_| w.write_all(b"\r\n"))
        .and_then(|_| w.flush())
        .map_err(|_| Error::Network)
}

pub struct Connection(BufReader<Wire>);
impl Connection {
    pub fn connect(host: &str, port: u16, security: Security) -> Result<Self> {
        Wire::connect(host, port, security).map(Self)
    }
    pub fn upgrade(&mut self, host: &str) -> Result<()> {
        upgrade(&mut self.0, host)
    }
}
impl Read for Connection {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.0.read(out)
    }
}
impl BufRead for Connection {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.0.fill_buf()
    }
    fn consume(&mut self, n: usize) {
        self.0.consume(n)
    }
}
impl Write for Connection {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.get_mut().write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.get_mut().flush()
    }
}

#[cfg(all(test, not(target_os = "mochios")))]
mod tests {
    use super::*;
    use rustls::{ServerConfig, ServerConnection, StreamOwned};
    use std::net::TcpListener;
    use std::thread;
    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        for s in [&client, &server] {
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            s.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
        }
        (client, server)
    }
    fn configs() -> (Arc<rustls::ClientConfig>, Arc<ServerConfig>) {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let roots = mochios_tls_client::root_store_from_der(&[cert.cert.der().as_ref()]).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let client = mochios_tls_client::build_client_config(
            roots,
            Arc::new(mochios_tls_client::FixedTimeProvider::at(now)),
        )
        .unwrap();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
        let server =
            ServerConfig::builder_with_provider(Arc::new(mochios_tls_client::crypto_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(vec![cert.cert.der().clone()], key.into())
                .unwrap();
        (Arc::new(client), Arc::new(server))
    }
    fn wire(socket: TcpStream) -> Wire {
        Wire {
            socket,
            tls: None,
            pending: vec![],
            offset: 0,
            deadline: Instant::now() + Duration::from_secs(5),
        }
    }
    #[test]
    fn verified_tls_carries_real_protocol_bytes_and_large_writes() {
        let (client, server) = pair();
        let (config, server_config) = configs();
        let worker = thread::spawn(move || {
            let mut stream =
                StreamOwned::new(ServerConnection::new(server_config).unwrap(), server);
            stream.write_all(b"* OK ready\r\n").unwrap();
            stream.flush().unwrap();
            let mut payload = vec![0; 40000];
            stream.read_exact(&mut payload).unwrap();
            assert!(payload.iter().all(|b| *b == b'a'));
            stream.write_all(b"A1 OK received\r\n").unwrap();
            stream.flush().unwrap();
        });
        let mut client = wire(client);
        client.upgrade_with_config("localhost", config).unwrap();
        let mut conn = Connection(BufReader::new(client));
        crate::protocol::imap_greeting(&mut conn).unwrap();
        conn.write_all(&vec![b'a'; 40000]).unwrap();
        assert_eq!(line(&mut conn).unwrap(), b"A1 OK received");
        worker.join().unwrap();
    }
    #[test]
    fn tls_rejects_hostname_and_untrusted_root_before_credentials() {
        for trusted in [true, false] {
            let (client, server) = pair();
            let (config, server_config) = configs();
            let worker = thread::spawn(move || {
                let mut stream =
                    StreamOwned::new(ServerConnection::new(server_config).unwrap(), server);
                let mut data = [0; 128];
                let result = stream.read(&mut data);
                assert!(!matches!(result,Ok(n) if n>0));
            });
            let config = if trusted {
                config
            } else {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                Arc::new(
                    mochios_tls_client::build_client_config(
                        mochios_tls_client::production_root_store(),
                        Arc::new(mochios_tls_client::FixedTimeProvider::at(now)),
                    )
                    .unwrap(),
                )
            };
            let mut client = wire(client);
            assert_eq!(
                client
                    .upgrade_with_config(
                        if trusted {
                            "wrong.example"
                        } else {
                            "localhost"
                        },
                        config
                    )
                    .err(),
                Some(Error::Tls)
            );
            drop(client);
            worker.join().unwrap();
        }
    }
    #[test]
    fn starttls_rejects_buffered_plaintext_before_upgrade() {
        let (client, mut server) = pair();
        server
            .write_all(b"+OK begin TLS\r\n+OK forged\r\n")
            .unwrap();
        let mut conn = Connection(BufReader::new(wire(client)));
        assert_eq!(line(&mut conn).unwrap(), b"+OK begin TLS");
        assert_eq!(conn.upgrade("localhost").err(), Some(Error::Protocol));
    }
}
