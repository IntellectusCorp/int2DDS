//! Connection stream that unifies plain TCP and TLS behind one type.
//!
//! `AsyncConnStream` lets the rest of the TCP transport read, write, and split
//! a connection without caring whether it is plaintext or TLS-encrypted: the
//! Plain/Tls branch is resolved once here, and every downstream task just uses
//! the `AsyncRead` / `AsyncWrite` impls. `into_split` yields owned read/write
//! halves so the conn_actor's reader and writer tasks can each own one end, and
//! the write half forwards vectored writes (`writev`) used by the framing path.

use std::net::SocketAddr;
use std::{io, pin::Pin};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsStream;

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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

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
