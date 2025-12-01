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

use crate::rtps::transport::tcp::framing::write_framed_message;
use crate::rtps::transport::{Transport, TransportType};

/// TCP sender for DDS/RTPS communication
///
/// Manages TCP connections to remote endpoints and sends framed messages.
/// Connections are cached and reused for efficiency.
#[derive(Debug, Clone)]
pub(crate) struct TcpSender {
    /// Working IP address for binding
    working_ip: String,

    /// Connection pool: SocketAddr -> TcpStream
    /// Uses DashMap for lock-free concurrent access
    connections: Arc<DashMap<SocketAddr, TcpStream>>,

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
            connections: Arc::new(DashMap::new()),
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

    /// Connect to a remote address and cache the connection
    ///
    /// This method checks if a connection already exists before attempting to connect.
    /// Thread-safe: only one connection will be established even if called concurrently.
    /// Uses the working_ip to bind the local side of the connection.
    ///
    /// # Arguments
    /// * `addr` - The remote socket address to connect to
    ///
    /// # Returns
    /// * `Ok(())` - Connection established successfully or already exists
    /// * `Err(io::Error)` - Connection failed
    fn connect(&self, addr: &SocketAddr) -> io::Result<()> {
        // Check if connection already exists
        if self.connections.contains_key(addr) {
            return Ok(());
        }

        // Create socket using socket2 to bind to working_ip
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
            Err(e) if e.raw_os_error() == Some(10035) || e.kind() == ErrorKind::WouldBlock => {
                // Connection in progress (Windows: WSAEWOULDBLOCK, Unix: EINPROGRESS)
                // Wait for connection with timeout
                use std::time::Instant;
                let start = Instant::now();

                loop {
                    if start.elapsed() >= self.connect_timeout {
                        return Err(io::Error::new(
                            ErrorKind::TimedOut,
                            format!("Connection timeout to {:?}", addr),
                        ));
                    }

                    // Check if there's an error on the socket
                    match socket2.take_error() {
                        Ok(Some(err)) => {
                            // Connection failed with error
                            debug!("TcpSender: Connection to {:?} failed: {:?}", addr, err);
                            return Err(io::Error::new(
                                ErrorKind::ConnectionRefused,
                                format!("Connection failed to {:?}: {:?}", addr, err),
                            ));
                        }
                        Ok(None) => {
                            // No error - check if connection is established
                            if socket2.peer_addr().is_ok() {
                                // Connection established
                                debug!("TcpSender: Connection to {:?} established (peer_addr check passed)", addr);
                                break;
                            }
                        }
                        Err(e) => {
                            // take_error() itself failed
                            return Err(e);
                        }
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

        // Configure socket options (best-effort, don't fail if these fail)
        // These optimizations are nice-to-have but not critical for functionality
        let nodelay = Self::get_nodelay();
        if let Err(_) = stream.set_nodelay(nodelay) {
            // warn!(
            //     "TcpSender: Failed to set nodelay={} for {:?}: {:?} (continuing anyway)",
            //     nodelay, addr, e
            // );
        }

        let write_timeout = Self::get_write_timeout();
        if let Err(_) = stream.set_write_timeout(Some(write_timeout)) {
            // warn!(
            //     "TcpSender: Failed to set write timeout={:?} for {:?}: {:?} (continuing anyway)",
            //     write_timeout, addr, e
            // );
        }

        // Store connection in the pool (DashMap handles concurrent inserts safely)
        // Use insert, which returns the old value if the key was already present
        if let Some(old_stream) = self.connections.insert(*addr, stream) {
            debug!("TcpSender: Connection to {:?} established by another thread", addr);
            drop(old_stream); // Close the old connection
        }

        debug!("TcpSender: Connected to {:?} from {}", addr, self.working_ip);
        Ok(())
    }

    /// Disconnect from a remote address and remove from connection pool
    ///
    /// # Arguments
    /// * `addr` - The remote socket address to disconnect from
    pub(crate) fn disconnect(&self, addr: &SocketAddr) {
        if let Some((_addr, stream)) = self.connections.remove(addr) {
            drop(stream); // Close connection
            debug!("TcpSender: Disconnected from {:?}", addr);
        }
    }

    /// Send a message to a specific address
    ///
    /// Automatically establishes connection if not already connected.
    /// Uses framing protocol to send the message.
    ///
    /// This method minimizes lock contention by:
    /// 1. Holding the lock only to access the HashMap
    /// 2. Cloning the TcpStream (cheap, uses Arc internally)
    /// 3. Performing network I/O without holding the lock
    ///
    /// # Arguments
    /// * `addr` - The destination socket address
    /// * `buffer` - The data to send
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of bytes sent (excluding framing overhead)
    /// * `Err(io::Error)` - Send failed
    pub(crate) fn send_msg(&self, addr: &SocketAddr, buffer: &[u8]) -> io::Result<usize> {
        // Establish connection if it doesn't exist
        if !self.connections.contains_key(addr) {
            self.connect(addr)?;
        }

        // Get and clone the stream
        // DashMap handles concurrent access without explicit locking
        let mut stream = self
            .connections
            .get(addr)
            .ok_or_else(|| io::Error::new(ErrorKind::NotConnected, "Connection not found"))?
            .value()
            .try_clone()?;

        // Perform network I/O without holding the lock
        // This allows concurrent sends to different addresses
        match write_framed_message(&mut stream, buffer) {
            Ok(()) => {
                debug!("TcpSender: Sent {} bytes to {:?}", buffer.len(), addr);
                Ok(buffer.len())
            }
            Err(e) => {
                // Connection might be broken, remove it
                self.disconnect(addr);
                Err(e)
            }
        }
    }

    /// Get the number of active connections
    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Close all connections
    pub(crate) fn close_all(self) {
        self.connections.clear();
        debug!("TcpSender: All connections closed");
    }
}

/// Implementation of Transport trait for TcpSender
impl Transport for TcpSender {
    fn send(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        self.send_msg(addr, data)
    }

    fn send_multicast(&self, _domain_id: u32, _data: &[u8]) -> io::Result<usize> {
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

    #[test]
    fn test_tcp_sender_creation() {
        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        assert_eq!(sender.port(), 0); // Ephemeral port
        assert_eq!(sender.connection_count(), 0);
    }

    #[test]
    fn test_tcp_sender_connect_and_send() {
        // Start a simple TCP server
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = listener.local_addr().unwrap();

        // Spawn server thread
        let server_handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            // Read framed message
            use crate::rtps::transport::tcp::framing::read_framed_message;
            let data = read_framed_message(&mut stream).unwrap();

            assert_eq!(data, b"Hello, TCP!");
        });

        // Client: Create sender and send message
        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        let result = sender.send_msg(&server_addr, b"Hello, TCP!");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 11); // "Hello, TCP!" length
        assert_eq!(sender.connection_count(), 1);

        server_handle.join().unwrap();
    }

    #[test]
    fn test_tcp_sender_reuses_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = listener.local_addr().unwrap();

        let server_handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            use crate::rtps::transport::tcp::framing::read_framed_message;

            // Receive two messages on same connection
            let msg1 = read_framed_message(&mut stream).unwrap();
            let msg2 = read_framed_message(&mut stream).unwrap();

            assert_eq!(msg1, b"First");
            assert_eq!(msg2, b"Second");
        });

        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();

        // Send first message
        sender.send_msg(&server_addr, b"First").unwrap();
        assert_eq!(sender.connection_count(), 1);

        // Send second message (should reuse connection)
        sender.send_msg(&server_addr, b"Second").unwrap();
        assert_eq!(sender.connection_count(), 1); // Still 1 connection

        server_handle.join().unwrap();
    }

    #[test]
    fn test_tcp_sender_disconnect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = listener.local_addr().unwrap();

        let _server_handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            // Read the message to prevent connection abort
            use crate::rtps::transport::tcp::framing::read_framed_message;
            let _ = read_framed_message(&mut stream);
        });

        let sender = TcpSender::new("127.0.0.1".to_string()).unwrap();
        sender.send_msg(&server_addr, b"Test").unwrap();
        assert_eq!(sender.connection_count(), 1);

        sender.disconnect(&server_addr);
        assert_eq!(sender.connection_count(), 0);
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
