//! # Transport Layer
//!
//! This module implements the network transport layer for RTPS communication.
//!
//! ## Overview
//!
//! The transport layer provides an abstraction over different network protocols,
//! allowing RTPS to communicate over UDP, TCP, or a hybrid of both.
//!
//! ## Transport Modes
//!
//! - **UDP**: Default mode with multicast discovery and unicast data
//! - **TCP**: Connection-oriented mode for NAT/firewall traversal
//! - **Hybrid**: Combined UDP multicast discovery with TCP unicast
//! - **SHM**: Shared memory for high-performance intra-host communication
//!
//! ## Submodules
//!
//! - [`port_manager`] - RTPS port number calculation and management
//! - [`socket`] - High-level socket abstraction
//! - [`tcp`] - TCP transport implementation
//! - [`udp`] - UDP transport implementation
//! - [`shm`] - Shared memory transport implementation
//!
//! ## Key Traits
//!
//! - [`Transport`] - Common interface for sending data
//! - [`Listener`] - Common interface for receiving data

#![allow(dead_code)]
#![allow(unused_variables)]

pub(crate) mod port_manager;
pub(crate) mod shm;
pub(crate) mod socket;
pub(crate) mod tcp;
pub(crate) mod udp;

use std::env;
use std::io;
use std::net::SocketAddr;
use std::sync::OnceLock;

/// Transport protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::upper_case_acronyms)]
pub enum TransportType {
    /// UDP transport (default)
    #[default]
    UDP,
    /// TCP transport
    TCP,
    /// Hybrid transport (both UDP and TCP simultaneously)
    Hybrid,
    /// Shared Memory transport
    SHM,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportType::UDP => write!(f, "udp"),
            TransportType::TCP => write!(f, "tcp"),
            TransportType::Hybrid => write!(f, "hybrid"),
            TransportType::SHM => write!(f, "shm"),
        }
    }
}

impl std::str::FromStr for TransportType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "udp" => Ok(TransportType::UDP),
            "tcp" => Ok(TransportType::TCP),
            "hybrid" => Ok(TransportType::Hybrid),
            "shm" => Ok(TransportType::SHM),
            _ => Err(format!(
                "Invalid transport type: {}. Valid options are 'udp', 'tcp', 'hybrid', or 'shm'",
                s
            )),
        }
    }
}

/// Cached transport type - read once from environment variable
static TRANSPORT_TYPE: OnceLock<TransportType> = OnceLock::new();

/// Get the transport type from environment variable INT2DDS_TRANSPORT
/// Defaults to UDP if not set or invalid
/// The value is cached after the first call
pub fn get_transport_type() -> TransportType {
    *TRANSPORT_TYPE.get_or_init(|| {
        let transport =
            env::var("INT2DDS_TRANSPORT").ok().and_then(|val| val.parse().ok()).unwrap_or_default();
        log::info!("[Transport] Using transport type: {:?}", transport);
        transport
    })
}

