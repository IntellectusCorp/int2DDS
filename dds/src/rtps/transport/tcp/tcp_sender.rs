//! Outbound TCP connections, written from the calling thread.
//!
//! A peer has at most two lazy outbound slots, keyed by the frame kind. A slot
//! is opened by the first frame that needs it and is dropped as soon as a write
//! fails. There is no application-level connection handshake.
//!
//! The calling thread never opens a connection itself. A frame for a peer with
//! no connection waits in the slot while the dial loop opens one, so a peer
//! nobody answers costs the caller nothing, however many such peers there are.

mod dial_loop;

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use log::{debug, warn};
use mio::{Poll, Waker};

use self::dial_loop::{DialLoop, DIAL_WAKE_TOKEN};
use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::tcp::connection_registry::ConnectionRegistry;
use crate::rtps::transport::tcp::framing::{encode_frame, validate_payload_size, TcpFrameKind};
use crate::rtps::transport::tcp::sync_connection::{
    OutboundConnection, SendOutcome, SEND_QUEUE_BYTE_BUDGET,
};
use crate::rtps::transport::tls::client::TlsClient;
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
}

type ConnectionKey = (SocketAddr, TcpFrameKind);

enum Slot {
    /// Frames sent before the connection exists, in order, under the same
    /// byte budget the connection's own send queue has.
    Connecting {
        frames: Vec<Vec<u8>>,
        bytes: usize,
        cancelled: Arc<AtomicBool>,
    },
    Live(Arc<OutboundConnection>),
}

impl Slot {
    fn waiting_with(frame: Vec<u8>) -> Self {
        let bytes = frame.len();
        Slot::Connecting { frames: vec![frame], bytes, cancelled: Arc::new(AtomicBool::new(false)) }
    }
}

pub(crate) struct TcpSender {
    working_ips: Vec<String>,
    listener_port: u16,
    bind_ip: Option<IpAddr>,

    connect_timeout: Duration,
    tls_handshake_timeout: Duration,

    stats: Arc<SendStats>,
    tls: Option<TlsClient>,
    shared: Arc<ConnectionRegistry>,
    slots: DashMap<ConnectionKey, Slot>,
    shutdown: Arc<AtomicBool>,

    /// Set only once the dial loop runs, so its absence means no connection
    /// can ever be opened.
    dial_waker: OnceLock<Waker>,
}

