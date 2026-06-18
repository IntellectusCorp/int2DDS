//! Per-connection state shared across the tasks that drive a connection.
//!
//! Each TCP connection is driven by several tasks at once — the reader/writer
//! task pair plus the keepalive and prune tasks — and they all need to see
//! the same connection state.
//! `MuxState` is that single source of truth: held in an `Arc`,
//! it keeps one `ConnectionEntry` per connection (state, remote
//! addr, writer inbox, cancel token, keepalive timing) and routes inbound
//! frames via `dispatch` — RTPS data to the DDS layer through the crossbeam
//! senders, control frames to their handlers. A connection is torn down by
//! firing its `CancellationToken`.

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
use crate::rtps::transport::tcp::protocol::{ControlMsg, ERR_CODE_IDLE_TIMEOUT, OP_IDLE_TIMEOUT};

mod handlers;

// ── ID + state types ─────────────────────────────────────────────────────────

/// Unique connection id, issued monotonically by `MuxState::next_conn_id`.
pub(crate) type ConnectionId = usize;

/// Connection state machine — drives which dispatch handler runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    /// Awaiting the first PEER_HELLO(Control conn) or PORT_BIND frame(Data conn).
    AwaitingFirstMessage,
    /// Control connection: PORT_RESERVE / KEEPALIVE.
    Control,
    /// Data connection: RTPS frames.
    Active,
    Closing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionDirection {
    /// Accepted by the listener — peer initiated.
    Inbound,
    /// Initiated by `TcpSender::do_connect_*`.
    Outbound,
}

/// Groups the control / discovery / user-data connections of one remote
/// participant so `remove_peer` can tear down all three together.
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

/// Per-connection bookkeeping shared across the actor pair and the prune task.
pub(crate) struct ConnectionEntry {
    pub(crate) remote_addr: SocketAddr,
    pub(crate) state: ConnectionState,
    pub(crate) direction: ConnectionDirection,
    pub(crate) bound_logical_port: Option<u16>,
    pub(crate) remote_guid_prefix: Option<GuidPrefix>,
    pub(crate) last_activity: Instant,
    /// conn_actor inbox: pushing a frame here sends it on this connection.
    pub(crate) writer_tx: mpsc::Sender<Vec<u8>>,
    /// Child token for the actor pair; cancelling it tears the pair down.
    pub(crate) cancel: CancellationToken,
    pub(crate) pending_ack: Option<Arc<Mutex<Option<oneshot::Sender<ControlMsg>>>>>,
    /// Consecutive intervals where KEEPALIVE_ACK missed `keepalive_timeout`.
    /// Maintained by the sender's keepalive_interval_task (this module only
    /// observes); peer is declared dead past `max_missed_keepalives`.
    pub(crate) missed_keepalives: AtomicU32,

    /// When the sender last pushed a KEEPALIVE. Set by
    /// `TcpSender::keepalive_interval_task`; `None` before the first tick.
    pub(crate) last_keepalive_sent_at: Mutex<Option<Instant>>,

    /// When `dispatch` last saw a KEEPALIVE_ACK; `None` until the first ACK.
    /// The sender compares it with `last_keepalive_sent_at` to judge whether
    /// the previous round-trip met the timeout.
    pub(crate) last_keepalive_ack_at: Mutex<Option<Instant>>,
}

// ── MuxState ─────────────────────────────────────────────────────────────────

/// Socket-level tuning applied to both accepted (inbound) and dialed (outbound)
/// TCP streams. Resolved per participant from `TcpConfig`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TcpSocketTuning {
    pub(crate) nodelay: bool,
    pub(crate) so_rcvbuf: Option<usize>,
    pub(crate) so_sndbuf: Option<usize>,
}

impl Default for TcpSocketTuning {
    fn default() -> Self {
        Self { nodelay: true, so_rcvbuf: None, so_sndbuf: None }
    }
}

/// Thread-safe shared state for the mux listener.
pub(crate) struct MuxState {
    pub(crate) domain_id: u32,
    pub(crate) participant_id: u32,
    #[allow(dead_code)]
    local_guid_prefix: GuidPrefix,

    /// Socket tuning shared with the accept loop and the outbound sender.
    pub(crate) tuning: TcpSocketTuning,

    pub(crate) connections: DashMap<ConnectionId, ConnectionEntry>,
    peer_connections: Mutex<HashMap<GuidPrefix, PeerConnectionGroup>>,

    /// Cookie issued at PORT_RESERVE → consumed at PORT_BIND.
    cookie_to_port: DashMap<[u8; 16], u16>,
    cookie_to_guid: DashMap<[u8; 16], GuidPrefix>,
    next_cookie: AtomicU8,

    pub(crate) next_conn_id: AtomicUsize,

    /// Async→sync bridge for inbound RTPS data; the DDS layer owns the receivers.
    discovery_tx: Sender<IncomingMessage>,
    user_data_tx: Sender<IncomingMessage>,
}

impl MuxState {
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
            let (writer_tx, remote_addr, idle_for) = match self.connections.get_mut(conn_id) {
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
}
