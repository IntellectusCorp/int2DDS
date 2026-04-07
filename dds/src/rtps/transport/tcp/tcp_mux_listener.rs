#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use log::{debug, info, warn};
use mio::net::{TcpListener as MioTcpListener, TcpStream as MioTcpStream};
use mio::{Interest, Registry, Token};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::plugin::IncomingMessage;
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::framing::{
    classify_frame, write_framed_message, FramedReader, TcpFrameKind,
};
use crate::rtps::transport::tcp::protocol::{
    generate_cookie, ControlMsg, ERR_CODE_IDLE_TIMEOUT, ERR_CODE_INVALID_COOKIE,
    ERR_CODE_INVALID_PORT, MSG_PORT_BIND, MSG_PORT_RESERVE, OP_IDLE_TIMEOUT,
};

const LISTENER_TOKEN: Token = Token(0);
const CONNECTION_TOKEN_START: usize = 65536;

/// TCP multiplexed listener with 3-step handshake:
/// PEER_HELLO → PORT_RESERVE (repeatable) → PORT_BIND (separate connection)
#[derive(Debug)]
pub(crate) struct TcpMuxListener {
    port: u16,
    domain_id: u32,
    participant_id: u32,
    local_guid_prefix: GuidPrefix,
    listener: Option<MioTcpListener>,
    connections: HashMap<Token, MuxConnection>,
    next_token: usize,
    peer_connections: HashMap<GuidPrefix, PeerConnectionGroup>,
    discovery_tx: Sender<IncomingMessage>,
    user_data_tx: Sender<IncomingMessage>,
    /// Cookie counter for PORT_RESERVE responses (0x31, 0x32, ...)
    next_cookie: u8,
    /// Maps cookie → logical_port for PORT_BIND verification
    cookie_to_port: HashMap<[u8; 16], u16>,
    /// Maps cookie → control connection's GuidPrefix so the matching
    /// PORT_BIND (which arrives on a brand-new TCP connection with a
    /// different source port) can be attached to the same peer group
    /// as the control connection that issued the cookie.
    cookie_to_guid: HashMap<[u8; 16], GuidPrefix>,
}

#[derive(Debug)]
struct PeerConnectionGroup {
    control_token: Option<Token>,
    discovery_token: Option<Token>,
    user_data_token: Option<Token>,
    /// Set to `Some(when)` the moment `control_token` transitions to None
    /// while data connections still exist. The data connections are kept
    /// alive during a short grace period so a brief network blip doesn't
    /// instantly tear down the data flow. After the grace period expires,
    /// `prune_orphan_data_connections` removes the entire group.
    ///
    /// A reconnecting peer is intentionally placed into a brand-new group;
    /// re-matching is left to the upper RTPS/SEDP layer.
    control_lost_at: Option<Instant>,
}

impl PeerConnectionGroup {
    fn new() -> Self {
        Self {
            control_token: None,
            discovery_token: None,
            user_data_token: None,
            control_lost_at: None,
        }
    }

    fn all_tokens(&self) -> Vec<Token> {
        [self.control_token, self.discovery_token, self.user_data_token]
            .iter()
            .flatten()
            .copied()
            .collect()
    }

    fn has_data_tokens(&self) -> bool {
        self.discovery_token.is_some() || self.user_data_token.is_some()
    }
}

/// Connection state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    /// Waiting for first message (PEER_HELLO or PORT_BIND)
    AwaitingFirstMessage,
    /// PEER_HELLO done — control connection, accepts PORT_RESERVE + KEEPALIVE
    Control,
    /// PORT_BIND done — data connection, accepts RTPS data
    Active,
    Closing,
}

#[derive(Debug)]
struct MuxConnection {
    stream: MioTcpStream,
    framed_reader: FramedReader,
    state: ConnectionState,
    remote_addr: SocketAddr,
    bound_logical_port: Option<u16>,
    remote_guid_prefix: Option<GuidPrefix>,
    /// Last time we successfully read any data from this peer.
    /// Used by `prune_idle_connections` to defend against silent peers
    /// (PEER_HELLO without follow-up, half-broken networks, etc.).
    last_activity: Instant,
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
        let listener = MioTcpListener::bind(addr)?;
        let actual_port = listener.local_addr()?.port();
        info!("TcpMuxListener: Listening on port {} (domain={})", actual_port, domain_id);

