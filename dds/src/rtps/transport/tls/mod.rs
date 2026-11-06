//! TLS configuration shared by the connection-oriented transports.
//!
//! [`TlsConfig`] describes the CA, certificate, key, peer-verification flag and
//! SNI server name, and turns them into the client and server configurations
//! that TCP and QUIC each hand to their own handshake.

pub(crate) mod client;
pub mod config;
pub(crate) mod identity;
pub(crate) mod peer_lists;
pub(crate) mod policy;
pub(crate) mod rejection;
pub(crate) mod verifier;

#[cfg(test)]
pub(crate) mod test_pki;
#[cfg(test)]
mod tests;

pub(crate) use config::TlsConfig;
