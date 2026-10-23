//! TLS configuration shared by the connection-oriented transports.
//!
//! [`TlsConfig`] describes the CA, certificate, key, peer-verification flag and
//! SNI server name, and turns them into the client and server configurations
//! that TCP and QUIC each hand to their own handshake.

pub mod config;

pub(crate) use config::TlsConfig;
