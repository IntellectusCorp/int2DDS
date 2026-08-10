//! # Route Gateway
//!
//! A gateway service that bridges DDS communication between a LAN (UDP) and a WAN (TCP).
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────── Route Gateway ─────────────┐
//! │                                         │
//! │  ┌ LocalNode ─┐    ┌─ RemoteNode ─┐    │
//! │  │ UDP, dom 0  │    │ TCP, dom 1    │   │
//! │  │ [Reader]────┼─┐  │  ┌──→[Writer] │   │
//! │  │ [Writer]←───┼─┤  │  ├───[Reader] │   │
//! │  └─────────────┘ │  └──┼─────────────┘   │
//! │             ┌────▼────▼────┐             │
//! │             │  TopicRelay  │             │
//! │             │ (take→write) │             │
//! │             └──────────────┘             │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Components
//!
//! - **TopicRelay**: Bidirectional forwarding of a single topic between
//!   LocalNode and RemoteNode using DynamicData.
//! - **AutoRelay**: Automatic discovery of topics and creation of TopicRelay
//!   instances based on a topic-name filter.
//! - **Config**: JSON configuration loader for the `route_gateway` binary.
//!
//! ## Supported WAN transport
//!
//! Currently only **TCP** is supported as the WAN transport for RemoteNode.
//! UDP/Hybrid/SHM are accepted by [`config::NodeConfig`] for forward
//! compatibility but are not part of the validated Route Gateway scenario.

pub mod auto_relay;
pub mod config;
pub mod relay;
pub mod topic_relay;

pub use auto_relay::{AutoRelay, QosResolver, TopicFilter};
pub use config::{AutoRelayConfig, NodeConfig, RouteGatewayConfig, TopicRelayRule};
pub use relay::{LinkRole, RelayConfig, RelayGateway};
pub use topic_relay::{TopicRelay, TopicRelayQos};
