#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dashmap::DashMap;
use log::{debug, info, warn};

use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::transport::error::TransportErrorCode;
use crate::rtps::transport::plugin::IncomingMessage;
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::framing::{
    classify_frame, write_framed_message, FramedReader, TcpFrameKind,
};
use crate::rtps::transport::tcp::protocol::{
    generate_cookie, ControlMsg, ERR_CODE_IDLE_TIMEOUT, ERR_CODE_INVALID_COOKIE,
    ERR_CODE_INVALID_PORT, MSG_PORT_BIND, MSG_PORT_RESERVE, OP_IDLE_TIMEOUT,
};
use crate::rtps::transport::tcp::stream_wrapper::TcpStreamWrapper;

/// Unique identifier for an accepted connection (replaces mio::Token).
pub(crate) type ConnectionId = usize;

/// Outcome of `handle_first_message` for the read loop to act on.
pub(crate) enum FirstMessageOutcome {
    /// Keep running the read loop on this connection (normal path).
    Continue,
}

/// Connection state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    /// Waiting for first message (PEER_HELLO / PORT_BIND).
    AwaitingFirstMessage,
    /// PEER_HELLO done — control connection, accepts PORT_RESERVE + KEEPALIVE.
    Control,
    /// PORT_BIND done — data connection, accepts RTPS data.
    Active,
    Closing,
}

/// Per-peer group: tracks which connections (control / discovery / user-data)
/// belong to the same remote participant.
#[derive(Debug)]
struct PeerConnectionGroup {
    control_conn: Option<ConnectionId>,
    discovery_conn: Option<ConnectionId>,
    user_data_conn: Option<ConnectionId>,
}

impl PeerConnectionGroup {
    fn new() -> Self {
        Self { control_conn: None, discovery_conn: None, user_data_conn: None }
    }

    fn all_conns(&self) -> Vec<ConnectionId> {
        [self.control_conn, self.discovery_conn, self.user_data_conn]
            .iter()
            .flatten()
            .copied()
            .collect()
    }

    fn has_data_conns(&self) -> bool {
        self.discovery_conn.is_some() || self.user_data_conn.is_some()
    }
}

/// Metadata stored in the shared map for each active connection.
/// The stream itself is owned by the connection's read thread.
pub(crate) struct ConnectionEntry {
    pub(crate) remote_addr: SocketAddr,
    pub(crate) state: ConnectionState,
    pub(crate) bound_logical_port: Option<u16>,
    pub(crate) remote_guid_prefix: Option<GuidPrefix>,
    pub(crate) last_activity: Instant,
    /// Set by prune logic to signal the read thread to exit.
    pub(crate) shutdown: Arc<AtomicBool>,
    /// When true, the read thread should send an ERROR(IDLE_TIMEOUT) before
    /// exiting (used by prune_idle_connections to notify the remote peer).
    pub(crate) error_on_exit: Arc<AtomicBool>,
}

// ── Shared state (all connections share this via Arc) ────────────────────────

/// Thread-safe shared state for the mux listener.
/// Connections are tracked in a DashMap; peer groups in a Mutex<HashMap>.
pub(crate) struct MuxListenerShared {
    pub(crate) domain_id: u32,
    pub(crate) participant_id: u32,
    local_guid_prefix: GuidPrefix,
    pub(crate) connections: DashMap<ConnectionId, ConnectionEntry>,
    peer_connections: Mutex<HashMap<GuidPrefix, PeerConnectionGroup>>,
    cookie_to_port: DashMap<[u8; 16], u16>,
    cookie_to_guid: DashMap<[u8; 16], GuidPrefix>,
    next_cookie: AtomicU8,
    pub(crate) next_conn_id: AtomicUsize,
    discovery_tx: Sender<IncomingMessage>,
    user_data_tx: Sender<IncomingMessage>,
}

