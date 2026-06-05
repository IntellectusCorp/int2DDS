//! Connection stream that unifies plain TCP and TLS behind one type.
//!
//! `AsyncConnStream` lets the rest of the TCP transport read, write, and split
//! a connection without caring whether it is plaintext or TLS-encrypted: the
//! Plain/Tls branch is resolved once here, and every downstream task just uses
//! the `AsyncRead` / `AsyncWrite` impls. `into_split` yields owned read/write
//! halves so the conn_actor's reader and writer tasks can each own one end, and
//! the write half forwards vectored writes (`writev`) used by the framing path.
//! TLS handshakes are performed here by `accept_tls_async` (server side) and
//! `connect_tls_async` (client side).

use std::net::SocketAddr;
use std::sync::Arc;
use std::{io, pin::Pin};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ServerConfig};

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};

/// A connection that is either plain TCP or TLS, exposed through one uniform
/// `AsyncRead` / `AsyncWrite` interface.
pub(crate) enum AsyncConnStream {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl AsyncConnStream {
    /// Remote peer's socket address.
    pub(crate) fn peer_addr(&self) -> io::Result<SocketAddr> {
        match self {
            Self::Plain(t) => t.peer_addr(),
            Self::Tls(TlsStream::Server(s)) => s.get_ref().0.peer_addr(),
            Self::Tls(TlsStream::Client(s)) => s.get_ref().0.peer_addr(),
        }
    }

    /// Set TCP_NODELAY on the underlying TCP socket.
    pub(crate) fn set_nodelay(&self, on: bool) -> io::Result<()> {
        match self {
            Self::Plain(t) => t.set_nodelay(on),
            Self::Tls(TlsStream::Server(s)) => s.get_ref().0.set_nodelay(on),
            Self::Tls(TlsStream::Client(s)) => s.get_ref().0.set_nodelay(on),
        }
    }

    /// Whether this connection is TLS-encrypted.
    pub(crate) fn is_tls(&self) -> bool {
        matches!(self, Self::Tls(_))
    }

    /// Split into owned read/write halves so the reader and writer tasks can
    /// run independently on the same connection.
    pub(crate) fn into_split(self) -> (AsyncConnReadHalf, AsyncConnWriteHalf) {
        match self {
            Self::Plain(s) => {
                let (r, w) = s.into_split();
                (AsyncConnReadHalf::Plain(r), AsyncConnWriteHalf::Plain(w))
            }
            Self::Tls(s) => {
                let (r, w) = tokio::io::split(s); // TlsStream has no into_split
                (AsyncConnReadHalf::Tls(r), AsyncConnWriteHalf::Tls(w))
            }
        }
    }
}

impl AsyncRead for AsyncConnStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Self::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for AsyncConnStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Self::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_flush(cx),
            Self::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Self::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Read half of a split `AsyncConnStream` — owned by the reader task.
pub(crate) enum AsyncConnReadHalf {
    Plain(tokio::net::tcp::OwnedReadHalf),
    Tls(tokio::io::ReadHalf<TlsStream<TcpStream>>),
}

impl AsyncRead for AsyncConnReadHalf {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(r) => Pin::new(r).poll_read(cx, buf),
            Self::Tls(r) => Pin::new(r).poll_read(cx, buf),
        }
    }
}

/// Write half of a split `AsyncConnStream` — owned by the writer task;
/// forwards vectored writes (`writev`) for the framing path.
pub(crate) enum AsyncConnWriteHalf {
    Plain(tokio::net::tcp::OwnedWriteHalf),
    Tls(tokio::io::WriteHalf<TlsStream<TcpStream>>),
}

impl AsyncWrite for AsyncConnWriteHalf {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(r) => Pin::new(r).poll_write(cx, buf),
            Self::Tls(r) => Pin::new(r).poll_write(cx, buf),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> std::task::Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(r) => Pin::new(r).poll_write_vectored(cx, bufs),
            Self::Tls(r) => Pin::new(r).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Self::Plain(r) => r.is_write_vectored(),
            Self::Tls(r) => r.is_write_vectored(),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(r) => Pin::new(r).poll_flush(cx),
            Self::Tls(r) => Pin::new(r).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(r) => Pin::new(r).poll_shutdown(cx),
            Self::Tls(r) => Pin::new(r).poll_shutdown(cx),
        }
    }
}

/// Wrap an already-connected TCP stream as a plaintext `AsyncConnStream`.
pub(crate) fn wrap_plain(tcp: TcpStream) -> AsyncConnStream {
    AsyncConnStream::Plain(tcp)
}

/// Server-side TLS handshake over an accepted TCP stream.
pub(crate) async fn accept_tls_async(
    tcp: TcpStream,
    cfg: Arc<ServerConfig>,
) -> io::Result<AsyncConnStream> {
    let acceptor = TlsAcceptor::from(cfg);
    let server_stream = acceptor
        .accept(tcp)
        .await
        .map_err(|e| transport_io_error(TransportErrorCode::TlsHandshakeFailed, e.to_string()))?;
    Ok(AsyncConnStream::Tls(TlsStream::Server(server_stream)))
}

