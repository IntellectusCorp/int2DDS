//! # Common RTPS Types and Utilities
//!
//! This module contains fundamental RTPS types, identifiers, and utilities used
//! throughout the RTPS implementation.
//!
//! ## Submodules
//!
//! - [`checksum`] - CRC checksum calculation for message integrity
//! - [`entity_id`] - Entity identifiers (EntityId_t)
//! - [`entity_kind`] - Entity kind enumeration (reader, writer, participant)
//! - [`guid`] - Globally Unique Identifiers (GUID_t)
//! - [`locator`] - Network locator addresses (Locator_t)
//! - [`parameters`] - Parameter list types for discovery
//! - [`rtps_error_code`] - RTPS error codes
//! - [`sequence`] - Sequence number types (SequenceNumber_t)
//! - [`time`] - Time representation (Time_t, Duration_t)
//! - [`types`] - Common type definitions

pub mod checksum;
pub mod entity_id;
pub mod entity_kind;
pub mod guid;
pub mod locator;
pub mod parameters;
pub mod rtps_error_code;
pub mod sequence;
pub mod time;
pub mod types;
