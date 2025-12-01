//! Data Distribution Service (DDS) API implementation.
//!
//! This module provides an implementation of the Object Management Group (OMG)
//! Data Distribution Service (DDS) specification for real-time, scalable, and reliable
//! data exchange.
//!
//! # Architecture Overview
//!
//! The DDS API is organized into several key modules:
//!
//! - [`domain`] - Domain participants and factory for creating DDS entities
//! - [`topic`] - Topics that define data types and communication channels
//! - [`publication`] - Publishers and DataWriters for sending data
//! - [`subscription`] - Subscribers and DataReaders for receiving data
//! - [`infrastructure`] - Core infrastructure (QoS, status, conditions, entities)
//! - [`core`] - Fundamental types (errors, time, type definitions)

pub mod core;
pub mod domain;
pub mod infrastructure;
pub mod publication;
pub mod subscription;
pub mod topic;
