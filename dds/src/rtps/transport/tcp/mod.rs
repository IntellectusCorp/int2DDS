//! TCP transport.
//!
//! Implements a single-listener RTPS TCP transport. One poll loop reads every
//! accepted connection and recovers each frame's kind from its content.
//! Outbound traffic uses at most one lazy connection per peer and traffic kind.
//!
//! TLS support is provided by the shared [`tls`] submodule, which is also
//! consumed by the DCPS bridge for property-driven configuration.

pub(crate) mod connection_registry;
pub(crate) mod framing;
pub(crate) mod stream;
pub(crate) mod sync_connection;
pub(crate) mod tcp_listener;
pub(crate) mod tcp_sender;
pub(crate) mod tcp_transport_plugin;
pub(crate) mod tls;
