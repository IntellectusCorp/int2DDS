//! # RTPS (Real-Time Publish-Subscribe) Protocol Implementation
//!
//! This module implements the RTPS wire protocol as defined in the OMG RTPS specification.
//! RTPS is the underlying protocol that enables DDS (Data Distribution Service) interoperability
//! between different vendor implementations.
//!
//! ## Architecture Overview
//!
//! The RTPS layer sits between the DDS API layer and the transport layer:
//!
//! ```text
//! ┌─────────────────────────────┐
//! │     DDS Application API     │
//! ├─────────────────────────────┤
//! │     RTPS Protocol Layer     │  ← This module
//! ├─────────────────────────────┤
//! │   Transport (UDP/TCP)       │
//! └─────────────────────────────┘
//! ```
//!
//! ## Submodules
//!
//! - [`builtin`] - Built-in endpoints for discovery (SPDP, SEDP)
//! - [`common`] - Common RTPS types and utilities
//! - [`dcps_bridge`] - Bridge connecting DCPS entities to RTPS entities
//! - [`entities`] - RTPS entities (RtpsParticipant, RtpsReader, RtpsWriter)
//! - [`logic`] - Core RTPS protocol logic and state machines
//! - [`messages`] - RTPS message structures and serialization
//! - [`service`] - RTPS services and background tasks
//! - [`task`] - Task management for RTPS operations
//! - [`transport`] - Transport layer implementations (UDP, TCP, Hybrid)
//!
//! ## Key Concepts
//!
//! ### Discovery
//!
//! RTPS uses a two-phase discovery protocol:
//! - **SPDP (Simple Participant Discovery Protocol)**: Discovers remote participants
//! - **SEDP (Simple Endpoint Discovery Protocol)**: Discovers remote readers and writers
//!
//! ### Reliability
//!
//! RTPS supports two reliability levels:
//! - **Best-Effort**: Fire-and-forget delivery with no acknowledgments
//! - **Reliable**: Guaranteed delivery with acknowledgments and retransmissions
//!
//! ### Transport Modes
//!
//! - **UDP**: Default mode using multicast for discovery and unicast for user data
//! - **TCP**: Connection-oriented mode for NAT traversal and firewalled environments
//! - **Hybrid**: Combines UDP multicast discovery with TCP unicast for reliable delivery

pub(crate) mod builtin;
pub mod common;
pub(crate) mod dcps_bridge;
pub(crate) mod entities;
pub(crate) mod logic;
pub(crate) mod messages;
pub(crate) mod service;
pub(crate) mod task;
pub(crate) mod transport;
