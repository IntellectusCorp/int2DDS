//! Inbound-frame dispatch — the connection state machine over `MuxState`.
//!
//! These `impl MuxState` methods consume frames read by
//! `conn_actor::reader_task` and advance each connection through its state:
//! handshake (PEER_HELLO / PORT_BIND), control traffic (PORT_RESERVE),
//! and RTPS data forwarding into the DDS layer. They live in a
//! child module so the state definition and storage stay in the parent while
//! the transition logic is isolated here; a child module can still reach the
//! parent type's private fields.

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;

use log::{debug, warn};
use tokio::sync::mpsc;

use crate::rtps::transport::error::TransportErrorCode;
use crate::rtps::transport::plugin::IncomingMessage;
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::framing::{classify_frame, TcpFrameKind};
use crate::rtps::transport::tcp::protocol::{
    decode_locator, generate_cookie, ControlMsg, ERR_CODE_INVALID_COOKIE, ERR_CODE_INVALID_PORT,
    ERR_CODE_MISSING_LOCATOR, MSG_PEER_HELLO, MSG_PORT_BIND, MSG_PORT_RESERVE,
};

use super::{
    addr_to_guid, send_control, ConnectionId, ConnectionState, MuxState, PeerConnectionGroup,
};

impl MuxState {
    // ── dispatch — entry point from conn_actor::reader_task ──────────────────

    /// Route one inbound frame based on the connection's current state.
    ///
    /// Called from `reader_task` for every frame read off the wire. Reads the
    /// connection's state, then delegates to the per-state handler. Handlers may
    /// push response frames into `writer_tx` (control acks, errors) or push
    /// RTPS data into the crossbeam channels (active state).
    #[allow(clippy::unused_async)]
    pub(crate) async fn dispatch(
        &self,
        conn_id: ConnectionId,
        payload: Vec<u8>,
        writer_tx: &mpsc::Sender<Vec<u8>>,
    ) {
        let state = match self.connections.get(&conn_id) {
            Some(entry) => entry.state,
            None => return, // entry already removed — race with close
        };

        match state {
            ConnectionState::AwaitingFirstMessage => {
                self.handle_first_message(conn_id, &payload, writer_tx);
            }
            ConnectionState::Control => {
                self.handle_control_frame(conn_id, &payload, writer_tx);
            }
            ConnectionState::Active => {
                self.handle_active_frame(conn_id, payload);
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
            ControlMsg::PeerHello { locator } => {
                // A peer must advertise its listener locator so we can identify it
                // (and group its inbound connection with its outbound ones). A
                // missing/zero locator is a contract violation — reject it.
                let (adv_ip, adv_port) = decode_locator(&locator);
                if adv_port == 0 || adv_ip.is_unspecified() {
                    warn!(
                        "TcpMuxListener [{}]: PEER_HELLO without a locator on conn {} — rejecting",
                        TransportErrorCode::TcpHandshakeHelloFailed,
                        conn_id
                    );
                    send_control(
                        writer_tx,
                        &ControlMsg::Error {
                            operation: MSG_PEER_HELLO,
                            code: ERR_CODE_MISSING_LOCATOR,
                            message: "PEER_HELLO missing advertised locator".to_string(),
                        },
                    );
                    if let Some(entry) = self.connections.get(&conn_id) {
                        entry.cancel.cancel();
                    }
                    return;
                }

                send_control(writer_tx, &ControlMsg::PeerHelloAck);

                if let Some(mut conn) = self.connections.get_mut(&conn_id) {
                    conn.state = ConnectionState::Control;
                }

                // Group this inbound connection under the peer's advertised
                // listener address, matching its outbound connections.
                let addr = SocketAddr::new(IpAddr::V4(adv_ip), adv_port);
                let synthetic_guid = addr_to_guid(addr);
                let mut pc = self.peer_connections.lock().expect("peer_connections lock");
                let group = pc.entry(synthetic_guid).or_insert_with(PeerConnectionGroup::new);
                group.control_conn = Some(conn_id);

                if let Some(mut conn) = self.connections.get_mut(&conn_id) {
                    conn.remote_guid_prefix = Some(synthetic_guid);
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

            ControlMsg::Error { operation, code, message } => {
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

    fn handle_active_frame(&self, conn_id: ConnectionId, payload: Vec<u8>) {
        if matches!(classify_frame(&payload), TcpFrameKind::RtpsData) {
            let remote_addr = match self.connections.get(&conn_id).map(|c| c.remote_addr) {
                Some(a) => a,
                None => return,
            };
            self.route_rtps_data(conn_id, payload, remote_addr);
        }
    }

    fn route_rtps_data(&self, conn_id: ConnectionId, payload: Vec<u8>, remote_addr: SocketAddr) {
        let logical_port = match self.connections.get(&conn_id).and_then(|c| c.bound_logical_port) {
            Some(p) => p,
            None => return,
        };

        let msg = IncomingMessage { data: payload, source: remote_addr };

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
