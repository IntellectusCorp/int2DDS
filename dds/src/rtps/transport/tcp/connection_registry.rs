//! Per-connection state shared across the tasks that drive a connection.
//!
//! Each TCP connection is driven by a reader/writer task pair that both need to
//! see the same connection state. `ConnectionRegistry` is that single source of truth:
//! held in an `Arc`, it keeps one `ConnectionEntry` per connection (state,
//! remote addr, writer inbox, cancel token) and routes inbound frames via
//! `dispatch` — RTPS data to the DDS layer through the channel senders,
//! control frames to their handlers. A connection is torn down by firing its
//! `CancellationToken`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use flume::Sender;
use log::{debug, warn};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::TransportErrorCode;
use crate::rtps::transport::plugin::IncomingMessage;
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::protocol::ControlMsg;

mod handlers;

// ── ID + state types ─────────────────────────────────────────────────────────

/// Unique connection id, issued monotonically by `ConnectionRegistry::next_conn_id`.
pub(crate) type ConnectionId = usize;

/// Connection state machine — drives which dispatch handler runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    /// Awaiting the first PEER_HELLO(Control conn) or PORT_BIND frame(Data conn).
    AwaitingFirstMessage,
    /// Control connection: PORT_RESERVE.
    Control,
    /// Data connection: RTPS frames.
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionDirection {
    /// Accepted by the listener — peer initiated.
    Inbound,
    /// Initiated by `TcpSender::do_connect_*`.
    Outbound,
}

/// Groups the control / discovery / user-data connections of one remote
/// participant for per-connection bookkeeping and lookup.
#[derive(Debug, Default)]
pub(crate) struct PeerConnectionGroup {
    pub(crate) control_conn: Option<ConnectionId>,
    pub(crate) discovery_conn: Option<ConnectionId>,
    pub(crate) user_data_conn: Option<ConnectionId>,
}

impl PeerConnectionGroup {
    fn new() -> Self {
        Self { control_conn: None, discovery_conn: None, user_data_conn: None }
    }

    pub(crate) fn all_conns(&self) -> Vec<ConnectionId> {
        [self.control_conn, self.discovery_conn, self.user_data_conn]
            .iter()
            .flatten()
            .copied()
            .collect()
    }

    pub(crate) fn has_data_conns(&self) -> bool {
        self.discovery_conn.is_some() || self.user_data_conn.is_some()
    }
}

/// Per-connection bookkeeping shared across the actor pair.
pub(crate) struct ConnectionEntry {
    pub(crate) remote_addr: SocketAddr,
    pub(crate) state: ConnectionState,
    pub(crate) direction: ConnectionDirection,
    pub(crate) bound_logical_port: Option<u16>,
    pub(crate) remote_guid_prefix: Option<GuidPrefix>,
    /// Child token for the actor pair; cancelling it tears the pair down.
    pub(crate) cancel: CancellationToken,
    pub(crate) pending_ack: Option<Arc<Mutex<Option<oneshot::Sender<ControlMsg>>>>>,
}

// ── ConnectionRegistry ─────────────────────────────────────────────────────────────────

/// OS keepalive tuning (`SO_KEEPALIVE` + `TCP_KEEPIDLE/INTVL/CNT`) applied to
/// every connection. `time` is the idle period before the first probe,
/// `interval` the gap between probes, `retries` the unanswered probes tolerated
/// before the OS tears the connection down.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KeepaliveParams {
    pub(crate) time: Duration,
    pub(crate) interval: Duration,
    pub(crate) retries: u32,
}

/// Socket-level tuning applied to both inbound and outbound
/// TCP streams. Resolved per participant from `TcpConfig`.
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

/// Bound how long unacknowledged data may stay outstanding before the OS drops
/// the connection, so a dead link surfaces as a write error (instead of blocking
/// the sender ~indefinitely). Applied to every outbound/inbound stream.
pub(crate) fn apply_unacked_timeout(tcp: &tokio::net::TcpStream, timeout: Option<Duration>) {
    let Some(t) = timeout else { return };
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // TCP_USER_TIMEOUT, milliseconds.
        let _ = socket2::SockRef::from(tcp).set_tcp_user_timeout(Some(t));
    }
    #[cfg(target_os = "macos")]
    {
        // TCP_RXT_CONNDROPTIME, whole seconds (round up, min 1).
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
    // TODO(windows): TCP_MAXRTMS (ms) / TCP_MAXRT (s) via setsockopt(IPPROTO_TCP)
    // once a Windows test environment is available; until then OS keepalive covers it.
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
    {
        let _ = (tcp, t);
    }
}

