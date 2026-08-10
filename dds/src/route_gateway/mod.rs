//! # Route Gateway
//!
//! A transparent RTPS relay between two networks. See [`relay`] for what it
//! does and why it does not republish anything.

pub mod relay;

pub use relay::{Ipv4Prefix, LinkRole, LinkStats, RelayConfig, RelayGateway};
