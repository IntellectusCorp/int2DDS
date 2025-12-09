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

pub(crate) mod platform;
pub(crate) mod ring_buffer;
pub(crate) mod shm_listener;
pub(crate) mod shm_sender;

pub(crate) use shm_listener::ShmListener;
pub(crate) use shm_sender::ShmSender;
