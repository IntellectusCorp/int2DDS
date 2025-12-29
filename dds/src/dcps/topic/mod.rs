//! Topics - Data type definitions and communication channels.
//!
//! This module provides topic management for DDS. Topics define the data type and name
//! for a communication channel, linking publishers and subscribers that share the same
//! topic name and compatible data types.
//!
//! # Key Components
//!
//! - [`topic`] - The main topic type that associates a name with a data type
//! - [`type_support`] - Type registration and serialization infrastructure (includes `DdsType` trait)
//! - [`topic_description`] - Common interface for topic-like entities
//! - [`topic_listener`] - Event callbacks for topic status changes
//! - [`content_filtered_topic`] - Topics with SQL-based content filtering
//! - [`multi_topic`] - Topics that aggregate multiple topics (not supported)
//! - [`qos`] - QoS policies for topics
//!
//! # Type Support
//!
//! Use the `#[derive(DdsType)]` macro to make your Rust types compatible with DDS:
//!
//! ```
//! use int2dds::topic::type_support::DdsType;
//!
//! #[derive(DdsType)]
//! #[dds_type(crate_path = "int2dds")]
//! struct MyData {
//!     #[dds(key)]
//!     id: u32,
//!     message: String,
//! }
//! ```

pub mod content_filtered_topic;
pub mod multi_topic;
pub mod qos;
#[doc(hidden)]
pub mod raw_data;
#[doc(hidden)]
pub mod sql;
pub mod topic;
pub mod topic_description;
pub mod topic_listener;
pub mod type_support;

#[doc(hidden)]
pub use raw_data::{RawData, RawDataTypeSupport};
#[doc(hidden)]
pub use topic::*;
#[doc(hidden)]
pub use type_support::{DdsType, TypeSupport};