/// Client-side TLS handshake, validating the server against `server_name` (SNI).
pub(crate) async fn connect_tls_async(
    tcp: TcpStream,
    config: Arc<ClientConfig>,
    server_name: &str,
) -> io::Result<AsyncConnStream> {
    let sni = ServerName::try_from(server_name.to_string())
        .map_err(|e| transport_io_error(TransportErrorCode::TlsConfigError, e.to_string()))?;

    let connector = TlsConnector::from(config);
    let client_stream = connector
        .connect(sni, tcp)
        .await
        .map_err(|e| transport_io_error(TransportErrorCode::TlsHandshakeFailed, e.to_string()))?;
    Ok(AsyncConnStream::Tls(TlsStream::Client(client_stream)))
}

#[cfg(test)]
mod tls_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use rcgen::{generate_simple_self_signed, CertifiedKey};
    use rustls::pki_types::CertificateDer;
    use rustls::pki_types::PrivateKeyDer;

    // ── cert helpers ────────────────────────────────────────────────────────

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

    // ── TLS happy path: full handshake + echo roundtrip ─────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn handshake_and_roundtrip() {
        let certs = gen_certs();
        let s_cfg = server_cfg(&certs);
        let c_cfg = client_cfg(&certs);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept");
            let mut server = accept_tls_async(tcp, s_cfg).await.expect("accept_tls");
            let mut buf = [0u8; 32];
            let n = server.read(&mut buf).await.expect("server read");
            server.write_all(&buf[..n]).await.expect("server write");
            server.flush().await.expect("server flush");
            buf[..n].to_vec()
        });

        let tcp = TcpStream::connect(addr).await.expect("connect");
        let mut client = connect_tls_async(tcp, c_cfg, "localhost").await.expect("connect_tls");

        let payload = b"hello-tls";
        client.write_all(payload).await.expect("client write");
        client.flush().await.expect("client flush");

        let mut echoed = vec![0u8; payload.len()];
        client.read_exact(&mut echoed).await.expect("client read");
        assert_eq!(echoed, payload);
        assert!(client.is_tls());
        assert_eq!(client.peer_addr().unwrap(), addr);

        let received = server.await.expect("server task");
        assert_eq!(received, payload);
    }

    // ── client rejects untrusted server cert ────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn untrusted_certificate_rejected() {
        let server_certs = gen_certs();
        let other_certs = gen_certs(); // different CA
        let s_cfg = server_cfg(&server_certs);
        let c_cfg = client_cfg(&other_certs);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept");
            // Expected to fail; ignore result.
            let _ = accept_tls_async(tcp, s_cfg).await;
        });

        let tcp = TcpStream::connect(addr).await.expect("connect");
        let result = connect_tls_async(tcp, c_cfg, "localhost").await;
        assert!(result.is_err(), "client should reject server cert it does not trust",);

        let _ = server.await;
    }

    // ── into_split returns independently usable halves ──────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn into_split_yields_independent_halves() {
        let certs = gen_certs();
        let s_cfg = server_cfg(&certs);
        let c_cfg = client_cfg(&certs);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept");
            let mut server = accept_tls_async(tcp, s_cfg).await.expect("accept_tls");
            let mut buf = [0u8; 16];
            let n = server.read(&mut buf).await.expect("server read");
            server.write_all(&buf[..n]).await.expect("server write");
            server.flush().await.ok();
        });

        let tcp = TcpStream::connect(addr).await.expect("connect");
        let client = connect_tls_async(tcp, c_cfg, "localhost").await.expect("connect_tls");
        let (mut read_half, mut write_half) = client.into_split();

        // Writer half spawned independently — proves halves don't alias.
        let writer = tokio::spawn(async move {
            write_half.write_all(b"shared").await.expect("write");
            write_half.flush().await.ok();
        });

        let mut echoed = vec![0u8; 6];
        read_half.read_exact(&mut echoed).await.expect("read");
        assert_eq!(&echoed, b"shared");

        writer.await.unwrap();
        let _ = server.await;
    }

    // ── plain TCP roundtrip via wrap_plain ──────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn wrap_plain_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept");
            let mut s = wrap_plain(tcp);
            let mut buf = [0u8; 16];
            let n = s.read(&mut buf).await.expect("read");
            s.write_all(&buf[..n]).await.expect("write");
            buf[..n].to_vec()
        });

        let tcp = TcpStream::connect(addr).await.expect("connect");
        let mut client = wrap_plain(tcp);
        assert!(!client.is_tls());

        client.write_all(b"plain").await.expect("write");
        let mut echoed = vec![0u8; 5];
        client.read_exact(&mut echoed).await.expect("read");
        assert_eq!(&echoed, b"plain");

        let received = server.await.expect("server task");
        assert_eq!(&received, b"plain");
    }
}
