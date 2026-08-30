//! Shared inbound connection bookkeeping and frame routing.
//!
//! The registry deliberately knows nothing about RTPS packet contents. A reader
//! supplies the kind decoded from the TCP frame header and the registry moves
//! the owned payload to the matching listening-task channel.

use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use flume::{Receiver, Sender, TrySendError};
use log::{debug, warn};
use mio::Waker;
use tokio_util::sync::CancellationToken;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::plugin::IncomingMessage;
use crate::rtps::transport::tcp::framing::{TcpBufferPool, TcpFrame, TcpFrameKind};

pub(crate) type ConnectionId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionDirection {
    Inbound,
    Outbound,
}

pub(crate) struct ConnectionEntry {
    pub(crate) remote_addr: SocketAddr,
    pub(crate) direction: ConnectionDirection,
    pub(crate) cancel: CancellationToken,
}

/// OS keepalive tuning applied to every inbound and outbound stream.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KeepaliveParams {
    pub(crate) time: Duration,
    pub(crate) interval: Duration,
    pub(crate) retries: u32,
}

/// Socket-level tuning resolved per participant from `TcpConfig`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TcpSocketTuning {
    pub(crate) nodelay: bool,
    pub(crate) so_rcvbuf: Option<usize>,
    pub(crate) so_sndbuf: Option<usize>,
    pub(crate) unacked_timeout: Option<Duration>,
    pub(crate) keepalive: Option<KeepaliveParams>,
}

impl Default for TcpSocketTuning {
    fn default() -> Self {
        Self {
            nodelay: true,
            so_rcvbuf: None,
            so_sndbuf: None,
            unacked_timeout: None,
            keepalive: None,
        }
    }
}

pub(crate) fn apply_unacked_timeout(tcp: &std::net::TcpStream, timeout: Option<Duration>) {
    let Some(t) = timeout else { return };
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let _ = socket2::SockRef::from(tcp).set_tcp_user_timeout(Some(t));
    }
    #[cfg(target_os = "macos")]
    {
        const TCP_RXT_CONNDROPTIME: libc::c_int = 0x80;
        use std::os::unix::io::AsRawFd;
        let secs = t.as_secs().max(1) as libc::c_int;
        unsafe {
            libc::setsockopt(
                tcp.as_raw_fd(),
                libc::IPPROTO_TCP,
                TCP_RXT_CONNDROPTIME,
                &secs as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
    {
        let _ = (tcp, t);
    }
}

pub(crate) fn apply_keepalive(tcp: &std::net::TcpStream, params: Option<KeepaliveParams>) {
    let Some(p) = params else { return };
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    {
        let ka = socket2::TcpKeepalive::new()
            .with_time(p.time)
            .with_interval(p.interval)
            .with_retries(p.retries);
        let _ = socket2::SockRef::from(tcp).set_tcp_keepalive(&ka);
    }
    #[cfg(target_os = "windows")]
    {
        let ka = socket2::TcpKeepalive::new().with_time(p.time).with_interval(p.interval);
        let _ = socket2::SockRef::from(tcp).set_tcp_keepalive(&ka);
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "windows"
    )))]
    {
        let ka = socket2::TcpKeepalive::new().with_time(p.time);
        let _ = socket2::SockRef::from(tcp).set_tcp_keepalive(&ka);
    }
}

pub(crate) fn apply_socket_tuning(tcp: &std::net::TcpStream, tuning: &TcpSocketTuning) {
    let _ = tcp.set_nodelay(tuning.nodelay);
    if let Some(size) = tuning.so_rcvbuf {
        let _ = socket2::SockRef::from(tcp).set_recv_buffer_size(size);
    }
    if let Some(size) = tuning.so_sndbuf {
        let _ = socket2::SockRef::from(tcp).set_send_buffer_size(size);
    }
    apply_unacked_timeout(tcp, tuning.unacked_timeout);
    apply_keepalive(tcp, tuning.keepalive);
}

const SELF_DELIVERY_BYTE_CAP: usize = 64 * 1024 * 1024;
const SELF_DELIVERY_COUNT_BACKSTOP: usize = 65_536;

pub(crate) const BACKOFF_BASE: Duration = Duration::from_millis(500);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

struct BackoffState {
    next_attempt: Instant,
    delay: Duration,
}

