//! Shared Memory (SHM) Transport Module
//!
//! This module implements shared memory transport for high-performance
//! intra-host communication in DDS.
//!
//! ## Overview
//!
//! SHM transport provides zero-copy data transfer between DDS participants
//! on the same host, significantly reducing latency and CPU overhead
//! compared to network-based transports.
//!
//! ## Components
//!
//! - [`ShmSender`] - Sends user data via shared memory
//! - [`ShmListener`] - Receives user data via shared memory
//! - [`ring_buffer`] - Lock-free ring buffer for message passing
//! - [`platform`] - Platform-specific shared memory implementations

pub(crate) mod config;
pub(crate) mod layout;
pub(crate) mod notify;
pub(crate) mod participant_slot;
pub(crate) mod peer_map;
pub(crate) mod platform;
pub(crate) mod pool;
pub(crate) mod pool_owner;
pub(crate) mod pool_reader;
pub(crate) mod registry;
pub(crate) mod registry_segment;
pub(crate) mod ring;
pub(crate) mod ring_buffer;
pub(crate) mod runtime;
pub(crate) mod segment;
pub(crate) mod shm_listener;
pub(crate) mod shm_sender;
pub(crate) mod shm_transport_plugin;
pub(crate) mod slot_ref;

#[cfg(test)]
mod integration_test;

#[cfg(test)]
pub(crate) mod test_region;
