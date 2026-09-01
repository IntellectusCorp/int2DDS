//! One outbound TCP connection, written from the calling thread.
//!
//! The socket is non-blocking and never decides on its own that a frame is
//! expendable. Bytes the send queue will not take are held in order and go out
//! as room appears, so a peer that has fallen behind costs latency rather than
//! data. A frame is refused only once the peer owes more than the queue
//! budget, which bounds memory rather than answering congestion.

use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::tcp::connection_registry::{apply_socket_tuning, TcpSocketTuning};
use crate::rtps::transport::tcp::tls::TlsConfig;

/// How much a peer may owe the stream before frames are refused instead of
/// queued. This is a bound on memory, not a congestion threshold, so it sits
/// far above any backlog a peer that is merely behind can build up.
const SEND_QUEUE_BYTE_BUDGET: usize = 8 * 1024 * 1024;

/// How long a peer may take no bytes at all while it still owes the stream.
/// Progress of any size resets it, so only a peer that has stopped entirely
/// costs the connection.
const SEND_STALL_DEADLINE: Duration = Duration::from_secs(5);

/// How far the queue's read cursor may run before the unsent bytes are moved
/// to the front, so draining a long backlog does not memmove on every write.
const QUEUE_COMPACT_THRESHOLD: usize = 64 * 1024;

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
    /// The whole frame is on its way: whatever the socket would not take now
    /// is held in order and goes out as room appears.
    Sent,
    /// The peer already owes more than the queue budget. The frame never
    /// started, so nothing is half-written and the connection stays usable.
    Stalled,
}

pub(crate) struct OutboundConnection {
    state: Mutex<ConnectionState>,
    failed: AtomicBool,
}

struct ConnectionState {
    wire: Wire,
    /// Bytes already owed to the stream, in order. TLS keeps its own unsent
    /// bytes instead, so this stays empty there.
    pending: Vec<u8>,
    /// How much of `pending` has reached the socket.
    pending_start: usize,
    /// When the socket last took a byte, so a peer that is behind can be told
    /// apart from one that has stopped.
    last_progress: Instant,
}

impl OutboundConnection {
    /// Connect, tune, and complete the TLS handshake if one is configured.
    /// Each of the two steps has its own bound; neither outlives this call.
    pub(crate) fn connect(
        addr: SocketAddr,
        tuning: &TcpSocketTuning,
        tls_config: Option<&TlsConfig>,
        connect_timeout: Duration,
        tls_handshake_timeout: Duration,
    ) -> io::Result<Self> {
        let socket = TcpStream::connect_timeout(&addr, connect_timeout).map_err(|error| {
            let code = match error.kind() {
                io::ErrorKind::ConnectionRefused => TransportErrorCode::TcpConnectionRefused,
                _ => TransportErrorCode::TcpConnectionTimeout,
            };
            transport_io_error(code, format!("TCP connect to {addr} failed: {error}"))
        })?;
        apply_socket_tuning(&socket, tuning);

        let wire = match tls_config {
            Some(config) => Wire::Tls(Box::new(handshake(socket, config, tls_handshake_timeout)?)),
            None => {
                socket.set_nonblocking(true)?;
                Wire::Plain(socket)
            }
        };

        Ok(Self {
            state: Mutex::new(ConnectionState {
                wire,
                pending: Vec::new(),
                pending_start: 0,
                last_progress: Instant::now(),
            }),
            failed: AtomicBool::new(false),
        })
    }

