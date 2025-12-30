//! # DCPS Bridge
//!
//! This module provides the bridge between the DDS (DCPS) layer and the RTPS layer.
//!
//! ## Overview
//!
//! The DCPS Bridge connects high-level DDS entities (DataWriter, DataReader, etc.)
//! to their corresponding RTPS entities (RtpsWriter, RtpsReader, etc.), enabling
//! the DDS API to communicate over the RTPS wire protocol.
//!
//! ## Responsibilities
//!
//! - Creating RTPS entities when DDS entities are created
//! - Forwarding data from DDS writers to RTPS writers
//! - Delivering received data from RTPS readers to DDS readers
//! - Managing entity lifecycle and QoS policies

#[allow(clippy::module_inception)]
pub(crate) mod dcps_bridge;
