//! TLS support for the TCP transport.
//!
//! This module provides:
//! - [`TlsConfig`]: high-level TLS configuration (paths to CA / cert / key,
//!   peer-verification flag, SNI server name).
//! - PEM loading helpers that turn the configuration into `rustls`
//!   [`rustls::ClientConfig`] / [`rustls::ServerConfig`] usable by the
//!   TCP transport's sender (client side) and listener (server side).
//!
//! Backed by [`rustls`](https://docs.rs/rustls) — pure-Rust, TLS 1.3 capable,
//! no system-OpenSSL dependency.

pub mod config;

#[allow(unused_imports)]
pub use config::TlsConfig;