    pub(crate) fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    /// Write one already-framed message. Never waits on the peer: whatever the
    /// socket will not take now is queued in order and goes out as room
    /// appears. Nothing but a send drains the queue, so every call pushes the
    /// backlog before it looks at its own frame.
    pub(crate) fn send(&self, frame: &[u8]) -> io::Result<SendOutcome> {
        let mut state = self.lock();

        if let Err(error) = state.drain_pending() {
            return Err(self.fail(error));
        }
        if state.has_stopped_taking_bytes() {
            let peer = state.peer();
            return Err(self.fail(transport_io_error(
                TransportErrorCode::TcpConnectionTimeout,
                format!("peer {peer} took no bytes for over {SEND_STALL_DEADLINE:?}"),
            )));
        }

        // Queued bytes own the stream position, so a frame may only go straight
        // to the socket once they are gone.
        let written = if state.queued() == 0 {
            match state.write_some(frame) {
                Ok(written) => written,
                Err(error) => return Err(self.fail(error)),
            }
        } else {
            0
        };

        let tail = &frame[written..];
        if !tail.is_empty() {
            if state.queued() + tail.len() > SEND_QUEUE_BYTE_BUDGET {
                return Ok(SendOutcome::Stalled);
            }
            state.pending.extend_from_slice(tail);
        }

        match state.wire.flush() {
            Ok(()) => Ok(SendOutcome::Sent),
            // TLS holds what it could not write; the next send retries it
            // before anything new goes out.
            Err(error) if retryable(&error) => Ok(SendOutcome::Sent),
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
    fn queued(&self) -> usize {
        self.pending.len() - self.pending_start
    }

    /// Hand the socket as much of `bytes` as it will take, noting any progress.
    fn write_some(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            match self.wire.write(bytes) {
                Ok(0) => return Ok(0),
                Ok(written) => {
                    self.last_progress = Instant::now();
                    return Ok(written);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(0),
                Err(error) => return Err(error),
            }
        }
    }

    /// Push out what the peer is already owed. It holds the stream position, so
    /// nothing new may go out until it is gone.
    fn drain_pending(&mut self) -> io::Result<()> {
        while self.pending_start < self.pending.len() {
            match self.wire.write(&self.pending[self.pending_start..]) {
                Ok(0) => break,
                Ok(written) => {
                    self.pending_start += written;
                    self.last_progress = Instant::now();
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }

        if self.pending_start == self.pending.len() {
            self.pending.clear();
            self.pending_start = 0;
        } else if self.pending_start >= QUEUE_COMPACT_THRESHOLD {
            self.pending.drain(..self.pending_start);
            self.pending_start = 0;
        }
        Ok(())
    }

    fn has_stopped_taking_bytes(&self) -> bool {
        self.queued() > 0 && self.last_progress.elapsed() >= SEND_STALL_DEADLINE
    }

    fn peer(&self) -> String {
        match self.wire.socket().peer_addr() {
            Ok(addr) => addr.to_string(),
            Err(_) => "<disconnected>".to_string(),
        }
    }
}

/// The socket refused the bytes for now but the connection is intact.
fn retryable(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::Instant;

    /// A peer that stops reading must cost latency, not data: its frames queue
    /// instead of being refused, no call waits on it, and every byte arrives
    /// once it reads again.
    #[test]
    fn a_peer_that_stops_reading_loses_no_bytes() {
        // The accepted socket inherits this receive buffer, so the peer cannot
        // absorb the traffic in kernel memory and the send queue really fills.
        let listener =
            socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).unwrap();
        listener.set_recv_buffer_size(2 * 1024).unwrap();
        listener.bind(&"127.0.0.1:0".parse::<SocketAddr>().unwrap().into()).unwrap();
        listener.listen(8).unwrap();
        let listener = std::net::TcpListener::from(listener);
        let peer = listener.local_addr().unwrap();

        let tuning = TcpSocketTuning { so_sndbuf: Some(4 * 1024), ..TcpSocketTuning::default() };
        let connection = OutboundConnection::connect(
            peer,
            &tuning,
            None,
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let (mut accepted, _) = listener.accept().unwrap();

        const FRAMES: usize = 32;
        let frame = vec![0x5au8; 32 * 1024];
        for _ in 0..FRAMES {
            let call = Instant::now();
            assert_eq!(connection.send(&frame).unwrap(), SendOutcome::Sent);
            assert!(call.elapsed() < Duration::from_millis(500), "a send waited on the peer");
        }

        // Nothing but a send drains the queue, so each of the peer's reads is
        // paired with one.
        accepted.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        let expected = FRAMES * frame.len();
        let mut received = 0usize;
        let mut sink = vec![0u8; 64 * 1024];
        let deadline = Instant::now() + Duration::from_secs(60);
        while received < expected {
            assert!(Instant::now() < deadline, "only {received} of {expected} bytes arrived");
            match accepted.read(&mut sink) {
                Ok(0) => break,
                Ok(read) => received += read,
                Err(_) => {}
            }
            connection.send(&[]).unwrap();
        }
        assert_eq!(received, expected, "a peer that fell behind must lose no bytes");
        assert!(!connection.is_failed());

        drop(connection);
        drop(accepted);
        drop(listener);
    }

    /// A backlog on its own must never cost the connection. Only a peer that
    /// has stopped taking bytes altogether may.
    #[test]
    fn a_peer_that_takes_nothing_costs_the_connection() {
        let listener =
            socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).unwrap();
        listener.set_recv_buffer_size(2 * 1024).unwrap();
        listener.bind(&"127.0.0.1:0".parse::<SocketAddr>().unwrap().into()).unwrap();
        listener.listen(8).unwrap();
        let listener = std::net::TcpListener::from(listener);
        let peer = listener.local_addr().unwrap();

        let tuning = TcpSocketTuning { so_sndbuf: Some(4 * 1024), ..TcpSocketTuning::default() };
        let connection = OutboundConnection::connect(
            peer,
            &tuning,
            None,
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let accepted = listener.accept().unwrap().0;

        let frame = vec![0u8; 32 * 1024];
        let mut backlogged = false;
        for _ in 0..64 {
            assert_eq!(connection.send(&frame).unwrap(), SendOutcome::Sent);
            if connection.state.lock().unwrap().queued() > 256 * 1024 {
                backlogged = true;
                break;
            }
        }
        assert!(backlogged, "the peer never stopped taking bytes");
        assert!(!connection.is_failed(), "a backlog alone must not cost the connection");

        connection.state.lock().unwrap().last_progress = Instant::now() - SEND_STALL_DEADLINE;
        let error = connection.send(&frame).expect_err("the connection must be given up");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(connection.is_failed());

        drop(connection);
        drop(accepted);
        drop(listener);
    }
}