pub(crate) struct ConnectionRegistry {
    pub(crate) tuning: TcpSocketTuning,
    buffer_pool: Arc<TcpBufferPool>,
    pub(crate) connections: DashMap<ConnectionId, ConnectionEntry>,
    next_conn_id: AtomicUsize,
    backoff: DashMap<SocketAddr, BackoffState>,

    discovery_tx: Sender<IncomingMessage>,
    user_data_tx: Sender<IncomingMessage>,

    self_delivery_tx: Sender<IncomingMessage>,
    self_delivery_rx: Mutex<Option<Receiver<IncomingMessage>>>,
    self_delivery_bytes: Arc<AtomicUsize>,
    self_delivery_waker: Mutex<Option<Arc<Waker>>>,
}

impl ConnectionRegistry {
    pub(crate) fn new(
        _domain_id: u32,
        _participant_id: u32,
        _local_guid_prefix: GuidPrefix,
        tuning: TcpSocketTuning,
        discovery_tx: Sender<IncomingMessage>,
        user_data_tx: Sender<IncomingMessage>,
    ) -> Self {
        let (self_delivery_tx, self_delivery_rx) = flume::bounded(SELF_DELIVERY_COUNT_BACKSTOP);
        Self {
            tuning,
            buffer_pool: TcpBufferPool::new(),
            connections: DashMap::new(),
            next_conn_id: AtomicUsize::new(0),
            backoff: DashMap::new(),
            discovery_tx,
            user_data_tx,
            self_delivery_tx,
            self_delivery_rx: Mutex::new(Some(self_delivery_rx)),
            self_delivery_bytes: Arc::new(AtomicUsize::new(0)),
            self_delivery_waker: Mutex::new(None),
        }
    }

    pub(crate) fn set_self_delivery_waker(&self, waker: Arc<Waker>) {
        if let Ok(mut guard) = self.self_delivery_waker.lock() {
            *guard = Some(Arc::clone(&waker));
        }
        let _ = waker.wake();
    }

    pub(crate) fn clear_self_delivery_waker(&self) {
        if let Ok(mut guard) = self.self_delivery_waker.lock() {
            *guard = None;
        }
    }

    pub(crate) fn try_receive_self_delivery(&self) -> Option<IncomingMessage> {
        let message = self.self_delivery_rx.lock().ok()?.as_ref()?.try_recv().ok()?;
        self.self_delivery_bytes.fetch_sub(message.data.len(), Ordering::AcqRel);
        Some(message)
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn buffer_pool(&self) -> &Arc<TcpBufferPool> {
        &self.buffer_pool
    }

    #[cfg(test)]
    fn peer_count(&self) -> usize {
        use std::collections::HashSet;
        self.connections.iter().map(|entry| entry.remote_addr).collect::<HashSet<_>>().len()
    }

    pub(crate) fn backoff_remaining(&self, addr: SocketAddr) -> Option<Duration> {
        let entry = self.backoff.get(&addr)?;
        let now = Instant::now();
        (entry.next_attempt > now).then(|| entry.next_attempt - now)
    }

    pub(crate) fn note_connect_failure(&self, addr: SocketAddr, err: &io::Error) {
        let now = Instant::now();
        let mut entry = self
            .backoff
            .entry(addr)
            .or_insert(BackoffState { next_attempt: now, delay: Duration::ZERO });
        let delay = if err.kind() == io::ErrorKind::ConnectionRefused || entry.delay.is_zero() {
            BACKOFF_BASE
        } else {
            (entry.delay * 2).min(BACKOFF_MAX)
        };
        entry.delay = delay;
        entry.next_attempt = now + delay;
    }

    pub(crate) fn clear_backoff(&self, addr: SocketAddr) {
        self.backoff.remove(&addr);
    }

    /// Self-addressed user data bypasses the socket. Local discovery is handled
    /// by the discovery logic and remains intentionally suppressed.
    pub(crate) fn deliver_to_self(
        &self,
        source: SocketAddr,
        kind: TcpFrameKind,
        data: &[u8],
    ) -> io::Result<bool> {
        if kind != TcpFrameKind::UserData {
            return Ok(false);
        }

        let len = data.len();
        if self
            .self_delivery_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(len).filter(|total| *total <= SELF_DELIVERY_BYTE_CAP)
            })
            .is_err()
        {
            return Err(transport_io_error(
                TransportErrorCode::TcpChannelFull,
                format!("intra-participant queue over byte cap, dropped a {len} byte frame"),
            ));
        }

