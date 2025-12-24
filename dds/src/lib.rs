//! # int2dds
//!
//! A Rust implementation of the Data Distribution Service (DDS) middleware standard,
//! following the Real-Time Publish-Subscribe (RTPS) protocol.
//!
//! ## Overview
//!
//! int2dds provides a DDS implementation for real-time pub-sub messaging
//! in distributed systems. It supports both reliable and best-effort communication
//! with QoS (Quality of Service) policies.
//!
//! ## Quick Start
//!
//! ### examples/publisher_example.rs
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use int2dds::{
//!     common::instance_handle::InstanceHandle,
//!     core::time::Duration,
//!     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
//!     infrastructure::{
//!         qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
//!         status::StatusMask,
//!     },
//!     publication::{
//!         data_writer_listener::DataWriterListener,
//!         qos::{DataWriterQos, PublisherQos},
//!     },
//!     topic::{qos::TopicQos, type_support::DdsType},
//! };
//!
//! #[derive(DdsType)]
//! #[dds_type(crate_path = "int2dds")]
//! struct HelloWorld {
//!     index: u32,
//!     message: String,
//! }
//!
//! struct MyListener;
//!
//! impl DataWriterListener for MyListener {
//!     type Foo = HelloWorld;
//!     fn on_publication_matched(
//!         &self,
//!         _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
//!         status: &int2dds::infrastructure::status::PublicationMatchedStatus,
//!     ) {
//!         if status.current_count() > 0 {
//!             println!("Subscriber matched!");
//!         } else {
//!             println!("Subscriber disconnected.");
//!         }
//!     }
//! }
//!
//! # fn main() {
//! let domain_id = 0;
//!
//! let factory = DomainParticipantFactory::get_instance();
//! let participant = factory
//!     .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
//!     .unwrap();
//!
//! let topic = participant
//!     .create_topic::<HelloWorld>(
//!         "HelloWorldTopic",
//!         "HelloWorld",
//!         TopicQos::default(),
//!         None,
//!         StatusMask::default(),
//!     )
//!     .unwrap();
//!
//! let publisher = participant
//!     .create_publisher(PublisherQos::default(), None, StatusMask::default())
//!     .unwrap();
//!
//! let writer_qos = DataWriterQos {
//!     reliability: ReliabilityQosPolicy {
//!         kind: ReliabilityQosPolicyKind::Reliable,
//!         max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
//!     },
//!     ..Default::default()
//! };
//!
//! let writer = publisher
//!     .create_datawriter::<HelloWorld>(
//!         &topic,
//!         writer_qos,
//!         Some(Arc::new(MyListener)),
//!         StatusMask::default(),
//!     )
//!     .unwrap();
//!
//! println!("Publisher started on domain {}", domain_id);
//!
//! let mut i = 0;
//! loop {
//!     let data = HelloWorld {
//!         index: i,
//!         message: format!("Hello, DDS! #{}", i),
//!     };
//!     writer.write(&data, InstanceHandle::NIL).unwrap();
//!     println!("Published: {:?}", data);
//!     std::thread::sleep(std::time::Duration::from_secs(1));
//!     i += 1;
//! }
//! # }
//! ```
//!
//! ### examples/subscriber_example.rs
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use int2dds::{
//!     core::time::Duration,
//!     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
//!     infrastructure::{
//!         qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
//!         status::StatusMask,
//!     },
//!     subscription::{
//!         data_reader_listener::DataReaderListener,
//!         qos::{DataReaderQos, SubscriberQos},
//!         sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
//!     },
//!     topic::{qos::TopicQos, type_support::DdsType},
//! };
//!
//! #[derive(DdsType)]
//! #[dds_type(crate_path = "int2dds")]
//! struct HelloWorld {
//!     index: u32,
//!     message: String,
//! }
//!
//! struct MyListener;
//!
//! impl DataReaderListener for MyListener {
//!     type Foo = HelloWorld;
//!
//!     fn on_subscription_matched(
//!         &self,
//!         _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
//!         status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
//!     ) {
//!         if status.current_count() > 0 {
//!             println!("Publisher matched!");
//!         } else {
//!             println!("Publisher disconnected.");
//!         }
//!     }
//!
//!     fn on_data_available(
//!         &self,
//!         reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
//!     ) {
//!         if let Ok(samples) = reader.take(
//!             10,
//!             &[SampleStateKind::ANY_SAMPLE_STATE],
//!             &[ViewStateKind::ANY_VIEW_STATE],
//!             &[InstanceStateKind::ANY_INSTANCE_STATE],
//!         ) {
//!             for sample in samples.iter() {
//!                 if let Ok(data) = sample.data() {
//!                     println!("Received: {:?}", data);
//!                 }
//!             }
//!         }
//!     }
//! }
//!
//! # fn main() {
//! let domain_id = 0;
//!
//! let factory = DomainParticipantFactory::get_instance();
//! let participant = factory
//!     .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
//!     .unwrap();
//!
//! let topic = participant
//!     .create_topic::<HelloWorld>(
//!         "HelloWorldTopic",
//!         "HelloWorld",
//!         TopicQos::default(),
//!         None,
//!         StatusMask::default(),
//!     )
//!     .unwrap();
//!
//! let subscriber = participant
//!     .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
//!     .unwrap();
//!
//! let reader_qos = DataReaderQos {
//!     reliability: ReliabilityQosPolicy {
//!         kind: ReliabilityQosPolicyKind::Reliable,
//!         max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
//!     },
//!     ..Default::default()
//! };
//!
//! let _reader = subscriber
//!     .create_datareader::<HelloWorld>(
//!         &topic,
//!         reader_qos,
//!         Some(Arc::new(MyListener)),
//!         StatusMask::default(),
//!     )
//!     .unwrap();
//!
//! println!("Subscriber started on domain {}", domain_id);
//! println!("Waiting for data...");
//!
//! loop {
//!     std::thread::sleep(std::time::Duration::from_secs(1));
//! }
//! # }
//! ```
//!
//! Run the examples:
//! ```bash
//! # Terminal 1
//! cargo run --example subscriber_example
//!
//! # Terminal 2
//! cargo run --example publisher_example
//! ```
//!
//! ## Features
//!
//! - **RTPS Protocol**: Implementation of the RTPS wire protocol
//! - **QoS Policies**: Support for Reliability, Durability, History, and more
//! - **Discovery**: Automatic endpoint discovery using SPDP and SEDP
//! - **Type Safety**: Compile-time type checking with `#[derive(DdsType)]`
//! - **Performance**: Zero-copy serialization with the `speedy` crate
//! - **Network Flexibility**: UDP multicast, and TCP transport support
//!
//! ## Architecture
//!
//! The crate is organized into three main layers:
//!
//! - [`dcps`]: High-level DDS API (domain participants, topics, readers, writers)
//! - [`rtps`]: Low-level RTPS protocol implementation
//! - [`common`]: Shared utilities and data structures
//!
//! ## Custom Types
//!
//! Define custom DDS types using the `DdsType` derive macro:
//!
//! ```rust
//! use int2dds::DdsType;
//!
//! #[derive(DdsType)]
//! #[dds_type(crate_path = "int2dds")]
//! pub struct SensorData {
//!     #[dds(key)]
//!     pub sensor_id: u32,
//!     pub temperature: f32,
//!     pub humidity: f32,
//! }
//! ```
//!
//! ## Environment Variables
//!
//! ### Transport & Discovery
//!
//! - `INT2DDS_TRANSPORT`: Transport protocol type (`udp`, `tcp`) - Default: `udp`
//! - `INT2DDS_DISCOVERY_MODE`: Discovery mode (`udp`, `tcp`, `hybrid`) - Default: `udp`
//!   - `udp`: UDP-only discovery and user data
//!   - `tcp`: TCP-only discovery and user data (requires `INT2DDS_INITIAL_PEERS`)
//!   - `hybrid`: UDP discovery + TCP user data
//! - `INT2DDS_INITIAL_PEERS`: Initial peer addresses for TCP discovery (format: `"ip:port,ip:port"`)
//! - `INT2DDS_EXTENDED_DISCOVERY`: Enable extended discovery (`true`/`false`) - Default: `false`
//!
//! ### Logging
//!
//! - `INT2DDS_LOG_TYPE`: Log output type (`console`, `file`, `all`, `none`) - Default: `none`
//! - `INT2DDS_CONSOLE_LOG_LEVEL`: Console log level (`trace`, `debug`, `info`, `warn`, `error`) - Default: `info`
//! - `INT2DDS_FILE_LOG_LEVEL`: File log level (`trace`, `debug`, `info`, `warn`, `error`) - Default: `info`
//!
//! ### Monitoring & Profiling
//!
//! - `INT2DDS_THREAD_MONITORING`: Enable thread monitoring (`true`/`false`) - Default: `false`
//! - `INT2DDS_THREAD_MONITORING_LOG_PATH`: Thread monitoring log file path - Default: `./thread_monitoring.log`
//! - `INT2DDS_FUNCTION_TIMING`: Enable function execution time measurement (`true`/`false`) - Default: `false`
//! - `INT2DDS_FUNCTION_TIMING_LOG_PATH`: Function timing log file path - Default: `./function_timing.log`
//!
//! ### Network Configuration
//!
//! - `INT2DDS_NETWORK_INTERFACE`: Network interface name (e.g., `eth0`, `wlan0`) - Default: auto-select
//! - `INT2DDS_NETWORK_IP`: Network IP address (e.g., `192.168.1.100`) - Default: auto-select
//! - `INT2DDS_UDP_SOCKET_BUFFER`: UDP socket buffer size in bytes - Default: OS default
//!
//! ### TCP Settings
//!
//! - `INT2DDS_TCP_CONNECT_TIMEOUT`: TCP connection timeout in milliseconds - Default: `5000`
//! - `INT2DDS_TCP_WRITE_TIMEOUT`: TCP write timeout in milliseconds - Default: `10000`
//! - `INT2DDS_TCP_NODELAY`: Enable TCP_NODELAY (disable Nagle's algorithm) (`true`/`false`) - Default: `true`
//!
//! ### Example Usage
//!
//! ```bash
//! # Windows PowerShell
//! $env:INT2DDS_DISCOVERY_MODE = "hybrid"
//! $env:INT2DDS_LOG_TYPE = "console"
//! $env:INT2DDS_CONSOLE_LOG_LEVEL = "info"
//! $env:INT2DDS_FUNCTION_TIMING = "true"
//! cargo run --example hello_world_reliable_publisher
//!
//! # Linux/macOS
//! INT2DDS_DISCOVERY_MODE=hybrid \
//! INT2DDS_LOG_TYPE=console \
//! INT2DDS_CONSOLE_LOG_LEVEL=info \
//! INT2DDS_FUNCTION_TIMING=true \
//! cargo run --example hello_world_reliable_publisher
//!
//! # Using CLI arguments (alternative to environment variables)
//! cargo run --example hello_world_reliable_publisher -- \
//!   --int2dds-log-type console \
//!   --int2dds-console-log-level info \
//!   --int2dds-network-interface eth0
//! ```
//!
//! ## Examples
//!
//! Check the `examples/` directory for comprehensive usage examples:
//!
//! - `hello_world_param`: Configurable Communication
//! - `qos_profile_publisher/subscriber`: QoS-based Communication
//! - `perftest_publisher/subscriber`: Performance testing

#[cfg(test)]
extern crate self as int2dds;

pub mod dcps;
#[doc(hidden)]
pub use crate::publication::data_writer::DataWriterBase;
#[doc(hidden)]
pub use crate::subscription::data_reader::DataReaderBase;
#[doc(hidden)]
pub use dcps::*;
pub mod common;
pub mod config;
pub mod rtps;
#[doc(hidden)]
pub mod serialize;
#[doc(hidden)]
pub use dcps::topic::DdsType;
#[doc(hidden)]
pub use int2dds_derive::DdsType as DeriveDdsType;

extern crate md5;

/// Test utilities for generating unique domain IDs to prevent test interference
#[cfg(test)]
pub mod test_utils {
    use std::sync::atomic::{AtomicI32, Ordering};

    // Start from 100 to avoid conflicts with hardcoded domain IDs (0-99)
    // Domain ID valid range: 0-232
    static TEST_DOMAIN_ID_COUNTER: AtomicI32 = AtomicI32::new(100);

    /// Returns a unique domain ID for test isolation.
    /// Each call returns a different value, ensuring tests don't interfere with each other.
    pub fn unique_domain_id() -> i32 {
        TEST_DOMAIN_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
    }
}
