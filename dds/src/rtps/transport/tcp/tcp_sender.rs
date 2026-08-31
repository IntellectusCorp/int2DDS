//! Outbound TCP connections, written from the calling thread.
//!
//! A peer has at most two lazy outbound slots, keyed by the frame kind. A slot
//! is opened by the first frame that needs it and is dropped as soon as a write
//! fails. There is no application-level connection handshake.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
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
        if last.map_or(true, |previous| now.duration_since(previous) >= Duration::from_secs(1)) {
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
}

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
        Arc::new(Self {
            working_ips,
            listener_port,
            connect_timeout: tcp_config.connect_timeout,
            tls_handshake_timeout: tcp_config.tls_handshake_timeout,
            stats: Arc::new(SendStats::default()),
            tls_config,
            shared,
            connections: Arc::new(DashMap::new()),
            shutdown,
        })
    }

    pub(crate) fn send_to_discovery(
        self: &Arc<Self>,
        addr: &SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        self.send_to(*addr, TcpFrameKind::Discovery, data)
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

        match connection.send(&frame) {
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
                    self.evict_connection(key, &connection);
                    self.stats.write_error.record("write error", addr, kind);
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
        .map_err(|error| {
            self.stats.connect_failed.record("connect failed", addr, kind);
            self.shared.note_connect_failure(key, &error);
            error
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

    pub(crate) fn disconnect_peer(&self, addr: SocketAddr) {
        self.connections.retain(|(peer, _), _| *peer != addr);
        self.shared.clear_peer_backoff(addr);
    }

    pub(crate) fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.connections.clear();
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
    use crate::rtps::transport::tcp::framing::test_message;

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
    /// socket cannot take are dropped the way a full UDP socket buffer drops
    /// them, and a frame it took only part of leaves its tail for the next
    /// send instead of waiting.
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

        let mut refused = 0;
        for _ in 0..64 {
            let call = Instant::now();
            let result = sender.send_to(peer, TcpFrameKind::UserData, &message);
            assert!(
                call.elapsed() < bound * 8,
                "one send held the thread for {:?}",
                call.elapsed()
            );
            if result.is_err() {
                refused += 1;
            }
        }

        assert!(refused > 0, "a peer that never reads must eventually refuse frames");
        sender.shutdown();
    }
}
