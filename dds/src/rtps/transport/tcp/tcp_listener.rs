#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr};

use log::{debug, error, warn};
use mio::net::{TcpListener as MioTcpListener, TcpStream as MioTcpStream};
use mio::{Interest, Registry, Token};

use crate::rtps::transport::tcp::framing::FramedReader;
use crate::rtps::transport::Listener;

/// TCP listener for DDS/RTPS communication
///
/// Manages incoming TCP connections and receives framed messages.
/// Works with mio for event-driven I/O.
#[derive(Debug)]
pub(crate) struct TcpListener {
    /// Port number this listener is bound to
    port: u16,

    /// The mio TCP listener socket
    listener: Option<MioTcpListener>,

    /// Active connections: SocketAddr -> TcpStream
    connections: HashMap<SocketAddr, MioTcpStream>,

    /// Connection token counter
    next_token: usize,

    /// Token to SocketAddr mapping
    token_to_addr: HashMap<Token, SocketAddr>,

    /// Framed readers for each connection (stateful message parsing)
    framed_readers: HashMap<SocketAddr, FramedReader>,
}

impl TcpListener {
    /// Create a new TcpListener
    ///
    /// # Arguments
    /// * `port` - The port to bind to
    ///
    /// # Returns
    /// A new TcpListener instance or an error
    pub(crate) fn new(port: u16) -> io::Result<Self> {
        debug!("TcpListener: Creating listener on port {}", port);

        // Bind to all interfaces (0.0.0.0)
        let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
        let listener = MioTcpListener::bind(addr)?;

        // Get the actual bound port (important when port=0 for ephemeral port)
        let actual_port = listener.local_addr()?.port();
        debug!("TcpListener: Successfully bound to {:?}", listener.local_addr()?);

        Ok(Self {
            port: actual_port,
            listener: Some(listener),
            connections: HashMap::new(),
            next_token: 65536, // Start from 65536 to avoid collision with port-based tokens (max port = 65535)
            token_to_addr: HashMap::new(),
            framed_readers: HashMap::new(),
        })
    }

    /// Accept a new incoming connection and register it with the poll
    ///
    /// The connection is stored in the internal HashMap and can be accessed
    /// later using `get_connection_mut()`.
    ///
    /// # Arguments
    /// * `registry` - Optional mio Registry to register the new connection
    ///
    /// # Returns
    /// * `Ok(Some(addr))` - New connection accepted, returns the remote address
    /// * `Ok(None)` - No pending connection (would block)
    /// * `Err(io::Error)` - Accept failed
    pub(crate) fn accept(&mut self, registry: Option<&Registry>) -> io::Result<Option<SocketAddr>> {
        let listener = match &self.listener {
            Some(l) => l,
            None => {
                return Err(io::Error::new(ErrorKind::NotConnected, "Listener not initialized"))
            }
        };

        match listener.accept() {
            Ok((mut stream, addr)) => {
                debug!("TcpListener: Accepted connection from {:?}", addr);

                // Configure socket options
                if let Err(e) = stream.set_nodelay(true) {
                    warn!("TcpListener: Failed to set nodelay for {:?}: {:?}", addr, e);
                }

                // Register the connection with poll if registry is provided
                if let Some(reg) = registry {
                    let token = Token(self.next_token);
                    self.next_token += 1;

                    if let Err(e) = reg.register(&mut stream, token, Interest::READABLE) {
                        error!("TcpListener: Failed to register connection {:?}: {:?}", addr, e);
                        return Err(e);
                    }

                    self.token_to_addr.insert(token, addr);
                    debug!("TcpListener: Registered connection {:?} with token {:?}", addr, token);
                }

                // Store the connection in the HashMap
                self.connections.insert(addr, stream);

                // Create a framed reader for this connection
                self.framed_readers.insert(addr, FramedReader::new());

                Ok(Some(addr))
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                // No pending connection, this is normal for non-blocking socket
                Ok(None)
            }
            Err(e) => {
                error!("TcpListener: Accept failed: {:?}", e);
                Err(e)
            }
        }
    }

    /// Get a mutable reference to a connection by address
    ///
    /// # Arguments
    /// * `addr` - The remote address
    ///
    /// # Returns
    /// A mutable reference to the TcpStream if it exists
    pub(crate) fn get_connection_mut(&mut self, addr: &SocketAddr) -> Option<&mut MioTcpStream> {
        self.connections.get_mut(addr)
    }

    /// Remove a connection
    ///
    /// # Arguments
    /// * `addr` - The remote address to disconnect
    pub(crate) fn remove_connection(&mut self, addr: &SocketAddr) {
        if let Some(stream) = self.connections.remove(addr) {
            drop(stream);
            self.framed_readers.remove(addr);
            debug!("TcpListener: Removed connection from {:?}", addr);
        }
    }

    /// Get the number of active connections
    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Get all connection addresses
    pub(crate) fn connection_addrs(&self) -> Vec<SocketAddr> {
        self.connections.keys().copied().collect()
    }

