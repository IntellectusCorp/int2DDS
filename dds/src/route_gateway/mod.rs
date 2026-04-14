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
//! - **AutoRelay** (planned): Automatic discovery of topics and creation
//!   of TopicRelay instances.

pub mod auto_relay;
pub mod topic_relay;

pub use auto_relay::{AutoRelay, TopicFilter};
pub use topic_relay::TopicRelay;
