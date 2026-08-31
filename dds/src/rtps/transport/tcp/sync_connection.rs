//! One outbound TCP connection, written from the calling thread.
//!
//! The socket is non-blocking, so the write itself is the congestion test: a
//! frame the send queue cannot take at all is reported as stalled and the
//! caller drops it, which is what keeps one unresponsive peer from holding up
//! sends to the others.

use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::tcp::connection_registry::{apply_socket_tuning, TcpSocketTuning};
use crate::rtps::transport::tcp::tls::TlsConfig;

/// How long a frame the socket took only part of may sit unfinished.
const PENDING_TAIL_DEADLINE: Duration = Duration::from_millis(1000);

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
    failed: AtomicBool,
}

struct ConnectionState {
    wire: Wire,
    /// Tail of a frame the socket accepted only part of. TLS keeps its own
    /// unsent bytes instead, so this stays empty there.
    pending: Vec<u8>,
    pending_since: Option<Instant>,
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
            transport_io_error(
                TransportErrorCode::TcpConnectionTimeout,
                format!("TCP connect to {addr} failed: {error}"),
            )
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
            state: Mutex::new(ConnectionState { wire, pending: Vec::new(), pending_since: None }),
            failed: AtomicBool::new(false),
        })
    }

    pub(crate) fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    /// Write one already-framed message. Never waits on the peer: a frame the
    /// socket has no room for at all is refused, and one it takes only part of
    /// leaves its tail behind for the next call.
    pub(crate) fn send(&self, frame: &[u8]) -> io::Result<SendOutcome> {
        let mut state = self.lock();

        match state.drain_pending() {
            Ok(true) => {}
            Ok(false) if state.tail_expired() => {
                return Err(self.fail(transport_io_error(
                    TransportErrorCode::TcpConnectionTimeout,
                    format!(
                        "peer {} left a frame unfinished for over {:?}",
                        state.peer(),
                        PENDING_TAIL_DEADLINE
                    ),
                )))
            }
            Ok(false) => return Ok(SendOutcome::Stalled),
            Err(error) => return Err(self.fail(error)),
        }

        let written = match state.wire.write(frame) {
            Ok(0) if !frame.is_empty() => return Ok(SendOutcome::Stalled),
            Ok(written) => written,
            Err(error) if retryable(&error) => return Ok(SendOutcome::Stalled),
            Err(error) => return Err(self.fail(error)),
        };
        if written < frame.len() {
            state.pending.extend_from_slice(&frame[written..]);
            state.pending_since = Some(Instant::now());
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
    /// Push out the tail an earlier frame left behind. It owns the stream
    /// position, so nothing new may go out until it is gone. Reports whether
    /// the stream is free.
    fn drain_pending(&mut self) -> io::Result<bool> {
        while !self.pending.is_empty() {
            match self.wire.write(&self.pending) {
                Ok(0) => return Ok(false),
                Ok(written) => drop(self.pending.drain(..written)),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        self.pending_since = None;
        Ok(true)
    }

    fn tail_expired(&self) -> bool {
        self.pending_since.is_some_and(|since| since.elapsed() >= PENDING_TAIL_DEADLINE)
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

    /// A peer that stops reading must be refused rather than waited on, and the
    /// tail of the frame its socket took only part of must go out on its own
    /// once the peer reads again.
    #[test]
    fn a_refused_peer_recovers_once_it_reads_again() {
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

        let frame = vec![0u8; 32 * 1024];
        let mut refused = false;
        for _ in 0..64 {
            let call = Instant::now();
            let outcome = connection.send(&frame).unwrap();
            assert!(call.elapsed() < Duration::from_millis(500), "a send waited on the peer");
            if outcome == SendOutcome::Stalled {
                refused = true;
                break;
            }
        }
        assert!(refused, "a peer that never reads must eventually refuse frames");

        accepted.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        let mut sink = vec![0u8; 64 * 1024];
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "the connection never recovered");
            let _ = accepted.read(&mut sink);
            if connection.send(&frame).unwrap() == SendOutcome::Sent {
                break;
            }
        }
        assert!(!connection.is_failed());

        drop(connection);
        drop(accepted);
        drop(listener);
    }

    /// A peer that never frees room must eventually cost the connection: the
    /// tail cannot be dropped and cannot be waited on, so the connection goes.
    #[test]
    fn a_tail_that_never_leaves_costs_the_connection() {
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
        for _ in 0..64 {
            if connection.send(&frame).unwrap() == SendOutcome::Stalled {
                break;
            }
        }
        assert!(
            !connection.state.lock().unwrap().pending.is_empty(),
            "the peer must have taken part of a frame"
        );

        std::thread::sleep(PENDING_TAIL_DEADLINE + Duration::from_millis(100));
        let error = connection.send(&frame).expect_err("the connection must be given up");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(connection.is_failed());

        drop(connection);
        drop(accepted);
        drop(listener);
    }
}
