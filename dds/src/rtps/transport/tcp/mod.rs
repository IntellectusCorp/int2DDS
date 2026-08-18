//! TCP transport.
//!
//! Implements the RTPS TCP transport: a multiplexed listener that demuxes
//! inbound connections into discovery / user-data / control channels, and
//! a tokio-based sender that fans out outbound traffic via per-peer
//! reader/writer task pairs.
//!
//! TLS support is provided by the shared [`tls`] submodule, which is also
//! consumed by the DCPS bridge for property-driven configuration.

pub(crate) mod connection_registry;
pub(crate) mod connection_tasks;
pub(crate) mod framing;
pub(crate) mod protocol;
pub(crate) mod self_delivery;
pub(crate) mod stream;
pub(crate) mod tcp_mux_listener;
pub(crate) mod tcp_sender;
pub(crate) mod tcp_transport_plugin;
pub(crate) mod tls;
pub(crate) mod write_state;
