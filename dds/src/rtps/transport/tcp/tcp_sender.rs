#![allow(dead_code)]
#![allow(unused_variables)]

use std::env;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use log::debug;
use socket2::{Domain, Protocol, SockAddr, Socket as Socket2, Type};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::framing::{read_framed_message, write_framed_message};
use crate::rtps::transport::tcp::protocol::{
    encode_locator, ControlMsg, MSG_PEER_HELLO_ACK, MSG_PORT_BIND_ACK,
};

/// Connection key: (physical address, logical_port)
type ConnectionKey = (SocketAddr, u16);

/// Logical port 0 = control connection
const CONTROL_LOGICAL_PORT: u16 = 0;

/// Cached peer info from PEER_HELLO handshake
#[derive(Debug, Clone)]
struct PeerInfo {
    /// Physical address of the control connection
    control_addr: SocketAddr,
}

/// TCP sender with 3-step handshake: PEER_HELLO → PORT_RESERVE → PORT_BIND
#[derive(Debug, Clone)]
pub(crate) struct TcpSender {
    working_ip: String,
    connections: Arc<DashMap<ConnectionKey, TcpStream>>,
    peer_info: Arc<DashMap<SocketAddr, PeerInfo>>,
    connect_timeout: Duration,
    local_guid_prefix: GuidPrefix,
    domain_id: u32,
    participant_id: u32,
    listener_port: u16,
}

impl TcpSender {
    const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5000;
    const DEFAULT_WRITE_TIMEOUT_MS: u64 = 10000;
    const DEFAULT_NODELAY: bool = true;

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
            "TcpSender: Created (domain={}, pid={}, port={})",
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

    fn get_write_timeout() -> Duration {
        Duration::from_millis(
            env::var("INT2DDS_TCP_WRITE_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(Self::DEFAULT_WRITE_TIMEOUT_MS),
        )
    }

    fn get_nodelay() -> bool {
        env::var("INT2DDS_TCP_NODELAY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(Self::DEFAULT_NODELAY)
    }

    pub(crate) fn port(&self) -> u16 {
        self.listener_port
    }

    // ========================================================================
    // 3-Step Handshake
    // ========================================================================

    /// Step 1: PEER_HELLO — establish control connection, exchange locators.
    fn ensure_control(&self, physical_addr: &SocketAddr) -> io::Result<()> {
        let key = (*physical_addr, CONTROL_LOGICAL_PORT);
        if self.connections.contains_key(&key) {
            return Ok(());
        }

        let mut stream = self.tcp_connect(physical_addr)?;

        let local_ip: std::net::Ipv4Addr =
            self.working_ip.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
        let locator = encode_locator(local_ip, self.listener_port);

        // Send PEER_HELLO
        let hello = ControlMsg::PeerHello { locator };
        write_framed_message(&mut stream, &hello.to_bytes())?;

        // Read PEER_HELLO_ACK
        let resp = self.read_control_response(&mut stream)?;
        if resp.to_bytes()[0] != MSG_PEER_HELLO_ACK {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Expected PEER_HELLO_ACK, got {}", resp.type_name()),
            ));
        }

        debug!("TcpSender: PEER_HELLO complete to {:?}", physical_addr);

        self.connections.insert(key, stream);
        self.peer_info.insert(*physical_addr, PeerInfo { control_addr: *physical_addr });

        Ok(())
    }

