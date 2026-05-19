//! TCP stream abstraction for plain and TLS-encrypted connections.
//!
//! Provides a trait that abstracts over plain TCP and TLS streams,
//! allowing the transport layer to switch between them without changing
//! connection management or framing logic.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, ServerConfig, ServerConnection, StreamOwned};

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};

/// Abstraction over TCP stream types (Plain TCP / future TLS).
///
/// All TCP connections in the transport layer use this trait instead of
/// `TcpStream` directly, so that TLS can be added later by implementing
/// this trait for `TlsStream<TcpStream>`.
pub(crate) trait TcpStreamWrapper: Read + Write + Send + Sync {
    /// Get the remote peer's address
    fn peer_addr(&self) -> io::Result<SocketAddr>;

    /// Set TCP_NODELAY option
    fn set_nodelay(&self, nodelay: bool) -> io::Result<()>;

    /// Set write timeout
    fn set_write_timeout(&self, dur: Option<std::time::Duration>) -> io::Result<()>;

    /// Set read timeout (applied to the underlying TCP socket).
    fn set_read_timeout(&self, dur: Option<std::time::Duration>) -> io::Result<()>;

    /// Clone the stream (for concurrent read/write)
    fn try_clone_box(&self) -> io::Result<Box<dyn TcpStreamWrapper>>;

    /// Whether this stream is TLS-encrypted
    fn is_tls(&self) -> bool {
        false
    }
}

/// Plain (unencrypted) TCP stream wrapper
pub(crate) struct PlainTcpStream {
    inner: TcpStream,
}

impl PlainTcpStream {
    pub(crate) fn new(stream: TcpStream) -> Self {
        Self { inner: stream }
    }

    /// Consume wrapper and return the inner TcpStream
    pub(crate) fn into_inner(self) -> TcpStream {
        self.inner
    }

    /// Get a reference to the inner TcpStream (for mio registration etc.)
    pub(crate) fn inner(&self) -> &TcpStream {
        &self.inner
    }
}

impl Read for PlainTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for PlainTcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl TcpStreamWrapper for PlainTcpStream {
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.inner.set_nodelay(nodelay)
    }

    fn set_write_timeout(&self, dur: Option<std::time::Duration>) -> io::Result<()> {
        self.inner.set_write_timeout(dur)
    }

    fn set_read_timeout(&self, dur: Option<std::time::Duration>) -> io::Result<()> {
        self.inner.set_read_timeout(dur)
    }

    fn try_clone_box(&self) -> io::Result<Box<dyn TcpStreamWrapper>> {
        let cloned = self.inner.try_clone()?;
        Ok(Box::new(PlainTcpStream::new(cloned)))
    }
}

/// Create a plain (unencrypted) TcpStreamWrapper from a raw TcpStream.
///
/// For TLS-wrapped streams use [`connect_tls`] (client side) or
/// [`accept_tls`] (server side).
pub(crate) fn wrap_stream(stream: TcpStream) -> Box<dyn TcpStreamWrapper> {
    Box::new(PlainTcpStream::new(stream))
}

// ── TLS-wrapped TCP stream ───────────────────────────────────────────────

/// Direction of the TLS connection. Determines whether a `ClientConnection`
/// or `ServerConnection` drives the TLS state machine.
enum TlsKind {
    Client(StreamOwned<ClientConnection, TcpStream>),
    Server(StreamOwned<ServerConnection, TcpStream>),
}

impl TlsKind {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Client(s) => s.read(buf),
            Self::Server(s) => s.read(buf),
        }
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Client(s) => s.write(buf),
            Self::Server(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Client(s) => s.flush(),
            Self::Server(s) => s.flush(),
        }
    }

    fn socket(&self) -> &TcpStream {
        match self {
            Self::Client(s) => s.get_ref(),
            Self::Server(s) => s.get_ref(),
        }
    }
}

