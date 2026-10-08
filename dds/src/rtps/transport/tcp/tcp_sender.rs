//! Outbound TCP connections, written from the calling thread.
//!
//! A peer has at most two lazy outbound slots, keyed by the frame kind. A slot
//! is opened by the first frame that needs it and is dropped as soon as a write
//! fails. There is no application-level connection handshake.
//!
//! Opening a slot is the one blocking step, so the discovery fan-out never takes
//! it on the calling thread: it hands the dial to a worker and moves on to the
//! next peer. A peer nobody answers therefore costs the announcement nothing,
//! however many such peers are configured.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{mpsc, Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use log::{debug, warn};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::tcp::connection_registry::ConnectionRegistry;
use crate::rtps::transport::tcp::framing::{encode_frame, validate_payload_size, TcpFrameKind};
use crate::rtps::transport::tcp::sync_connection::{OutboundConnection, SendOutcome};
use crate::rtps::transport::tcp::tls::TlsConfig;
use crate::rtps::transport::TcpConfig;

#[derive(Default)]
struct DropCounter {
    count: AtomicU64,
    last_warn: StdMutex<Option<Instant>>,
}

impl DropCounter {
    fn record(&self, cause: &str, addr: SocketAddr, kind: TcpFrameKind) {
        let total = self.count.fetch_add(1, Ordering::Relaxed) + 1;
        let now = Instant::now();
        let mut last = match self.last_warn.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if last.is_none_or(|previous| now.duration_since(previous) >= Duration::from_secs(1)) {
            *last = Some(now);
            warn!("TcpSender: dropped {:?} frame to {}: {} ({} total)", kind, addr, cause, total);
        }
    }

    #[cfg(test)]
    fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

#[derive(Default)]
struct SendStats {
    connect_failed: DropCounter,
    backoff: DropCounter,
    peer_stalled: DropCounter,
    write_error: DropCounter,
}

type ConnectionKey = (SocketAddr, TcpFrameKind);

pub(crate) struct TcpSender {
    working_ips: Vec<String>,
    listener_port: u16,

    connect_timeout: Duration,
    tls_handshake_timeout: Duration,

    stats: Arc<SendStats>,
    tls_config: Option<Arc<TlsConfig>>,
    shared: Arc<ConnectionRegistry>,
    connections: Arc<DashMap<ConnectionKey, Arc<OutboundConnection>>>,
    shutdown: Arc<AtomicBool>,

    pending_dials: Arc<DashMap<ConnectionKey, Vec<u8>>>,
    dial_wakeup: Sender<ConnectionKey>,
}

/// How long the dial worker waits before checking whether it should still run.
const DIAL_WORKER_IDLE_TICK: Duration = Duration::from_millis(200);

impl TcpSender {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        _domain_id: u32,
        _participant_id: u32,
        working_ips: Vec<String>,
        listener_port: u16,
        _local_guid_prefix: GuidPrefix,
        tls_config: Option<Arc<TlsConfig>>,
        shared: Arc<ConnectionRegistry>,
        tcp_config: &TcpConfig,
        shutdown: Arc<AtomicBool>,
    ) -> Arc<Self> {
        let (dial_wakeup, dial_requests) = mpsc::channel();
        let sender = Arc::new(Self {
            working_ips,
            listener_port,
            connect_timeout: tcp_config.connect_timeout,
            tls_handshake_timeout: tcp_config.tls_handshake_timeout,
            stats: Arc::new(SendStats::default()),
            tls_config,
            shared,
            connections: Arc::new(DashMap::new()),
            shutdown,
            pending_dials: Arc::new(DashMap::new()),
            dial_wakeup,
        });
        Self::spawn_dial_worker(&sender, dial_requests);
        sender
    }

    fn spawn_dial_worker(sender: &Arc<Self>, requests: Receiver<ConnectionKey>) {
        let weak = Arc::downgrade(sender);
        let spawned = std::thread::Builder::new().name("tcp_discovery_dial".to_string()).spawn(
            move || loop {
                let requested = match requests.recv_timeout(DIAL_WORKER_IDLE_TICK) {
                    Ok(key) => Some(key),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                };
                let Some(sender) = weak.upgrade() else {
                    return;
                };
                if sender.shutdown.load(Ordering::Acquire) {
                    return;
                }
                if let Some(key) = requested {
                    sender.dial_pending(key);
                }
            },
        );
        if let Err(error) = spawned {
            warn!("TcpSender: no dial worker, discovery will connect inline: {}", error);
        }
    }