/// Enable OS keepalive on a inbound/outbound stream so an idle connection whose
/// peer has silently gone (crash, link loss) is reaped by the kernel without an
/// application-level keepalive.
pub(crate) fn apply_keepalive(tcp: &tokio::net::TcpStream, params: Option<KeepaliveParams>) {
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
        // TCP_KEEPCNT (with_retries) is unsupported on Windows; set idle + interval.
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

/// Apply the full socket tuning (`nodelay`, send/recv buffers, unacked timeout,
/// keepalive) to a freshly established stream. Shared by the outbound connect
/// (`create_stream`) and the inbound accept loop so both sides tune identically.
pub(crate) fn apply_socket_tuning(tcp: &tokio::net::TcpStream, tuning: &TcpSocketTuning) {
    let _ = tcp.set_nodelay(tuning.nodelay);
    if let Some(sz) = tuning.so_rcvbuf {
        let _ = socket2::SockRef::from(tcp).set_recv_buffer_size(sz);
    }
    if let Some(sz) = tuning.so_sndbuf {
        let _ = socket2::SockRef::from(tcp).set_send_buffer_size(sz);
    }
    apply_unacked_timeout(tcp, tuning.unacked_timeout);
    apply_keepalive(tcp, tuning.keepalive);
}

/// First reconnect-backoff delay after a failed outbound connect.
pub(crate) const BACKOFF_BASE: Duration = Duration::from_millis(500);
/// Cap for the exponential reconnect-backoff growth (kept high on purpose so a
/// truly unreachable peer is not hammered).
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Per-peer reconnect backoff. After a failed outbound connect, no new connect
/// to the peer is attempted until `next_attempt`; `delay` doubles per
/// consecutive failure (capped at `BACKOFF_MAX`). Cleared on an outbound success
/// or when the peer proves reachable via its inbound PEER_HELLO.
struct BackoffState {
    next_attempt: Instant,
    delay: Duration,
}

/// Thread-safe shared state for the mux listener.
pub(crate) struct ConnectionRegistry {
    pub(crate) domain_id: u32,
    pub(crate) participant_id: u32,
    #[allow(dead_code)]
    local_guid_prefix: GuidPrefix,

    pub(crate) tuning: TcpSocketTuning,

    pub(crate) connections: DashMap<ConnectionId, ConnectionEntry>,
    peer_connections: Mutex<HashMap<GuidPrefix, PeerConnectionGroup>>,

    backoff: DashMap<SocketAddr, BackoffState>,

    /// Cookie issued at PORT_RESERVE → consumed at PORT_BIND.
    cookie_to_port: DashMap<[u8; 16], u16>,
    cookie_to_guid: DashMap<[u8; 16], GuidPrefix>,
    next_cookie: AtomicU8,

    pub(crate) next_conn_id: AtomicUsize,

    discovery_tx: Sender<IncomingMessage>,
    user_data_tx: Sender<IncomingMessage>,
}

impl ConnectionRegistry {
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        local_guid_prefix: GuidPrefix,
        tuning: TcpSocketTuning,
        discovery_tx: Sender<IncomingMessage>,
        user_data_tx: Sender<IncomingMessage>,
    ) -> Self {
        Self {
            domain_id,
            participant_id,
            local_guid_prefix,
            tuning,
            connections: DashMap::new(),
            peer_connections: Mutex::new(HashMap::new()),
            backoff: DashMap::new(),
            cookie_to_port: DashMap::new(),
            cookie_to_guid: DashMap::new(),
            next_cookie: AtomicU8::new(0x31),
            next_conn_id: AtomicUsize::new(0),
            discovery_tx,
            user_data_tx,
        }
    }

    // ── basic counters ───────────────────────────────────────────────────────

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn peer_count(&self) -> usize {
        self.peer_connections.lock().expect("peer_connections lock").len()
    }

    // ── reconnect backoff ──────────────────────────────────────────────────────

    /// Remaining fail-fast window for `addr`, or `None` if a connect may proceed.
    pub(crate) fn backoff_remaining(&self, addr: SocketAddr) -> Option<Duration> {
        let entry = self.backoff.get(&addr)?;
        let now = Instant::now();
        (entry.next_attempt > now).then(|| entry.next_attempt - now)
    }

    /// Record a failed outbound connect: grow the backoff (exponential, capped).
    pub(crate) fn note_connect_failure(&self, addr: SocketAddr) {
        let now = Instant::now();
        let mut entry = self
            .backoff
            .entry(addr)
            .or_insert(BackoffState { next_attempt: now, delay: Duration::ZERO });
        let delay =
            if entry.delay.is_zero() { BACKOFF_BASE } else { (entry.delay * 2).min(BACKOFF_MAX) };
        entry.delay = delay;
        entry.next_attempt = now + delay;
    }

    /// Clear a peer's backoff.
    pub(crate) fn clear_backoff(&self, addr: SocketAddr) {
        self.backoff.remove(&addr);
    }

    /// Register a freshly-accepted inbound connection.
    ///
    /// Called by `mux_listener::accept_task` after spawning the reader/writer task pair.
    /// Returns the assigned `ConnectionId` so the caller can keep a local handle
    /// for logging / metrics.
    pub(crate) fn register_inbound_connection(
        &self,
        remote_addr: SocketAddr,
        cancel: CancellationToken,
    ) -> ConnectionId {
        let conn_id = self.next_conn_id.fetch_add(1, Ordering::SeqCst);
        self.connections.insert(
            conn_id,
            ConnectionEntry {
                remote_addr,
                state: ConnectionState::AwaitingFirstMessage,
                direction: ConnectionDirection::Inbound,
                bound_logical_port: None,
                remote_guid_prefix: None,
                cancel,
                // Inbound connections never await outbound responses — slot stays None.
                pending_ack: None,
            },
        );
        conn_id
    }

    /// Register an outbound **control** connection that has just completed
    /// the PEER_HELLO handshake. The `pending_ack` slot is the single-slot
    /// oneshot mailbox where `dispatch` will route PORT_RESERVE_ACK /
    /// PORT_BIND_ACK / Error responses for this connection.
    ///
    /// Starts in `Control` state — bypasses `AwaitingFirstMessage` since the
    /// initial handshake was driven inline by the sender before this entry
    /// was created. Also pre-registers the peer group so subsequent data
    /// connections can be grouped under the same GUID.
    pub(crate) fn register_outbound_control_connection(
        &self,
        remote_addr: SocketAddr,
        cancel: CancellationToken,
        pending_ack: Arc<Mutex<Option<oneshot::Sender<ControlMsg>>>>,
    ) -> ConnectionId {
        let conn_id = self.next_conn_id.fetch_add(1, Ordering::SeqCst);
        let synthetic_guid = addr_to_guid(remote_addr);

        self.connections.insert(
            conn_id,
            ConnectionEntry {
                remote_addr,
                state: ConnectionState::Control,
                direction: ConnectionDirection::Outbound,
                bound_logical_port: None,
                remote_guid_prefix: Some(synthetic_guid),
                cancel,
                pending_ack: Some(pending_ack),
            },
        );

        let mut pc = self.peer_connections.lock().expect("peer_connections lock");
        let group = pc.entry(synthetic_guid).or_insert_with(PeerConnectionGroup::new);
        group.control_conn = Some(conn_id);

        debug!(
            "TcpMuxListener: Registered outbound control conn {} (addr={:?})",
            conn_id, remote_addr
        );
        conn_id
    }

    /// Register an outbound **data** connection that has just completed the
    /// PORT_BIND handshake. Starts in `Active` state with `bound_logical_port`
    /// already set, so `dispatch` routes inbound RTPS data straight to the
    /// inbound channels. No `pending_ack` — data connections don't expect
    /// control responses.
    pub(crate) fn register_outbound_data_connection(
        &self,
        remote_addr: SocketAddr,
        logical_port: u16,
        cancel: CancellationToken,
    ) -> ConnectionId {
        let conn_id = self.next_conn_id.fetch_add(1, Ordering::SeqCst);
        let synthetic_guid = addr_to_guid(remote_addr);

        self.connections.insert(
            conn_id,
            ConnectionEntry {
                remote_addr,
                state: ConnectionState::Active,
                direction: ConnectionDirection::Outbound,
                bound_logical_port: Some(logical_port),
                remote_guid_prefix: Some(synthetic_guid),
                cancel,
                pending_ack: None,
            },
        );

        let mut pc = self.peer_connections.lock().expect("peer_connections lock");
        let group = pc.entry(synthetic_guid).or_insert_with(PeerConnectionGroup::new);
        if PortManager::is_discovery_unicast_port_logically(self.domain_id, logical_port) {
            group.discovery_conn = Some(conn_id);
        } else {
            group.user_data_conn = Some(conn_id);
        }

        debug!(
            "TcpMuxListener: Registered outbound data conn {} (addr={:?}, port={})",
            conn_id, remote_addr, logical_port
        );
        conn_id
    }

    // ── connection / peer cleanup ────────────────────────────────────────────

    /// Update peer_connections bookkeeping then remove the connection entry.
    pub(crate) fn remove_connection(&self, conn_id: ConnectionId) {
        let guid_opt = self.connections.get(&conn_id).and_then(|e| e.remote_guid_prefix);
        if let Some(guid) = guid_opt {
            let mut control_removed = false;
            {
                let mut pc = self.peer_connections.lock().expect("peer_connections lock");
                if let Some(group) = pc.get_mut(&guid) {
                    if group.control_conn == Some(conn_id) {
                        group.control_conn = None;
                        control_removed = true;
                    }
                    if group.discovery_conn == Some(conn_id) {
                        group.discovery_conn = None;
                    }
                    if group.user_data_conn == Some(conn_id) {
                        group.user_data_conn = None;
                    }

                    if group.all_conns().is_empty() {
                        pc.remove(&guid);
                    }
                }
            }

            if control_removed {
                self.purge_cookies_for_guid(guid);
            }
        }
        self.remove_connection_inner(conn_id);
    }

    /// Drop every pending PORT_RESERVE cookie issued for `guid`.
    fn purge_cookies_for_guid(&self, guid: GuidPrefix) {
        let stale: Vec<[u8; 16]> =
            self.cookie_to_guid.iter().filter(|e| *e.value() == guid).map(|e| *e.key()).collect();
        for cookie in stale {
            self.cookie_to_guid.remove(&cookie);
            self.cookie_to_port.remove(&cookie);
        }
    }

    /// Tear down every connection (inbound + outbound) grouped under the peer at
    /// `addr`, cancelling their actor pairs and purging its pending cookies.
    /// Used by the DDS-unmatch cleanup path so a peer's inbound connections are
    /// released too — they group under the peer's advertised listener address.
    pub(crate) fn remove_peer_by_addr(&self, addr: SocketAddr) {
        let guid = addr_to_guid(addr);
        let conns = {
            let mut pc = self.peer_connections.lock().expect("peer_connections lock");
            pc.remove(&guid).map(|g| g.all_conns()).unwrap_or_default()
        };
        for conn_id in conns {
            self.remove_connection_inner(conn_id);
        }
        self.purge_cookies_for_guid(guid);
    }

    fn remove_connection_inner(&self, conn_id: ConnectionId) {
        if let Some((_, entry)) = self.connections.remove(&conn_id) {
            // Wake the reader/writer task pair so they can tear down even if the
            // caller did not cancel them explicitly.
            entry.cancel.cancel();
            debug!("TcpMuxListener: Removed conn {} (addr={:?})", conn_id, entry.remote_addr);
        }
    }
}

