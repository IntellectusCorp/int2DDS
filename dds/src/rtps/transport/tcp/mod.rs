//! TCP transport.
//!
//! Implements a single-listener RTPS TCP transport. Every accepted connection
//! has a generic frame reader; the explicit frame kind routes payloads to the
//! discovery or user-data listener. Outbound traffic uses at most one lazy
//! connection per peer and traffic kind.
//!
//! TLS support is provided by the shared [`tls`] submodule, which is also
//! consumed by the DCPS bridge for property-driven configuration.

pub(crate) mod connection_registry;
pub(crate) mod connection_tasks;
pub(crate) mod framing;
pub(crate) mod stream;
pub(crate) mod tcp_mux_listener;
pub(crate) mod tcp_sender;
pub(crate) mod tcp_transport_plugin;
pub(crate) mod tls;
pub(crate) mod write_state;
