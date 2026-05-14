//! Shared state for the TCP mux listener.
//!
//! `MuxState` owns the per-connection bookkeeping and the protocol dispatch
//! logic. It is held in an `Arc` and shared across all conn_actor tasks; the
//! actors call `dispatch` to route inbound frames and use the `Sender` halves
//! of the discovery / user-data crossbeam channels to bridge into the sync
//! DDS layer.
//!
//! Compared to the sync version in `tcp/tcp_mux_listener.rs`, the per-entry
//! `Arc<AtomicBool>` shutdown flags are replaced with a per-connection
//! `CancellationToken` that the conn_actor pair shares; pruning a connection
//! is now a `cancel.cancel()` call rather than a polled flag.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dashmap::DashMap;
use log::{debug, warn};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::TransportErrorCode;
use crate::rtps::transport::plugin::IncomingMessage;
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp_async::framing::{classify_frame, TcpFrameKind};
use crate::rtps::transport::tcp_async::protocol::{
    generate_cookie, ControlMsg, ERR_CODE_IDLE_TIMEOUT, ERR_CODE_INVALID_COOKIE,
    ERR_CODE_INVALID_PORT, MSG_PORT_BIND, MSG_PORT_RESERVE, OP_IDLE_TIMEOUT,
};

// ── ID + state types ─────────────────────────────────────────────────────────

/// Unique identifier for an accepted connection (replaces the sync version's
/// mio::Token). Issued monotonically by `MuxState::next_conn_id`.
pub(crate) type ConnectionId = usize;

/// Connection state machine — drives which dispatch handler runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    /// Waiting for the first PEER_HELLO or PORT_BIND frame.
    AwaitingFirstMessage,
    /// PEER_HELLO done — control connection (PORT_RESERVE / KEEPALIVE).
    Control,
    /// PORT_BIND done — data connection (RTPS frames).
    Active,
    Closing,
}

/// Who initiated this connection. Used by `prune_idle_connections` to skip
/// outbound entries: their lifecycle is owned by `TcpSender` (eviction on
/// mpsc-Closed, keepalive-failure disconnect, orphan_prune) — and worse, an
/// outbound data conn receives almost no inbound traffic, so `last_activity`
/// never refreshes and the idle prune would fire on a perfectly healthy
/// send-only stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionDirection {
    /// Accepted by the listener — peer initiated.
    Inbound,
    /// Initiated by `TcpSender::do_connect_*` — we initiated.
    Outbound,
}

/// Per-peer grouping: tracks which connections (control / discovery /
/// user-data) belong to the same remote participant. Lets `remove_peer`
/// tear down all three together.
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

/// Per-connection bookkeeping shared across the actor pair and the prune
/// task. `writer_tx` is the conn_actor's inbox; pushing into it sends a
/// frame on this connection. `cancel` is the actor pair's child token —
/// cancelling it tears the pair down.
pub(crate) struct ConnectionEntry {
    pub(crate) remote_addr: SocketAddr,
    pub(crate) state: ConnectionState,
    pub(crate) direction: ConnectionDirection,
    pub(crate) bound_logical_port: Option<u16>,
    pub(crate) remote_guid_prefix: Option<GuidPrefix>,
    pub(crate) last_activity: Instant,
    /// Outbound inbox of the conn_actor for this connection.
    pub(crate) writer_tx: mpsc::Sender<Vec<u8>>,
    /// Cancellation handle for the conn_actor pair.
    pub(crate) cancel: CancellationToken,
    pub(crate) pending_ack: Option<Arc<Mutex<Option<oneshot::Sender<ControlMsg>>>>>,
    /// Consecutive keepalive intervals where the peer's KEEPALIVE_ACK did
    /// NOT arrive within `keepalive_timeout`. Maintained entirely by the
    /// sender's keepalive_interval_task (this module only observes); peer
    /// is declared dead once the count exceeds `max_missed_keepalives`.
    pub(crate) missed_keepalives: AtomicU32,

    /// Timestamp of the last KEEPALIVE the sender pushed on this connection.
    /// Set by `TcpSender::keepalive_interval_task` after each `try_send`.
    /// `None` before the first tick.
    pub(crate) last_keepalive_sent_at: Mutex<Option<Instant>>,

    /// Timestamp of the most recent KEEPALIVE_ACK observed by `dispatch`.
    /// Set by `handle_control_frame::KeepaliveAck`. `None` before the first
    /// ACK ever arrives. The sender compares this with `last_keepalive_sent_at`
    /// to decide whether the previous round-trip met the timeout.
    pub(crate) last_keepalive_ack_at: Mutex<Option<Instant>>,
}

