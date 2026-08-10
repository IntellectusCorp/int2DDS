//! Transparent RTPS relay between two networks.
//!
//! ```text
//!   network A                    network B
//!   ┌──────────┐   ┌────────┐   ┌────────┐   ┌──────────┐
//!   │ Writer   │──▶│ gateway│══▶│ gateway│──▶│ Reader   │
//!   └──────────┘   └────────┘   └────────┘   └──────────┘
//!        └──────── matched RTPS endpoints, end to end ────────┘
//! ```
//!
//! No sample is ever republished. The gateway rewrites the addresses inside
//! each SPDP announcement it forwards, so a participant on network A discovers
//! one on network B as if it sat at the gateway's address. Every message after
//! that is forwarded by destination GUID prefix alone, which leaves QoS,
//! retransmission and back pressure where they belong: between the original
//! writer and the final reader.

mod config;
mod discovery_rewrite;
mod gateway;
mod link;
mod peer_policy;
mod peer_table;
mod rtps_scan;
mod stats;

pub use config::{LinkRole, RelayConfig};
pub use gateway::RelayGateway;
pub use peer_policy::Ipv4Prefix;
pub use stats::LinkStats;