    /// Get SocketAddr from Token
    ///
    /// # Arguments
    /// * `token` - The mio Token
    ///
    /// # Returns
    /// The SocketAddr associated with the token, if it exists
    pub(crate) fn get_addr_from_token(&self, token: Token) -> Option<SocketAddr> {
        self.token_to_addr.get(&token).copied()
    }

    /// Check if a token belongs to a TCP connection
    ///
    /// # Arguments
    /// * `token` - The mio Token to check
    ///
    /// # Returns
    /// true if the token is registered as a connection token
    pub(crate) fn is_connection_token(&self, token: Token) -> bool {
        self.token_to_addr.contains_key(&token)
    }

    /// Get the port number
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Read a framed message from a connection (stateful, handles partial reads)
    ///
    /// # Arguments
    /// * `addr` - The remote address to read from
    ///
    /// # Returns
    /// * `Ok(Some(message))` - Complete message read
    /// * `Ok(None)` - Incomplete message, need more data
    /// * `Err(io::Error)` - Read failed
    pub(crate) fn read_framed_message(&mut self, addr: &SocketAddr) -> io::Result<Option<Vec<u8>>> {
        let stream = self.connections.get_mut(addr).ok_or_else(|| {
            io::Error::new(ErrorKind::NotConnected, format!("No connection for {:?}", addr))
        })?;

        let reader = self.framed_readers.get_mut(addr).ok_or_else(|| {
            io::Error::new(ErrorKind::NotConnected, format!("No framed reader for {:?}", addr))
        })?;

        reader.read_message(stream)
    }

    /// Close the listener and all connections
    pub(crate) fn close(&mut self) {
        self.connections.clear();
        self.framed_readers.clear();
        self.listener.take();
        debug!("TcpListener: Closed (port {})", self.port);
    }

    /// Get a reference to the mio TcpListener
    pub(crate) fn socket(&self) -> Option<&MioTcpListener> {
        self.listener.as_ref()
    }

    /// Get a mutable reference to the mio TcpListener
    pub(crate) fn socket_mut(&mut self) -> Option<&mut MioTcpListener> {
        self.listener.as_mut()
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        self.close();
    }
}

/// Implementation of Listener trait for TcpListener
impl Listener for TcpListener {
    fn socket_udp(&mut self) -> Option<&mut mio::net::UdpSocket> {
        // TCP listener doesn't have a UDP socket
        None
    }

    fn socket_tcp(&mut self) -> Option<&mut MioTcpListener> {
        self.listener.as_mut()
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn close(&mut self) {
        TcpListener::close(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream as StdTcpStream;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_tcp_listener_creation() {
        let listener = TcpListener::new(0).unwrap(); // port 0 = random port
        assert!(listener.port() > 0);
        assert_eq!(listener.connection_count(), 0);
    }

    #[test]
    fn test_tcp_listener_accept() {
        let mut listener = TcpListener::new(0).unwrap();
        let port = listener.port();

        // Spawn a client thread
        let client_handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100)); // Wait for listener to be ready
            StdTcpStream::connect(format!("127.0.0.1:{}", port)).unwrap()
        });

        // Accept connection (with timeout)
        let start = std::time::Instant::now();
        let mut accepted = false;

        while start.elapsed() < Duration::from_secs(2) {
            if let Ok(Some(addr)) = listener.accept(None) {
                debug!("Accepted connection from {:?}", addr);
                accepted = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert!(accepted, "Failed to accept connection");
        assert_eq!(listener.connection_count(), 1);

        client_handle.join().unwrap();
    }

    #[test]
    fn test_tcp_listener_multiple_connections() {
        let mut listener = TcpListener::new(0).unwrap();
        let port = listener.port();

        let client_handles: Vec<_> = (0..3)
            .map(|i| {
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(50 * i));
                    StdTcpStream::connect(format!("127.0.0.1:{}", port)).unwrap()
                })
            })
            .collect();

        // Accept all connections
        let start = std::time::Instant::now();
        let mut accepted_count = 0;

        while start.elapsed() < Duration::from_secs(2) && accepted_count < 3 {
            if let Ok(Some(_addr)) = listener.accept(None) {
                accepted_count += 1;
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(accepted_count, 3);
        assert_eq!(listener.connection_count(), 3);

        for handle in client_handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_tcp_listener_remove_connection() {
        let mut listener = TcpListener::new(0).unwrap();
        let port = listener.port();

        let _client = StdTcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();

        // Accept connection
        let start = std::time::Instant::now();
        let mut addr = None;

        while start.elapsed() < Duration::from_secs(1) {
            if let Ok(Some(accepted_addr)) = listener.accept(None) {
                addr = Some(accepted_addr);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let addr = addr.expect("Failed to accept connection");
        assert_eq!(listener.connection_count(), 1);

        listener.remove_connection(&addr);
        assert_eq!(listener.connection_count(), 0);
    }

    #[test]
    fn test_tcp_listener_close() {
        let mut listener = TcpListener::new(0).unwrap();
        let port = listener.port();

        let _client = StdTcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();

        // Accept connection
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(1) {
            if listener.accept(None).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(listener.connection_count(), 1);

        listener.close();
        assert_eq!(listener.connection_count(), 0);
        assert!(listener.socket().is_none());
    }
}