impl TcpSender {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        _domain_id: u32,
        _participant_id: u32,
        working_ips: Vec<String>,
        listener_port: u16,
        bind_ip: Option<IpAddr>,
        _local_guid_prefix: GuidPrefix,
        tls: Option<TlsClient>,
        shared: Arc<ConnectionRegistry>,
        tcp_config: &TcpConfig,
        shutdown: Arc<AtomicBool>,
    ) -> Arc<Self> {
        let sender = Arc::new(Self {
            working_ips,
            listener_port,
            bind_ip,
            connect_timeout: tcp_config.connect_timeout,
            tls_handshake_timeout: tcp_config.tls_handshake_timeout,
            stats: Arc::new(SendStats::default()),
            tls,
            shared,
            slots: DashMap::new(),
            shutdown,
            dial_waker: OnceLock::new(),
        });
        if let Err(error) = Self::spawn_dial_worker(&sender) {
            log::error!(
                "TcpSender: no dial worker, no new TCP connection can be opened: {}",
                error
            );
        }
        sender
    }

    fn spawn_dial_worker(sender: &Arc<Self>) -> io::Result<()> {
        let poll = Poll::new()?;
        let waker = Waker::new(poll.registry(), DIAL_WAKE_TOKEN)?;
        let weak = Arc::downgrade(sender);
        let dial_loop = DialLoop::new(poll);
        std::thread::Builder::new()
            .name("tcp_dial".to_string())
            .spawn(move || dial_loop.run_until_shutdown(weak))?;
        let _ = sender.dial_waker.set(waker);
        Ok(())
    }

    /// Announce to one peer without ever waiting on a connection.
    pub(crate) fn send_to_discovery(
        self: &Arc<Self>,
        addr: &SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        self.send_to(*addr, TcpFrameKind::Discovery, data)
    }

    /// Write one frame to a peer, or leave it waiting for the connection when
    /// there is none yet. `Ok` means the frame is owed to the peer in order,
    /// not that it has reached the socket.
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
        let mut frame = Vec::with_capacity(data.len() + 8);
        encode_frame(data, &mut frame)?;

        if let Some(connection) = self.live_connection(&key) {
            match self.write_frame(key, &connection, &frame) {
                // The stream the frame was meant for is gone, and a new one
                // starts from a frame boundary, so the whole frame waits for it.
                Err(_) if connection.is_failed() => {}
                result => return result,
            }
        }
        self.queue_for_dial(key, frame)
    }

    /// Leave a frame in its slot until the dial loop has a connection for it.
    fn queue_for_dial(&self, key: ConnectionKey, frame: Vec<u8>) -> io::Result<()> {
        let Some(waker) = self.dial_waker.get() else {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                format!("no dial worker to open a connection to {}", key.0),
            ));
        };
        let promoted = match self.slots.entry(key) {
            Entry::Occupied(mut entry) => match entry.get_mut() {
                Slot::Connecting { frames, bytes, .. } => {
                    if *bytes + frame.len() > SEND_QUEUE_BYTE_BUDGET {
                        return Err(self.no_room_for(key));
                    }
                    *bytes += frame.len();
                    frames.push(frame);
                    return Ok(());
                }
                Slot::Live(connection) if !connection.is_failed() => Arc::clone(connection),
                Slot::Live(_) => {
                    self.refuse_during_backoff(key)?;
                    entry.insert(Slot::waiting_with(frame));
                    let _ = waker.wake();
                    return Ok(());
                }
            },
            Entry::Vacant(entry) => {
                self.refuse_during_backoff(key)?;
                entry.insert(Slot::waiting_with(frame));
                let _ = waker.wake();
                return Ok(());
            }
        };
        // Promoted since the caller looked; written outside the slot's lock,
        // which a failed write takes again to evict the connection.
        self.write_frame(key, &promoted, &frame)
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
            Ok(SendOutcome::Stalled) => Err(self.no_room_for(key)),
            Err(error) => {
                if connection.is_failed() {
                    self.evict_connection(key, connection);
                    debug!("TcpSender: write {:?} to {} failed: {}", kind, addr, error);
                }
                Err(error)
            }
        }
    }

    fn no_room_for(&self, key: ConnectionKey) -> io::Error {
        let (addr, kind) = key;
        self.stats.peer_stalled.record("peer send queue full", addr, kind);
        transport_io_error(
            TransportErrorCode::TcpPeerSendQueueFull,
            format!("peer {addr} has no room for a {kind:?} frame"),
        )
    }

    fn live_connection(&self, key: &ConnectionKey) -> Option<Arc<OutboundConnection>> {
        match self.slots.get(key)?.value() {
            Slot::Live(connection) if !connection.is_failed() => Some(Arc::clone(connection)),
            _ => None,
        }
    }

    fn refuse_during_backoff(&self, key: ConnectionKey) -> io::Result<()> {
        let Some(remaining) = self.shared.backoff_remaining(key) else {
            return Ok(());
        };
        let (addr, kind) = key;
        self.stats.backoff.record("reconnect backoff", addr, kind);
        Err(transport_io_error(
            TransportErrorCode::TcpReconnectBackoff,
            format!("peer {addr} is in reconnect backoff for another {remaining:?}"),
        ))
    }

    /// Record how a dial ended: a failure feeds the backoff and drops the
    /// frames that waited for it, and a success clears the backoff and takes
    /// the slot.
    fn record_dial_result(
        &self,
        key: ConnectionKey,
        cancelled: &Arc<AtomicBool>,
        result: io::Result<OutboundConnection>,
    ) {
        let Entry::Occupied(mut entry) = self.slots.entry(key) else {
            return;
        };
        if !matches!(entry.get(), Slot::Connecting { cancelled: current, .. }
            if Arc::ptr_eq(current, cancelled) && !current.load(Ordering::Acquire))
        {
            return;
        }
        let (addr, kind) = key;
        let connection = match result {
            Ok(connection) => connection,
            Err(error) => {
                self.stats.connect_failed.record("connect failed", addr, kind);
                // A refusal costs the caller one round trip, and discovery never
                // dials more often than its own periods, so holding it back only
                // drops the reply a participant that just reappeared is waiting for.
                let refused = error.kind() == io::ErrorKind::ConnectionRefused;
                if !(refused && kind == TcpFrameKind::Discovery) {
                    self.shared.note_connect_failure(key, &error);
                }
                entry.remove();
                return;
            }
        };
        self.shared.clear_backoff(key);
        if let Err(error) = self.promote(key, entry.get_mut(), connection) {
            entry.remove();
            debug!("TcpSender: write {:?} to {} failed: {}", kind, addr, error);
        }
    }

    /// The waiting frames go out and the slot turns live under one lock, so a
    /// frame sent meanwhile can neither overtake them nor be left behind in a
    /// queue nobody drains.
    fn promote(
        &self,
        key: ConnectionKey,
        slot: &mut Slot,
        connection: OutboundConnection,
    ) -> io::Result<()> {
        let Slot::Connecting { frames, .. } = slot else {
            return Ok(());
        };
        let frames = std::mem::take(frames);
        let connection = Arc::new(connection);
        for frame in &frames {
            connection.send(frame)?;
        }
        *slot = Slot::Live(connection);
        debug!("TcpSender: established {:?} connection to {}", key.1, key.0);
        Ok(())
    }

    fn evict_connection(&self, key: ConnectionKey, connection: &Arc<OutboundConnection>) {
        self.slots.remove_if(
            &key,
            |_, slot| matches!(slot, Slot::Live(current) if Arc::ptr_eq(current, connection)),
        );
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
        self.slots.retain(|(peer, _), slot| {
            if *peer != addr {
                return true;
            }
            if let Slot::Connecting { cancelled, .. } = slot {
                cancelled.store(true, Ordering::Release);
            }
            false
        });
        self.shared.clear_peer_backoff(addr);
        if let Some(waker) = self.dial_waker.get() {
            let _ = waker.wake();
        }
    }

    pub(crate) fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.slots.clear();
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
        self.slots.iter().filter(|slot| matches!(slot.value(), Slot::Live(_))).count()
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
    use crate::rtps::transport::tls::TlsConfig;
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
            None,
            [0; 12],
            None,
            registry,
            &config,
            Arc::new(AtomicBool::new(false)),
        )
    }

    /// Dials finish on the dial loop, so a test waits for their outcome.
    fn eventually(wait: Duration, done: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + wait;
        while !done() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
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

        sender.send_to(peer, TcpFrameKind::UserData, &message).expect("queued for the dial");
        assert!(
            eventually(Duration::from_secs(2), || sender.stats.connect_failed.count() == 1
                && !sender.slots.contains_key(&(peer, TcpFrameKind::UserData))),
            "the failed dial left its slot behind"
        );
        assert_eq!(sender.connection_count(), 0);

        let second = sender.send_to(peer, TcpFrameKind::UserData, &message).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(sender.stats.backoff.count(), 1);

        sender
            .send_to(peer, TcpFrameKind::Discovery, &message)
            .expect("the other kind is not held back by this backoff");
        assert_eq!(sender.stats.backoff.count(), 1);

        sender.shutdown();
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

        assert!(eventually(Duration::from_secs(2), || sender.connection_count() == 2));
        std::thread::sleep(Duration::from_millis(100));
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
        sender.send_to(peer, TcpFrameKind::Discovery, &while_down).expect("queued for the dial");
        assert!(
            eventually(Duration::from_secs(2), || sender.stats.connect_failed.count() == 1
                && !sender.slots.contains_key(&(peer, TcpFrameKind::Discovery))),
            "the dial to the stopped peer was never refused"
        );
        assert!(sender.shared.backoff_remaining((peer, TcpFrameKind::Discovery)).is_none());

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

        let key = (peer, TcpFrameKind::UserData);
        assert!(eventually(Duration::from_secs(2), || sender.live_connection(&key).is_some()));
        let connection = sender.live_connection(&key).expect("an open slot");
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

    /// An address that drops every SYN without a reply, for as long as it lives.
    struct SilentPeer {
        addr: SocketAddr,
        _holds: Option<(socket2::Socket, Vec<TcpStream>)>,
    }

    // Only these kernels drop new SYNs without a reply once the accept queue is
    // full, which is what stands in for a host that is down.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn silent_peer() -> Option<SilentPeer> {
        let listener = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)
            .expect("socket");
        listener.bind(&"127.0.0.1:0".parse::<SocketAddr>().unwrap().into()).expect("bind");
        listener.listen(0).expect("listen");
        let addr = listener.local_addr().expect("addr").as_socket().expect("inet addr");
        let queued = (0..2)
            .filter_map(|_| TcpStream::connect_timeout(&addr, Duration::from_millis(100)).ok())
            .collect();
        Some(SilentPeer { addr, _holds: Some((listener, queued)) })
    }

    /// Elsewhere a full accept queue answers with a reset, so a documentation
    /// address stands in, but only where this host routes it into silence;
    /// `None` lets the caller skip instead of passing for the wrong reason.
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    fn silent_peer() -> Option<SilentPeer> {
        // Clear of the 192.0.2.1:7400 that the test sender takes as its own address.
        static NEXT_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(20000);
        let port = NEXT_PORT.fetch_add(2, Ordering::Relaxed);
        let addr = SocketAddr::from(([192, 0, 2, 1], port));
        let probe =
            crate::rtps::transport::tcp::dial::start_dial(addr, None, Duration::from_secs(1))
                .ok()?;
        std::thread::sleep(Duration::from_millis(150));
        let unanswered = matches!(probe.take_error(), Ok(None))
            && matches!(probe.peer_addr(), Err(error) if error.kind() == io::ErrorKind::NotConnected);
        unanswered.then_some(SilentPeer { addr, _holds: None })
    }

    /// A peer that answers nothing must still put discovery into backoff, so
    /// the next frame is refused at once instead of waiting through another
    /// dial that is bound to time out.
    #[test]
    fn a_discovery_dial_that_times_out_enters_backoff() {
        let Some(silent) = silent_peer() else {
            eprintln!("skipped: no address on this host drops SYNs without a reply");
            return;
        };
        let peer = silent.addr;

        let bound = Duration::from_millis(200);
        let config = TcpConfig { connect_timeout: bound, ..TcpConfig::default() };
        let sender = make_sender(config);
        let message = test_message(0xC2, b"discovery");

        sender.send_to(peer, TcpFrameKind::Discovery, &message).expect("queued for the dial");
        assert!(
            eventually(bound * 5, || sender.stats.connect_failed.count() == 1),
            "the dial to a peer that answers nothing never timed out"
        );
        assert!(sender.shared.backoff_remaining((peer, TcpFrameKind::Discovery)).is_some());

        let call = Instant::now();
        let second = sender.send_to(peer, TcpFrameKind::Discovery, &message).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::WouldBlock, "the second dial was not held back");
        assert!(call.elapsed() < bound / 2, "the second send waited {:?}", call.elapsed());

        sender.shutdown();
    }

    /// A candidate that never answers its SYN must not hold back the
    /// announcement to a peer that is listening: the live peer is reached in
    /// its own round trip, not after every timeout queued ahead of it.
    #[test]
    fn an_unanswered_candidate_does_not_hold_back_a_live_peer() {
        let Some(silent) = (0..3).map(|_| silent_peer()).collect::<Option<Vec<_>>>() else {
            eprintln!("skipped: no address on this host drops SYNs without a reply");
            return;
        };
        let live = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let peer = live.local_addr().expect("addr");

        let bound = Duration::from_millis(300);
        let config = TcpConfig { connect_timeout: bound, ..TcpConfig::default() };
        let sender = make_sender(config);
        let message = test_message(0xC2, b"discovery");

        let start = Instant::now();
        for candidate in &silent {
            sender
                .send_to_discovery(&candidate.addr, &message)
                .expect("handoff to the dial worker");
        }
        sender.send_to_discovery(&peer, &message).expect("handoff to the dial worker");

        let mut stream =
            accept_within(&live, Duration::from_secs(3)).expect("the live peer was never dialed");
        let waited = start.elapsed();
        assert_eq!(read_frame(&mut stream, &message), test_framed(&message));
        assert!(waited < bound, "the live peer waited {waited:?} behind unanswered candidates");

        sender.shutdown();
    }

    /// A send to a peer with no connection yet must never hold the calling
    /// thread for the dial: the frame waits for the connection instead, so a
    /// peer that answers nothing costs the caller nothing.
    #[test]
    fn a_send_to_an_unanswered_peer_does_not_hold_the_caller() {
        let Some(silent) = silent_peer() else {
            eprintln!("skipped: no address on this host drops SYNs without a reply");
            return;
        };
        let bound = Duration::from_millis(300);
        let config = TcpConfig { connect_timeout: bound, ..TcpConfig::default() };
        let sender = make_sender(config);
        let message = test_message(0x02, b"user data");

        let call = Instant::now();
        let result = sender.send_to(silent.addr, TcpFrameKind::UserData, &message);
        let held = call.elapsed();

        assert!(held < bound / 2, "the caller was held {held:?} by a dial nobody answers");
        result.expect("a frame for a peer still being dialed is queued");

        sender.shutdown();
    }

    #[test]
    fn a_failed_queued_write_removes_the_connection_before_the_next_send() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let socket = TcpStream::connect(peer).unwrap();
        let old = listener.accept().unwrap().0;
        let tuning = TcpSocketTuning::default();
        let sender = Arc::new(TcpSender {
            working_ips: vec![],
            listener_port: 0,
            bind_ip: None,
            connect_timeout: Duration::from_secs(1),
            tls_handshake_timeout: Duration::from_secs(1),
            stats: Arc::new(SendStats::default()),
            tls: None,
            shared: Arc::new(ConnectionRegistry::new(0, 0, [0; 12], tuning)),
            slots: DashMap::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
            dial_waker: OnceLock::new(),
        });
        let key = (peer, TcpFrameKind::UserData);
        let message = test_message(0x02, b"discarded after the failed write");
        let slot = Slot::waiting_with(test_framed(&message));
        let Slot::Connecting { cancelled, .. } = &slot else { unreachable!() };
        let cancelled = Arc::clone(cancelled);
        sender.slots.insert(key, slot);
        let connection = OutboundConnection::plain(socket, &tuning).unwrap();
        connection.shutdown_write();

        sender.record_dial_result(key, &cancelled, Ok(connection));
        let retained = sender.slots.contains_key(&key);
        TcpSender::spawn_dial_worker(&sender).unwrap();
        let message = test_message(0x02, b"sent on a new connection");
        sender.send_to(peer, key.1, &message).unwrap();
        let mut new = accept_within(&listener, Duration::from_secs(1)).unwrap();
        let received = read_frame(&mut new, &message);
        sender.shutdown();
        drop(old);
        drop(new);
        drop(listener);

        assert!(!retained, "the failed queued write left a connection in the slot");
        assert_eq!(received, test_framed(&message));
    }

    #[test]
    fn results_from_a_disconnected_attempt_leave_the_new_queue_untouched() {
        for succeeded in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let peer = listener.local_addr().unwrap();
            let socket = TcpStream::connect(peer).unwrap();
            let accepted = listener.accept().unwrap().0;
            let tuning = TcpSocketTuning::default();
            let sender = TcpSender {
                working_ips: vec![],
                listener_port: 0,
                bind_ip: None,
                connect_timeout: Duration::from_secs(1),
                tls_handshake_timeout: Duration::from_secs(1),
                stats: Arc::new(SendStats::default()),
                tls: None,
                shared: Arc::new(ConnectionRegistry::new(0, 0, [0; 12], tuning)),
                slots: DashMap::new(),
                shutdown: Arc::new(AtomicBool::new(false)),
                dial_waker: OnceLock::new(),
            };
            let key = (peer, TcpFrameKind::UserData);
            let old = Slot::waiting_with(vec![1]);
            let Slot::Connecting { cancelled, .. } = &old else { unreachable!() };
            let cancelled = Arc::clone(cancelled);
            sender.slots.insert(key, old);
            sender.disconnect_peer(peer);
            sender.slots.insert(key, Slot::waiting_with(vec![2]));

            let result = if succeeded {
                Ok(OutboundConnection::plain(socket, &tuning).unwrap())
            } else {
                drop(socket);
                Err(io::Error::from(io::ErrorKind::TimedOut))
            };
            sender.record_dial_result(key, &cancelled, result);
            let waiting = sender.slots.get(&key).is_some_and(|slot| {
                matches!(slot.value(), Slot::Connecting { frames, cancelled, .. }
                    if frames == &vec![vec![2]] && !cancelled.load(Ordering::Acquire))
            });
            let failures = sender.stats.connect_failed.count();
            let backoff = sender.shared.backoff_remaining(key);
            sender.shutdown();
            drop(accepted);
            drop(listener);

            assert!(cancelled.load(Ordering::Acquire));
            assert!(waiting, "an old result changed the new queue (success: {succeeded})");
            assert_eq!(failures, 0);
            assert!(backoff.is_none());
        }
    }

    #[test]
    fn disconnecting_a_peer_cancels_its_handshake_before_redial() {
        let dir = tempfile::tempdir().unwrap();
        let cert_file = dir.path().join("cert.pem");
        let key_file = dir.path().join("key.pem");
        let generated = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        std::fs::write(&cert_file, generated.cert.pem()).unwrap();
        std::fs::write(&key_file, generated.key_pair.serialize_pem()).unwrap();
        let tls = TlsConfig {
            ca_file: cert_file.clone(),
            cert_file,
            key_file,
            server_name: Some("localhost".to_string()),
            verify_peer: false,
            revocation: None,
            allow_file: None,
            deny_file: None,
        };
        let server_config = tls.build_server_config().unwrap();
        let config = TcpConfig {
            connect_timeout: Duration::from_secs(3),
            tls_handshake_timeout: Duration::from_secs(3),
            ..TcpConfig::default()
        };
        let registry = Arc::new(ConnectionRegistry::new(0, 0, [0; 12], TcpSocketTuning::default()));
        let sender = TcpSender::new(
            0,
            0,
            vec![],
            0,
            None,
            [0; 12],
            Some(TlsClient::new(&tls).unwrap()),
            registry,
            &config,
            Arc::new(AtomicBool::new(false)),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let message = test_message(0x02, b"before disconnect");
        sender.send_to(peer, TcpFrameKind::UserData, &message).unwrap();
        let mut old = accept_within(&listener, Duration::from_secs(1)).unwrap();
        let mut hello = [0u8; 4096];
        assert!(old.read(&mut hello).unwrap() > 0);

        sender.disconnect_peer(peer);
        let message = test_message(0x02, b"after disconnect");
        sender.send_to(peer, TcpFrameKind::UserData, &message).unwrap();
        old.set_nonblocking(true).unwrap();
        let closed = eventually(Duration::from_millis(500), || {
            let mut sink = [0u8; 4096];
            match (&old).read(&mut sink) {
                Ok(0) => true,
                Err(error) if error.kind() == io::ErrorKind::ConnectionReset => true,
                _ => false,
            }
        });
        let received = accept_within(&listener, Duration::from_millis(500)).map(|socket| {
            let connection = rustls::ServerConnection::new(server_config).unwrap();
            let mut stream = rustls::StreamOwned::new(connection, socket);
            let mut received = vec![0u8; test_framed(&message).len()];
            stream.read_exact(&mut received).unwrap();
            received
        });
        let failures = sender.stats.connect_failed.count();
        let backoff = sender.shared.backoff_remaining((peer, TcpFrameKind::UserData));
        sender.shutdown();
        drop(old);
        drop(listener);

        assert!(closed, "disconnect left the old handshake socket open until its deadline");
        assert_eq!(
            received,
            Some(test_framed(&message)),
            "the old attempt held back the new frame"
        );
        assert_eq!(failures, 0, "cancellation must not count as a connection failure");
        assert!(backoff.is_none(), "cancellation must not delay the new attempt");
    }

    /// A TLS peer that takes the connection and then never answers the
    /// handshake must cost only its own handshake deadline, never the dials
    /// queued behind it.
    #[test]
    fn a_stalled_tls_handshake_does_not_hold_back_other_dials() {
        let dir = tempfile::tempdir().unwrap();
        let cert_file = dir.path().join("cert.pem");
        let key_file = dir.path().join("key.pem");
        let generated = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        std::fs::write(&cert_file, generated.cert.pem()).unwrap();
        std::fs::write(&key_file, generated.key_pair.serialize_pem()).unwrap();
        let tls = TlsConfig {
            ca_file: cert_file.clone(),
            cert_file,
            key_file,
            server_name: Some("localhost".to_string()),
            verify_peer: false,
            revocation: None,
            allow_file: None,
            deny_file: None,
        };
        let server_config = tls.build_server_config().unwrap();

        // The kernel completes the TCP connection from the accept queue, but
        // nothing ever answers the ClientHello.
        let stalled = std::net::TcpListener::bind("127.0.0.1:0").expect("stalled listener");
        let live = std::net::TcpListener::bind("127.0.0.1:0").expect("live listener");
        let live_peer = live.local_addr().expect("addr");

        let handshake_bound = Duration::from_secs(1);
        let config = TcpConfig {
            connect_timeout: Duration::from_secs(1),
            tls_handshake_timeout: handshake_bound,
            ..TcpConfig::default()
        };
        let tuning = TcpSocketTuning {
            nodelay: config.nodelay,
            so_rcvbuf: config.so_rcvbuf,
            so_sndbuf: config.so_sndbuf,
            unacked_timeout: config.unacked_timeout,
            keepalive: None,
        };
        let registry = Arc::new(ConnectionRegistry::new(0, 0, [0; 12], tuning));
        let sender = TcpSender::new(
            0,
            0,
            vec!["192.0.2.1".into()],
            7400,
            None,
            [0; 12],
            Some(TlsClient::new(&tls).unwrap()),
            registry,
            &config,
            Arc::new(AtomicBool::new(false)),
        );
        let message = test_message(0x02, b"over tls");

        let stalled_peer = stalled.local_addr().expect("addr");
        sender.send_to(stalled_peer, TcpFrameKind::UserData, &message).expect("queued");
        // Lets the stalled dial reach its handshake before the live one is queued.
        std::thread::sleep(Duration::from_millis(100));

        let start = Instant::now();
        sender.send_to(live_peer, TcpFrameKind::UserData, &message).expect("queued");
        let socket =
            accept_within(&live, Duration::from_secs(3)).expect("the live peer was never dialed");
        let connection = rustls::ServerConnection::new(server_config).unwrap();
        let mut stream = rustls::StreamOwned::new(connection, socket);
        let mut received = vec![0u8; test_framed(&message).len()];
        stream.read_exact(&mut received).expect("a whole frame over tls");
        let waited = start.elapsed();

        assert_eq!(received, test_framed(&message));
        assert!(
            waited < handshake_bound / 2,
            "the live peer waited {waited:?} behind a stalled handshake"
        );

        sender.shutdown();
        drop(stalled);
    }
}
