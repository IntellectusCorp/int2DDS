//! Core type definitions and constants for DDS.
//!
//! This module provides fundamental type aliases and constants used throughout the DDS API.
//! It includes domain and participant identifiers, resource limit constants, and internal
//! state types.

pub const LENGTH_UNLIMITED: i32 = -1;
pub type DomainId = i32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InstanceState {
    Registered,
    Unregistered,
    Disposed,
}
