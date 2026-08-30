//! One outbound TCP connection, written from the calling thread.
//!
//! The socket is non-blocking, so the write itself is the congestion test: a
//! frame the send queue cannot take at all is reported as stalled and the
//! caller drops it, which is what keeps one unresponsive peer from holding up
//! sends to the others. Only a frame the queue took *part* of has to be
//! finished -- the stream would otherwise desynchronize mid-frame -- and that
//! wait is bounded by `block_timeout`; exceeding it costs the connection.

use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rustls::pki_types::ServerName;

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::tcp::connection_registry::{apply_socket_tuning, TcpSocketTuning};
use crate::rtps::transport::tcp::tls::TlsConfig;

/// Longest a connect or a half-written frame may wait when no bound is
/// configured.
const UNBOUNDED_WAIT: Duration = Duration::from_secs(5);

enum Wire {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Wire {
    fn socket(&self) -> &TcpStream {
        match self {
            Wire::Plain(stream) => stream,
            Wire::Tls(stream) => &stream.sock,
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Wire::Plain(stream) => stream.write(bytes),
            Wire::Tls(stream) => stream.write(bytes),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Wire::Plain(stream) => stream.flush(),
            Wire::Tls(stream) => stream.flush(),
        }
    }
}

/// What one `send` did with the frame.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SendOutcome {
    /// The whole frame reached the socket.
    Sent,
    /// The send queue had no room at all. The frame never started, so nothing
    /// is half-written and the connection stays usable.
    Stalled,
}

pub(crate) struct OutboundConnection {
    state: Mutex<ConnectionState>,
    block_timeout: Option<Duration>,
    failed: AtomicBool,
}

struct ConnectionState {
    wire: Wire,
}

impl OutboundConnection {
    /// Connect, tune, and complete the TLS handshake if one is configured.
    /// `block_timeout` bounds the whole thing.
    pub(crate) fn connect(
        addr: SocketAddr,
        tuning: &TcpSocketTuning,
        tls_config: Option<&TlsConfig>,
        block_timeout: Option<Duration>,
    ) -> io::Result<Self> {
        let wait = block_timeout.unwrap_or(UNBOUNDED_WAIT);
        let socket = TcpStream::connect_timeout(&addr, wait).map_err(|error| {
            transport_io_error(
                TransportErrorCode::TcpConnectionTimeout,
                format!("TCP connect to {addr} failed: {error}"),
            )
        })?;
        apply_socket_tuning(&socket, tuning);

        let wire = match tls_config {
            Some(config) => Wire::Tls(Box::new(handshake(socket, config, wait)?)),
            None => {
                socket.set_nonblocking(true)?;
                Wire::Plain(socket)
            }
        };

        Ok(Self {
            state: Mutex::new(ConnectionState { wire }),
            block_timeout,
            failed: AtomicBool::new(false),
        })
    }

    pub(crate) fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    /// Write one already-framed message. Never waits on a peer that has no room
    /// at all; waits only to finish a frame the socket already started, and
    /// only up to `block_timeout`.
    pub(crate) fn send(&self, frame: &[u8]) -> io::Result<SendOutcome> {
        let mut state = self.lock();

        let written = match state.wire.write(frame) {
            Ok(0) if !frame.is_empty() => return Ok(SendOutcome::Stalled),
            Ok(written) => written,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                return Ok(SendOutcome::Stalled)
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => 0,
            Err(error) => return Err(self.fail(error)),
        };

        if written == frame.len() {
            return match state.flush_started(self.block_timeout) {
                Ok(()) => Ok(SendOutcome::Sent),
                Err(error) => Err(self.fail(error)),
            };
        }

        match state.finish_started_frame(&frame[written..], self.block_timeout) {
            Ok(()) => Ok(SendOutcome::Sent),
            Err(error) => Err(self.fail(error)),
        }
    }

    fn fail(&self, error: io::Error) -> io::Error {
        self.failed.store(true, Ordering::Release);
        error
    }

    fn lock(&self) -> MutexGuard<'_, ConnectionState> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl ConnectionState {
    /// The socket took part of a frame, so the rest cannot be dropped without
    /// desynchronizing the stream. Block for it, bounded.
    fn finish_started_frame(
        &mut self,
        rest: &[u8],
        block_timeout: Option<Duration>,
    ) -> io::Result<()> {
        let socket = self.wire.socket();
        socket.set_write_timeout(block_timeout)?;
        socket.set_nonblocking(false)?;

        let result = self.write_all_blocking(rest);

        let socket = self.wire.socket();
        let _ = socket.set_write_timeout(None);
        let _ = socket.set_nonblocking(true);
        result
    }

    fn write_all_blocking(&mut self, rest: &[u8]) -> io::Result<()> {
        let mut sent = 0;
        while sent < rest.len() {
            match self.wire.write(&rest[sent..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "TCP write returned 0 mid-frame",
                    ))
                }
                Ok(written) => sent += written,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        self.wire.flush()
    }

    /// TLS keeps accepted plaintext in its own buffer, so a fully accepted
    /// frame can still be unsent. That is the same mid-frame state as a partial
    /// write and gets the same bounded wait.
    fn flush_started(&mut self, block_timeout: Option<Duration>) -> io::Result<()> {
        match self.wire.flush() {
            Ok(()) => Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                self.finish_started_frame(&[], block_timeout)
            }
            Err(error) => Err(error),
        }
    }
}

fn handshake(
    socket: TcpStream,
    config: &TlsConfig,
    wait: Duration,
) -> io::Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>> {
    let client_config = config.build_client_config().map_err(|error| {
        transport_io_error(TransportErrorCode::TlsConfigError, error.to_string())
    })?;
    let server_name = ServerName::try_from(config.server_name().to_string()).map_err(|error| {
        transport_io_error(TransportErrorCode::TlsConfigError, error.to_string())
    })?;
    let connection =
        rustls::ClientConnection::new(client_config, server_name).map_err(|error| {
            transport_io_error(TransportErrorCode::TlsHandshakeFailed, error.to_string())
        })?;

    socket.set_read_timeout(Some(wait))?;
    socket.set_write_timeout(Some(wait))?;
    let mut stream = rustls::StreamOwned::new(connection, socket);
    while stream.conn.is_handshaking() {
        stream.conn.complete_io(&mut stream.sock).map_err(|error| {
            transport_io_error(
                TransportErrorCode::TlsHandshakeFailed,
                format!("TLS handshake failed: {error}"),
            )
        })?;
    }
    stream.sock.set_read_timeout(None)?;
    stream.sock.set_write_timeout(None)?;
    stream.sock.set_nonblocking(true)?;
    Ok(stream)
}
