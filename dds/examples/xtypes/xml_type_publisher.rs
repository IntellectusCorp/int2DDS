//! XML Type Publisher Example
//!
//! Publishes `SensorData` defined entirely in `sensor_data.xml` — no
//! compile-time type, no code generation. The type is loaded at runtime via
//! [`XmlTypeRegistry`] and published as `DynamicData`.
//!
//! The companion `dynamic_type_subscriber` example receives this data by
//! discovering the TypeObject at runtime, demonstrating that XML-defined
//! types ride the existing discovery path unchanged.
//!
//! # Usage
//!
//! ```bash
//! # Terminal 1: subscriber (discovers the type at runtime, no XML needed)
//! cargo run --example dynamic_type_subscriber -- --domain 0
//!
//! # Terminal 2: this publisher
//! cargo run --example xml_type_publisher -- --domain 0
//! ```

use std::sync::Arc;
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
    config::xml::XmlTypeRegistry,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    topic::qos::TopicQos,
};

#[derive(Parser, Debug)]
#[command(author, version, about = "XML Type Publisher - publishes SensorData defined in XML", long_about = None)]
struct Args {
    /// Domain ID
    #[arg(short, long, default_value = "0")]
    domain: i32,

    /// Path to the XML type definition file
    #[arg(short, long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/xtypes/sensor_data.xml"))]
    file: String,

    /// Topic name
    #[arg(short, long, default_value = "SensorTopic")]
    topic: String,

    /// Type name to look up in the XML file
    #[arg(long, default_value = "SensorData")]
    type_name: String,
}

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let args = Args::parse();
    let shutdown = Shutdown::install();

    println!("XML Type Publisher — type loaded at runtime from {}", args.file);

    let registry = XmlTypeRegistry::from_file(&args.file).expect("Failed to load XML types");
    let type_support = Arc::new(registry.get(&args.type_name).expect("Type not found in XML file"));

    if let Some(struct_desc) = type_support.dynamic_type().as_struct() {
        println!("Loaded type: {}", args.type_name);
        for member in struct_desc.members() {
            let key_marker = if member.is_key { " [KEY]" } else { "" };
            println!("  - {}: {:?}{}", member.name, member.member_type, key_marker);
        }
    }
    println!();

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
        .create_topic_dynamic(
            &args.topic,
            type_support.clone(),
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
        .create_datawriter_dynamic(
            &topic,
            type_support.clone(),
            writer_qos,
            None,
            StatusMask::default(),
        )
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

    let mut sensor_id = 1i32;
    let mut index = 0usize;

    while !shutdown.is_stopped() {
        let temperature = 20.0 + (index as f64 * 0.5) % 15.0;
        let humidity = 40.0 + (index as f64 * 0.3) % 30.0;

        let mut data = type_support.create_data();
        data.set("sensor_id", sensor_id).unwrap();
        data.set("temperature", temperature).unwrap();
        data.set("humidity", humidity).unwrap();

        match writer.write(&data, InstanceHandle::NIL) {
            Ok(_) => {
                println!("[SEND] id={} temp={:.1}°C hum={:.1}%", sensor_id, temperature, humidity);
                let _ = std::io::Write::flush(&mut std::io::stdout());
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