/// TLS-wrapped TCP stream.
///
/// rustls connections are stateful and cannot be cloned. To preserve the
/// `TcpStreamWrapper::try_clone_box` contract, the inner state is wrapped
/// in `Arc<Mutex<_>>`. Cloning yields another handle to the same TLS
/// connection; concurrent reads/writes are serialised by the mutex.
pub(crate) struct TlsTcpStream {
    inner: Arc<Mutex<TlsKind>>,
    peer_addr: SocketAddr,
}

impl TlsTcpStream {
    fn new(kind: TlsKind) -> io::Result<Self> {
        let peer_addr = kind.socket().peer_addr()?;
        Ok(Self { inner: Arc::new(Mutex::new(kind)), peer_addr })
    }
}

impl Read for TlsTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.lock().expect("TlsTcpStream lock poisoned").read(buf)
    }
}

impl Write for TlsTcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.lock().expect("TlsTcpStream lock poisoned").write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.lock().expect("TlsTcpStream lock poisoned").flush()
    }
}

impl TcpStreamWrapper for TlsTcpStream {
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.peer_addr)
    }

    fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.inner.lock().expect("TlsTcpStream lock poisoned").socket().set_nodelay(nodelay)
    }

    fn set_write_timeout(&self, dur: Option<std::time::Duration>) -> io::Result<()> {
        self.inner.lock().expect("TlsTcpStream lock poisoned").socket().set_write_timeout(dur)
    }

    fn set_read_timeout(&self, dur: Option<std::time::Duration>) -> io::Result<()> {
        self.inner.lock().expect("TlsTcpStream lock poisoned").socket().set_read_timeout(dur)
    }

    fn try_clone_box(&self) -> io::Result<Box<dyn TcpStreamWrapper>> {
        Ok(Box::new(TlsTcpStream { inner: self.inner.clone(), peer_addr: self.peer_addr }))
    }

    fn is_tls(&self) -> bool {
        true
    }
}

/// Open a TLS connection over an established TCP socket (client role).
/// The TLS handshake is performed synchronously before returning.
pub(crate) fn connect_tls(
    tcp: TcpStream,
    config: Arc<ClientConfig>,
    server_name: &str,
) -> io::Result<Box<dyn TcpStreamWrapper>> {
    let server_name = ServerName::try_from(server_name.to_string())
        .map_err(|e| transport_io_error(TransportErrorCode::TlsConfigError, e.to_string()))?;
    let mut conn = ClientConnection::new(config, server_name)
        .map_err(|e| transport_io_error(TransportErrorCode::TlsConfigError, e.to_string()))?;
    let mut tcp = tcp;
    conn.complete_io(&mut tcp)
        .map_err(|e| transport_io_error(TransportErrorCode::TlsHandshakeFailed, e.to_string()))?;
    let stream = StreamOwned::new(conn, tcp);
    Ok(Box::new(TlsTcpStream::new(TlsKind::Client(stream))?))
}

/// Accept a TLS connection on an established TCP socket (server role).
/// The TLS handshake is performed synchronously before returning.
pub(crate) fn accept_tls(
    tcp: TcpStream,
    config: Arc<ServerConfig>,
) -> io::Result<Box<dyn TcpStreamWrapper>> {
    let mut conn = ServerConnection::new(config)
        .map_err(|e| transport_io_error(TransportErrorCode::TlsConfigError, e.to_string()))?;
    let mut tcp = tcp;
    conn.complete_io(&mut tcp)
        .map_err(|e| transport_io_error(TransportErrorCode::TlsHandshakeFailed, e.to_string()))?;
    let stream = StreamOwned::new(conn, tcp);
    Ok(Box::new(TlsTcpStream::new(TlsKind::Server(stream))?))
}