/// Transport trait for abstracting network transport mechanisms (UDP, TCP, etc.)
///
/// This trait defines the common interface that all transport implementations must provide.
/// It allows the DDS layer to work with different transport protocols without knowing
/// the specific implementation details.
pub(crate) trait Transport: Send + Sync {
    /// Send data to a specific address
    ///
    /// # Arguments
    /// * `addr` - The destination socket address
    /// * `data` - The data buffer to send
    ///
    /// # Returns
    /// Number of bytes sent or an IO error
    fn send(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize>;

    /// Send discovery message via multicast
    ///
    /// Note: TCP does not support multicast, so TCP implementations should return an error.
    ///
    /// # Arguments
    /// * `domain_id` - The DDS domain ID (used to determine multicast port)
    /// * `data` - The data buffer to send
    ///
    /// # Returns
    /// Number of bytes sent or an IO error
    fn send_multicast(&self, domain_id: u32, data: &[u8]) -> io::Result<usize>;

    /// Send data to a specific logical port on a remote TCP endpoint.
    ///
    /// In TCP single-port mode, the physical address is the remote's single listening port
    /// (e.g., 7400), and the logical_port identifies which RTPS channel (discovery or user data)
    /// the data should be routed to one the remote participant.
    ///
    /// Default implementation falls back to `send()`, ignoring the logical port.
    /// TCP transport overrides this with BIND-based connection routing.
    ///
    /// # Arguments
    /// * `addr` - The remote physical address (single TCP listening port)
    /// * `logical_port` - The RTPS logical port number (e.g., 7410 for discovery, 7411 for user data)
    /// * `data` - The data buffer to send
    fn send_to_logical_port(
        &self,
        addr: &SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<usize> {
        self.send(addr, data)
    }

    /// Send data to a remote endpoint's discovery channel.
    /// For TCP, this routes to the peer's discovery logical port.
    /// For UDP/SHM, this is equivalent to `send()`.
    fn send_to_discovery(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        self.send(addr, data)
    }

    /// Send data to a remote endpoint's user data channel.
    /// For TCP, this routes to the peer's user data logical port.
    /// For UDP/SHM, this is equivalent to `send()`.
    fn send_to_user_data(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        self.send(addr, data)
    }

    /// Get the local port number used by this transport
    fn port(&self) -> u16;

    /// Get the transport type (UDP or TCP)
    fn transport_type(&self) -> TransportType;

    /// Close the transport and release resources
    fn close(self);
}

/// Listener trait for abstracting network listeners
///
/// This trait provides access to the underlying socket for event-driven I/O
/// using mio. Different transport types return different socket types.
pub(crate) trait Listener: Send {
    /// Get a mutable reference to the UDP socket (if applicable)
    ///
    /// Returns Some for UDP listeners, None for TCP listeners
    fn socket_udp(&mut self) -> Option<&mut mio::net::UdpSocket>;

    /// Get a mutable reference to the TCP listener socket (if applicable)
    ///
    /// Returns Some for TCP listeners, None for UDP listeners
    fn socket_tcp(&mut self) -> Option<&mut mio::net::TcpListener>;

    /// Get the local port number
    fn port(&self) -> u16;

    /// Close the listener and release resources
    fn close(&mut self);
}

/// Transport sender enum that can hold either UDP, TCP, or SHM sender
///
/// This enum allows the Socket struct to work with different transport types
/// without knowing the specific implementation at compile time.
#[derive(Debug)]
pub(crate) enum TransportSender {
    /// UDP transport sender
    Udp(udp::UdpSender),
    /// TCP transport sender
    Tcp(tcp::TcpSender),
    /// Shared Memory transport sender
    Shm(shm::ShmSender),
}

impl TransportSender {
    /// Force close the socket even when there are other Arc references.
    /// This can be called with &self, unlike close() which requires ownership.
    pub(crate) fn force_close(&self) {
        match self {
            TransportSender::Udp(sender) => sender.force_close(),
            TransportSender::Tcp(_sender) => {
                // TCP sender doesn't support force_close yet
                log::warn!("[TransportSender] force_close not implemented for TCP");
            }
            TransportSender::Shm(sender) => {
                sender.force_close();
            }
        }
    }
}

impl Transport for TransportSender {
    fn send(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        match self {
            TransportSender::Udp(sender) => sender.send(addr, data),
            TransportSender::Tcp(sender) => sender.send(addr, data),
            TransportSender::Shm(sender) => sender.send(addr, data),
        }
    }

    fn send_multicast(&self, domain_id: u32, data: &[u8]) -> io::Result<usize> {
        match self {
            TransportSender::Udp(sender) => sender.send_multicast(domain_id, data),
            TransportSender::Tcp(sender) => sender.send_multicast(domain_id, data),
            TransportSender::Shm(sender) => sender.send_multicast(domain_id, data),
        }
    }

    fn send_to_logical_port(
        &self,
        addr: &SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<usize> {
        match self {
            TransportSender::Udp(sender) => sender.send(addr, data), // UDP ignores logical port
            TransportSender::Tcp(sender) => sender.send_to_logical_port(addr, logical_port, data),
            TransportSender::Shm(sender) => sender.send(addr, data), // SHM ignores logical port
        }
    }

    fn port(&self) -> u16 {
        match self {
            TransportSender::Udp(sender) => sender.port(),
            TransportSender::Tcp(sender) => sender.port(),
            TransportSender::Shm(sender) => sender.port(),
        }
    }

    fn transport_type(&self) -> TransportType {
        match self {
            TransportSender::Udp(sender) => sender.transport_type(),
            TransportSender::Tcp(sender) => sender.transport_type(),
            TransportSender::Shm(sender) => sender.transport_type(),
        }
    }

    fn close(self) {
        match self {
            TransportSender::Udp(sender) => sender.close(),
            TransportSender::Tcp(sender) => sender.close(),
            TransportSender::Shm(sender) => sender.close(),
        }
    }
}
