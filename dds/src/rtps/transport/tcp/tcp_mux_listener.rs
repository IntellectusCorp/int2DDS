#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Instant;

use crossbeam_channel::Sender;
use log::{debug, error, info, warn};
use mio::net::{TcpListener as MioTcpListener, TcpStream as MioTcpStream};
use mio::{Interest, Registry, Token};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::framing::{classify_frame, FramedReader, TcpFrameKind};
use crate::rtps::transport::tcp::protocol::{BindResponse, BindStatus, BindType, ControlMsg};

/// Token for the listener socket itself
const LISTENER_TOKEN: Token = Token(0);

/// Starting token index for accepted connections
const CONNECTION_TOKEN_START: usize = 65536;

/// Default keepalive interval in seconds
const DEFAULT_KEEPALIVE_INTERVAL_SECS: u64 = 30;

/// Maximum missed keepalive acks before closing peer connections
const MAX_MISSED_KEEPALIVES: u32 = 3;

/// TCP muliplexed listener for single-port DDS/RTPS communication.
///
/// Accepts all TCP connections on a single physical port (7400 + 250 * domain_id).
/// Each connection goes through a BIND handshake to determine its type
/// (Control, Discovery, UserData), then frames are routed to the
/// appropriate channel.
#[derive(Debug)]
pub(crate) struct TcpMuxListener {
    /// Physical port (7400 + 250 * domain_id)
    port: u16,

    /// DDS domain ID
    domain_id: u32,

    /// Local participant ID
    participant_id: u32,

    /// Local GUID prefix
    local_guid_prefix: GuidPrefix,

    /// The mio TCP listener socket
    listener: Option<MioTcpListener>,

    /// All accepted connections indexed by token
    connections: HashMap<Token, MuxConnection>,

    /// Token counter
    next_token: usize,

    /// Peer connection groups indexed by remote guid_prefix
    peer_connections: HashMap<GuidPrefix, PeerConnectionGroup>,

    /// Channel for routing discovery RTPS data to discovery listening task
    discovery_tx: Sender<(Vec<u8>, SocketAddr)>,

    /// Channel for routing user RTPS data to user listening task
    user_data_tx: Sender<(Vec<u8>, SocketAddr)>,
}

/// Tracks the 3 connections from a single remote peer
#[derive(Debug)]
struct PeerConnectionGroup {
    control_token: Option<Token>,
    discovery_token: Option<Token>,
    user_data_token: Option<Token>,
    last_keepalive_sent: Instant,
    last_keepalive_ack: Instant,
    missed_keepalives: u32,
}

impl PeerConnectionGroup {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            control_token: None,
            discovery_token: None,
            user_data_token: None,
            last_keepalive_sent: now,
            last_keepalive_ack: now,
            missed_keepalives: 0,
        }
    }

    /// Collect all active tokens for this peer
    fn all_tokens(&self) -> Vec<Token> {
        let mut tokens = Vec::with_capacity(3);
        if let Some(t) = self.control_token {
            tokens.push(t);
        }
        if let Some(t) = self.discovery_token {
            tokens.push(t);
        }
        if let Some(t) = self.user_data_token {
            tokens.push(t);
        }
        tokens
    }
}

/// State of a single multiplexed connection
#[derive(Debug)]
struct MuxConnection {
    stream: MioTcpStream,
    framed_reader: FramedReader,
    state: ConnectionState,
    bind_type: Option<BindType>,
    bound_logical_port: Option<u16>,
    remote_addr: SocketAddr,
    remote_guid_prefix: Option<GuidPrefix>,
}

/// Connection lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    /// Connection accepted, waiting for BindRequest
    AwaitingBind,
    /// BIND completed, ready for data
    Active,
    /// Connection being closed
    Closing,
}

