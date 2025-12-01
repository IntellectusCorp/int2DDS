//! Data subscription - Reading and receiving data samples.
//!
//! This module provides the subscription side of DDS communication. Subscribers contain
//! DataReaders that receive typed data samples from topics. DataReaders handle
//! deserialization, filtering, and provide both polling and event-driven access patterns.
//!
//! # Key Components
//!
//! - [`subscriber`] - Container and factory for DataReader objects
//! - [`data_reader`] - Receives data samples from a topic
//! - [`data_sample`] - Container for received samples with metadata
//! - [`sample_info`] - Metadata about samples (state, timestamps, source)
//! - [`data_reader_listener`] - Event callbacks for DataReader status changes
//! - [`subscriber_listener`] - Event callbacks for Subscriber and child DataReader events
//! - [`read_condition`] - Conditions for filtering samples by state
//! - [`query_condition`] - Conditions with content-based SQL filtering
//! - [`qos`] - QoS policies for subscribers and data readers
//!
//! # Typical Usage
//!
//! 1. Create a Subscriber from a DomainParticipant
//! 2. Create a DataReader for a specific Topic
//! 3. Read or take data samples using `read()`, `take()`
//! 4. Access sample metadata through `SampleInfo`

pub mod data_reader;
pub(crate) mod data_reader_history;
pub mod data_reader_listener;
pub mod data_sample;
pub mod qos;
pub mod query_condition;
pub mod read_condition;
pub mod sample_info;
pub mod subscriber;
pub mod subscriber_listener;
