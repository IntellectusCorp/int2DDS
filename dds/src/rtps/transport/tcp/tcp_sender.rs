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

use crate::rtps::messages::tcp_control_message::TcpControlMessage;
use crate::rtps::transport::tcp::framing::write_framed_message;
use crate::rtps::transport::{Transport, TransportType};

/// TCP sender for DDS/RTPS communication with connection separation
///
/// Maintains separate connection pools for control and data connections:
/// - Control connections: carry DDS handshake, keepalive, and close messages
/// - Data connections: carry only RTPS messages
#[derive(Debug, Clone)]
pub(crate) struct TcpSender {
    /// Working IP address for binding
    working_ip: String,

    /// Control connection pool: remote SocketAddr -> TcpStream
    control_connections: Arc<DashMap<SocketAddr, TcpStream>>,

    /// Data connection pool: remote SocketAddr -> TcpStream
    data_connections: Arc<DashMap<SocketAddr, TcpStream>>,

    /// Connection timeout duration
    connect_timeout: Duration,
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
    pub(crate) fn new(working_ip: String) -> io::Result<Self> {
        let connect_timeout_ms = env::var("INT2DDS_TCP_CONNECT_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(Self::DEFAULT_CONNECT_TIMEOUT_MS);

        Ok(Self {
            working_ip,
            control_connections: Arc::new(DashMap::new()),
            data_connections: Arc::new(DashMap::new()),
            connect_timeout: Duration::from_millis(connect_timeout_ms),
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

    /// Get the local port number
    ///
    /// Note: TCP sender doesn't bind to a specific port (uses ephemeral ports)
    /// Returns 0 to indicate dynamic port allocation
    pub(crate) fn port(&self) -> u16 {
        0 // TCP sender uses ephemeral ports
    }

    /// Establish a TCP connection to a remote address
    ///
    /// Creates a socket bound to working_ip, connects with timeout,
    /// and configures socket options (nodelay, write timeout).
    ///
    /// # Arguments
    /// * `addr` - The remote socket address to connect to
    ///
    /// # Returns
    /// A configured TcpStream connected to the remote address
    fn establish_connection(&self, addr: &SocketAddr) -> io::Result<TcpStream> {
        // Create socket using socket2 to bidn to working_ip
        let socket2 = Socket2::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;

        // Bind to working_ip with ephemeral port (0)
        let local_ip: IpAddr = self.working_ip.parse().map_err(|e| {
            io::Error::new(ErrorKind::InvalidInput, format!("Invalid working_ip: {}", e))
        })?;
        let local_addr = SocketAddr::new(local_ip, 0);
        socket2.bind(&SockAddr::from(local_addr))?;

        // Set non-blocking for timeout support
        socket2.set_nonblocking(true)?;

        // Attempt to connect
        match socket2.connect(&SockAddr::from(*addr)) {
            Ok(_) => {}
            Err(e)
                if e.raw_os_error() == Some(10035)
                    || e.raw_os_error() == Some(115)
                    || e.kind() == ErrorKind::WouldBlock =>
            {
                // Connection in progress (Windows: WSAEWOULDBLOCK, Unix: EINPROGRESS)
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
                            debug!("TcpSender: Connection to {:?} failed: {:?}", addr, err);
                            return Err(io::Error::new(
                                ErrorKind::ConnectionRefused,
                                format!("Connection failed to {:?}: {:?}", addr, err),
                            ));
                        }

                        Ok(None) => {
                            if socket2.peer_addr().is_ok() {
                                debug!(
                                    "TcpSender: Connection to {:?} established (peer_addr check passed)",
                                    addr
                                );
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

        // Set back to blocking mode
        socket2.set_nonblocking(false)?;

        // Convert to TcpStream
        let stream: TcpStream = socket2.into();

        // Configure socket options (best-effort)
        let _ = stream.set_nodelay(Self::get_nodelay());
        let _ = stream.set_write_timeout(Some(Self::get_write_timeout()));

        Ok(stream)
    }

    /// Connect to a remote control port
    ///
    /// Establishes a TCP connection for control message exchange
    /// (handshake, keepalive, close).
    ///
    /// # Arguments
    /// * `addr` - The remote control port address
    pub(crate) fn connect_control(&self, addr: &SocketAddr) -> io::Result<()> {
        if self.control_connections.contains_key(addr) {
            return Ok(());
        }

        let stream = self.establish_connection(addr)?;

        // consider concurrency issue(establish_connection takes hundreds of mills)
        if let Some(old) = self.control_connections.insert(*addr, stream) {
            debug!("TcpSender: Control connection to {:?} replaced by another thread", addr);
            drop(old);
        }

        debug!("TcpSender: Control connection to {:?} from {}", addr, self.working_ip);
        Ok(())
    }

    /// Connect to a remote data port
    ///
    /// Establishes a TCP connection for RTPS message exchange.
    /// Should only be called after handshake completes and the remote
    /// data port is known.
    ///
    /// # Arguments
    /// * `addr` - The remote data port address
    pub(crate) fn connect_data(&self, addr: &SocketAddr) -> io::Result<()> {
        if self.data_connections.contains_key(addr) {
            return Ok(());
        }

        let stream = self.establish_connection(addr)?;

        // consider concurrency issue(establish_connection takes hundreds of mills)
        if let Some(old) = self.data_connections.insert(*addr, stream) {
            debug!("TcpSender: Data connection to {:?} replaced by another thread", addr);
            drop(old);
        }

        debug!("TcpSender: Data connection to {:?} from {}", addr, self.working_ip);
        Ok(())
    }

    /// Send a control message on the control connection
    ///
    /// # Arguments
    /// * `addr` - The remote control port address
    /// * `msg` - The control message to send
    pub(crate) fn send_control(
        &self,
        addr: &SocketAddr,
        msg: &TcpControlMessage,
    ) -> io::Result<()> {
        if !self.control_connections.contains_key(addr) {
            self.connect_control(addr)?;
        }

        let mut stream = self
            .control_connections
            .get(addr)
            .ok_or_else(|| io::Error::new(ErrorKind::NotConnected, "Control connection not found"))?
            .value()
            .try_clone()?;

        let bytes = msg.serialize()?;
        match write_framed_message(&mut stream, &bytes) {
            Ok(()) => {
                debug!("TcpSender: Sent control message to {:?}", addr);
                Ok(())
            }
            Err(e) => {
                self.disconnect_control(addr);
                Err(e)
            }
        }
    }

    /// Send RTPS data on the data connection
    ///
    /// # Arguments
    /// * `addr` - The remote data port address
    /// * `buffer` - The RTPS message bytes to send
    pub(crate) fn send_data(&self, addr: &SocketAddr, buffer: &[u8]) -> io::Result<usize> {
        if !self.data_connections.contains_key(addr) {
            return Err(io::Error::new(
                ErrorKind::NotConnected,
                format!("No data connection to {:?} (handshake required first)", addr),
            ));
        }

        let mut stream = self
            .data_connections
            .get(addr)
            .ok_or_else(|| io::Error::new(ErrorKind::NotConnected, "Data connection not found"))?
            .value()
            .try_clone()?;

        match write_framed_message(&mut stream, buffer) {
            Ok(()) => {
                debug!("TcpSender: Sent {} bytes data to {:?}", buffer.len(), addr);
                Ok(buffer.len())
            }
            Err(e) => {
                self.disconnect_data(addr);
                Err(e)
            }
        }
    }

    /// Disconnect a control connection
    pub(crate) fn disconnect_control(&self, addr: &SocketAddr) {
        if let Some((_addr, stream)) = self.control_connections.remove(addr) {
            drop(stream);
            debug!("TcpSender: Disconnected control from {:?}", addr);
        }
    }

    /// Disconnect a data connection
    pub(crate) fn disconnect_data(&self, addr: &SocketAddr) {
        if let Some((_addr, stream)) = self.data_connections.remove(addr) {
            drop(stream);
            debug!("TcpSender: Disconnected data from {:?}", addr);
        }
    }

    /// Disconnect all connections (control + data) for a remote address
    pub(crate) fn disconnect(&self, addr: &SocketAddr) {
        self.disconnect_control(addr);
        self.disconnect_data(addr);
    }

    /// Get a mutable reference to a control connection stream
    ///
    /// Used by TcpHandshakeManager to write handshake messages directly
    pub(crate) fn get_control_stream(
        &self,
        addr: &SocketAddr,
    ) -> Option<dashmap::mapref::one::Ref<'_, SocketAddr, TcpStream>> {
        self.control_connections.get(addr)
    }

    /// Get the number of active control connections
    pub(crate) fn control_connection_count(&self) -> usize {
        self.control_connections.len()
    }

    /// Get the number of active data connections
    pub(crate) fn data_connection_count(&self) -> usize {
        self.data_connections.len()
    }

    /// Close all connections
    pub(crate) fn close_all(self) {
        self.control_connections.clear();
        self.data_connections.clear();
        debug!("TcpSender: All connections closed");
    }
}

/// Implementation of Transport trait for TcpSender
///
/// The Transport trait sends RTPS data, so it maps to data connections.
impl Transport for TcpSender {
    fn send(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        self.send_data(addr, data)
    }

    fn send_multicast(&self, domain_id: u32, data: &[u8]) -> io::Result<usize> {
        // TCP does not support multicast
        Err(io::Error::new(ErrorKind::Unsupported, "TCP transport does not support multicast"))
    }

    fn port(&self) -> u16 {
        self.port()
    }

    fn transport_type(&self) -> TransportType {
        TransportType::TCP
    }

    fn close(self) {
        self.close_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::TcpListener;
    use std::thread;

    use crate::rtps::messages::tcp_control_message::HandshakeData;
    use crate::rtps::transport::tcp::framing::read_framed_message;

    #[test]
    fn test_tcp_sender_creation() {
        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        assert_eq!(sender.port(), 0);
        assert_eq!(sender.control_connection_count(), 0);
        assert_eq!(sender.data_connection_count(), 0);
    }

    #[test]
    fn test_tcp_sender_control_send() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = listener.local_addr().unwrap();

        let server_handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let data = read_framed_message(&mut stream).unwrap();
            let msg = TcpControlMessage::deserialize(&data).unwrap();
            assert!(matches!(msg, TcpControlMessage::Keepalive));
        });

        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        sender.send_control(&server_addr, &TcpControlMessage::Keepalive).unwrap();
        assert_eq!(sender.control_connection_count(), 1);
        assert_eq!(sender.data_connection_count(), 0);

        server_handle.join().unwrap();
    }

    #[test]
    fn test_tcp_sender_data_send() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = listener.local_addr().unwrap();

        let server_handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let data = read_framed_message(&mut stream).unwrap();
            assert_eq!(data, b"RTPS data payload");
        });

        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();

        // Data send requires explicit connect_data first (no auto-connect)
        sender.connect_data(&server_addr).unwrap();
        let result = sender.send_data(&server_addr, b"RTPS data payload");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 17);
        assert_eq!(sender.data_connection_count(), 1);
        assert_eq!(sender.control_connection_count(), 0);

        server_handle.join().unwrap();
    }