impl MuxListenerShared {
    fn new(
        domain_id: u32,
        participant_id: u32,
        local_guid_prefix: GuidPrefix,
        discovery_tx: Sender<IncomingMessage>,
        user_data_tx: Sender<IncomingMessage>,
    ) -> Self {
        Self {
            domain_id,
            participant_id,
            local_guid_prefix,
            connections: DashMap::new(),
            peer_connections: Mutex::new(HashMap::new()),
            cookie_to_port: DashMap::new(),
            cookie_to_guid: DashMap::new(),
            next_cookie: AtomicU8::new(0x31),
            next_conn_id: AtomicUsize::new(0),
            discovery_tx,
            user_data_tx,
        }
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn peer_count(&self) -> usize {
        self.peer_connections.lock().expect("peer_connections lock").len()
    }

    // ── Idle timeout pruning ─────────────────────────────────────────────────

    /// Drop connections whose last_activity has exceeded `timeout`.
    ///
    /// Sets the `error_on_exit` flag so the read thread sends an
    /// ERROR(IDLE_TIMEOUT) notice to the remote peer before closing.
    /// Returns the number of connections removed from the map.
    pub(crate) fn prune_idle_connections(&self, timeout: Duration) -> usize {
        let now = Instant::now();
        let stale: Vec<ConnectionId> = self
            .connections
            .iter()
            .filter(|e| now.duration_since(e.last_activity) > timeout)
            .map(|e| *e.key())
            .collect();

        for conn_id in &stale {
            if let Some(entry) = self.connections.get(conn_id) {
                warn!(
                    "TcpMuxListener [{}]: Pruning idle conn {} from {:?} (idle {:?})",
                    TransportErrorCode::TcpConnectionIdlePruned,
                    conn_id,
                    entry.remote_addr,
                    now.duration_since(entry.last_activity),
                );
                // Signal read thread to send ERROR before exiting.
                entry.error_on_exit.store(true, Ordering::SeqCst);
                entry.shutdown.store(true, Ordering::SeqCst);
            }
            self.remove_connection(*conn_id);
        }

        stale.len()
    }

    // ── Connection / peer cleanup ────────────────────────────────────────────

    pub(crate) fn remove_peer(&self, guid: GuidPrefix) {
        let group = self.peer_connections.lock().expect("peer_connections lock").remove(&guid);
        if let Some(group) = group {
            for conn_id in group.all_conns() {
                self.remove_connection_inner(conn_id);
            }
            debug!("TcpMuxListener: Removed peer {}", Guid::guid_prefix_to_string(&guid));
        }
    }

    /// Update peer_connections bookkeeping and remove the connection entry.
    pub(crate) fn remove_connection(&self, conn_id: ConnectionId) {
        let guid_opt = self.connections.get(&conn_id).and_then(|e| e.remote_guid_prefix);
        if let Some(guid) = guid_opt {
            let mut pc = self.peer_connections.lock().expect("peer_connections lock");
            if let Some(group) = pc.get_mut(&guid) {
                if group.control_conn == Some(conn_id) {
                    group.control_conn = None;
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
        self.remove_connection_inner(conn_id);
    }

    fn remove_connection_inner(&self, conn_id: ConnectionId) {
        if let Some((_, entry)) = self.connections.remove(&conn_id) {
            // Signal the read thread to exit (if not already done).
            entry.shutdown.store(true, Ordering::SeqCst);
            debug!("TcpMuxListener: Removed conn {} (addr={:?})", conn_id, entry.remote_addr);
        }
    }

    // ── Protocol handlers (called from connection read threads) ──────────────

    pub(crate) fn handle_first_message(
        &self,
        stream: &mut Box<dyn TcpStreamWrapper>,
        conn_id: ConnectionId,
        payload: &[u8],
    ) -> FirstMessageOutcome {
        let msg = match ControlMsg::from_bytes(payload) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "TcpMuxListener [{}]: Bad first message on conn {}: {:?}",
                    TransportErrorCode::TcpControlProtocolError,
                    conn_id,
                    e
                );
                // Signal the read thread to exit.
                if let Some(entry) = self.connections.get(&conn_id) {
                    entry.shutdown.store(true, Ordering::SeqCst);
                }
                return FirstMessageOutcome::Continue;
            }
        };

        match msg {
            ControlMsg::PeerHello { locator: _ } => {
                send_control(stream, &ControlMsg::PeerHelloAck);

                if let Some(mut conn) = self.connections.get_mut(&conn_id) {
                    conn.state = ConnectionState::Control;
                }

                // Register in peer group (synthetic guid from remote address).
                let remote_addr = self.connections.get(&conn_id).map(|c| c.remote_addr);
                if let Some(addr) = remote_addr {
                    let synthetic_guid = addr_to_guid(addr);
                    let mut pc = self.peer_connections.lock().expect("peer_connections lock");
                    let group = pc.entry(synthetic_guid).or_insert_with(PeerConnectionGroup::new);
                    group.control_conn = Some(conn_id);

                    if let Some(mut conn) = self.connections.get_mut(&conn_id) {
                        conn.remote_guid_prefix = Some(synthetic_guid);
                    }
                }

                debug!("TcpMuxListener: PEER_HELLO ok (conn={})", conn_id);
                FirstMessageOutcome::Continue
            }

            ControlMsg::PortBind { cookie } => {
                self.handle_port_bind(stream, conn_id, &cookie);
                FirstMessageOutcome::Continue
            }

            other => {
                warn!(
                    "TcpMuxListener: Expected PEER_HELLO / PORT_BIND, got {} on conn {}",
                    other.type_name(),
                    conn_id
                );
                if let Some(entry) = self.connections.get(&conn_id) {
                    entry.shutdown.store(true, Ordering::SeqCst);
                }
                FirstMessageOutcome::Continue
            }
        }
    }

    pub(crate) fn handle_control_frame(
        &self,
        stream: &mut Box<dyn TcpStreamWrapper>,
        conn_id: ConnectionId,
        payload: &[u8],
    ) {
        let msg = match ControlMsg::from_bytes(payload) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "TcpMuxListener [{}]: Bad control msg on conn {}: {:?}",
                    TransportErrorCode::TcpControlProtocolError,
                    conn_id,
                    e
                );
                return;
            }
        };

        match msg {
            ControlMsg::PortReserve { logical_port } => {
                let my_disc = PortManager::get_discovery_traffic_unicast_port(
                    self.domain_id,
                    self.participant_id,
                );
                let my_user =
                    PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id);

                if logical_port != my_disc && logical_port != my_user {
                    warn!(
                        "TcpMuxListener [{}]: Invalid port {} on conn {}",
                        TransportErrorCode::TcpControlInvalidPort,
                        logical_port,
                        conn_id
                    );
                    send_control(
                        stream,
                        &ControlMsg::Error {
                            operation: MSG_PORT_RESERVE,
                            code: ERR_CODE_INVALID_PORT,
                            message: "no matching port".to_string(),
                        },
                    );
                    return;
                }

                // Generate cookie atomically.
                let counter_val = self.next_cookie.fetch_add(1, Ordering::SeqCst);
                let mut c = counter_val;
                let cookie = generate_cookie(&mut c);

                self.cookie_to_port.insert(cookie, logical_port);
                if let Some(ctrl_guid) =
                    self.connections.get(&conn_id).and_then(|c| c.remote_guid_prefix)
                {
                    self.cookie_to_guid.insert(cookie, ctrl_guid);
                }

                send_control(stream, &ControlMsg::PortReserveAck { cookie });

                debug!(
                    "TcpMuxListener: PORT_RESERVE ok (port={}, cookie=0x{:02x})",
                    logical_port, cookie[0]
                );
            }

            ControlMsg::Keepalive => {
                send_control(stream, &ControlMsg::KeepaliveAck);
            }

            other => {
                debug!(
                    "TcpMuxListener: Ignoring {} on control conn {}",
                    other.type_name(),
                    conn_id
                );
            }
        }
    }

    fn handle_port_bind(
        &self,
        stream: &mut Box<dyn TcpStreamWrapper>,
        conn_id: ConnectionId,
        cookie: &[u8; 16],
    ) {
        let logical_port = match self.cookie_to_port.remove(cookie) {
            Some((_, port)) => port,
            None => {
                let cookie_hex: String = cookie.iter().map(|b| format!("{:02x}", b)).collect();
                warn!(
                    "TcpMuxListener [{}]: Unknown cookie [{}] on conn {}",
                    TransportErrorCode::TcpControlInvalidCookie,
                    cookie_hex,
                    conn_id
                );
                send_control(
                    stream,
                    &ControlMsg::Error {
                        operation: MSG_PORT_BIND,
                        code: ERR_CODE_INVALID_COOKIE,
                        message: format!("invalid cookie [{}]", cookie_hex),
                    },
                );
                if let Some(entry) = self.connections.get(&conn_id) {
                    entry.shutdown.store(true, Ordering::SeqCst);
                }
                return;
            }
        };

        send_control(stream, &ControlMsg::PortBindAck);

        if let Some(mut conn) = self.connections.get_mut(&conn_id) {
            conn.bound_logical_port = Some(logical_port);
            conn.state = ConnectionState::Active;
        }

        // Resolve peer group — prefer the guid from PORT_RESERVE time so the
        // data connection lands in the same group as the control connection.
        let group_guid = self
            .cookie_to_guid
            .remove(cookie)
            .map(|(_, g)| g)
            .or_else(|| self.connections.get(&conn_id).map(|c| addr_to_guid(c.remote_addr)));

        if let Some(guid) = group_guid {
            let mut pc = self.peer_connections.lock().expect("peer_connections lock");
            let group = pc.entry(guid).or_insert_with(PeerConnectionGroup::new);

            if PortManager::is_discovery_unicast_port_logically(self.domain_id, logical_port) {
                group.discovery_conn = Some(conn_id);
            } else {
                group.user_data_conn = Some(conn_id);
            }

            if let Some(mut conn) = self.connections.get_mut(&conn_id) {
                conn.remote_guid_prefix = Some(guid);
            }
        }

        debug!(
            "TcpMuxListener: PORT_BIND ok (conn={}, port={}, cookie=0x{:02x})",
            conn_id, logical_port, cookie[0]
        );
    }

    pub(crate) fn handle_active_frame(&self, conn_id: ConnectionId, payload: &[u8]) {
        if matches!(classify_frame(payload), TcpFrameKind::RtpsData) {
            let remote_addr = match self.connections.get(&conn_id).map(|c| c.remote_addr) {
                Some(a) => a,
                None => return,
            };
            self.route_rtps_data(conn_id, payload, remote_addr);
        }
    }

    fn route_rtps_data(&self, conn_id: ConnectionId, payload: &[u8], remote_addr: SocketAddr) {
        let logical_port = match self.connections.get(&conn_id).and_then(|c| c.bound_logical_port) {
            Some(p) => p,
            None => return,
        };

        let msg = IncomingMessage { data: payload.to_vec(), source: remote_addr };

        if PortManager::is_discovery_unicast_port_logically(self.domain_id, logical_port) {
            if let Err(e) = self.discovery_tx.try_send(msg) {
                warn!(
                    "TcpMuxListener [{}]: Failed to route discovery: {:?}",
                    TransportErrorCode::TcpChannelFull,
                    e
                );
            }
        } else if PortManager::is_user_unicast_port_logically(self.domain_id, logical_port) {
            if let Err(e) = self.user_data_tx.try_send(msg) {
                warn!(
                    "TcpMuxListener [{}]: Failed to route user data: {:?}",
                    TransportErrorCode::TcpChannelFull,
                    e
                );
            }
        }
    }
}