    /// Announce to one peer without ever waiting on a connection.
    ///
    /// An established peer is written to on this thread, which is the whole
    /// point of keeping the slot. A peer with no slot yet is handed to the dial
    /// worker with this announcement attached, so it still receives this round
    /// once the connection comes up, and an unreachable peer costs the caller
    /// nothing beyond the handoff.
    pub(crate) fn send_to_discovery(
        self: &Arc<Self>,
        addr: &SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        let addr = *addr;
        let key = (addr, TcpFrameKind::Discovery);
        if self.is_self_connection(&addr) || self.live_connection(&key).is_some() {
            return self.send_to(addr, TcpFrameKind::Discovery, data);
        }

        validate_payload_size(data.len())?;
        let mut frame = Vec::with_capacity(data.len() + 8);
        encode_frame(data, &mut frame)?;
        self.pending_dials.insert(key, frame);
        let _ = self.dial_wakeup.send(key);
        Ok(())
    }

    /// Open the slot a queued announcement is waiting on and deliver it.
    ///
    /// A failure here is ordinary — a configured peer that is not up yet — so it
    /// only feeds the backoff. The announcement is dropped rather than held,
    /// since the next one supersedes it anyway.
    fn dial_pending(self: &Arc<Self>, key: ConnectionKey) {
        let Some((_, frame)) = self.pending_dials.remove(&key) else {
            return;
        };
        if self.live_connection(&key).is_some() {
            return;
        }
        if let Ok(connection) = self.open_connection(key) {
            let _ = connection.send(&frame);
        }
    }

    pub(crate) fn send_to(
        self: &Arc<Self>,
        addr: SocketAddr,
        kind: TcpFrameKind,
        data: &[u8],
    ) -> io::Result<()> {
        validate_payload_size(data.len())?;

        if self.is_self_connection(&addr) {
            self.shared.deliver_to_self(addr, kind, data)?;
            return Ok(());
        }

        let key = (addr, kind);
        let connection = match self.live_connection(&key) {
            Some(connection) => connection,
            None => self.open_connection(key)?,
        };

        let mut frame = Vec::with_capacity(data.len() + 8);
        encode_frame(data, &mut frame)?;

        match self.write_frame(key, &connection, &frame) {
            // The stream the frame was meant for is gone, and a new one starts
            // from a frame boundary, so the whole frame goes out again, once.
            Err(_) if connection.is_failed() => {
                let reopened = self.open_connection(key)?;
                let result = self.write_frame(key, &reopened, &frame);
                if result.is_err() && reopened.is_failed() {
                    self.stats.write_error.record("write error", addr, kind);
                }
                result
            }
            result => result,
        }
    }

    fn write_frame(
        &self,
        key: ConnectionKey,
        connection: &Arc<OutboundConnection>,
        frame: &[u8],
    ) -> io::Result<()> {
        let (addr, kind) = key;
        match connection.send(frame) {
            Ok(SendOutcome::Sent) => Ok(()),
            Ok(SendOutcome::Stalled) => {
                self.stats.peer_stalled.record("peer send queue full", addr, kind);
                Err(transport_io_error(
                    TransportErrorCode::TcpPeerSendQueueFull,
                    format!("peer {addr} has no room for a {kind:?} frame"),
                ))
            }
            Err(error) => {
                if connection.is_failed() {
                    self.evict_connection(key, connection);
                    debug!("TcpSender: write {:?} to {} failed: {}", kind, addr, error);
                }
                Err(error)
            }
        }
    }

    fn live_connection(&self, key: &ConnectionKey) -> Option<Arc<OutboundConnection>> {
        let connection = self.connections.get(key)?;
        if connection.is_failed() {
            return None;
        }
        Some(Arc::clone(&connection))
    }

