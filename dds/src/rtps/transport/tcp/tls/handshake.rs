//! TLS handshake over an established TCP stream.
//!
//! Wraps a connected `TcpStream` in a `rustls` session and yields an
//! `AsyncConnStream::Tls`: `accept_tls_async` drives the server side of the
//! handshake, `connect_tls_async` the client side (validating the server
//! against the SNI name). The resulting stream feeds the same conn_actor
//! read/write path as a plaintext connection.

use std::io;
use std::sync::Arc;

use tokio::net::TcpStream;
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ServerConfig};

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::tcp::stream::AsyncConnStream;

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
mod tests {
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
}
