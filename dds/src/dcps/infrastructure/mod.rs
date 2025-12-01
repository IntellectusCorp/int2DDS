//! Core DDS infrastructure components.
//!
//! This module provides the foundational infrastructure used by all DDS entities,
//! including entity base traits, QoS policies, status reporting, and synchronization
//! primitives like conditions and waitsets.
//!
//! # Key Components
//!
//! ## Entity Framework
//! - [`entity`] - Base traits for all DDS entities
//! - [`domain_entity`] - Marker trait for domain-owned entities
//!
//! ## Quality of Service
//! - [`qos_policy`] - All QoS policy definitions (Reliability, Durability, History, etc.)
//!
//! ## Status and Events
//! - [`status`] - Status types for entity state changes
//! - [`status_condition`] - Conditions triggered by status changes
//!
//! ## Synchronization
//! - [`condition`] - Base condition trait for waitsets
//! - [`guard_condition`] - Application-controlled conditions
//! - [`wait_set`] - Wait for multiple conditions simultaneously
//!
//! ## Internal Components
//! - `history_cache` - Sample storage and history management
//! - `deadline_monitor` - Internal deadline QoS compliance monitoring
//! - `liveliness_monitor` - Internal liveliness QoS compliance monitoring

pub mod condition;
pub(crate) mod deadline_monitor;
pub mod domain_entity;
pub mod entity;
pub mod guard_condition;
pub(crate) mod history_cache;
pub(crate) mod liveliness_monitor;
pub mod qos_policy;
pub mod status;
pub mod status_condition;
pub mod wait_set;