        Ok(Self {
            port: actual_port,
            domain_id,
            participant_id,
            local_guid_prefix,
            listener: Some(listener),
            connections: HashMap::new(),
            next_token: CONNECTION_TOKEN_START,
            peer_connections: HashMap::new(),
            discovery_tx,
            user_data_tx,
            next_cookie: 0x31,
            cookie_to_port: HashMap::new(),
            cookie_to_guid: HashMap::new(),
        })
    }

    pub(crate) fn listener_mut(&mut self) -> Option<&mut MioTcpListener> {
        self.listener.as_mut()
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn accept(&mut self, registry: &Registry) -> io::Result<Option<Token>> {
        let listener = match &self.listener {
            Some(l) => l,
            None => return Err(io::Error::new(ErrorKind::NotConnected, "Not initialized")),
        };

        match listener.accept() {
            Ok((mut stream, addr)) => {
                let token = Token(self.next_token);
                self.next_token += 1;
                let _ = stream.set_nodelay(true);
                registry.register(&mut stream, token, Interest::READABLE)?;

                debug!("TcpMuxListener: Accepted from {:?} (token={:?})", addr, token);

                self.connections.insert(
                    token,
                    MuxConnection {
                        stream,
                        framed_reader: FramedReader::new(),
                        state: ConnectionState::AwaitingFirstMessage,
                        remote_addr: addr,
                        bound_logical_port: None,
                        remote_guid_prefix: None,
                        last_activity: Instant::now(),
                    },
                );
                Ok(Some(token))
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn on_readable(&mut self, token: Token, registry: &Registry) {
        loop {
            let payload = {
                let conn = match self.connections.get_mut(&token) {
                    Some(c) if c.state != ConnectionState::Closing => c,
                    _ => return,
                };

                match conn.framed_reader.read_message(&mut conn.stream) {
                    Ok(Some(msg)) => {
                        conn.last_activity = Instant::now();
                        msg
                    }
                    Ok(None) => continue,
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => return,
                    Err(e) => {
                        debug!("TcpMuxListener: Read error on {:?}: {:?}", token, e);
                        self.remove_connection(token, registry);
                        return;
                    }
                }
            };

            let state = self.connections.get(&token).map(|c| c.state);

            match state {
                Some(ConnectionState::AwaitingFirstMessage) => {
                    self.handle_first_message(token, &payload, registry);
                }
                Some(ConnectionState::Control) => {
                    self.handle_control_frame(token, &payload, registry);
                }
                Some(ConnectionState::Active) => {
                    self.handle_active_frame(token, &payload, registry);
                }
                _ => {}
            }
        }
    }

    // ── First message: PEER_HELLO or PORT_BIND ──────────────────────────────

    fn handle_first_message(&mut self, token: Token, payload: &[u8], registry: &Registry) {
        let msg = match ControlMsg::from_bytes(payload) {
            Ok(m) => m,
            Err(e) => {
                warn!("TcpMuxListener: Bad first message on {:?}: {:?}", token, e);
                self.remove_connection(token, registry);
                return;
            }
        };

        match msg {
            ControlMsg::PeerHello { locator } => {
                // Send PEER_HELLO_ACK
                let ack = ControlMsg::PeerHelloAck;
                self.send_control(token, &ack);

                if let Some(conn) = self.connections.get_mut(&token) {
                    conn.state = ConnectionState::Control;
                }

                // Register in peer connection group (synthetic guid from addr)
                let remote_addr = self.connections.get(&token).map(|c| c.remote_addr);
                if let Some(addr) = remote_addr {
                    let mut synthetic_guid = [0u8; 12];
                    if let SocketAddr::V4(v4) = addr {
                        synthetic_guid[0..4].copy_from_slice(&v4.ip().octets());
                        synthetic_guid[4..6].copy_from_slice(&v4.port().to_be_bytes());
                    }

                    let group = self
                        .peer_connections
                        .entry(synthetic_guid)
                        .or_insert_with(PeerConnectionGroup::new);

                    group.control_token = Some(token);

                    if let Some(conn) = self.connections.get_mut(&token) {
                        conn.remote_guid_prefix = Some(synthetic_guid);
                    }
                }

                debug!("TcpMuxListener: PEER_HELLO ok (token={:?})", token);
            }

            ControlMsg::PortBind { cookie } => {
                self.handle_port_bind(token, &cookie, registry);
            }

            other => {
                warn!(
                    "TcpMuxListener: Expected PEER_HELLO or PORT_BIND, got {} on {:?}",
                    other.type_name(),
                    token
                );
                self.remove_connection(token, registry);
            }
        }
    }

    // ── Control connection: PORT_RESERVE + KEEPALIVE ────────────────────────

    fn handle_control_frame(&mut self, token: Token, payload: &[u8], registry: &Registry) {
        let msg = match ControlMsg::from_bytes(payload) {
            Ok(m) => m,
            Err(e) => {
                warn!("TcpMuxListener: Bad control msg on {:?}: {:?}", token, e);
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
                    warn!("TcpMuxListener: Invalid port {} on {:?}", logical_port, token);
                    let err = ControlMsg::Error {
                        operation: MSG_PORT_RESERVE,
                        code: ERR_CODE_INVALID_PORT,
                        message: "no matching port".to_string(),
                    };
                    self.send_control(token, &err);
                    return;
                }

                // Issue cookie and store mappings.
                // The cookie → guid mapping lets the matching PORT_BIND
                // (which arrives on a fresh TCP connection with a different
                // source port) attach itself to the same peer group as the
                // control connection that issued the cookie.
                let cookie = generate_cookie(&mut self.next_cookie);
                self.cookie_to_port.insert(cookie, logical_port);
                if let Some(ctrl_guid) =
                    self.connections.get(&token).and_then(|c| c.remote_guid_prefix)
                {
                    self.cookie_to_guid.insert(cookie, ctrl_guid);
                }

                let ack = ControlMsg::PortReserveAck { cookie };
                self.send_control(token, &ack);

                debug!(
                    "TcpMuxListener: PORT_RESERVE ok (port={}, cookie=0x{:02x})",
                    logical_port, cookie[0]
                );
            }

            ControlMsg::Keepalive => {
                self.send_control(token, &ControlMsg::KeepaliveAck);
            }

            other => {
                debug!("TcpMuxListener: Ignoring {} on control {:?}", other.type_name(), token);
            }
        }
    }

    // ── PORT_BIND handler (from first message or control) ───────────────────

    fn handle_port_bind(&mut self, token: Token, cookie: &[u8; 16], registry: &Registry) {
        // Look up logical port from cookie
        let logical_port = match self.cookie_to_port.remove(cookie) {
            Some(port) => port,
            None => {
                let cookie_hex: String = cookie.iter().map(|b| format!("{:02x}", b)).collect();
                warn!("TcpMuxListener: Unknown cookie [{}] on {:?}", cookie_hex, token);
                let err = ControlMsg::Error {
                    operation: MSG_PORT_BIND,
                    code: ERR_CODE_INVALID_COOKIE,
                    message: format!("invalid cookie [{}]", cookie_hex),
                };
                self.send_control(token, &err);
                self.remove_connection(token, registry);
                return;
            }
        };

        // Send PORT_BIND_ACK
        self.send_control(token, &ControlMsg::PortBindAck);

        if let Some(conn) = self.connections.get_mut(&token) {
            conn.bound_logical_port = Some(logical_port);
            conn.state = ConnectionState::Active;
        }

        // Resolve the peer group: prefer the GuidPrefix that the control
        // connection associated with this cookie at PORT_RESERVE time. The
        // PORT_BIND arrives on a brand-new TCP connection with a different
        // source port, so falling back on the bound socket address would
        // place the data connection in a different group than the control.
        let group_guid = self.cookie_to_guid.remove(cookie).or_else(|| {
            let remote_addr = self.connections.get(&token).map(|c| c.remote_addr);
            remote_addr.map(|addr| {
                let mut synthetic_guid = [0u8; 12];
                if let SocketAddr::V4(v4) = addr {
                    synthetic_guid[0..4].copy_from_slice(&v4.ip().octets());
                    synthetic_guid[4..6].copy_from_slice(&v4.port().to_be_bytes());
                }
                synthetic_guid
            })
        });

        if let Some(guid) = group_guid {
            let group = self
                .peer_connections
                .entry(guid)
                .or_insert_with(PeerConnectionGroup::new);

            if PortManager::is_discovery_unicast_port(self.domain_id, logical_port) {
                group.discovery_token = Some(token);
            } else {
                group.user_data_token = Some(token);
            }

            if let Some(conn) = self.connections.get_mut(&token) {
                conn.remote_guid_prefix = Some(guid);
            }
        }

        debug!(
            "TcpMuxListener: PORT_BIND ok (token={:?}, port={}, cookie=0x{:02x})",
            token, logical_port, cookie[0]
        );
    }

    // ── Active data connection: RTPS data ───────────────────────

    fn handle_active_frame(&mut self, token: Token, payload: &[u8], registry: &Registry) {
        let kind = classify_frame(payload);

        match kind {
            TcpFrameKind::RtpsData => {
                let remote_addr = self.connections.get(&token).map(|c| c.remote_addr).unwrap();
                self.route_rtps_data(token, payload, remote_addr);
            }
            _ => {}
        }
    }

    fn route_rtps_data(&self, token: Token, payload: &[u8], remote_addr: SocketAddr) {
        let logical_port = match self.connections.get(&token).and_then(|c| c.bound_logical_port) {
            Some(p) => p,
            None => return,
        };

        let msg = IncomingMessage { data: payload.to_vec(), source: remote_addr };

        if PortManager::is_discovery_unicast_port(self.domain_id, logical_port) {
            if let Err(e) = self.discovery_tx.try_send(msg) {
                warn!("TcpMuxListener: Failed to route discovery: {:?}", e);
            }
        } else if PortManager::is_user_unicast_port(self.domain_id, logical_port) {
            if let Err(e) = self.user_data_tx.try_send(msg) {
                warn!("TcpMuxListener: Failed to route user data: {:?}", e);
            }
        }
    }

    // ── Control sending ─────────────────────────────────────────────────────

    fn send_control(&mut self, token: Token, msg: &ControlMsg) {
        let conn = match self.connections.get_mut(&token) {
            Some(c) => c,
            None => return,
        };

        if let Err(e) = write_framed_message(&mut conn.stream, &msg.to_bytes()) {
            warn!("TcpMuxListener: Failed to send {} to {:?}: {:?}", msg.type_name(), token, e);
        }
    }

    // ── Idle timeout pruning ────────────────────────────────────────────────

    /// Drop incoming connections whose `last_activity` has exceeded `timeout`.
    ///
    /// Before tearing them down, sends an ERROR(IDLE_TIMEOUT) on a best-effort
    /// basis so cooperative clients can log the reason. Returns the number of
    /// connections that were pruned.
    ///
    /// This protects the server from peers that perform a partial handshake
    /// (e.g. PEER_HELLO followed by silence) and never send another byte: the
    /// server has no incoming-direction keepalive of its own, so without this
    /// pruning a single broken or malicious peer could pin a token forever.
    pub(crate) fn prune_idle_connections(
        &mut self,
        timeout: Duration,
        registry: &Registry,
    ) -> usize {
        let now = Instant::now();
        let stale: Vec<Token> = self
            .connections
            .iter()
            .filter(|(_, c)| now.duration_since(c.last_activity) > timeout)
            .map(|(t, _)| *t)
            .collect();

        for token in &stale {
            if let Some(conn) = self.connections.get(token) {
                warn!(
                    "TcpMuxListener: Pruning idle incoming connection {:?} from {:?} (idle for {:?})",
                    token,
                    conn.remote_addr,
                    now.duration_since(conn.last_activity)
                );
            }
            // Best-effort ERROR notice — ignore failures.
            let err = ControlMsg::Error {
                operation: OP_IDLE_TIMEOUT,
                code: ERR_CODE_IDLE_TIMEOUT,
                message: "incoming connection idle timeout".to_string(),
            };
            self.send_control(*token, &err);
            self.remove_connection(*token, registry);
        }

        stale.len()
    }

    /// Drop data connections in groups whose control connection has been
    /// gone for longer than `grace`. Returns the number of groups that were
    /// fully cleaned up.
    ///
    /// A short grace period absorbs transient control disconnects so that a
    /// brief network blip does not instantly tear down the data flow. After
    /// the grace expires the orphan group is removed entirely; reconnecting
    /// peers land in a brand-new group and re-matching is left to the upper
    /// RTPS/SEDP layer.
    pub(crate) fn prune_orphan_data_connections(
        &mut self,
        grace: Duration,
        registry: &Registry,
    ) -> usize {
        let now = Instant::now();
        let stale_guids: Vec<GuidPrefix> = self
            .peer_connections
            .iter()
            .filter_map(|(guid, group)| match group.control_lost_at {
                Some(lost) if now.duration_since(lost) > grace && group.has_data_tokens() => {
                    Some(*guid)
                }
                _ => None,
            })
            .collect();

        for guid in &stale_guids {
            warn!(
                "TcpMuxListener: Pruning orphan data connections for {:?} after grace period",
                guid
            );
            self.remove_peer(*guid, registry);
        }

        stale_guids.len()
    }

    // ── Connection cleanup ──────────────────────────────────────────────────

    pub(crate) fn remove_peer(&mut self, guid: GuidPrefix, registry: &Registry) {
        if let Some(group) = self.peer_connections.remove(&guid) {
            for token in group.all_tokens() {
                self.remove_connection_inner(token, registry);
            }
            debug!("TcpMuxListener: Removed peer {:?}", guid);
        }
    }

    pub(crate) fn remove_connection(&mut self, token: Token, registry: &Registry) {
        if let Some(conn) = self.connections.get(&token) {
            if let Some(guid) = conn.remote_guid_prefix {
                if let Some(group) = self.peer_connections.get_mut(&guid) {
                    let was_control = group.control_token == Some(token);
                    if was_control {
                        group.control_token = None;
                    }
                    if group.discovery_token == Some(token) {
                        group.discovery_token = None;
                    }
                    if group.user_data_token == Some(token) {
                        group.user_data_token = None;
                    }

                    // If the control connection just disappeared but data
                    // connections are still around, start the grace period.
                    // Subsequent prune sweeps will tear them down once it
                    // expires.
                    if was_control && group.has_data_tokens() && group.control_lost_at.is_none() {
                        group.control_lost_at = Some(Instant::now());
                        debug!(
                            "TcpMuxListener: control connection lost for {:?}, grace period started",
                            guid
                        );
                    }

                    if group.all_tokens().is_empty() {
                        self.peer_connections.remove(&guid);
                    }
                }
            }
        }
        self.remove_connection_inner(token, registry);
    }

    fn remove_connection_inner(&mut self, token: Token, registry: &Registry) {
        if let Some(mut conn) = self.connections.remove(&token) {
            let _ = registry.deregister(&mut conn.stream);
            debug!("TcpMuxListener: Removed {:?} from {:?}", token, conn.remote_addr);
        }
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }
    pub(crate) fn peer_count(&self) -> usize {
        self.peer_connections.len()
    }

    pub(crate) fn close(&mut self) {
        self.connections.clear();
        self.peer_connections.clear();
        self.cookie_to_port.clear();
        self.listener.take();
        info!("TcpMuxListener: Closed (port {})", self.port);
    }
}

impl Drop for TcpMuxListener {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::tcp::framing::write_framed_message;
    use crate::rtps::transport::tcp::protocol::{
        ControlMsg, MSG_ERROR, MSG_PEER_HELLO_ACK,
    };
    use crossbeam_channel::bounded;
    use mio::{Events, Poll};
    use std::io::Read;
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    fn make_listener() -> (TcpMuxListener, crossbeam_channel::Receiver<IncomingMessage>) {
        let (disc_tx, _disc_rx) = bounded(64);
        let (user_tx, user_rx) = bounded(64);
        let listener = TcpMuxListener::new(
            0, // OS-assigned ephemeral port
            0,
            0,
            [0u8; 12],
            disc_tx,
            user_tx,
        )
        .expect("listener creation");
        (listener, user_rx)
    }

    #[test]
    fn test_peer_connection_group_all_tokens() {
        let mut group = PeerConnectionGroup::new();
        assert!(group.all_tokens().is_empty());
        assert!(!group.has_data_tokens());
        assert!(group.control_lost_at.is_none());

        group.control_token = Some(Token(100));
        assert_eq!(group.all_tokens().len(), 1);
        assert!(!group.has_data_tokens());

        group.discovery_token = Some(Token(101));
        group.user_data_token = Some(Token(102));
        assert_eq!(group.all_tokens().len(), 3);
        assert!(group.has_data_tokens());
    }

    /// Helper: build a fully populated PeerConnectionGroup with synthetic
    /// tokens AND register matching MuxConnection entries so that
    /// `prune_orphan_data_connections` can actually deregister them.
    fn install_fake_group(
        listener: &mut TcpMuxListener,
        guid: GuidPrefix,
        ctrl: Option<Token>,
        disc: Option<Token>,
        user: Option<Token>,
        control_lost_at: Option<Instant>,
    ) {
        listener.peer_connections.insert(
            guid,
            PeerConnectionGroup {
                control_token: ctrl,
                discovery_token: disc,
                user_data_token: user,
                control_lost_at,
            },
        );
        // The actual MuxConnection entries are not strictly needed for the
        // grace-period logic itself (which only inspects peer_connections),
        // but `remove_peer` will try to deregister them — leave them out and
        // let it be a no-op for synthetic tokens.
    }

    #[test]
    fn test_orphan_grace_keeps_data_during_window() {
        let (mut listener, _) = make_listener();
        let registry = Poll::new().unwrap().registry().try_clone().unwrap();
        let _ = registry; // silence unused: not actually needed because synthetic tokens have no streams

        let guid = [0xAA; 12];
        // Control was just lost, data tokens still present, well within grace.
        install_fake_group(
            &mut listener,
            guid,
            None,
            Some(Token(1001)),
            Some(Token(1002)),
            Some(Instant::now()),
        );

        // Use a long grace; pruning must not touch the group yet.
        let dummy_poll = Poll::new().unwrap();
        let pruned =
            listener.prune_orphan_data_connections(Duration::from_secs(60), dummy_poll.registry());
        assert_eq!(pruned, 0);
        assert!(listener.peer_connections.contains_key(&guid));
    }

    #[test]
    fn test_orphan_grace_prunes_after_window() {
        let (mut listener, _) = make_listener();

        let guid = [0xBB; 12];
        // Pretend control was lost a long time ago.
        install_fake_group(
            &mut listener,
            guid,
            None,
            Some(Token(2001)),
            Some(Token(2002)),
            Some(Instant::now() - Duration::from_secs(10)),
        );

        let dummy_poll = Poll::new().unwrap();
        let pruned =
            listener.prune_orphan_data_connections(Duration::from_secs(1), dummy_poll.registry());
        assert_eq!(pruned, 1);
        assert!(!listener.peer_connections.contains_key(&guid));
    }

    #[test]
    fn test_orphan_grace_ignores_groups_with_alive_control() {
        let (mut listener, _) = make_listener();

        let guid = [0xCC; 12];
        // control_token is Some, control_lost_at is None — fully healthy.
        install_fake_group(
            &mut listener,
            guid,
            Some(Token(3000)),
            Some(Token(3001)),
            Some(Token(3002)),
            None,
        );

        let dummy_poll = Poll::new().unwrap();
        let pruned =
            listener.prune_orphan_data_connections(Duration::from_nanos(0), dummy_poll.registry());
        assert_eq!(pruned, 0);
        assert!(listener.peer_connections.contains_key(&guid));
    }

    #[test]
    fn test_orphan_grace_ignores_data_only_with_no_lost_marker() {
        let (mut listener, _) = make_listener();

        let guid = [0xDD; 12];
        // Data tokens exist but control_lost_at was never set (e.g. data
        // bound before any control loss). Should NOT be pruned.
        install_fake_group(
            &mut listener,
            guid,
            None,
            Some(Token(4001)),
            None,
            None,
        );

        let dummy_poll = Poll::new().unwrap();
        let pruned =
            listener.prune_orphan_data_connections(Duration::from_nanos(0), dummy_poll.registry());
        assert_eq!(pruned, 0);
        assert!(listener.peer_connections.contains_key(&guid));
    }

    #[test]
    fn test_listener_binds_to_ephemeral_port() {
        let (listener, _) = make_listener();
        assert!(listener.port() != 0, "OS should have assigned a port");
        assert_eq!(listener.connection_count(), 0);
    }

    #[test]
    fn test_accept_registers_connection() {
        let (mut listener, _) = make_listener();
        let port = listener.port();

        let mut poll = Poll::new().unwrap();
        let mux_token = Token(0);
        poll.registry()
            .register(listener.listener_mut().unwrap(), mux_token, Interest::READABLE)
            .unwrap();

        // Drive a client connection on a separate thread.
        let client = std::thread::spawn(move || {
            TcpStream::connect(format!("127.0.0.1:{}", port)).expect("client connect")
        });

        // Wait until the listener fires a readable event.
        let mut events = Events::with_capacity(8);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
            if events.iter().any(|e| e.token() == mux_token && e.is_readable()) {
                break;
            }
            assert!(Instant::now() < deadline, "no accept-ready event");
        }

        let token = listener
            .accept(poll.registry())
            .unwrap()
            .expect("accept should yield a token");
        let _stream = client.join().unwrap();

        assert_eq!(listener.connection_count(), 1);

        // The accepted connection must be tracked with a fresh activity stamp.
        let conn = listener.connections.get(&token).unwrap();
        assert_eq!(conn.state, ConnectionState::AwaitingFirstMessage);
        assert!(conn.last_activity.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn test_prune_idle_connections_with_zero_timeout() {
        let (mut listener, _) = make_listener();
        let port = listener.port();

        let mut poll = Poll::new().unwrap();
        let mux_token = Token(0);
        poll.registry()
            .register(listener.listener_mut().unwrap(), mux_token, Interest::READABLE)
            .unwrap();

        // Connect from a background thread; the kernel completes the TCP
        // handshake immediately because we're on loopback.
        let _client = std::thread::spawn(move || {
            let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            // Hold the client side open until the test finishes.
            std::thread::sleep(Duration::from_secs(2));
            drop(stream);
        });

        // Wait for accept-ready and accept once.
        let mut events = Events::with_capacity(8);
        for _ in 0..20 {
            poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
            if events.iter().any(|e| e.token() == mux_token && e.is_readable()) {
                break;
            }
        }
        listener.accept(poll.registry()).unwrap();
        assert_eq!(listener.connection_count(), 1);

        // Pruning with a zero timeout must remove the still-fresh connection.
        let pruned = listener.prune_idle_connections(Duration::from_nanos(0), poll.registry());
        assert_eq!(pruned, 1);
        assert_eq!(listener.connection_count(), 0);
    }

    #[test]
    fn test_prune_keeps_fresh_connections() {
        let (mut listener, _) = make_listener();
        let port = listener.port();

        let mut poll = Poll::new().unwrap();
        let mux_token = Token(0);
        poll.registry()
            .register(listener.listener_mut().unwrap(), mux_token, Interest::READABLE)
            .unwrap();

        let _client = std::thread::spawn(move || {
            let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(stream);
        });

        let mut events = Events::with_capacity(8);
        for _ in 0..20 {
            poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
            if events.iter().any(|e| e.token() == mux_token && e.is_readable()) {
                break;
            }
        }
        listener.accept(poll.registry()).unwrap();
        assert_eq!(listener.connection_count(), 1);

        // A long timeout must not prune a fresh connection.
        let pruned = listener.prune_idle_connections(Duration::from_secs(60), poll.registry());
        assert_eq!(pruned, 0);
        assert_eq!(listener.connection_count(), 1);
    }

    #[test]
    fn test_prune_emits_idle_timeout_error_to_peer() {
        let (mut listener, _) = make_listener();
        let port = listener.port();

        let mut poll = Poll::new().unwrap();
        let mux_token = Token(0);
        poll.registry()
            .register(listener.listener_mut().unwrap(), mux_token, Interest::READABLE)
            .unwrap();

        // Client thread holds the socket and waits for the server's ERROR.
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            // Read length(4) + magic(4) + payload up to 64 bytes
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).expect("read length");
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut data = vec![0u8; len];
            stream.read_exact(&mut data).expect("read payload");
            data
        });

        let mut events = Events::with_capacity(8);
        for _ in 0..20 {
            poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
            if events.iter().any(|e| e.token() == mux_token && e.is_readable()) {
                break;
            }
        }
        listener.accept(poll.registry()).unwrap();

        // Force prune.
        let pruned = listener.prune_idle_connections(Duration::from_nanos(0), poll.registry());
        assert_eq!(pruned, 1);

        let received = client.join().unwrap();
        // received = magic(4 bytes) + payload
        assert_eq!(&received[..4], b"INT2");
        let payload = &received[4..];
        assert_eq!(payload[0], MSG_ERROR);
        assert_eq!(payload[1], OP_IDLE_TIMEOUT);
        let code = u16::from_be_bytes([payload[2], payload[3]]);
        assert_eq!(code, ERR_CODE_IDLE_TIMEOUT);
    }

    #[test]
    fn test_handshake_first_message_advances_state() {
        let (mut listener, _) = make_listener();
        let port = listener.port();

        let mut poll = Poll::new().unwrap();
        let mux_token = Token(0);
        poll.registry()
            .register(listener.listener_mut().unwrap(), mux_token, Interest::READABLE)
            .unwrap();

        // Background client: connect, send PEER_HELLO, expect ACK back.
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();

            let hello = ControlMsg::PeerHello { locator: [0u8; 16] };
            write_framed_message(&mut stream, &hello.to_bytes()).unwrap();

            // Read ACK
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).unwrap();
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut data = vec![0u8; len];
            stream.read_exact(&mut data).unwrap();
            data
        });

        // Drive accept + readable events on the listener side.
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut accepted_token: Option<Token> = None;
        let mut events = Events::with_capacity(16);

        while Instant::now() < deadline {
            poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
            for ev in events.iter() {
                if ev.token() == mux_token && ev.is_readable() {
                    if let Ok(Some(t)) = listener.accept(poll.registry()) {
                        accepted_token = Some(t);
                    }
                } else if ev.is_readable() {
                    listener.on_readable(ev.token(), poll.registry());
                }
            }
            if let Some(t) = accepted_token {
                if let Some(c) = listener.connections.get(&t) {
                    if c.state == ConnectionState::Control {
                        break;
                    }
                }
            }
        }

        let received = client.join().unwrap();
        assert_eq!(&received[..4], b"INT2");
        assert_eq!(received[4], MSG_PEER_HELLO_ACK);

        let token = accepted_token.expect("accepted");
        let conn = listener.connections.get(&token).unwrap();
        assert_eq!(conn.state, ConnectionState::Control);
    }

    /// Drive accept + on_readable until the listener observes a read error
    /// or until `deadline`. Used by the RST/FIN cleanup tests.
    fn pump_until_token_gone(
        listener: &mut TcpMuxListener,
        poll: &mut Poll,
        token: Token,
        deadline: Instant,
    ) {
        let mut events = Events::with_capacity(16);
        while Instant::now() < deadline {
            poll.poll(&mut events, Some(Duration::from_millis(50))).unwrap();
            for ev in events.iter() {
                if ev.token() == Token(0) && ev.is_readable() {
                    let _ = listener.accept(poll.registry());
                } else if ev.is_readable() {
                    listener.on_readable(ev.token(), poll.registry());
                }
            }
            if !listener.connections.contains_key(&token) {
                return;
            }
        }
    }

    #[test]
    fn test_rst_close_cleans_up_connection_and_token() {
        use socket2::{Domain, SockAddr, Socket, Type};

        let (mut listener, _) = make_listener();
        let port = listener.port();

        let mut poll = Poll::new().unwrap();
        let mux_token = Token(0);
        poll.registry()
            .register(listener.listener_mut().unwrap(), mux_token, Interest::READABLE)
            .unwrap();

        // Background client: connect with SO_LINGER 0 then drop → RST.
        let client = std::thread::spawn(move || {
            let sock = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
            let addr: std::net::SocketAddr =
                format!("127.0.0.1:{}", port).parse().unwrap();
            sock.connect(&SockAddr::from(addr)).unwrap();
            sock.set_linger(Some(Duration::from_secs(0))).unwrap();
            // SO_LINGER 0 + drop = RST
            drop(sock);
        });

        // Wait for accept-ready and accept once.
        let mut events = Events::with_capacity(8);
        let accept_deadline = Instant::now() + Duration::from_secs(2);
        let mut accepted = None;
        while Instant::now() < accept_deadline {
            poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
            if events.iter().any(|e| e.token() == mux_token && e.is_readable()) {
                accepted = listener.accept(poll.registry()).unwrap();
                if accepted.is_some() {
                    break;
                }
            }
        }
        let token = accepted.expect("accepted token");
        client.join().unwrap();
        assert_eq!(listener.connection_count(), 1);

        // Drive on_readable until the read error is observed and the
        // connection is removed by `remove_connection`.
        pump_until_token_gone(
            &mut listener,
            &mut poll,
            token,
            Instant::now() + Duration::from_secs(2),
        );

        assert!(
            !listener.connections.contains_key(&token),
            "connection token must be removed after RST"
        );
        assert_eq!(listener.connection_count(), 0);
    }

    #[test]
    fn test_fin_close_cleans_up_connection_and_token() {
        use std::net::{Shutdown, TcpStream};

        let (mut listener, _) = make_listener();
        let port = listener.port();

        let mut poll = Poll::new().unwrap();
        let mux_token = Token(0);
        poll.registry()
            .register(listener.listener_mut().unwrap(), mux_token, Interest::READABLE)
            .unwrap();

        // Background client: connect, then shutdown(WR) and drop → graceful FIN
        let client = std::thread::spawn(move || {
            let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            stream.shutdown(Shutdown::Both).unwrap();
            drop(stream);
        });

        // Accept first
        let mut events = Events::with_capacity(8);
        let accept_deadline = Instant::now() + Duration::from_secs(2);
        let mut accepted = None;
        while Instant::now() < accept_deadline {
            poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
            if events.iter().any(|e| e.token() == mux_token && e.is_readable()) {
                accepted = listener.accept(poll.registry()).unwrap();
                if accepted.is_some() {
                    break;
                }
            }
        }
        let token = accepted.expect("accepted token");
        client.join().unwrap();
        assert_eq!(listener.connection_count(), 1);

        // Pump until the EOF causes remove_connection.
        pump_until_token_gone(
            &mut listener,
            &mut poll,
            token,
            Instant::now() + Duration::from_secs(2),
        );

        assert!(
            !listener.connections.contains_key(&token),
            "connection token must be removed after graceful FIN"
        );
        assert_eq!(listener.connection_count(), 0);
    }

    #[test]
    fn test_rst_burst_does_not_leak_tokens() {
        use socket2::{Domain, SockAddr, Socket, Type};

        let (mut listener, _) = make_listener();
        let port = listener.port();

        let mut poll = Poll::new().unwrap();
        let mux_token = Token(0);
        poll.registry()
            .register(listener.listener_mut().unwrap(), mux_token, Interest::READABLE)
            .unwrap();

        const ROUNDS: usize = 8;

        // Hammer the listener with RSTs from a burst of clients.
        let clients: Vec<_> = (0..ROUNDS)
            .map(|_| {
                std::thread::spawn(move || {
                    let sock = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
                    let addr: std::net::SocketAddr =
                        format!("127.0.0.1:{}", port).parse().unwrap();
                    sock.connect(&SockAddr::from(addr)).unwrap();
                    sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                    drop(sock);
                })
            })
            .collect();
        for c in clients {
            c.join().unwrap();
        }

        // Drive the listener until everything has been observed and cleaned up.
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut events = Events::with_capacity(32);
        while Instant::now() < deadline {
            poll.poll(&mut events, Some(Duration::from_millis(50))).unwrap();
            for ev in events.iter() {
                if ev.token() == mux_token && ev.is_readable() {
                    while listener.accept(poll.registry()).unwrap().is_some() {}
                } else if ev.is_readable() {
                    listener.on_readable(ev.token(), poll.registry());
                }
            }
            if listener.connection_count() == 0 {
                break;
            }
        }

        assert_eq!(
            listener.connection_count(),
            0,
            "RST burst leaked connection tokens"
        );
        assert_eq!(
            listener.peer_count(),
            0,
            "RST burst leaked peer groups"
        );
    }
}
