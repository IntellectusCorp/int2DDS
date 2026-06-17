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

pub(crate) mod error;
pub(crate) mod hybrid_transport_plugin;
pub(crate) mod plugin;
pub(crate) mod port_manager;
pub(crate) mod shm;
pub(crate) mod socket;
pub(crate) mod tcp;
pub(crate) mod tokens;
pub(crate) mod transport_config;
pub(crate) mod udp;

pub(crate) use transport_config::{HybridConfig, TcpConfig, TransportConfig, UdpConfig};

use std::env;
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