        let msg = IncomingMessage { data: self.buffer_pool.copy_from_slice(data), source };
        match self.self_delivery_tx.try_send(msg) {
            Ok(()) => {
                if let Ok(guard) = self.self_delivery_waker.lock() {
                    if let Some(waker) = guard.as_ref() {
                        let _ = waker.wake();
                    }
                }
                Ok(true)
            }
            Err(TrySendError::Full(_)) => {
                self.self_delivery_bytes.fetch_sub(len, Ordering::AcqRel);
                Err(transport_io_error(
                    TransportErrorCode::TcpChannelFull,
                    format!(
                        "intra-participant queue over count backstop, dropped a {len} byte frame"
                    ),
                ))
            }
            Err(TrySendError::Disconnected(_)) => {
                self.self_delivery_bytes.fetch_sub(len, Ordering::AcqRel);
                Ok(true)
            }
        }
    }

    pub(crate) fn register_connection(
        &self,
        remote_addr: SocketAddr,
        direction: ConnectionDirection,
        cancel: CancellationToken,
    ) -> ConnectionId {
        let conn_id = self.next_conn_id.fetch_add(1, Ordering::SeqCst);
        self.connections.insert(conn_id, ConnectionEntry { remote_addr, direction, cancel });
        conn_id
    }

    /// Route solely from the explicit wire kind. This is the point at which TCP
    /// receive backpressure reaches the socket: a full single-slot RTPS channel
    /// parks this connection's reader task.
    pub(crate) async fn route_frame(&self, conn_id: ConnectionId, frame: TcpFrame) {
        let Some(entry) = self.connections.get(&conn_id) else { return };
        let source = entry.remote_addr;
        let direction = entry.direction;
        drop(entry);

        if direction == ConnectionDirection::Outbound {
            self.clear_backoff(source);
        }

        let msg = IncomingMessage { data: frame.payload, source };
        let result = match frame.kind {
            TcpFrameKind::Discovery => self.discovery_tx.send_async(msg).await,
            TcpFrameKind::UserData => self.user_data_tx.send_async(msg).await,
        };
        if let Err(error) = result {
            warn!(
                "TCP reader [{}]: failed to route {:?} frame on connection {}: {}",
                TransportErrorCode::TcpChannelFull,
                frame.kind,
                conn_id,
                error
            );
        }
    }

    pub(crate) fn remove_connection(&self, conn_id: ConnectionId) {
        if let Some((_, entry)) = self.connections.remove(&conn_id) {
            entry.cancel.cancel();
            debug!("TCP registry: removed conn {} ({:?})", conn_id, entry.remote_addr);
        }
    }

    /// Exact-address cleanup reliably covers outbound connections. Accepted
    /// sockets have ephemeral remote ports and are normally reaped by EOF or
    /// keepalive instead.
    pub(crate) fn remove_peer_by_addr(&self, addr: SocketAddr) {
        let ids: Vec<_> = self
            .connections
            .iter()
            .filter(|entry| entry.remote_addr == addr)
            .map(|entry| *entry.key())
            .collect();
        for id in ids {
            self.remove_connection(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use mio::{Events, Poll, Waker};

    use crate::rtps::transport::tokens::ListenerToken;

    fn registry() -> (Arc<ConnectionRegistry>, Receiver<IncomingMessage>, Receiver<IncomingMessage>)
    {
        let (discovery_tx, discovery_rx) = flume::bounded(1);
        let (user_tx, user_rx) = flume::bounded(1);
        (
            Arc::new(ConnectionRegistry::new(
                0,
                0,
                [0; 12],
                TcpSocketTuning::default(),
                discovery_tx,
                user_tx,
            )),
            discovery_rx,
            user_rx,
        )
    }

    #[tokio::test]
    async fn routes_arbitrary_payload_only_by_kind() {
        let (registry, discovery_rx, user_rx) = registry();
        let peer: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let id = registry.register_connection(
            peer,
            ConnectionDirection::Inbound,
            CancellationToken::new(),
        );

        registry
            .route_frame(
                id,
                TcpFrame {
                    kind: TcpFrameKind::Discovery,
                    payload: Bytes::from_static(b"not RTPS"),
                },
            )
            .await;
        assert_eq!(discovery_rx.recv().unwrap().data.as_ref(), b"not RTPS");
        assert!(user_rx.is_empty());

        registry
            .route_frame(
                id,
                TcpFrame { kind: TcpFrameKind::UserData, payload: Bytes::from_static(b"anything") },
            )
            .await;
        assert_eq!(user_rx.recv().unwrap().data.as_ref(), b"anything");
    }

    #[test]
    fn backoff_is_exponential_and_success_clears_it() {
        let (registry, _, _) = registry();
        let peer: SocketAddr = "127.0.0.1:7400".parse().unwrap();
        let error = io::Error::from(io::ErrorKind::TimedOut);
        registry.note_connect_failure(peer, &error);
        let first = registry.backoff_remaining(peer).unwrap();
        registry.note_connect_failure(peer, &error);
        let second = registry.backoff_remaining(peer).unwrap();
        assert!(second > first);
        registry.clear_backoff(peer);
        assert!(registry.backoff_remaining(peer).is_none());
    }

    #[test]
    fn exact_peer_cleanup_cancels_matching_connections_only() {
        let (registry, _, _) = registry();
        let a: SocketAddr = "127.0.0.1:7400".parse().unwrap();
        let b: SocketAddr = "127.0.0.1:7500".parse().unwrap();
        let ca = CancellationToken::new();
        registry.register_connection(a, ConnectionDirection::Outbound, ca.clone());
        registry.register_connection(b, ConnectionDirection::Outbound, CancellationToken::new());
        assert_eq!(registry.peer_count(), 2);
        registry.remove_peer_by_addr(a);
        assert!(ca.is_cancelled());
        assert_eq!(registry.connection_count(), 1);
    }

    #[test]
    fn socket_qos_is_applied_before_stream_use() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::net::TcpStream::connect(addr).unwrap();
        let (accepted, _) = listener.accept().unwrap();

        let tuning = TcpSocketTuning {
            nodelay: true,
            so_rcvbuf: Some(128 * 1024),
            so_sndbuf: Some(128 * 1024),
            unacked_timeout: Some(Duration::from_secs(7)),
            keepalive: Some(KeepaliveParams {
                time: Duration::from_secs(11),
                interval: Duration::from_secs(3),
                retries: 2,
            }),
        };
        apply_socket_tuning(&client, &tuning);
        apply_socket_tuning(&accepted, &tuning);

        assert!(client.nodelay().unwrap());
        let socket = socket2::SockRef::from(&client);
        assert!(socket.keepalive().unwrap());
        assert!(socket.recv_buffer_size().unwrap() >= 128 * 1024);
        assert!(socket.send_buffer_size().unwrap() >= 128 * 1024);
        #[cfg(any(target_os = "linux", target_os = "android"))]
        assert_eq!(socket.tcp_user_timeout().unwrap(), Some(Duration::from_secs(7)));
    }

    #[test]
    fn self_delivery_wakes_the_poller_and_releases_queued_bytes() {
        let (registry, _, _) = registry();
        let source: SocketAddr = "127.0.0.1:7400".parse().unwrap();
        assert!(!registry.deliver_to_self(source, TcpFrameKind::Discovery, b"discovery").unwrap());
        assert!(registry.deliver_to_self(source, TcpFrameKind::UserData, b"user").unwrap());
        assert_eq!(registry.self_delivery_bytes.load(Ordering::Acquire), 4);

        let mut poll = Poll::new().unwrap();
        let waker =
            Arc::new(Waker::new(poll.registry(), ListenerToken::Shutdown.to_mio()).unwrap());
        registry.set_self_delivery_waker(waker);
        let mut events = Events::with_capacity(2);
        poll.poll(&mut events, Some(Duration::from_secs(1))).unwrap();
        assert!(events.iter().any(|event| event.token() == ListenerToken::Shutdown.to_mio()));

        let message = registry.try_receive_self_delivery().unwrap();
        assert_eq!(message.source, source);
        assert_eq!(message.data.as_ref(), b"user");
        assert_eq!(registry.self_delivery_bytes.load(Ordering::Acquire), 0);
        assert!(registry.try_receive_self_delivery().is_none());

        registry.clear_self_delivery_waker();
        drop(registry);
        drop(poll);
    }
}
