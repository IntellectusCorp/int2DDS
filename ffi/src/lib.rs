//! # int2dds FFI (Foreign Function Interface)
//!
//! This crate provides C-compatible bindings for the int2dds DDS implementation.
//!
//! ## Overview
//!
//! The FFI layer exposes a C API that can be used to:
//! - Integrate int2dds with C/C++ applications
//! - Use DDS functionality from any language with C FFI support
//!
//! ## Architecture
//!
//! The FFI uses opaque pointer handles for all DDS entities. Memory is managed
//! through Arc reference counting on the Rust side, with explicit create/delete
//! functions for the C API.
//!
//! ## Modules
//!
//! - [`error`] - Error codes and conversion utilities
//! - [`types`] - Opaque pointer types for FFI handles
//! - [`context`] - DomainParticipantFactory management
//! - [`env`] - Environment-variable–driven configuration helpers
//! - [`participant`] - DomainParticipant management
//! - [`topic`] - Topic creation and management
//! - [`publisher`] - Publisher and DataWriter functions
//! - [`subscriber`] - Subscriber and DataReader functions
//! - [`qos`] - QoS policy configuration
//! - [`condition`] - GuardCondition for manual triggering
//! - [`status_condition`] - StatusCondition for status-based waiting
//! - [`waitset`] - WaitSet for condition-based waiting
//! - [`status`] - FFI-compatible status structures
//! - [`listener`] - Callback-based listener support
//!
//! ## Usage Example (C)
//!
//! ```c
//! Int2DdsParticipantFactory* factory;
//! Int2DdsParticipant* participant;
//!
//! int2dds_domain_participant_factory_get_instance(&factory);
//! int2dds_create_participant(factory, NULL, 0, &participant);
//!
//! // ... use DDS entities ...
//!
//! int2dds_delete_participant(participant);
//! int2dds_domain_participant_factory_finalize(factory);
//! ```

#[macro_use]
pub mod error;

pub mod abi;
pub mod c_layout;
pub mod condition;
pub mod config;
pub mod context;
pub mod data;
pub mod discovery;
pub mod dynamic;
pub mod dynamic_value;
pub mod env;
pub mod last_error;
pub mod listener;
pub mod participant;
pub mod publisher;
pub mod qos;
pub mod raw_type_support;
pub mod read_condition;
pub mod status;
pub mod status_condition;
pub mod subscriber;
pub mod topic;
pub mod type_info;
pub mod types;
pub mod waitset;
pub mod xml;