/// Derive a synthetic GuidPrefix from a remote socket address.
/// Used to group connections from the same participant when we don't yet
/// know the real GUID prefix.
fn addr_to_guid(addr: SocketAddr) -> GuidPrefix {
    let mut g = [0u8; 12];
    if let SocketAddr::V4(v4) = addr {
        g[0..4].copy_from_slice(&v4.ip().octets());
        g[4..6].copy_from_slice(&v4.port().to_be_bytes());
    }
    g
}

fn send_control(writer_tx: &mpsc::Sender<Vec<u8>>, msg: &ControlMsg) {
    if let Err(e) = writer_tx.try_send(msg.to_bytes()) {
        warn!(
            "TcpMuxListener [{}]: Failed to send {}: {:?}",
            TransportErrorCode::TcpControlSendFailed,
            msg.type_name(),
            e
        );
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// `PeerConnectionGroup` tracks occupied roles and reports `has_data_conns`.
    #[test]
    fn peer_connection_group_all_tokens() {
        let mut group = PeerConnectionGroup::new();
        assert!(group.all_conns().is_empty());
        assert!(!group.has_data_conns());

        group.control_conn = Some(100);
        assert_eq!(group.all_conns().len(), 1);
        assert!(!group.has_data_conns());

        group.discovery_conn = Some(101);
        group.user_data_conn = Some(102);
        assert_eq!(group.all_conns().len(), 3);
        assert!(group.has_data_conns());
    }

    /// The tuning reaches the socket.
    ///
    /// Every timeout that lets a dead peer be noticed is a kernel setting, so if
    /// these silently fail to apply there is nothing of ours left to observe: the
    /// connection would just hang on the OS defaults (~15 min for unacked data,
    /// keepalive off) and look healthy. What the kernel then does with them is
    /// its own business — an expiry surfaces as an ordinary read/write error.
    #[tokio::test(flavor = "multi_thread")]
    async fn socket_tuning_is_applied_to_the_stream() {
        let tuning = TcpSocketTuning {
            nodelay: true,
            so_rcvbuf: None,
            so_sndbuf: None,
            unacked_timeout: Some(Duration::from_secs(7)),
            keepalive: Some(KeepaliveParams {
                time: Duration::from_secs(11),
                interval: Duration::from_secs(3),
                retries: 4,
            }),
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = tokio::spawn(async move { listener.accept().await.unwrap() });
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let _accepted = accept.await.unwrap();

        apply_socket_tuning(&tcp, &tuning);

        let sock = socket2::SockRef::from(&tcp);
        assert!(tcp.nodelay().unwrap(), "nodelay must be set");
        assert!(sock.keepalive().unwrap(), "keepalive must be enabled");
        assert_eq!(sock.keepalive_time().unwrap(), Duration::from_secs(11));
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
        {
            assert_eq!(sock.keepalive_interval().unwrap(), Duration::from_secs(3));
            assert_eq!(sock.keepalive_retries().unwrap(), 4);
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        assert_eq!(sock.tcp_user_timeout().unwrap(), Some(Duration::from_secs(7)));
    }

    /// `None` leaves the OS defaults alone rather than applying a zero.
    #[tokio::test(flavor = "multi_thread")]
    async fn default_tuning_leaves_keepalive_off() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = tokio::spawn(async move { listener.accept().await.unwrap() });
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let _accepted = accept.await.unwrap();

        apply_socket_tuning(&tcp, &TcpSocketTuning::default());

        let sock = socket2::SockRef::from(&tcp);
        assert!(!sock.keepalive().unwrap(), "default tuning must not enable keepalive");
        #[cfg(any(target_os = "linux", target_os = "android"))]
        assert_eq!(
            sock.tcp_user_timeout().unwrap(),
            None,
            "unacked timeout must be left at the OS default",
        );
    }

    /// Removing a control connection purges its pending PORT_RESERVE cookies
    /// (reserved but never bound), without touching another peer's cookies.
    #[test]
    fn remove_control_connection_purges_pending_cookies() {
        use flume::bounded;

        let (d_tx, _d_rx) = bounded(8);
        let (u_tx, _u_rx) = bounded(8);
        let shared =
            ConnectionRegistry::new(0, 0, [0u8; 12], TcpSocketTuning::default(), d_tx, u_tx);

        let addr: SocketAddr = "127.0.0.1:7400".parse().unwrap();
        let conn_id = shared.register_outbound_control_connection(
            addr,
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
        );
        let guid = addr_to_guid(addr);

        // Two pending cookies for this peer, one for another peer.
        shared.cookie_to_guid.insert([1u8; 16], guid);
        shared.cookie_to_port.insert([1u8; 16], 100);
        let other = addr_to_guid("127.0.0.1:7500".parse().unwrap());
        shared.cookie_to_guid.insert([9u8; 16], other);
        shared.cookie_to_port.insert([9u8; 16], 200);

        shared.remove_connection(conn_id);

        assert!(shared.cookie_to_guid.get(&[1u8; 16]).is_none(), "peer cookie purged");
        assert!(shared.cookie_to_port.get(&[1u8; 16]).is_none(), "peer cookie port purged");
        assert!(shared.cookie_to_guid.get(&[9u8; 16]).is_some(), "other peer's cookie kept");
        assert!(shared.cookie_to_port.get(&[9u8; 16]).is_some(), "other peer's cookie port kept");
    }
}