    fn open_connection(&self, key: ConnectionKey) -> io::Result<Arc<OutboundConnection>> {
        let (addr, kind) = key;
        if let Some(remaining) = self.shared.backoff_remaining(key) {
            self.stats.backoff.record("reconnect backoff", addr, kind);
            return Err(transport_io_error(
                TransportErrorCode::TcpReconnectBackoff,
                format!("peer {addr} is in reconnect backoff for another {remaining:?}"),
            ));
        }

        let connection = OutboundConnection::connect(
            addr,
            &self.shared.tuning,
            self.tls_config.as_deref(),
            self.connect_timeout,
            self.tls_handshake_timeout,
        )
        .inspect_err(|error| {
            self.stats.connect_failed.record("connect failed", addr, kind);
            // A refusal costs the caller one round trip, and discovery never
            // dials more often than its own periods, so holding it back only
            // drops the reply a participant that just reappeared is waiting for.
            let refused = error.kind() == io::ErrorKind::ConnectionRefused;
            if !(refused && kind == TcpFrameKind::Discovery) {
                self.shared.note_connect_failure(key, error);
            }
        })?;

        self.shared.clear_backoff(key);
        debug!("TcpSender: established {:?} connection to {}", kind, addr);

        let connection = Arc::new(connection);
        self.connections.insert(key, Arc::clone(&connection));
        Ok(connection)
    }

    fn evict_connection(&self, key: ConnectionKey, connection: &Arc<OutboundConnection>) {
        self.connections.remove_if(&key, |_, current| Arc::ptr_eq(current, connection));
    }

    /// Discard what a peer's earlier failures left behind.
    ///
    /// Reaching a candidate is guesswork and most guesses fail, so the backoff
    /// those failures set is about an address nobody was known to be at. Once a
    /// participant turns up there the guessing is over, and holding the peer
    /// back for the rest of a delay earned while it did not exist would only
    /// postpone the first real exchange.
    pub(crate) fn forget_failures(&self, addr: SocketAddr) {
        self.shared.clear_peer_backoff(addr);
    }

    pub(crate) fn disconnect_peer(&self, addr: SocketAddr) {
        self.connections.retain(|(peer, _), _| *peer != addr);
        self.shared.clear_peer_backoff(addr);
    }

    pub(crate) fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.connections.clear();
        self.pending_dials.clear();
    }

    pub(crate) fn is_self_connection(&self, addr: &SocketAddr) -> bool {
        if addr.port() != self.listener_port {
            return false;
        }
        match addr.ip() {
            IpAddr::V4(ip) => {
                ip.is_loopback()
                    || self.working_ips.iter().any(|candidate| *candidate == ip.to_string())
            }
            IpAddr::V6(_) => false,
        }
    }

    #[cfg(test)]
    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    #[cfg(test)]
    pub(crate) fn is_shut_down(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }
}

impl Drop for TcpSender {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::tcp::connection_registry::TcpSocketTuning;
    use crate::rtps::transport::tcp::framing::{test_framed, test_message};
    use std::io::Read;
    use std::net::TcpStream;

    fn make_sender(config: TcpConfig) -> Arc<TcpSender> {
        let tuning = TcpSocketTuning {
            nodelay: config.nodelay,
            so_rcvbuf: config.so_rcvbuf,
            so_sndbuf: config.so_sndbuf,
            unacked_timeout: config.unacked_timeout,
            keepalive: None,
        };
        let registry = Arc::new(ConnectionRegistry::new(0, 0, [0; 12], tuning));
        TcpSender::new(
            0,
            0,
            vec!["192.0.2.1".into()],
            7400,
            [0; 12],
            None,
            registry,
            &config,
            Arc::new(AtomicBool::new(false)),
        )
    }