#[cfg(test)]
mod tls_tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::thread;

    use rcgen::{generate_simple_self_signed, CertifiedKey};
    use rustls::pki_types::CertificateDer;
    use rustls::pki_types::PrivateKeyDer;

    struct TestCerts {
        cert: CertificateDer<'static>,
        key: PrivateKeyDer<'static>,
    }

    fn gen_certs() -> TestCerts {
        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["localhost".into()]).expect("rcgen");
        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der: PrivateKeyDer<'static> =
            PrivateKeyDer::try_from(key_pair.serialize_der()).expect("key conv");
        TestCerts { cert: cert_der, key: key_der }
    }

    fn server_cfg(c: &TestCerts) -> Arc<ServerConfig> {
        let cfg = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![c.cert.clone()], c.key.clone_key())
            .expect("server cfg");
        Arc::new(cfg)
    }

    fn client_cfg(c: &TestCerts) -> Arc<ClientConfig> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(c.cert.clone()).expect("roots");
        Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
    }

    #[test]
    fn handshake_and_roundtrip() {
        let certs = gen_certs();
        let s_cfg = server_cfg(&certs);
        let c_cfg = client_cfg(&certs);

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();

        let server_thread = thread::spawn(move || {
            let (tcp, _) = listener.accept().expect("accept");
            let mut server = accept_tls(tcp, s_cfg).expect("accept_tls");
            let mut buf = [0u8; 32];
            let n = server.read(&mut buf).expect("server read");
            server.write_all(&buf[..n]).expect("server write");
            server.flush().expect("server flush");
            buf[..n].to_vec()
        });

        let tcp = TcpStream::connect(addr).expect("connect");
        let mut client = connect_tls(tcp, c_cfg, "localhost").expect("connect_tls");

        let payload = b"hello-tls";
        client.write_all(payload).expect("client write");
        client.flush().expect("client flush");

        let mut echoed = vec![0u8; payload.len()];
        client.read_exact(&mut echoed).expect("client read");
        assert_eq!(echoed, payload);
        assert!(client.is_tls());
        assert_eq!(client.peer_addr().unwrap(), addr);

        let received = server_thread.join().expect("server thread");
        assert_eq!(received, payload);
    }

    #[test]
    fn untrusted_certificate_rejected() {
        let server_certs = gen_certs();
        let other_certs = gen_certs(); // different CA
        let s_cfg = server_cfg(&server_certs);
        let c_cfg = client_cfg(&other_certs);

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();

        let server_thread = thread::spawn(move || {
            let (tcp, _) = listener.accept().expect("accept");
            // Expected to fail; ignore result.
            let _ = accept_tls(tcp, s_cfg);
        });

        let tcp = TcpStream::connect(addr).expect("connect");
        match connect_tls(tcp, c_cfg, "localhost") {
            Ok(_) => panic!("client should reject server cert it does not trust"),
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::ConnectionAborted),
        }

        let _ = server_thread.join();
    }

    #[test]
    fn try_clone_yields_shared_handle() {
        let certs = gen_certs();
        let s_cfg = server_cfg(&certs);
        let c_cfg = client_cfg(&certs);

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();

        let server_thread = thread::spawn(move || {
            let (tcp, _) = listener.accept().expect("accept");
            let mut server = accept_tls(tcp, s_cfg).expect("accept_tls");
            let mut buf = [0u8; 16];
            let n = server.read(&mut buf).expect("server read");
            server.write_all(&buf[..n]).expect("server write");
            server.flush().ok();
        });

        let tcp = TcpStream::connect(addr).expect("connect");
        let client = connect_tls(tcp, c_cfg, "localhost").expect("connect_tls");
        let mut clone = client.try_clone_box().expect("try_clone_box");

        let payload = b"shared";
        clone.write_all(payload).expect("clone write");
        clone.flush().expect("clone flush");

        let mut echoed = vec![0u8; payload.len()];
        clone.read_exact(&mut echoed).expect("clone read");
        assert_eq!(echoed, payload);

        let _ = server_thread.join();
    }
}