// ── MuxState ─────────────────────────────────────────────────────────────────

/// Thread-safe shared state for the mux listener. Held in an `Arc` and
/// referenced by every conn_actor and timer task.
pub(crate) struct MuxState {
    pub(crate) domain_id: u32,
    pub(crate) participant_id: u32,
    #[allow(dead_code)]
    local_guid_prefix: GuidPrefix,

    pub(crate) connections: DashMap<ConnectionId, ConnectionEntry>,
    peer_connections: Mutex<HashMap<GuidPrefix, PeerConnectionGroup>>,

    /// Cookie issued at PORT_RESERVE → consumed at PORT_BIND.
    cookie_to_port: DashMap<[u8; 16], u16>,
    cookie_to_guid: DashMap<[u8; 16], GuidPrefix>,
    next_cookie: AtomicU8,

    pub(crate) next_conn_id: AtomicUsize,

    /// Crossbeam senders — the async→sync bridge for inbound RTPS data.
    /// The sync DDS layer owns the matching receivers.
    discovery_tx: Sender<IncomingMessage>,
    user_data_tx: Sender<IncomingMessage>,
}

impl MuxState {
    pub(crate) fn new(
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

    // ── basic counters ───────────────────────────────────────────────────────

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn peer_count(&self) -> usize {
        self.peer_connections.lock().expect("peer_connections lock").len()
    }

    /// Register a freshly-accepted inbound connection.
    ///
    /// Called by `mux_listener::accept_task` after spawning the conn_actor pair.
    /// Returns the assigned `ConnectionId` so the caller can keep a local handle
    /// for logging / metrics.
    pub(crate) fn register_inbound_connection(
        &self,
        remote_addr: SocketAddr,
        writer_tx: mpsc::Sender<Vec<u8>>,
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
                last_activity: Instant::now(),
                writer_tx,
                cancel,
                // Inbound connections never await outbound responses — slot stays None.
                pending_ack: None,
                missed_keepalives: AtomicU32::new(0),
                last_keepalive_sent_at: Mutex::new(None),
                last_keepalive_ack_at: Mutex::new(None),
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
        writer_tx: mpsc::Sender<Vec<u8>>,
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
                last_activity: Instant::now(),
                writer_tx,
                cancel,
                pending_ack: Some(pending_ack),
                missed_keepalives: AtomicU32::new(0),
                last_keepalive_sent_at: Mutex::new(None),
                last_keepalive_ack_at: Mutex::new(None),
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
    /// crossbeam channels. No `pending_ack` — data connections don't expect
    /// control responses.
    pub(crate) fn register_outbound_data_connection(
        &self,
        remote_addr: SocketAddr,
        logical_port: u16,
        writer_tx: mpsc::Sender<Vec<u8>>,
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
                last_activity: Instant::now(),
                writer_tx,
                cancel,
                pending_ack: None,
                missed_keepalives: AtomicU32::new(0),
                last_keepalive_sent_at: Mutex::new(None),
                last_keepalive_ack_at: Mutex::new(None),
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

    // ── idle pruning ─────────────────────────────────────────────────────────

    /// Cancel any connection whose last_activity is older than `timeout`.
    ///
    /// Each pruned connection gets its cancel token fired; the conn_actor
    /// reader task wakes up, the writer task drains any remaining frames,
    /// and the pair tears down on its own. No more polled "shutdown" /
    /// "error_on_exit" flags from the sync version.
    /// Returns the number of connections pruned.
    pub(crate) fn prune_idle_connections(&self, timeout: Duration) -> usize {
        let now = Instant::now();
        // Only INBOUND connections are subject to the listener's idle prune.
        // Outbound entries have no inbound traffic on data conns (peer doesn't
        // reply on a send-only stream) so `last_activity` never refreshes, and
        // their lifecycle is already managed by `TcpSender` (mpsc-Closed
        // eviction, keepalive disconnect, orphan_prune). Pruning them here
        // would tear down healthy send-only streams and emit Error frames the
        // peer's `handle_active_frame` discards anyway.
        let stale: Vec<ConnectionId> = self
            .connections
            .iter()
            .filter(|e| e.direction == ConnectionDirection::Inbound)
            .filter(|e| now.duration_since(e.last_activity) > timeout)
            .map(|e| *e.key())
            .collect();

        let mut pruned = 0;
        for conn_id in &stale {
            // Re-check + transition to Closing under the entry's write lock.
            // The held guard serialises with `dispatch`'s `get_mut`, so any
            // frame that races us either:
            //   - lands BEFORE this guard: refreshes `last_activity` → we skip
            //   - lands AFTER this guard:  sees `state == Closing` → `dispatch`
            //                              drops it (no response generated)
            // This closes the window where a PORT_RESERVE was handled (and
            // PORT_RESERVE_ACK emitted) between the stale snapshot and the
            // eviction.
            let (writer_tx, remote_addr, idle_for) =
                match self.connections.get_mut(conn_id) {
                    Some(mut entry) => {
                        let idle_for = now.duration_since(entry.last_activity);
                        if idle_for <= timeout {
                            continue; // refreshed since snapshot — not actually idle
                        }
                        entry.state = ConnectionState::Closing;
                        (entry.writer_tx.clone(), entry.remote_addr, idle_for)
                    }
                    None => continue,
                };

            warn!(
                "TcpMuxListener [{}]: Pruning idle conn {} from {:?} (idle {:?})",
                TransportErrorCode::TcpConnectionIdlePruned,
                conn_id,
                remote_addr,
                idle_for,
            );

            let err = ControlMsg::Error {
                operation: OP_IDLE_TIMEOUT,
                code: ERR_CODE_IDLE_TIMEOUT,
                message: "incoming connection idle timeout".to_string(),
            };
            let _ = writer_tx.try_send(err.to_bytes());

            self.remove_connection(*conn_id);
            pruned += 1;
        }

        pruned
    }

    // ── connection / peer cleanup ────────────────────────────────────────────

    /// Drop every connection associated with `guid` (control + discovery + user).
    pub(crate) fn remove_peer(&self, guid: GuidPrefix) {
        let group = self.peer_connections.lock().expect("peer_connections lock").remove(&guid);
        if let Some(group) = group {
            for conn_id in group.all_conns() {
                self.remove_connection_inner(conn_id);
            }
            debug!("TcpMuxListener: Removed peer {:?}", guid);
        }
    }

    /// Update peer_connections bookkeeping then remove the connection entry.
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
            // Wake the conn_actor pair so they can tear down even if the
            // caller did not cancel them explicitly.
            entry.cancel.cancel();
            debug!("TcpMuxListener: Removed conn {} (addr={:?})", conn_id, entry.remote_addr);
        }
    }

    // ── dispatch — entry point from conn_actor::reader_task ──────────────────

    /// Route one inbound frame based on the connection's current state.
    ///
    /// Called from `reader_task` for every frame read off the wire. Updates
    /// last_activity, then delegates to the per-state handler. Handlers may
    /// push response frames into `writer_tx` (control acks, errors) or push
    /// RTPS data into the crossbeam channels (active state).
    #[allow(clippy::unused_async)]
    pub(crate) async fn dispatch(
        &self,
        conn_id: ConnectionId,
        payload: Vec<u8>,
        writer_tx: &mpsc::Sender<Vec<u8>>,
    ) {
        let state = match self.connections.get_mut(&conn_id) {
            Some(mut entry) => {
                entry.last_activity = Instant::now();
                entry.state
            }
            None => return, // entry already removed — race with prune/close
        };

        match state {
            ConnectionState::AwaitingFirstMessage => {
                self.handle_first_message(conn_id, &payload, writer_tx);
            }
            ConnectionState::Control => {
                self.handle_control_frame(conn_id, &payload, writer_tx);
            }
            ConnectionState::Active => {
                self.handle_active_frame(conn_id, &payload);
            }
            ConnectionState::Closing => {
                // Drop frames on closing connections — actor will exit shortly.
            }
        }
    }

    fn handle_first_message(
        &self,
        conn_id: ConnectionId,
        payload: &[u8],
        writer_tx: &mpsc::Sender<Vec<u8>>,
    ) {
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
                    entry.cancel.cancel();
                }
                return;
            }
        };

        match msg {
            ControlMsg::PeerHello { locator: _ } => {
                send_control(writer_tx, &ControlMsg::PeerHelloAck);

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
            }

            ControlMsg::PortBind { cookie } => {
                self.handle_port_bind(conn_id, &cookie, writer_tx);
            }

            other => {
                warn!(
                    "TcpMuxListener: Expected PEER_HELLO / PORT_BIND, got {} on conn {}",
                    other.type_name(),
                    conn_id
                );
                if let Some(entry) = self.connections.get(&conn_id) {
                    entry.cancel.cancel();
                }
            }
        }
    }

    fn handle_control_frame(
        &self,
        conn_id: ConnectionId,
        payload: &[u8],
        writer_tx: &mpsc::Sender<Vec<u8>>,
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
                        writer_tx,
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

                send_control(writer_tx, &ControlMsg::PortReserveAck { cookie });

                debug!(
                    "TcpMuxListener: PORT_RESERVE ok (port={}, cookie=0x{:02x})",
                    logical_port, cookie[0]
                );
            }

            ControlMsg::Keepalive => {
                send_control(writer_tx, &ControlMsg::KeepaliveAck);
            }

            // Outbound responses on this (control) connection. `dispatch` has
            // already refreshed `last_activity`; here we wake the connect_task
            // awaiting the answer via the single-slot oneshot mailbox.
            //
            // KEEPALIVE_ACK is handled separately so it does NOT consume the
            // pending_ack slot — otherwise a periodic keepalive ack could steal
            // the cookie destined for a pending PORT_RESERVE.
            ControlMsg::KeepaliveAck => {
                // Just record the arrival time. The sender's
                // keepalive_interval_task compares this with
                // `last_keepalive_sent_at` on each tick to decide whether
                // the round-trip met `keepalive_timeout`; the counter
                // reset / increment decision lives there, not here.
                if let Some(entry) = self.connections.get(&conn_id) {
                    *entry
                        .last_keepalive_ack_at
                        .lock()
                        .expect("last_keepalive_ack_at lock") = Some(Instant::now());
                }
            }

            ControlMsg::Error { operation, code, message } => {
                // Peer (server) signalled this control connection is being
                // torn down for idle timeout. Invalidate immediately so the
                // next send_to spawns a fresh PEER_HELLO instead of pushing
                // PORT_RESERVE down a dying socket and waiting out the
                // handshake timeout.
                if operation == OP_IDLE_TIMEOUT && code == ERR_CODE_IDLE_TIMEOUT {
                    debug!(
                        "TcpMuxListener: peer signalled idle timeout on conn {} — tearing down",
                        conn_id
                    );
                    // Surface to any in-flight PORT_RESERVE waiter (fail fast).
                    // No stray-warn — this Error is unsolicited by design.
                    let _ = self.route_response_to_waiter(
                        conn_id,
                        ControlMsg::Error { operation, code, message },
                    );
                    // Cancel the actor pair. The sender's outbound cache is
                    // evicted on the next send_to (try_send → Closed branch)
                    // or by orphan_prune.
                    if let Some(entry) = self.connections.get(&conn_id) {
                        entry.cancel.cancel();
                    }
                    return;
                }

                let response = ControlMsg::Error { operation, code, message };
                let kind = response.type_name();
                if !self.route_response_to_waiter(conn_id, response) {
                    warn!(
                        "TcpMuxListener: Stray {} on conn {} (no waiter registered)",
                        kind, conn_id
                    );
                }
            }

            response @ (ControlMsg::PortReserveAck { .. } | ControlMsg::PortBindAck) => {
                let kind = response.type_name();
                if !self.route_response_to_waiter(conn_id, response) {
                    warn!(
                        "TcpMuxListener: Stray {} on conn {} (no waiter registered)",
                        kind, conn_id
                    );
                }
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

    /// Hand `response` to the single-slot `pending_ack` mailbox installed by
    /// `TcpSender` on outbound control connections. Returns `true` if a waiter
    /// was registered and consumed the slot; the caller decides whether to
    /// emit a stray-warn on `false`.
    fn route_response_to_waiter(&self, conn_id: ConnectionId, response: ControlMsg) -> bool {
        let slot = self.connections.get(&conn_id).and_then(|e| e.pending_ack.clone());
        if let Some(slot) = slot {
            if let Some(tx) = slot.lock().expect("pending_ack lock").take() {
                // `send` returns Err if the receiver was dropped (e.g.
                // connect_task timed out and gave up). Either way we've
                // consumed the slot, which is the correct semantic.
                let _ = tx.send(response);
                return true;
            }
        }
        false
    }

    fn handle_active_frame(&self, conn_id: ConnectionId, payload: &[u8]) {
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

    fn handle_port_bind(
        &self,
        conn_id: ConnectionId,
        cookie: &[u8; 16],
        writer_tx: &mpsc::Sender<Vec<u8>>,
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
                    writer_tx,
                    &ControlMsg::Error {
                        operation: MSG_PORT_BIND,
                        code: ERR_CODE_INVALID_COOKIE,
                        message: format!("invalid cookie [{}]", cookie_hex),
                    },
                );
                if let Some(entry) = self.connections.get(&conn_id) {
                    entry.cancel.cancel();
                }
                return;
            }
        };

        send_control(writer_tx, &ControlMsg::PortBindAck);

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

    /// `PeerConnectionGroup` correctly tracks which connection roles are
    /// occupied and reports `has_data_conns` based on the discovery / user
    /// slots being populated.
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
}
