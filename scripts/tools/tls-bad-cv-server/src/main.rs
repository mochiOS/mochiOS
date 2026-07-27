use std::fmt;
use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, SubjectPublicKeyInfoDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::{CertifiedKey, Signer, SigningKey};
use rustls::{Error, ServerConfig, ServerConnection, SignatureAlgorithm, SignatureScheme};

struct BadCertificateVerifyKey {
    inner: Arc<dyn SigningKey>,
}

impl fmt::Debug for BadCertificateVerifyKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BadCertificateVerifyKey(test-only)")
    }
}

impl SigningKey for BadCertificateVerifyKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        self.inner
            .choose_scheme(offered)
            .map(|inner| Box::new(BadCertificateVerifySigner { inner }) as Box<dyn Signer>)
    }

    fn public_key(&self) -> Option<SubjectPublicKeyInfoDer<'_>> {
        self.inner.public_key()
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        self.inner.algorithm()
    }
}

struct BadCertificateVerifySigner {
    inner: Box<dyn Signer>,
}

impl fmt::Debug for BadCertificateVerifySigner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BadCertificateVerifySigner(test-only)")
    }
}

impl Signer for BadCertificateVerifySigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let mut signature = self.inner.sign(message)?;
        let Some(last) = signature.last_mut() else {
            return Err(Error::General(
                "test signer produced empty signature".into(),
            ));
        };
        *last ^= 0x01;
        eprintln!("tls-bad-cv-server: bad-certificate-verify=sent");
        Ok(signature)
    }

    fn scheme(&self) -> SignatureScheme {
        self.inner.scheme()
    }
}

#[derive(Debug)]
struct FixedResolver(Arc<CertifiedKey>);

impl ResolvesServerCert for FixedResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

fn certificates(path: &Path) -> io::Result<Vec<CertificateDer<'static>>> {
    CertificateDer::pem_file_iter(path)
        .map_err(io::Error::other)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io::Error::other)
}

fn private_key(path: &Path) -> io::Result<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_file(path).map_err(io::Error::other)
}

fn config(certificate: &Path, key: &Path) -> io::Result<Arc<ServerConfig>> {
    let provider = rustls::crypto::ring::default_provider();
    let signing_key = provider
        .key_provider
        .load_private_key(private_key(key)?)
        .map_err(io::Error::other)?;
    let certified_key = CertifiedKey::new(
        certificates(certificate)?,
        Arc::new(BadCertificateVerifyKey { inner: signing_key }),
    );
    let config = ServerConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(FixedResolver(Arc::new(certified_key))));
    Ok(Arc::new(config))
}

fn serve_connection(stream: TcpStream, config: Arc<ServerConfig>) -> io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;
    let connection = ServerConnection::new(config).map_err(io::Error::other)?;
    let mut stream = rustls::StreamOwned::new(connection, stream);
    let mut byte = [0u8; 1];
    match stream.read(&mut byte) {
        Ok(_) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::InvalidData
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::UnexpectedEof
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn main() -> io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let port = args
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "port is missing"))?
        .to_string_lossy()
        .parse::<u16>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid port"))?;
    let ready = args
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ready path is missing"))?;
    let certificate = args
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "certificate is missing"))?;
    let key = args
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "key is missing"))?;
    if args.next().is_some() || port == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid arguments",
        ));
    }
    let config = config(Path::new(&certificate), Path::new(&key))?;
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    File::create(ready)?.write_all(format!("{port}\n").as_bytes())?;
    eprintln!("tls-bad-cv-server: port={port} ready");
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                if let Err(error) = serve_connection(stream, config.clone()) {
                    eprintln!("tls-bad-cv-server: expected-peer-error={error}");
                }
            }
            Err(error) => eprintln!("tls-bad-cv-server: accept-error={error}"),
        }
    }
    Ok(())
}
