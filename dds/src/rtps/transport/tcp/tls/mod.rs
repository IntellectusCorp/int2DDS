//! TLS support for the TCP transport.
//!
//! This module provides:
//! - [`TlsConfig`]: high-level TLS configuration (paths to CA / cert / key,
//!   peer-verification flag, SNI server name).
//! - PEM loading helpers that turn the configuration into `rustls`
//!   [`rustls::ClientConfig`] / [`rustls::ServerConfig`] usable by the
//!   TCP transport's sender (client side) and listener (server side).
//! - The TLS handshake (`accept_tls_async` / `connect_tls_async`) that turns
//!   a connected TCP stream into a TLS `AsyncConnStream`.
//!
//! Backed by [`rustls`](https://docs.rs/rustls) — pure-Rust, TLS 1.3 capable,
//! no system-OpenSSL dependency.

pub mod config;
pub mod handshake;

#[allow(unused_imports)]
pub use config::TlsConfig;
pub(crate) use handshake::{accept_tls_async, connect_tls_async};