    /// A peer that cannot be reached must not leave a slot behind, and the
    /// second attempt must be refused by the backoff instead of waiting again.
    /// The other frame kind keeps its own slot and is still free to dial.
    #[test]
    fn an_unreachable_peer_leaves_no_connection_and_enters_backoff() {
        let config =
            TcpConfig { connect_timeout: Duration::from_millis(50), ..TcpConfig::default() };
        let sender = make_sender(config);
        let peer: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let message = test_message(0x02, b"payload");

        let first = sender.send_to(peer, TcpFrameKind::UserData, &message).unwrap_err();
        assert_ne!(first.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(sender.connection_count(), 0);

        let second = sender.send_to(peer, TcpFrameKind::UserData, &message).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(sender.stats.backoff.count(), 1);

        let other_kind = sender.send_to(peer, TcpFrameKind::Discovery, &message).unwrap_err();
        assert_ne!(other_kind.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(sender.stats.backoff.count(), 1);
    }

    /// Each frame kind opens its own slot to the same peer, and no more.
    #[test]
    fn cache_has_at_most_two_roles_per_peer() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let peer = listener.local_addr().expect("addr");
        let config =
            TcpConfig { connect_timeout: Duration::from_millis(500), ..TcpConfig::default() };
        let sender = make_sender(config);
        let message = test_message(0x02, b"payload");

        for _ in 0..2 {
            sender.send_to(peer, TcpFrameKind::Discovery, &message).expect("discovery send");
            sender.send_to(peer, TcpFrameKind::UserData, &message).expect("user send");
        }

        assert_eq!(sender.connection_count(), 2);
        sender.shutdown();
    }

    /// A peer that never reads must never hold a sending thread. Frames its
    /// socket cannot take are queued in order rather than dropped, so falling
    /// behind costs the peer latency and costs the caller nothing.
    #[test]
    fn a_peer_that_never_reads_cannot_hold_the_caller() {
        // The accepted socket inherits this receive buffer, so the peer cannot
        // absorb the test's traffic in kernel memory and the send queue really
        // does fill up.
        let listener = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)
            .expect("socket");
        listener.set_recv_buffer_size(2 * 1024).expect("rcvbuf");
        listener.bind(&"127.0.0.1:0".parse::<SocketAddr>().unwrap().into()).expect("bind");
        listener.listen(8).expect("listen");
        let listener = std::net::TcpListener::from(listener);
        let peer = listener.local_addr().expect("addr");
        let bound = Duration::from_millis(50);
        let config =
            TcpConfig { connect_timeout: bound, so_sndbuf: Some(4 * 1024), ..TcpConfig::default() };
        let sender = make_sender(config);
        let message = test_message(0x02, &vec![0u8; 32 * 1024]);

        for _ in 0..64 {
            let call = Instant::now();
            let result = sender.send_to(peer, TcpFrameKind::UserData, &message);
            assert!(
                call.elapsed() < bound * 8,
                "one send held the thread for {:?}",
                call.elapsed()
            );
            result.expect("a peer that is only behind must not cost a frame");
        }

        assert_eq!(sender.stats.peer_stalled.count(), 0);
        sender.shutdown();
    }

