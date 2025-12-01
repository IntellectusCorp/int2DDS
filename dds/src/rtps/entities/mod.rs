//! # RTPS Entities
//!
//! This module defines the core RTPS entities that participate in data distribution.
//!
//! ## Overview
//!
//! RTPS entities are the building blocks of the protocol. Each entity has a unique
//! GUID and participates in the publish-subscribe communication pattern.
//!
//! ## Submodules
//!
//! - [`endpoint`] - Base endpoint functionality for readers and writers
//! - [`entity`] - Base RTPS entity trait and common functionality
//! - [`group`] - Publisher and subscriber group management
//! - [`history`] - History cache for storing data samples
//! - [`participant`] - RTPS participant (represents a DDS DomainParticipant)
//! - [`qos`] - QoS policies for RTPS entities
//! - [`reader`] - RTPS reader (stateful and stateless)
//! - [`writer`] - RTPS writer (stateful and stateless)

pub(crate) mod endpoint;
pub(crate) mod entity;
pub(crate) mod group;
pub(crate) mod history;
pub(crate) mod participant;
pub(crate) mod qos;
pub(crate) mod reader;
pub(crate) mod writer;
