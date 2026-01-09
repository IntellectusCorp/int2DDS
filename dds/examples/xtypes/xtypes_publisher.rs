//! XTypes Publisher Example
//!
//! This publisher uses a compile-time defined type (SensorData) to publish data.
//! The XTypes Subscriber can receive this data without knowing the type at compile time.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example xtypes_publisher -- --domain 0
//! ```

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
        wait_set::WaitSet,
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

/// SensorData - the data type published by this example.
///
/// The XTypes Subscriber will receive this data without knowing
/// this struct definition at compile time.
#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct SensorData {
    #[dds(key)]
    sensor_id: i32,
    temperature: f64,
    humidity: f64,
    location: String,
}

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let args = Args::parse();

    println!("Publishing SensorData with compile-time type definition.");
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

    // Wait for subscriber match
    println!("Waiting for subscriber to match...");
    let mut condition = writer.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();
    wait_set.wait(Duration::infinite()).unwrap();
    writer.get_publication_matched_status().unwrap();
    println!("Subscriber matched! Starting to publish...\n");

    let locations = ["Lab A", "Lab B", "Warehouse", "Office"];
    let mut sensor_id = 1;
    let mut index = 0;

    loop {
        let data = SensorData {
            sensor_id,
            temperature: 20.0 + (index as f64 * 0.5) % 15.0,
            humidity: 40.0 + (index as f64 * 0.3) % 30.0,
            location: locations[index % locations.len()].to_string(),
        };

        match writer.write(&data, InstanceHandle::NIL) {
            Ok(_) => {
                println!(
                    "[SEND] sensor_id={}, temp={:.1}°C, humidity={:.1}%, location={}",
                    data.sensor_id, data.temperature, data.humidity, data.location
                );
            }
            Err(e) => {
                eprintln!("Write failed: {:?}", e);
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
        index += 1;
        if index % 5 == 0 {
            sensor_id = (sensor_id % 3) + 1;
        }
    }
}