    #[test]
    fn test_tcp_sender_data_send_without_connect_fails() {
        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        let addr = SocketAddr::from(([127, 0, 0, 1], 9999));

        // send_data without prior connect_data should fail
        let result = sender.send_data(&addr, b"test");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::NotConnected);
    }

    #[test]
    fn test_tcp_sender_disconnect_control() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = listener.local_addr().unwrap();

        let _server_handle = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_secs(1));
        });

        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        sender.connect_control(&server_addr).unwrap();
        assert_eq!(sender.control_connection_count(), 1);

        sender.disconnect_control(&server_addr);
        assert_eq!(sender.control_connection_count(), 0);
    }

    #[test]
    fn test_tcp_sender_disconnect_all() {
        let ctrl_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let ctrl_addr = ctrl_listener.local_addr().unwrap();
        let data_addr = data_listener.local_addr().unwrap();

        let h1 = thread::spawn(move || {
            let _ = ctrl_listener.accept();
            thread::sleep(Duration::from_secs(1));
        });
        let h2 = thread::spawn(move || {
            let _ = data_listener.accept();
            thread::sleep(Duration::from_secs(1));
        });

        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        sender.connect_control(&ctrl_addr).unwrap();
        sender.connect_data(&data_addr).unwrap();

        assert_eq!(sender.control_connection_count(), 1);
        assert_eq!(sender.data_connection_count(), 1);

        // disconnect() removes both types for same addr, but here addrs differ
        // Use specific disconnects
        sender.disconnect_control(&ctrl_addr);
        sender.disconnect_data(&data_addr);

        assert_eq!(sender.control_connection_count(), 0);
        assert_eq!(sender.data_connection_count(), 0);

        let _ = h1.join();
        let _ = h2.join();
    }

    #[test]
    fn test_tcp_sender_multicast_unsupported() {
        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        let result = sender.send_multicast(0, b"test");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::Unsupported);
    }

    #[test]
    fn test_tcp_sender_transport_type() {
        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        assert_eq!(sender.transport_type(), TransportType::TCP);
    }
}
