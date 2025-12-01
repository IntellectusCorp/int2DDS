//! Data publication - Writing and sending data samples.
//!
//! This module provides the publication side of DDS communication. Publishers contain
//! DataWriters that send typed data samples to topics. DataWriters handle serialization,
//! QoS enforcement, and reliable delivery.
//!
//! # Key Components
//!
//! - [`publisher`] - Container and factory for DataWriter objects
//! - [`data_writer`] - Sends data samples to a topic
//! - [`data_writer_listener`] - Event callbacks for DataWriter status changes
//! - [`publisher_listener`] - Event callbacks for Publisher and child DataWriter events
//! - [`qos`] - QoS policies for publishers and data writers
//!
//! # Typical Usage
//!
//! 1. Create a Publisher from a DomainParticipant
//! 2. Create a DataWriter for a specific Topic
//! 3. Write data samples using `write()` or manage instances with `register_instance()`, `dispose()`, etc.

pub mod data_writer;
pub(crate) mod data_writer_history;
pub mod data_writer_listener;
pub mod publisher;
pub mod publisher_listener;
pub mod qos;