    /// Steps 2+3: PORT_RESERVE on control connection, then PORT_BIND on new connection.
    fn ensure_data(&self, physical_addr: &SocketAddr, logical_port: u16) -> io::Result<()> {
        let key = (*physical_addr, logical_port);
        if self.connections.contains_key(&key) {
            return Ok(());
        }

        // Ensure control connection exists
        self.ensure_control(physical_addr)?;

        // Step 2: PORT_RESERVE on the control connection
        let control_key = (*physical_addr, CONTROL_LOGICAL_PORT);
        let cookie = {
            let mut control_stream = self
                .connections
                .get(&control_key)
                .ok_or_else(|| io::Error::new(ErrorKind::NotConnected, "Control connection lost"))?
                .value()
                .try_clone()?;

            let reserve = ControlMsg::PortReserve { logical_port };
            write_framed_message(&mut control_stream, &reserve.to_bytes())?;

            let resp = self.read_control_response(&mut control_stream)?;
            match resp {
                ControlMsg::PortReserveAck { cookie } => cookie,
                ControlMsg::Error { code, message } => {
                    return Err(io::Error::new(
                        ErrorKind::ConnectionRefused,
                        format!("PORT_RESERVE rejected (code={}): {}", code, message),
                    ));
                }
                other => {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        format!("Expected PORT_RESERVE_ACK, got {}", other.type_name()),
                    ));
                }
            }
        };

        debug!(
            "TcpSender: PORT_RESERVE complete (port={}, cookie=0x{:02x})",
            logical_port, cookie[0]
        );

        // Step 3: PORT_BIND on a NEW TCP connection
        let mut data_stream = self.tcp_connect(physical_addr)?;

        let bind = ControlMsg::PortBind { cookie };
        write_framed_message(&mut data_stream, &bind.to_bytes())?;

        let resp = self.read_control_response(&mut data_stream)?;
        if resp.to_bytes()[0] != MSG_PORT_BIND_ACK {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Expected PORT_BIND_ACK, got {}", resp.type_name()),
            ));
        }

        debug!("TcpSender: PORT_BIND complete (port={}, cookie=0x{:02x})", logical_port, cookie[0]);

        self.connections.insert(key, data_stream);
        Ok(())
    }

    // ========================================================================
    // Sending
    // ========================================================================

    /// Send RTPS data to a logical port. Performs handshake if needed.
    pub(crate) fn send_to_logical_port(
        &self,
        addr: &SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<usize> {
        self.ensure_data(addr, logical_port)?;

        let key = (*addr, logical_port);
        let mut stream = self
            .connections
            .get(&key)
            .ok_or_else(|| io::Error::new(ErrorKind::NotConnected, "Connection not found"))?
            .value()
            .try_clone()?;

        match write_framed_message(&mut stream, data) {
            Ok(()) => {
                debug!("TcpSender: Sent {} bytes to {:?}", data.len(), key);
                Ok(data.len())
            }
            Err(e) => {
                // BrokenPipe / ConnectionReset → peer is dead, clean up everything
                if e.kind() == ErrorKind::BrokenPipe
                    || e.kind() == ErrorKind::ConnectionReset
                    || e.kind() == ErrorKind::ConnectionAborted
                {
                    debug!("TcpSender: Peer {:?} disconnected, cleaning up", addr);
                    self.disconnect_peer(addr);
                } else {
                    self.connections.remove(&key);
                }
                Err(e)
            }
        }
    }

    /// Send to discovery channel.
    pub(crate) fn send_to_discovery(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        let port =
            PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id);
        self.send_to_logical_port(addr, port, data)
    }

    /// Send to user data channel.
    pub(crate) fn send_to_user_data(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        let port = PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id);
        self.send_to_logical_port(addr, port, data)
    }

    pub(crate) fn get_peer_discovery_port(&self, addr: &SocketAddr) -> io::Result<u16> {
        Ok(PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id))
    }

    pub(crate) fn get_peer_user_port(&self, addr: &SocketAddr) -> io::Result<u16> {
        Ok(PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id))
    }

    pub(crate) fn disconnect_peer(&self, addr: &SocketAddr) {
        self.connections.retain(|key, _| key.0 != *addr);
        self.peer_info.remove(addr);
        debug!("TcpSender: Disconnected peer {:?}", addr);
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn peer_count(&self) -> usize {
        self.peer_info.len()
    }

    pub(crate) fn close_all(self) {
        self.connections.clear();
        self.peer_info.clear();
        debug!("TcpSender: All connections closed");
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    fn tcp_connect(&self, addr: &SocketAddr) -> io::Result<TcpStream> {
        let socket2 = Socket2::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;

        let local_ip: IpAddr = self.working_ip.parse().map_err(|e| {
            io::Error::new(ErrorKind::InvalidInput, format!("Invalid working_ip: {}", e))
        })?;
        socket2.bind(&SockAddr::from(SocketAddr::new(local_ip, 0)))?;
        socket2.set_nonblocking(true)?;

        match socket2.connect(&SockAddr::from(*addr)) {
            Ok(_) => {}
            Err(e)
                if e.raw_os_error() == Some(10035)
                    || e.raw_os_error() == Some(115)
                    || e.kind() == ErrorKind::WouldBlock =>
            {
                let start = std::time::Instant::now();
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
                        Ok(None) if socket2.peer_addr().is_ok() => break,
                        Ok(None) => {}
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

    fn read_control_response(&self, stream: &mut TcpStream) -> io::Result<ControlMsg> {
        let payload = read_framed_message(stream)?;
        ControlMsg::from_bytes(&payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sender() -> TcpSender {
        TcpSender::new("127.0.0.1".to_string(), [0x01; 12], 0, 0, 7400).unwrap()
    }

    #[test]
    fn test_tcp_sender_creation() {
        let sender = create_test_sender();
        assert_eq!(sender.port(), 7400);
        assert_eq!(sender.connection_count(), 0);
        assert_eq!(sender.peer_count(), 0);
    }

    #[test]
    fn test_disconnect_nonexistent_peer() {
        let sender = create_test_sender();
        let addr: SocketAddr = "192.168.1.10:7400".parse().unwrap();
        sender.disconnect_peer(&addr);
        assert_eq!(sender.connection_count(), 0);
    }
}