impl TcpMuxListener {
    /// Create a new TcpMuxListener bound to the physical port.
    ///
    /// # Arguments
    /// * `port` - The physical port to bind (typically 7400 + 250 * domain_id)
    /// * `domain_id` - DDS domain ID
    /// * `participant_id` - Local participant ID
    /// * `local_guid_prefix` - Local participant's GUID prefix
    /// * `discovery_tx` - Channel sender for discovery RTPS data
    /// * `user_data_tx` - Channel sender for user RTPS data
    pub(crate) fn new(
        port: u16,
        domain_id: u32,
        participant_id: u32,
        local_guid_prefix: GuidPrefix,
        discovery_tx: Sender<(Vec<u8>, SocketAddr)>,
        user_data_tx: Sender<(Vec<u8>, SocketAddr)>,
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
        })
    }

    /// Get a mutable reference to the mio TcpListener for poll registration
    pub(crate) fn listener_mut(&mut self) -> Option<&mut MioTcpListener> {
        self.listener.as_mut()
    }

    /// Get the listening port
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Accept a new incoming connection and register with the poll registry.
    ///
    /// The connection starts in 'AwaitingBind' state - it must send a BindRequest
    /// before any RTPS data is accepted.
    pub(crate) fn accept(&mut self, registry: &Registry) -> io::Result<Option<Token>> {
        let listener = match &self.listener {
            Some(l) => l,
            None => {
                return Err(io::Error::new(ErrorKind::NotConnected, "MuxListener not initialized"))
            }
        };

        match listener.accept() {
            Ok((mut stream, addr)) => {
                let token = Token(self.next_token);
                self.next_token += 1;

                if let Err(e) = stream.set_nodelay(true) {
                    warn!("TcpMuxListener: Failed to set nodelay for {:?}: {:?}", addr, e);
                }

                registry.register(&mut stream, token, Interest::READABLE)?;

                debug!("TcpMuxListener: Accepted connection from {:?} (token={:?})", addr, token);

                let conn = MuxConnection {
                    stream,
                    framed_reader: FramedReader::new(),
                    state: ConnectionState::AwaitingBind,
                    bind_type: None,
                    bound_logical_port: None,
                    remote_addr: addr,
                    remote_guid_prefix: None,
                };
                self.connections.insert(token, conn);

                Ok(Some(token))
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) => {
                error!("TcpMuxListener: Accept failed: {:?}", e);
                Err(e)
            }
        }
    }

    /// Process a readable event for a connection token
    ///
    /// Reads framed data, handles BIND handshake for new connections,
    /// routes RTPS data to the appropriate channel, and handles control messages.
    pub(crate) fn on_readable(&mut self, token: Token, registry: &Registry) {
        // Read all available frames from this connection
        loop {
            let frame = {
                let conn = match self.connections.get_mut(&token) {
                    Some(c) => c,
                    None => return,
                };

                if conn.state == ConnectionState::Closing {
                    return;
                }

                match conn.framed_reader.read_message(&mut conn.stream) {
                    Ok(Some(payload)) => payload,
                    Ok(None) => continue, // partial read, try again
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => return,
                    Err(e) => {
                        debug!("TcpMuxListener: Read error on token {:?}: {:?}", token, e);
                        self.remove_connection(token, registry);
                        return;
                    }
                }
            };

            // Dispatch based on connection state
            let state = self.connections.get(&token).map(|c| c.state);
            match state {
                Some(ConnectionState::AwaitingBind) => {
                    self.handle_bind(token, &frame, registry);
                }
                Some(ConnectionState::Active) => {
                    self.handle_active_frame(token, &frame, registry);
                }
                _ => {}
            }
        }
    }

    /// Handle the BIND handshake for a newly accepted connection.
    fn handle_bind(&mut self, token: Token, payload: &[u8], registry: &Registry) {
        let kind = classify_frame(payload);
        if kind != TcpFrameKind::Control {
            warn!(
                "TcpMuxListener: Expected BindRequest on token {:?}, got {:?}. Closing.",
                token, kind
            );
            self.remove_connection(token, registry);
            return;
        }

        let msg = match ControlMsg::from_bytes(payload) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "TcpMuxListener: Failed to parse control message on token {:?}: {:?}",
                    token, e
                );
                self.remove_connection(token, registry);
                return;
            }
        };

        let req = match msg {
            ControlMsg::BindRequest(req) => req,
            other => {
                warn!(
                    "TcpMuxListener: Expected BindRequest on token {:?}, got {:?}. Closing.",
                    token, other
                );
                self.remove_connection(token, registry);
                return;
            }
        };

        // Validate domain
        if req.domain_id != self.domain_id {
            warn!(
                "TcpMuxListener: Domain mismatch from {:?}: expected {}, got {}",
                token, self.domain_id, req.domain_id
            );
            self.send_bind_response(token, BindStatus::DomainMismatch);
            self.remove_connection(token, registry);
            return;
        }

        // Validate logical port for RTPS_DATA binds
        if req.bind_type == BindType::RtpsData {
            if !PortManager::is_discovery_unicast_port(self.domain_id, req.logical_port)
                && !PortManager::is_user_unicast_port(self.domain_id, req.logical_port)
            {
                warn!("TcpMuxListener: Invalid logical port {} from {:?}", req.logical_port, token);
                self.send_bind_response(token, BindStatus::InvalidRequest);
                self.remove_connection(token, registry);
                return;
            }
        }

        // Send BindResponse with our info
        self.send_bind_response(token, BindStatus::Ok);

        // Update connection state
        if let Some(conn) = self.connections.get_mut(&token) {
            conn.state = ConnectionState::Active;
            conn.bind_type = Some(req.bind_type);
            conn.remote_guid_prefix = Some(req.guid_prefix);
            conn.bound_logical_port = match req.bind_type {
                BindType::Control => None,
                BindType::RtpsData => Some(req.logical_port),
            };
        }

        // Register in peer connection group
        let group =
            self.peer_connections.entry(req.guid_prefix).or_insert_with(PeerConnectionGroup::new);

        match req.bind_type {
            BindType::Control => {
                group.control_token = Some(token);
                debug!(
                    "TcpMuxListener: Control connection bound (token={:?}, peer={:?})",
                    token, req.guid_prefix
                );
            }
            BindType::RtpsData => {
                if PortManager::is_discovery_unicast_port(self.domain_id, req.logical_port) {
                    group.discovery_token = Some(token);
                    debug!(
                        "TcpMuxListener: Discovery connection bound (token={:?}, logical_port={}, peer={:?})",
                        token, req.logical_port, req.guid_prefix
                    );
                } else {
                    group.user_data_token = Some(token);
                    debug!(
                        "TcpMuxListener: UserData connection bound (token={:?}, logical_port={}, peer={:?})",
                        token, req.logical_port, req.guid_prefix
                    );
                }
            }
        }
    }

    /// Handle a frame on an Active connection.
    fn handle_active_frame(&mut self, token: Token, payload: &[u8], registry: &Registry) {
        let (bind_type, remote_addr) = match self.connections.get(&token) {
            Some(conn) => (conn.bind_type, conn.remote_addr),
            None => return,
        };

        match bind_type {
            Some(BindType::Control) => {
                self.handle_control_frame(token, payload, registry);
            }
            Some(BindType::RtpsData) => {
                let kind = classify_frame(payload);
                match kind {
                    TcpFrameKind::RtpsData => {
                        self.route_rtps_data(token, payload, remote_addr);
                    }
                    TcpFrameKind::Control => {
                        // Control message on RTPS_DATA connection — only Close is valid
                        if let Ok(ControlMsg::Close) = ControlMsg::from_bytes(payload) {
                            debug!("TcpMuxListener: Close received on data connection {:?}", token);
                            self.remove_connection(token, registry);
                        } else {
                            warn!(
                                "TcpMuxListener: Unexpected control message on data connection {:?}",
                                token
                            );
                        }
                    }
                    TcpFrameKind::Unknown => {
                        warn!("TcpMuxListener: Unknown frame on data connection {:?}", token);
                    }
                }
            }
            None => {
                warn!("TcpMuxListener: Frame on unbound connection {:?}", token);
            }
        }
    }

    /// Handle a control message on a Control connection
    fn handle_control_frame(&mut self, token: Token, payload: &[u8], registry: &Registry) {
        let msg = match ControlMsg::from_bytes(payload) {
            Ok(m) => m,
            Err(e) => {
                warn!("TcpMuxListener: Invalid control frame on {:?}: {:?}", token, e);
                return;
            }
        };

        match msg {
            ControlMsg::Keepalive => {
                // Respond with KeepaliveAck
                self.send_control_message(token, &ControlMsg::KeepaliveAck);
            }
            ControlMsg::KeepaliveAck => {
                // Update peer's keepalive ack timestamp
                if let Some(conn) = self.connections.get(&token) {
                    if let Some(guid) = conn.remote_guid_prefix {
                        if let Some(group) = self.peer_connections.get_mut(&guid) {
                            group.last_keepalive_ack = Instant::now();
                            group.missed_keepalives = 0;
                        }
                    }
                }
            }
            ControlMsg::Close => {
                debug!("TcpMuxListener: Close received on control connection {:?}", token);
                // Close all connections for this peer
                let guid = self.connections.get(&token).and_then(|c| c.remote_guid_prefix);
                if let Some(guid) = guid {
                    self.remove_peer(guid, registry);
                } else {
                    self.remove_connection(token, registry);
                }
            }
            other => {
                warn!(
                    "TcpMuxListener: Unexpected message {:?} on control connection {:?}",
                    other, token
                );
            }
        }
    }

    /// Route RTPS data payload to the appropriate channel based on connection type.
    fn route_rtps_data(&self, token: Token, payload: &[u8], remote_addr: SocketAddr) {
        let conn = match self.connections.get(&token) {
            Some(c) => c,
            None => return,
        };

        let logical_port = match conn.bound_logical_port {
            Some(p) => p,
            None => return,
        };

        if PortManager::is_discovery_unicast_port(self.domain_id, logical_port) {
            if let Err(e) = self.discovery_tx.try_send((payload.to_vec(), remote_addr)) {
                warn!("TcpMuxListener: Failed to route discovery data: {:?}", e);
            }
        } else if PortManager::is_user_unicast_port(self.domain_id, logical_port) {
            if let Err(e) = self.user_data_tx.try_send((payload.to_vec(), remote_addr)) {
                warn!("TcpMuxListener: Failed to route user data: {:?}", e);
            }
        }
    }

    /// Send a BindResponse to a connection.
    fn send_bind_response(&mut self, token: Token, status: BindStatus) {
        let resp = ControlMsg::BindResponse(BindResponse {
            status,
            guid_prefix: self.local_guid_prefix,
            domain_id: self.domain_id,
            participant_id: self.participant_id,
            listener_port: self.port,
        });
        self.send_control_message(token, &resp);
    }

    /// Wrtie a control message to a connection's stream.
    fn send_control_message(&mut self, token: Token, msg: &ControlMsg) {
        let conn = match self.connections.get_mut(&token) {
            Some(c) => c,
            None => return,
        };

        let payload = msg.to_bytes();
        let len = payload.len() as u32;

        use std::io::Write;
        if let Err(e) = conn
            .stream
            .write_all(&len.to_be_bytes())
            .and_then(|_| conn.stream.write_all(&payload))
            .and_then(|_| conn.stream.flush())
        {
            warn!("TcpMuxListener: Failed to send control message to {:?}: {:?}", token, e);
        }
    }

    /// Send keepalive on all peer control connections.
    /// Returns list of peers whose keepalive was missed too many times.
    pub(crate) fn send_keepalives(&mut self) -> Vec<GuidPrefix> {
        let now = Instant::now();
        let interval = std::time::Duration::from_secs(DEFAULT_KEEPALIVE_INTERVAL_SECS);
        let mut dead_peers = Vec::new();

        let guids: Vec<GuidPrefix> = self.peer_connections.keys().copied().collect();

        for guid in guids {
            let group = match self.peer_connections.get_mut(&guid) {
                Some(g) => g,
                None => continue,
            };

            if now.duration_since(group.last_keepalive_sent) < interval {
                continue;
            }

            if group.missed_keepalives >= MAX_MISSED_KEEPALIVES {
                warn!(
                    "TcpMuxListener: Peer {:?} missed {} keepalives, marking dead",
                    guid, group.missed_keepalives
                );
                dead_peers.push(guid);
                continue;
            }

            if let Some(control_token) = group.control_token {
                group.last_keepalive_sent = now;
                group.missed_keepalives += 1;
                self.send_control_message(control_token, &ControlMsg::Keepalive);
            }
        }

        dead_peers
    }

    /// Remove all connections for a peer.
    pub(crate) fn remove_peer(&mut self, guid: GuidPrefix, registry: &Registry) {
        if let Some(group) = self.peer_connections.remove(&guid) {
            for token in group.all_tokens() {
                self.remove_connection_inner(token, registry);
            }
            debug!("TcpMuxListener: Removed all connections for peer {:?}", guid);
        }
    }

    /// Remove a single connection by token.
    pub(crate) fn remove_connection(&mut self, token: Token, registry: &Registry) {
        // Also remove from peer_connections if applicable
        if let Some(conn) = self.connections.get(&token) {
            if let Some(guid) = conn.remote_guid_prefix {
                if let Some(group) = self.peer_connections.get_mut(&guid) {
                    if group.control_token == Some(token) {
                        group.control_token = None;
                    }
                    if group.discovery_token == Some(token) {
                        group.discovery_token = None;
                    }
                    if group.user_data_token == Some(token) {
                        group.user_data_token = None;
                    }
                    // Remove group if all connections are gone
                    if group.all_tokens().is_empty() {
                        self.peer_connections.remove(&guid);
                    }
                }
            }
        }
        self.remove_connection_inner(token, registry);
    }

    /// Inner connection removal (deregister + drop)
    fn remove_connection_inner(&mut self, token: Token, registry: &Registry) {
        if let Some(mut conn) = self.connections.remove(&token) {
            if let Err(e) = registry.deregister(&mut conn.stream) {
                debug!("TcpMuxListener: Failed to deregister token {:?}: {:?}", token, e);
            }
            debug!("TcpMuxListener: Removed connection {:?} from {:?}", token, conn.remote_addr);
        }
    }

    /// Get the number of active connections
    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Get the number of connected peers
    pub(crate) fn peer_count(&self) -> usize {
        self.peer_connections.len()
    }

    /// Close the listener and all connections
    pub(crate) fn close(&mut self) {
        self.connections.clear();
        self.peer_connections.clear();
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

    #[test]
    fn test_peer_connection_group_all_tokens() {
        let mut group = PeerConnectionGroup::new();
        assert!(group.all_tokens().is_empty());

        group.control_token = Some(Token(100));
        assert_eq!(group.all_tokens().len(), 1);

        group.discovery_token = Some(Token(101));
        group.user_data_token = Some(Token(102));
        assert_eq!(group.all_tokens().len(), 3);
    }
}
