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
}

#[derive(Debug)]
struct PeerConnectionGroup {
    control_token: Option<Token>,
    discovery_token: Option<Token>,
    user_data_token: Option<Token>,
}

impl PeerConnectionGroup {
    fn new() -> Self {
        Self { control_token: None, discovery_token: None, user_data_token: None }
    }

    fn all_tokens(&self) -> Vec<Token> {
        [self.control_token, self.discovery_token, self.user_data_token]
            .iter()
            .flatten()
            .copied()
            .collect()
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

                // Issue cookie and store mapping
                let cookie = generate_cookie(&mut self.next_cookie);
                self.cookie_to_port.insert(cookie, logical_port);

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

            if PortManager::is_discovery_unicast_port(self.domain_id, logical_port) {
                group.discovery_token = Some(token);
            } else {
                group.user_data_token = Some(token);
            }

            if let Some(conn) = self.connections.get_mut(&token) {
                conn.remote_guid_prefix = Some(synthetic_guid);
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
                    if group.control_token == Some(token) {
                        group.control_token = None;
                    }
                    if group.discovery_token == Some(token) {
                        group.discovery_token = None;
                    }
                    if group.user_data_token == Some(token) {
                        group.user_data_token = None;
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

        group.control_token = Some(Token(100));
        assert_eq!(group.all_tokens().len(), 1);

        group.discovery_token = Some(Token(101));
        group.user_data_token = Some(Token(102));
        assert_eq!(group.all_tokens().len(), 3);
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
}
