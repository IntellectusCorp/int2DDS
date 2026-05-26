//! TCP transport.
//!
//! Implements the RTPS TCP transport: a multiplexed listener that demuxes
//! inbound connections into discovery / user-data / control channels, and
//! a tokio-based sender that fans out outbound traffic via per-peer
//! conn_actor pairs.
//!
//! TLS support is provided by the shared [`tls`] submodule, which is also
//! consumed by the DCPS bridge for property-driven configuration.

pub(crate) mod conn_actor;
pub(crate) mod framing;
pub(crate) mod mux_state;
pub(crate) mod protocol;
pub(crate) mod sender;
pub(crate) mod stream;
pub(crate) mod tcp_mux_listener;
pub(crate) mod tcp_sender;
pub(crate) mod tcp_transport_plugin;
pub(crate) mod tls;