    fn accept_within(listener: &std::net::TcpListener, wait: Duration) -> Option<TcpStream> {
        listener.set_nonblocking(true).expect("nonblocking listener");
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).expect("blocking stream");
                    stream.set_read_timeout(Some(wait)).expect("read timeout");
                    return Some(stream);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        }
        None
    }

    fn read_frame(stream: &mut TcpStream, message: &[u8]) -> Vec<u8> {
        let mut frame = vec![0u8; test_framed(message).len()];
        stream.read_exact(&mut frame).expect("a whole frame");
        frame
    }

    /// A peer that went away and came back on the same port must receive the
    /// very first frame sent after its return. The old connection is dead even
    /// though nothing has failed on it yet, so writing into it loses the frame.
    #[test]
    fn the_first_frame_after_a_peer_restarts_on_the_same_port_arrives() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let peer = listener.local_addr().expect("addr");
        let config =
            TcpConfig { connect_timeout: Duration::from_millis(500), ..TcpConfig::default() };
        let sender = make_sender(config);
        let before = test_message(0x02, b"before the restart");
        let after = test_message(0x02, b"after the restart");

        sender.send_to(peer, TcpFrameKind::Discovery, &before).expect("first send");
        let mut old = accept_within(&listener, Duration::from_secs(2)).expect("first connection");
        assert_eq!(read_frame(&mut old, &before), test_framed(&before));

        drop(old);
        drop(listener);
        let restarted = std::net::TcpListener::bind(peer).expect("same port again");
        // Lets the old peer's FIN reach the sender's socket, as it would long
        // before a crashed process has been started again.
        std::thread::sleep(Duration::from_millis(100));

        let result = sender.send_to(peer, TcpFrameKind::Discovery, &after);
        let mut new = accept_within(&restarted, Duration::from_secs(2))
            .expect("the frame never reached the restarted peer: no connection was opened");
        assert_eq!(read_frame(&mut new, &after), test_framed(&after));
        result.expect("send after the restart");

        sender.shutdown();
    }

    /// A discovery frame tried while the peer was down must not hold back the
    /// one sent right after it comes back: that is the reply a restarted
    /// participant needs before it can match.
    #[test]
    fn a_refused_discovery_dial_does_not_hold_back_the_next_frame() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let peer = listener.local_addr().expect("addr");
        let config =
            TcpConfig { connect_timeout: Duration::from_millis(500), ..TcpConfig::default() };
        let sender = make_sender(config);
        let before = test_message(0xC2, b"before the restart");
        let while_down = test_message(0xC2, b"while the peer is down");
        let after = test_message(0xC2, b"after the restart");

        sender.send_to(peer, TcpFrameKind::Discovery, &before).expect("first send");
        let mut old = accept_within(&listener, Duration::from_secs(2)).expect("first connection");
        assert_eq!(read_frame(&mut old, &before), test_framed(&before));

        drop(old);
        drop(listener);
        std::thread::sleep(Duration::from_millis(100));
        let refused = sender.send_to(peer, TcpFrameKind::Discovery, &while_down).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::ConnectionRefused);

        let restarted = std::net::TcpListener::bind(peer).expect("same port again");
        let result = sender.send_to(peer, TcpFrameKind::Discovery, &after);
        let mut new = accept_within(&restarted, Duration::from_secs(2))
            .expect("the frame after the restart was held back by the refused dial");
        assert_eq!(read_frame(&mut new, &after), test_framed(&after));
        result.expect("send after the restart");

        sender.shutdown();
    }

    /// A frame whose write fails must not be dropped: the slot is reopened and
    /// the same frame goes out on the new connection, once.
    #[test]
    fn a_frame_whose_write_fails_is_sent_again_on_a_new_connection() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let peer = listener.local_addr().expect("addr");
        let config =
            TcpConfig { connect_timeout: Duration::from_millis(500), ..TcpConfig::default() };
        let sender = make_sender(config);
        let first = test_message(0x02, b"first");
        let retried = test_message(0x02, b"retried");

        sender.send_to(peer, TcpFrameKind::UserData, &first).expect("first send");
        let mut old = accept_within(&listener, Duration::from_secs(2)).expect("first connection");
        assert_eq!(read_frame(&mut old, &first), test_framed(&first));

        let connection =
            sender.live_connection(&(peer, TcpFrameKind::UserData)).expect("an open slot");
        connection.shutdown_write();

        let result = sender.send_to(peer, TcpFrameKind::UserData, &retried);
        let mut new = accept_within(&listener, Duration::from_secs(2))
            .expect("the failed frame was dropped: no new connection was opened");
        assert_eq!(read_frame(&mut new, &retried), test_framed(&retried));
        result.expect("send that had to be retried");

        new.set_read_timeout(Some(Duration::from_millis(200))).expect("read timeout");
        let mut extra = [0u8; 1];
        assert!(
            !matches!(new.read(&mut extra), Ok(read) if read > 0),
            "the frame went out more than once"
        );

        drop(old);
        sender.shutdown();
    }

    /// A peer that answers nothing must still put discovery into backoff: a
    /// dial that times out holds the calling thread, so the next frame must not
    /// wait for it again.
    #[test]
    fn a_discovery_dial_that_times_out_enters_backoff() {
        // With its accept queue full the kernel drops new SYNs without a reply,
        // as a host that is down would.
        let listener = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)
            .expect("socket");
        listener.bind(&"127.0.0.1:0".parse::<SocketAddr>().unwrap().into()).expect("bind");
        listener.listen(0).expect("listen");
        let peer = listener.local_addr().expect("addr").as_socket().expect("inet addr");
        let _queued: Vec<TcpStream> = (0..2)
            .filter_map(|_| TcpStream::connect_timeout(&peer, Duration::from_millis(100)).ok())
            .collect();

        let bound = Duration::from_millis(200);
        let config = TcpConfig { connect_timeout: bound, ..TcpConfig::default() };
        let sender = make_sender(config);
        let message = test_message(0xC2, b"discovery");

        let first = sender.send_to(peer, TcpFrameKind::Discovery, &message).unwrap_err();
        assert_eq!(first.kind(), io::ErrorKind::TimedOut);

        let call = Instant::now();
        let second = sender.send_to(peer, TcpFrameKind::Discovery, &message).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::WouldBlock, "the second dial was not held back");
        assert!(call.elapsed() < bound / 2, "the second send waited {:?}", call.elapsed());

        sender.shutdown();
    }
}
