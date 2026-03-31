#![allow(dead_code)]
#![allow(unused_variables)]

use std::env;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use log::debug;
use socket2::{Domain, Protocol, SockAddr, Socket as Socket2, Type};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::framing::write_framed_message;
use crate::rtps::transport::tcp::protocol::{
    BindRequest, BindResponse, BindStatus, BindType, ControlMsg,
};

/// Connection key: (physical address, connection type)
/// logical_port = 0 means Control connection, otherwise RTPS_DATA connection
type ConnectionKey = (SocketAddr, u16);

/// Logical port value representing a control connection
const CONTROL_LOGICAL_PORT: u16 = 0;

/// Cached remote peer information obtained from Control BIND handshake
#[derive(Debug, Clone)]
struct PeerInfo {
    guid_prefix: GuidPrefix,
    participant_id: u32,
    listener_port: u16,
}

/// TCP sender for DDS/RTPS communication
///
/// Manages TCP connections to remote endpoints and sends framed messages.
/// In single-port mode, each peer has up to 3 connections:
/// - Control (bind_type=Control, logical_port=0): keepalive, liveliness
/// - Discovery (bind_type=RtpsData, logical_port=discovery_port): SPDP/SEDP
/// - UserData (bind_type=RtpsData, logical_port=user_port): application data
#[derive(Debug, Clone)]
pub(crate) struct TcpSender {
    /// Working IP address for binding
    working_ip: String,

    /// Connection pool: (SocketAddr, logical_port) -> TcpStream
    /// logical_port=0 is reserved for Control connections
    /// Uses DashMap for lock-free concurrent access
    connections: Arc<DashMap<ConnectionKey, TcpStream>>,

    /// Remote peer info cache: physical_addr -> PeerInfo
    /// Populated during Control BIND handshake
    peer_info: Arc<DashMap<SocketAddr, PeerInfo>>,

    /// Connection timeout duration
    connect_timeout: Duration,

    /// Local GUID prefix
    local_guid_prefix: GuidPrefix,

    /// DDS domain ID
    domain_id: u32,

    /// Local participant ID
    participant_id: u32,

    /// Local listener port (physical port, 7400 + 250 * domainId)
    listener_port: u16,
}

impl TcpSender {
    /// Default connection timeout (5 seconds)
    const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5000;

    /// Default write timeout (10 seconds)
    const DEFAULT_WRITE_TIMEOUT_MS: u64 = 10000;

    /// Default nodelay setting (true - disable Nagle's algorithm)
    const DEFAULT_NODELAY: bool = true;

