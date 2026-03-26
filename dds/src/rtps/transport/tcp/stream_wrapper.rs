//! TCP stream abstraction for future TLS support.
//!
//! Provides a trait that abstracts over plain TCP and (future) TLS streams,
//! allowing the transport layer to switch between them without changing
//! connection management or framing logic.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};

/// Abstraction over TCP stream types (Plain TCP / future TLS).
///
/// All TCP connections in the transport layer use this trait instead of
/// `TcpStream` directly, so that TLS can be added later by implementing
/// this trait for `TlsStream<TcpStream>`.
pub(crate) trait TcpStreamWrapper: Read + Write + Send {
    /// Get the remote peer's address
    fn peer_addr(&self) -> io::Result<SocketAddr>;

    /// Set TCP_NODELAY option
    fn set_nodelay(&self, nodelay: bool) -> io::Result<()>;

    /// Set write timeout
    fn set_write_timeout(&self, dur: Option<std::time::Duration>) -> io::Result<()>;

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

    fn try_clone_box(&self) -> io::Result<Box<dyn TcpStreamWrapper>> {
        let cloned = self.inner.try_clone()?;
        Ok(Box::new(PlainTcpStream::new(cloned)))
    }
}

/// Create a TcpStreamWrapper from a raw TcpStream.
/// In the future, this will check TLS configuration and perform TLS handshake if enabled.
pub(crate) fn wrap_stream(stream: TcpStream) -> Box<dyn TcpStreamWrapper> {
    // TODO: When TLS is implemented:
    // if is_tls_enabled() {
    //     perform TLS handshake and return TlsStream wrapper
    // }
    Box::new(PlainTcpStream::new(stream))
}