// ── TcpMuxListener ───────────────────────────────────────────────────────────

/// TCP multiplexed listener.
///
/// Binds a TCP listen socket and exposes shared state for connection threads.
/// The actual accept loop lives in `TcpMuxListeningLoopTask` (tcp_transport_plugin.rs).
pub(crate) struct TcpMuxListener {
    port: u16,
    listener: Option<TcpListener>,
    pub(crate) shared: Arc<MuxListenerShared>,
}

impl TcpMuxListener {
    pub(crate) fn new(
        port: u16,
        domain_id: u32,
        participant_id: u32,
        local_guid_prefix: GuidPrefix,
        discovery_tx: Sender<IncomingMessage>,
        user_data_tx: Sender<IncomingMessage>,
    ) -> io::Result<Self> {
        let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
        let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)?;
        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;
        socket.bind(&addr.into())?;
        socket.listen(128)?;

        let listener = TcpListener::from(socket);
        let actual_port = listener.local_addr()?.port();
        info!("TcpMuxListener: Listening on port {} (domain={})", actual_port, domain_id);

        Ok(Self {
            port: actual_port,
            listener: Some(listener),
            shared: Arc::new(MuxListenerShared::new(
                domain_id,
                participant_id,
                local_guid_prefix,
                discovery_tx,
                user_data_tx,
            )),
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Remove the underlying `TcpListener` for use in the accept loop.
    pub(crate) fn take_listener(&mut self) -> Option<TcpListener> {
        self.listener.take()
    }

    /// Accept one connection from the listener and spawn its read thread.
    ///
    /// Returns the `ConnectionId` assigned to the new connection.
    pub(crate) fn accept_connection(
        &self,
        stream: Box<dyn TcpStreamWrapper>,
        addr: SocketAddr,
        terminated: Arc<AtomicBool>,
        idle_timeout: Duration,
    ) -> ConnectionId {
        let conn_id = self.shared.next_conn_id.fetch_add(1, Ordering::SeqCst);
        let shutdown = Arc::new(AtomicBool::new(false));
        let error_on_exit = Arc::new(AtomicBool::new(false));

        self.shared.connections.insert(
            conn_id,
            ConnectionEntry {
                remote_addr: addr,
                state: ConnectionState::AwaitingFirstMessage,
                bound_logical_port: None,
                remote_guid_prefix: None,
                last_activity: Instant::now(),
                shutdown: shutdown.clone(),
                error_on_exit: error_on_exit.clone(),
            },
        );

        let conn_shared = Arc::clone(&self.shared);
        let _ = thread::Builder::new()
            .name(format!("tcp_conn_{}", conn_id))
            .stack_size(128 * 1024)
            .spawn(move || {
                read_loop(
                    stream,
                    conn_id,
                    conn_shared,
                    terminated,
                    shutdown,
                    error_on_exit,
                    idle_timeout,
                );
            });

        conn_id
    }

    // Convenience delegates for callers that have a TcpMuxListener reference.

    pub(crate) fn prune_idle_connections(&self, timeout: Duration) -> usize {
        self.shared.prune_idle_connections(timeout)
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.shared.connection_count()
    }

    pub(crate) fn peer_count(&self) -> usize {
        self.shared.peer_count()
    }

    pub(crate) fn close(&mut self) {
        self.listener.take(); // Drop the listener socket.
                              // Signal all active read threads to exit before clearing the map.
        for entry in self.shared.connections.iter() {
            entry.shutdown.store(true, Ordering::SeqCst);
        }
        self.shared.connections.clear();
        self.shared.peer_connections.lock().expect("peer_connections lock").clear();
        self.shared.cookie_to_port.clear();
        self.shared.cookie_to_guid.clear();
        info!("TcpMuxListener: Closed (port {})", self.port);
    }
}

impl Drop for TcpMuxListener {
    fn drop(&mut self) {
        self.close();
    }
}

// ── Per-connection read thread ───────────────────────────────────────────────

/// Run the read loop for a single connection.
///
/// Owns the `stream` exclusively (read + write for protocol responses).
/// Exits when `terminated`, `shutdown`, or a read error is detected.
pub(crate) fn read_loop(
    mut stream: Box<dyn TcpStreamWrapper>,
    conn_id: ConnectionId,
    shared: Arc<MuxListenerShared>,
    terminated: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    error_on_exit: Arc<AtomicBool>,
    idle_timeout: Duration,
) {
    // 100 ms read timeout — allows periodic checks of the control flags.
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let mut framed = FramedReader::new();

    loop {
        // Global termination or per-connection shutdown.
        if terminated.load(Ordering::SeqCst) || shutdown.load(Ordering::SeqCst) {
            if error_on_exit.load(Ordering::SeqCst) {
                let err = ControlMsg::Error {
                    operation: OP_IDLE_TIMEOUT,
                    code: ERR_CODE_IDLE_TIMEOUT,
                    message: "incoming connection idle timeout".to_string(),
                };
                let _ = write_framed_message(&mut stream, &err.to_bytes());
            }
            break;
        }

        match framed.read_message(&mut stream) {
            Ok(Some(msg)) => {
                // Update last_activity timestamp.
                if let Some(mut e) = shared.connections.get_mut(&conn_id) {
                    e.last_activity = Instant::now();
                }

                let state = shared.connections.get(&conn_id).map(|e| e.state);
                match state {
                    Some(ConnectionState::AwaitingFirstMessage) => {
                        match shared.handle_first_message(&mut stream, conn_id, &msg) {
                            FirstMessageOutcome::Continue => {}
                        }
                    }
                    Some(ConnectionState::Control) => {
                        shared.handle_control_frame(&mut stream, conn_id, &msg);
                    }
                    Some(ConnectionState::Active) => {
                        shared.handle_active_frame(conn_id, &msg);
                    }
                    _ => break,
                }

                // Re-check shutdown after handling (handler may have set it).
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }
            }

            // Partial message — framing is buffering, continue reading.
            Ok(None) => continue,

            // Read timeout — no data within 100 ms.
            Err(ref e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                // Check control flags (same as top of loop, hit earlier on long waits).
                if terminated.load(Ordering::SeqCst) || shutdown.load(Ordering::SeqCst) {
                    if error_on_exit.load(Ordering::SeqCst) {
                        let err = ControlMsg::Error {
                            operation: OP_IDLE_TIMEOUT,
                            code: ERR_CODE_IDLE_TIMEOUT,
                            message: "incoming connection idle timeout".to_string(),
                        };
                        let _ = write_framed_message(&mut stream, &err.to_bytes());
                    }
                    break;
                }

                // Check per-connection idle timeout.
                let last_act = shared.connections.get(&conn_id).map(|e| e.last_activity);
                if let Some(last) = last_act {
                    if last.elapsed() > idle_timeout {
                        let err = ControlMsg::Error {
                            operation: OP_IDLE_TIMEOUT,
                            code: ERR_CODE_IDLE_TIMEOUT,
                            message: "incoming connection idle timeout".to_string(),
                        };
                        let _ = write_framed_message(&mut stream, &err.to_bytes());
                        break;
                    }
                }
            }

            // Real read error (EOF, RST, etc.) — close connection.
            Err(e) => {
                debug!("TcpMuxListener: Read error on conn {}: {:?}", conn_id, e);
                break;
            }
        }
    }

    // Cleanup — idempotent if already removed by prune.
    shared.remove_connection(conn_id);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

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

/// Write a framed control message to `stream`.  Best-effort — errors are
/// logged and ignored so that a write failure does not crash the caller.
fn send_control(stream: &mut Box<dyn TcpStreamWrapper>, msg: &ControlMsg) {
    if let Err(e) = write_framed_message(stream, &msg.to_bytes()) {
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
    use crate::rtps::transport::tcp::framing::write_framed_message;
    use crate::rtps::transport::tcp::protocol::{ControlMsg, MSG_ERROR, MSG_PEER_HELLO_ACK};
    use crate::rtps::transport::tcp::stream_wrapper::wrap_stream;
    use crossbeam_channel::bounded;
    use std::io::Read;
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_listener() -> (TcpMuxListener, crossbeam_channel::Receiver<IncomingMessage>) {
        let (disc_tx, _disc_rx) = bounded(64);
        let (user_tx, user_rx) = bounded(64);
        let listener =
            TcpMuxListener::new(0, 0, 0, [0u8; 12], disc_tx, user_tx).expect("listener creation");
        (listener, user_rx)
    }

    /// Accept one raw connection from the listener and register it in the shared
    /// map (no read thread).  Returns the assigned ConnectionId.
    fn accept_raw_no_thread(listener: &TcpMuxListener) -> ConnectionId {
        let (tcp, addr) = accept_with_retry(listener.listener.as_ref().expect("listener present"));
        let conn_id = listener.shared.next_conn_id.fetch_add(1, Ordering::SeqCst);
        let shutdown = Arc::new(AtomicBool::new(false));
        let error_on_exit = Arc::new(AtomicBool::new(false));
        listener.shared.connections.insert(
            conn_id,
            ConnectionEntry {
                remote_addr: addr,
                state: ConnectionState::AwaitingFirstMessage,
                bound_logical_port: None,
                remote_guid_prefix: None,
                last_activity: Instant::now(),
                shutdown,
                error_on_exit,
            },
        );
        conn_id
    }

    /// Accept one connection and spawn its read thread.  Returns the ConnectionId.
    fn accept_and_spawn(
        listener: &TcpMuxListener,
        terminated: Arc<AtomicBool>,
        idle_timeout: Duration,
    ) -> ConnectionId {
        let (tcp, addr) = accept_with_retry(listener.listener.as_ref().expect("listener present"));
        listener.accept_connection(wrap_stream(tcp), addr, terminated, idle_timeout)
    }

    /// Block-with-retry wrapper around a non-blocking listener's accept().
    /// Spins for up to ~2 s so test threads can synchronize with the
    /// client-side connect() without racing.
    fn accept_with_retry(listener: &std::net::TcpListener) -> (std::net::TcpStream, SocketAddr) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match listener.accept() {
                Ok(pair) => return pair,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        panic!("accept timed out (no incoming connection within 2s)");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("accept: {:?}", e),
            }
        }
    }

    /// Spin-wait (up to `deadline`) until `pred` returns true.
    fn wait_until(deadline: Instant, pred: impl Fn() -> bool) -> bool {
        while Instant::now() < deadline {
            if pred() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    // ── PeerConnectionGroup unit tests ────────────────────────────────────────

    #[test]
    fn test_peer_connection_group_all_tokens() {
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

    // ── Listener bind / accept ────────────────────────────────────────────────

    #[test]
    fn test_listener_binds_to_ephemeral_port() {
        let (listener, _) = make_listener();
        assert!(listener.port() != 0, "OS should assign a port");
        assert_eq!(listener.connection_count(), 0);
    }

    #[test]
    fn test_accept_registers_connection() {
        let (listener, _) = make_listener();
        let port = listener.port();

        let _client = std::thread::spawn(move || {
            TcpStream::connect(format!("127.0.0.1:{}", port)).expect("connect")
        });

        let conn_id = accept_raw_no_thread(&listener);

        assert_eq!(listener.connection_count(), 1);
        let entry = listener.shared.connections.get(&conn_id).unwrap();
        assert_eq!(entry.state, ConnectionState::AwaitingFirstMessage);
        assert!(entry.last_activity.elapsed() < Duration::from_secs(1));
    }

    // ── Idle pruning ──────────────────────────────────────────────────────────

    #[test]
    fn test_prune_idle_connections_with_zero_timeout() {
        let (listener, _) = make_listener();
        let port = listener.port();

        let _client = std::thread::spawn(move || {
            let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(stream);
        });

        accept_raw_no_thread(&listener);
        assert_eq!(listener.connection_count(), 1);

        let pruned = listener.prune_idle_connections(Duration::from_nanos(0));
        assert_eq!(pruned, 1);
        assert_eq!(listener.connection_count(), 0);
    }

    #[test]
    fn test_prune_keeps_fresh_connections() {
        let (listener, _) = make_listener();
        let port = listener.port();

        let _client = std::thread::spawn(move || {
            let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(stream);
        });

        accept_raw_no_thread(&listener);
        assert_eq!(listener.connection_count(), 1);

        let pruned = listener.prune_idle_connections(Duration::from_secs(60));
        assert_eq!(pruned, 0);
        assert_eq!(listener.connection_count(), 1);
    }

    #[test]
    fn test_prune_emits_idle_timeout_error_to_peer() {
        let (listener, _) = make_listener();
        let port = listener.port();
        let terminated = Arc::new(AtomicBool::new(false));

        // Client connects and waits to receive ERROR(IDLE_TIMEOUT).
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).expect("read length");
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut data = vec![0u8; len];
            stream.read_exact(&mut data).expect("read payload");
            data
        });

        // Accept and spawn read thread (needed to send ERROR asynchronously).
        let _conn_id = accept_and_spawn(&listener, terminated.clone(), Duration::from_secs(60));

        // Force prune with zero timeout → sets error_on_exit + shutdown flags.
        // The read thread will send ERROR within 100 ms (on its next wakeup).
        let pruned = listener.prune_idle_connections(Duration::from_nanos(0));
        assert_eq!(pruned, 1);
        assert_eq!(listener.connection_count(), 0);

        // Verify the client received ERROR(IDLE_TIMEOUT).
        let received = client.join().expect("client thread");
        assert_eq!(&received[..4], b"INT2", "frame magic mismatch");
        let payload = &received[4..];
        assert_eq!(payload[0], MSG_ERROR);
        assert_eq!(payload[1], OP_IDLE_TIMEOUT);
        let code = u16::from_be_bytes([payload[2], payload[3]]);
        assert_eq!(code, ERR_CODE_IDLE_TIMEOUT);

        terminated.store(true, Ordering::SeqCst);
    }

    // ── Handshake state machine ───────────────────────────────────────────────

    #[test]
    fn test_handshake_first_message_advances_state() {
        let (listener, _) = make_listener();
        let port = listener.port();
        let terminated = Arc::new(AtomicBool::new(false));

        // Signal channel: main thread tells client it may close.
        let (done_tx, done_rx) = bounded::<()>(1);

        // Client: connect, send PEER_HELLO, expect PEER_HELLO_ACK, then
        // keep the connection open until the main thread is done checking state.
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();

            let hello = ControlMsg::PeerHello { locator: [0u8; 16] };
            write_framed_message(&mut stream, &hello.to_bytes()).unwrap();

            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).unwrap();
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut data = vec![0u8; len];
            stream.read_exact(&mut data).unwrap();

            // Hold connection open until main thread finishes the state check.
            let _ = done_rx.recv();
            data
        });

        let conn_id = accept_and_spawn(&listener, terminated.clone(), Duration::from_secs(60));

        // Wait up to 3 s for the read thread to advance the state to Control.
        let deadline = Instant::now() + Duration::from_secs(3);
        let advanced = wait_until(deadline, || {
            listener
                .shared
                .connections
                .get(&conn_id)
                .map(|e| e.state == ConnectionState::Control)
                .unwrap_or(false)
        });

        // Allow client to close its end.
        let _ = done_tx.send(());
        assert!(advanced, "state did not advance to Control within deadline");

        let received = client.join().expect("client thread");
        assert_eq!(&received[..4], b"INT2");
        assert_eq!(received[4], MSG_PEER_HELLO_ACK);

        terminated.store(true, Ordering::SeqCst);
    }

    // ── Connection cleanup on RST / FIN ───────────────────────────────────────

    #[test]
    fn test_rst_close_cleans_up_connection_and_token() {
        use socket2::{Domain, SockAddr, Socket, Type};

        let (listener, _) = make_listener();
        let port = listener.port();
        let terminated = Arc::new(AtomicBool::new(false));

        let client = std::thread::spawn(move || {
            let sock = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
            sock.connect(&SockAddr::from(addr)).unwrap();
            sock.set_linger(Some(Duration::from_secs(0))).unwrap();
            drop(sock); // RST
        });

        let conn_id = accept_and_spawn(&listener, terminated.clone(), Duration::from_secs(60));
        client.join().unwrap();

        // Read thread should detect the error and remove the connection.
        let deadline = Instant::now() + Duration::from_secs(3);
        let gone = wait_until(deadline, || !listener.shared.connections.contains_key(&conn_id));
        assert!(gone, "connection not removed after RST");
        assert_eq!(listener.connection_count(), 0);

        terminated.store(true, Ordering::SeqCst);
    }

    #[test]
    fn test_fin_close_cleans_up_connection_and_token() {
        use std::net::Shutdown;

        let (listener, _) = make_listener();
        let port = listener.port();
        let terminated = Arc::new(AtomicBool::new(false));

        let client = std::thread::spawn(move || {
            let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            stream.shutdown(Shutdown::Both).unwrap();
        });

        let conn_id = accept_and_spawn(&listener, terminated.clone(), Duration::from_secs(60));
        client.join().unwrap();

        let deadline = Instant::now() + Duration::from_secs(3);
        let gone = wait_until(deadline, || !listener.shared.connections.contains_key(&conn_id));
        assert!(gone, "connection not removed after FIN");
        assert_eq!(listener.connection_count(), 0);

        terminated.store(true, Ordering::SeqCst);
    }

    #[test]
    fn test_rst_burst_does_not_leak_tokens() {
        use socket2::{Domain, SockAddr, Socket, Type};

        let (listener, _) = make_listener();
        let port = listener.port();
        let terminated = Arc::new(AtomicBool::new(false));

        const ROUNDS: usize = 8;

        let clients: Vec<_> = (0..ROUNDS)
            .map(|_| {
                std::thread::spawn(move || {
                    let sock = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
                    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
                    sock.connect(&SockAddr::from(addr)).unwrap();
                    sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                    drop(sock);
                })
            })
            .collect();
        for c in clients {
            c.join().unwrap();
        }

        // Accept all RST clients and spawn read threads.
        for _ in 0..ROUNDS {
            accept_and_spawn(&listener, terminated.clone(), Duration::from_secs(60));
        }

        // Wait for all read threads to detect errors and clean up.
        let deadline = Instant::now() + Duration::from_secs(5);
        let clean = wait_until(deadline, || listener.connection_count() == 0);
        assert!(clean, "RST burst leaked connection tokens");
        assert_eq!(listener.peer_count(), 0, "RST burst leaked peer groups");

        terminated.store(true, Ordering::SeqCst);
    }
}