    /// Create a new TcpSender
    ///
    /// Reads configuration from environment variables:
    /// - INT2DDS_TCP_CONNECT_TIMEOUT: Connection timeout in milliseconds (default: 5000)
    /// - INT2DDS_TCP_WRITE_TIMEOUT: Write timeout in milliseconds (default: 10000)
    /// - INT2DDS_TCP_NODELAY: Enable TCP nodelay (default: true)
    ///
    /// # Arguments
    /// * `working_ip` - The local IP address to bind to
    ///
    /// # Returns
    /// A new TcpSender instance
    pub(crate) fn new(
        working_ip: String,
        local_guid_prefix: GuidPrefix,
        domain_id: u32,
        participant_id: u32,
        listener_port: u16,
    ) -> io::Result<Self> {
        let connect_timeout_ms = env::var("INT2DDS_TCP_CONNECT_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(Self::DEFAULT_CONNECT_TIMEOUT_MS);

        debug!(
            "TcpSender: Created (domain={}, pid={}, listener_port={})",
            domain_id, participant_id, listener_port
        );

        Ok(Self {
            working_ip,
            connections: Arc::new(DashMap::new()),
            peer_info: Arc::new(DashMap::new()),
            connect_timeout: Duration::from_millis(connect_timeout_ms),
            local_guid_prefix,
            domain_id,
            participant_id,
            listener_port,
        })
    }

    /// Get the write timeout from environment variable or default
    fn get_write_timeout() -> Duration {
        let timeout_ms = env::var("INT2DDS_TCP_WRITE_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(Self::DEFAULT_WRITE_TIMEOUT_MS);
        Duration::from_millis(timeout_ms)
    }

    /// Get the nodelay setting from environment variable or default
    fn get_nodelay() -> bool {
        env::var("INT2DDS_TCP_NODELAY")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(Self::DEFAULT_NODELAY)
    }

    pub(crate) fn port(&self) -> u16 {
        self.listener_port
    }

    // ========================================================================
    // Connection management
    // ========================================================================

    /// Ensure a Control connection to the remote physical address.
    /// Performs BIND handshake and caches remote PeerInfo (guid_prefix, pid).
    fn ensure_control_connection(&self, physical_addr: &SocketAddr) -> io::Result<PeerInfo> {
        // Check cache first
        if let Some(info) = self.peer_info.get(physical_addr) {
            let key = (*physical_addr, CONTROL_LOGICAL_PORT);
            if self.connections.contains_key(&key) {
                return Ok(info.clone());
            }
        }

        let mut stream = self.tcp_connect(physical_addr)?;

        // BIND handshake: send BindRequest(Control)
        let bind_req = ControlMsg::BindRequest(BindRequest {
            guid_prefix: self.local_guid_prefix,
            domain_id: self.domain_id,
            participant_id: self.participant_id,
            listener_port: self.listener_port,
            bind_type: BindType::Control,
            logical_port: 0,
        });
        self.send_control_msg(&mut stream, &bind_req)?;
        let resp = self.read_bind_response(&mut stream)?;

        let info = PeerInfo {
            guid_prefix: resp.guid_prefix,
            participant_id: resp.participant_id,
            listener_port: resp.listener_port,
        };

        let key = (*physical_addr, CONTROL_LOGICAL_PORT);
        self.connections.insert(key, stream);
        self.peer_info.insert(*physical_addr, info.clone());

        debug!(
            "TcpSender: Control BIND complete to {:?} (remote pid={}, remote listener_port={})",
            physical_addr, info.participant_id, info.listener_port
        );

        Ok(info)
    }

    /// Ensure an RTPS_DATA connection for a specific logical port.
    /// Control connection is established first if needed.
    fn ensure_data_connection(
        &self,
        physical_addr: &SocketAddr,
        logical_port: u16,
    ) -> io::Result<()> {
        let key = (*physical_addr, logical_port);
        if self.connections.contains_key(&key) {
            return Ok(());
        }

        // Control must exist first (provides remote pid)
        if !self.peer_info.contains_key(physical_addr) {
            self.ensure_control_connection(physical_addr)?;
        }

        let mut stream = self.tcp_connect(physical_addr)?;

        // BIND handshake: send BindRequest(RtpsData)
        let bind_req = ControlMsg::BindRequest(BindRequest {
            guid_prefix: self.local_guid_prefix,
            domain_id: self.domain_id,
            participant_id: self.participant_id,
            listener_port: self.listener_port,
            bind_type: BindType::RtpsData,
            logical_port,
        });
        self.send_control_msg(&mut stream, &bind_req)?;
        let _resp = self.read_bind_response(&mut stream)?;

        self.connections.insert(key, stream);

        debug!(
            "TcpSender: Data BIND complete to {:?} logical_port={}",
            physical_addr, logical_port
        );

        Ok(())
    }

    // ========================================================================
    // Sending
    // ========================================================================

    /// Send RTPS data to a specific logical port on a remote peer.
    /// Automatically establishes Control + Data connections if needed.
    pub(crate) fn send_to_logical_port(
        &self,
        addr: &SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<usize> {
        self.ensure_data_connection(addr, logical_port)?;

        let key = (*addr, logical_port);
        self.write_to_connection(&key, data)
    }

    /// Get the discovery logical port for a remote peer.
    /// Requires that a control connection has been established (peer_info cached).
    pub(crate) fn get_peer_discovery_port(&self, addr: &SocketAddr) -> io::Result<u16> {
        let peer_info = self.peer_info.get(addr).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "No peer info (control connection not established)",
            )
        })?;
        Ok(PortManager::get_discovery_traffic_unicast_port(
            self.domain_id,
            peer_info.participant_id,
        ))
    }

