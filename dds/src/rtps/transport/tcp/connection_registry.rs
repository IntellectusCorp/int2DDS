//! Per-participant state both halves of the TCP transport share.
//!
//! Socket tuning applied to every stream, the reconnect backoff a failed peer
//! earns, and the queue that carries a participant's own user data past the
//! socket. The registry knows nothing about RTPS packet contents.

use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use flume::{Receiver, Sender, TrySendError};
use mio::Waker;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::plugin::IncomingMessage;
use crate::rtps::transport::tcp::framing::{TcpBufferPool, TcpFrameKind};

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

type BackoffKey = (SocketAddr, TcpFrameKind);

pub(crate) struct ConnectionRegistry {
    pub(crate) tuning: TcpSocketTuning,
    buffer_pool: Arc<TcpBufferPool>,
    backoff: DashMap<BackoffKey, BackoffState>,

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
    ) -> Self {
        let (self_delivery_tx, self_delivery_rx) = flume::bounded(SELF_DELIVERY_COUNT_BACKSTOP);
        Self {
            tuning,
            buffer_pool: TcpBufferPool::new(),
            backoff: DashMap::new(),
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

    pub(crate) fn buffer_pool(&self) -> &Arc<TcpBufferPool> {
        &self.buffer_pool
    }

    pub(crate) fn backoff_remaining(&self, key: BackoffKey) -> Option<Duration> {
        let entry = self.backoff.get(&key)?;
        let now = Instant::now();
        (entry.next_attempt > now).then(|| entry.next_attempt - now)
    }

    pub(crate) fn note_connect_failure(&self, key: BackoffKey, err: &io::Error) {
        let now = Instant::now();
        let mut entry = self
            .backoff
            .entry(key)
            .or_insert(BackoffState { next_attempt: now, delay: Duration::ZERO });
        let delay = if err.kind() == io::ErrorKind::ConnectionRefused || entry.delay.is_zero() {
            BACKOFF_BASE
        } else {
            (entry.delay * 2).min(BACKOFF_MAX)
        };
        entry.delay = delay;
        entry.next_attempt = now + delay;
    }

    pub(crate) fn clear_backoff(&self, key: BackoffKey) {
        self.backoff.remove(&key);
    }

    pub(crate) fn clear_peer_backoff(&self, addr: SocketAddr) {
        self.backoff.retain(|(peer, _), _| *peer != addr);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use mio::{Events, Poll, Waker};

    use crate::rtps::transport::tokens::ListenerToken;

    fn registry() -> Arc<ConnectionRegistry> {
        Arc::new(ConnectionRegistry::new(0, 0, [0; 12], TcpSocketTuning::default()))
    }

    #[test]
    fn backoff_is_exponential_and_success_clears_it() {
        let registry = registry();
        let peer: SocketAddr = "127.0.0.1:7400".parse().unwrap();
        let key = (peer, TcpFrameKind::Discovery);
        let error = io::Error::from(io::ErrorKind::TimedOut);
        registry.note_connect_failure(key, &error);
        let first = registry.backoff_remaining(key).unwrap();
        registry.note_connect_failure(key, &error);
        let second = registry.backoff_remaining(key).unwrap();
        assert!(second > first);
        registry.clear_backoff(key);
        assert!(registry.backoff_remaining(key).is_none());
    }

    /// A refusal means the peer is not up yet, so the delay stays flat instead
    /// of growing past the announcement period.
    #[test]
    fn a_refusal_does_not_grow_the_delay() {
        let registry = registry();
        let peer: SocketAddr = "127.0.0.1:7400".parse().unwrap();
        let key = (peer, TcpFrameKind::Discovery);
        let error = io::Error::from(io::ErrorKind::ConnectionRefused);
        registry.note_connect_failure(key, &error);
        let first = registry.backoff_remaining(key).unwrap();
        registry.note_connect_failure(key, &error);
        let second = registry.backoff_remaining(key).unwrap();
        assert!(second <= first + Duration::from_millis(5));
        assert!(second <= BACKOFF_BASE);
    }

    /// One kind's failed dial must leave the other kind free to connect.
    #[test]
    fn a_backoff_is_confined_to_its_frame_kind() {
        let registry = registry();
        let peer: SocketAddr = "127.0.0.1:7400".parse().unwrap();
        let error = io::Error::from(io::ErrorKind::ConnectionRefused);
        registry.note_connect_failure((peer, TcpFrameKind::UserData), &error);
        assert!(registry.backoff_remaining((peer, TcpFrameKind::Discovery)).is_none());
        assert!(registry.backoff_remaining((peer, TcpFrameKind::UserData)).is_some());

        registry.clear_peer_backoff(peer);
        assert!(registry.backoff_remaining((peer, TcpFrameKind::UserData)).is_none());
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
        let registry = registry();
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
