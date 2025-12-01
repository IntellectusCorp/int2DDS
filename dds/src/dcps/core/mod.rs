//! Core DDS types and utilities.
//!
//! This module contains fundamental types used throughout the DDS API, including
//! error handling, time representation, and basic type definitions.
//!
//! # Modules
//!
//! - [`error`] - Error types and result type aliases
//! - [`time`] - Duration and timestamp types for QoS policies and timeouts
//! - [`types`] - Basic type aliases and constants (DomainId, ParticipantId, etc.)

pub mod error;
pub mod time;
pub mod types;