    /// Get the user data logical port for a remote peer.
    pub(crate) fn get_peer_user_port(&self, addr: &SocketAddr) -> io::Result<u16> {
        let peer_info = self.peer_info.get(addr).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "No peer info (control connection not established)",
            )
        })?;
        Ok(PortManager::get_user_traffic_unicast_port(self.domain_id, peer_info.participant_id))
    }

    /// Disconnect all connections for a remote peer.
    /// Sends Close on the control connection before dropping.
    pub(crate) fn disconnect_peer(&self, addr: &SocketAddr) {
        let control_key = (*addr, CONTROL_LOGICAL_PORT);
        if let Some(mut entry) = self.connections.get_mut(&control_key) {
            let _ = self.send_control_msg(entry.value_mut(), &ControlMsg::Close);
        }

        self.connections.retain(|key, _| key.0 != *addr);
        self.peer_info.remove(addr);

        debug!("TcpSender: Disconnected peer {:?}", addr);
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    /// Establish a raw TCP connection to an address.
    /// Set port = 0 to be allocated ephemeral port by os later.
    fn tcp_connect(&self, addr: &SocketAddr) -> io::Result<TcpStream> {
        let socket2 = Socket2::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;

        let local_ip: IpAddr = self.working_ip.parse().map_err(|e| {
            io::Error::new(ErrorKind::InvalidInput, format!("Invalid working_ip: {}", e))
        })?;
        let local_addr = SocketAddr::new(local_ip, 0);
        socket2.bind(&SockAddr::from(local_addr))?;

        socket2.set_nonblocking(true)?;

        match socket2.connect(&SockAddr::from(*addr)) {
            Ok(_) => {}
            Err(e)
                if e.raw_os_error() == Some(10035)       // Windows: WSAEWOULDBLOCK
                   || e.raw_os_error() == Some(115)         // Linux: EINPROGRESS
                   || e.kind() == ErrorKind::WouldBlock =>
            {
                use std::time::Instant;
                let start = Instant::now();

                loop {
                    if start.elapsed() >= self.connect_timeout {
                        return Err(io::Error::new(
                            ErrorKind::TimedOut,
                            format!("Connection timeout to {:?}", addr),
                        ));
                    }

                    match socket2.take_error() {
                        Ok(Some(err)) => {
                            return Err(io::Error::new(
                                ErrorKind::ConnectionRefused,
                                format!("Connection failed to {:?}: {:?}", addr, err),
                            ));
                        }
                        Ok(None) => {
                            if socket2.peer_addr().is_ok() {
                                break;
                            }
                        }
                        Err(e) => return Err(e),
                    }

                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            Err(e) => return Err(e),
        }

        socket2.set_nonblocking(false)?;

        let stream: TcpStream = socket2.into();

        let _ = stream.set_nodelay(Self::get_nodelay());
        let _ = stream.set_write_timeout(Some(Self::get_write_timeout()));

        Ok(stream)
    }

    /// Write framed data to a cached connection.
    fn write_to_connection(&self, key: &ConnectionKey, buffer: &[u8]) -> io::Result<usize> {
        let mut stream = self
            .connections
            .get(key)
            .ok_or_else(|| io::Error::new(ErrorKind::NotConnected, "Connection not found"))?
            .value()
            .try_clone()?;

        match write_framed_message(&mut stream, buffer) {
            Ok(()) => {
                debug!("TcpSender: Sent {} bytes to {:?}", buffer.len(), key);
                Ok(buffer.len())
            }
            Err(e) => {
                self.connections.remove(key);
                Err(e)
            }
        }
    }

    /// Send a control message on a stream.
    fn send_control_msg(&self, stream: &mut TcpStream, msg: &ControlMsg) -> io::Result<()> {
        let payload = msg.to_bytes();
        let len = payload.len() as u32;
        stream.write_all(&len.to_be_bytes())?;
        stream.write_all(&payload)?;
        stream.flush()?;
        Ok(())
    }

    /// Read a BindResponse from a stream.
    fn read_bind_response(&self, stream: &mut TcpStream) -> io::Result<BindResponse> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf)?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len == 0 || len > 1024 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Invalid BindResponse frame length: {}", len),
            ));
        }

        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload)?;

        match ControlMsg::from_bytes(&payload)? {
            ControlMsg::BindResponse(resp) => {
                if resp.status != BindStatus::Ok {
                    return Err(io::Error::new(
                        ErrorKind::ConnectionRefused,
                        format!("BIND rejected: {:?}", resp.status),
                    ));
                }
                Ok(resp)
            }
            other => Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Expected BindResponse, got {:?}", other),
            )),
        }
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn peer_count(&self) -> usize {
        self.peer_info.len()
    }

    /// Close all connections (sends Close on each control connection first)
    /// Send data to a remote endpoint's discovery channel.
    /// Establishes control connection first (BIND handshake), then resolves
    /// the peer's discovery logical port from peer_info.
    pub(crate) fn send_to_discovery(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        self.ensure_control_connection(addr)?;
        let logical_port = self.get_peer_discovery_port(addr)?;
        self.send_to_logical_port(addr, logical_port, data)
    }

    /// Send data to a remote endpoint's user data channel.
    /// Establishes control connection first (BIND handshake), then resolves
    /// the peer's user data logical port from peer_info.
    pub(crate) fn send_to_user_data(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        self.ensure_control_connection(addr)?;
        let logical_port = self.get_peer_user_port(addr)?;
        self.send_to_logical_port(addr, logical_port, data)
    }

    pub(crate) fn close_all(self) {
        for entry in self.connections.iter() {
            let (addr, lp) = entry.key();
            if *lp == CONTROL_LOGICAL_PORT {
                if let Ok(mut stream) = entry.value().try_clone() {
                    let _ = self.send_control_msg(&mut stream, &ControlMsg::Close);
                }
            }
        }
        self.connections.clear();
        self.peer_info.clear();
        debug!("TcpSender: All connections closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sender() -> TcpSender {
        TcpSender::new(
            "127.0.0.1".to_string(),
            [0x01; 12],
            0,    // domain_id
            0,    // participant_id
            7400, // listener_port
        )
        .unwrap()
    }

    #[test]
    fn test_tcp_sender_creation() {
        let sender = create_test_sender();
        assert_eq!(sender.port(), 7400);
        assert_eq!(sender.connection_count(), 0);
        assert_eq!(sender.peer_count(), 0);
    }

    #[test]
    fn test_get_peer_port_without_control_connection() {
        let sender = create_test_sender();
        let addr: SocketAddr = "192.168.1.10:7400".parse().unwrap();

        // No control connection → should fail
        assert!(sender.get_peer_discovery_port(&addr).is_err());
        assert!(sender.get_peer_user_port(&addr).is_err());
    }

    #[test]
    fn test_disconnect_nonexistent_peer() {
        let sender = create_test_sender();
        let addr: SocketAddr = "192.168.1.10:7400".parse().unwrap();

        // Should not panic
        sender.disconnect_peer(&addr);
        assert_eq!(sender.connection_count(), 0);
    }
}
