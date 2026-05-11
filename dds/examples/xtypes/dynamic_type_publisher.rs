//! XTypes Publisher Example
//!
//! Publishes a `SensorData` topic. The companion `dynamic_type_subscriber` example
//! receives this data WITHOUT knowing the type at compile time, by discovering
//! the TypeObject at runtime and decoding samples through `DynamicData`.
//!
//! XTypes features demonstrated here:
//!
//! * `#[dds(key)]` — marks the instance key field.
//! * `#[dds(bound = N)]` — declares a maximum length for the `String` field
//!   which appears in the published TypeObject.
//!
//! `SensorData` is intentionally declared as **Final** extensibility because
//! that is what the dynamic type-object discovery path supports today end to
//! end. For the wire-format demos of Mutable / Appendable / Hash member-IDs /
//! optional fields see the dedicated `xtypes_extensibility_*`,
//! `xtypes_member_id_*`, and `xtypes_bitmask_bitset_*` example pairs, which
//! use compile-time-typed readers on the subscriber side.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example dynamic_type_publisher -- --domain 0
//! ```

use std::time::Duration as StdDuration;

#[path = "../common/shutdown.rs"]
mod shutdown;
use shutdown::{cleanup_participant, Shutdown};

use clap::Parser;
use int2dds::{
    common::{
        env::{set_console_log_level, set_log_type},
        instance_handle::InstanceHandle,
        log::{LogLevel, LogType},
    },
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    topic::{qos::TopicQos, type_support::DdsType},
};

#[derive(Parser, Debug)]
#[command(author, version, about = "XTypes Publisher - publishes SensorData", long_about = None)]
struct Args {
    /// Domain ID
    #[arg(short, long, default_value = "0")]
    domain: i32,
}

/// SensorData — published as Final extensibility for compatibility with the
/// dynamic type-object discovery path used by `xtypes_subscriber`.
#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct SensorData {
    #[dds(key)]
    sensor_id: i32,
    temperature: f64,
    humidity: f64,
    /// Bounded string — max length recorded in TypeObject.
    #[dds(bound = 32)]
    location: String,
}

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let args = Args::parse();
    let shutdown = Shutdown::install();

    println!("XTypes Publisher — Final SensorData with bounded string");
    println!("The XTypes Subscriber receives this using DynamicData.\n");

    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(
            args.domain,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create participant");

    let topic = participant
        .create_topic::<SensorData>(
            "SensorTopic",
            "SensorData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");

    let publisher = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .expect("Failed to create publisher");

    let writer_qos = DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        ..Default::default()
    };

    let writer = publisher
        .create_datawriter::<SensorData>(&topic, writer_qos, None, StatusMask::default())
        .expect("Failed to create datawriter");

    println!("Waiting for subscriber to match...");
    while !shutdown.is_stopped() {
        let status = writer.get_publication_matched_status().unwrap();
        if status.current_count() > 0 {
            break;
        }
        if shutdown.wait_timeout(StdDuration::from_millis(100)) {
            drop(writer);
            cleanup_participant(participant);
            return;
        }
    }
    println!("Subscriber matched! Starting to publish...\n");

    let locations = ["Lab A", "Lab B", "Warehouse", "Office"];
    let mut sensor_id = 1;
    let mut index = 0usize;

    while !shutdown.is_stopped() {
        let data = SensorData {
            sensor_id,
            temperature: 20.0 + (index as f64 * 0.5) % 15.0,
            humidity: 40.0 + (index as f64 * 0.3) % 30.0,
            location: locations[index % locations.len()].to_string(),
        };

        match writer.write(&data, InstanceHandle::NIL) {
            Ok(_) => {
                println!(
                    "[SEND] id={} temp={:.1}°C hum={:.1}% loc={}",
                    data.sensor_id, data.temperature, data.humidity, data.location
                );
            }
            Err(e) => eprintln!("Write failed: {:?}", e),
        }

        if shutdown.wait_timeout(StdDuration::from_secs(1)) {
            break;
        }
        index += 1;
        if index % 5 == 0 {
            sensor_id = (sensor_id % 3) + 1;
        }
    }

    drop(writer);
    cleanup_participant(participant);
}
