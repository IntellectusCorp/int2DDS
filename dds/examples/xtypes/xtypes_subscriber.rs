//! XTypes Subscriber Example
//!
//! This subscriber receives data WITHOUT knowing the type at compile time.
//! It discovers the TypeObject from the publisher via the Discovery protocol,
//! then uses DynamicData to inspect fields dynamically at runtime.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example xtypes_subscriber -- --domain 0
//! ```
//!
//! # How it works
//!
//! 1. Waits for a publisher to appear via builtin DCPSPublication topic
//! 2. Extracts TypeObject from PublicationBuiltinTopicData (sent via PID_TYPE_OBJECT)
//! 3. Creates DynamicTypeSupport from the discovered TypeObject
//! 4. Creates a DataReader<DynamicData> without compile-time type
//! 5. Accesses fields by name: `data.get::<i32>("sensor_id")`

use std::sync::Arc;

use clap::Parser;
use int2dds::{
    common::{
        builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
        env::{set_console_log_level, set_log_type},
        log::{LogLevel, LogType},
    },
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
        wait_set::WaitSet,
    },
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
    xtypes::TypeObject,
};

#[derive(Parser, Debug)]
#[command(author, version, about = "XTypes Subscriber - receives data without compile-time type", long_about = None)]
struct Args {
    /// Domain ID
    #[arg(short, long, default_value = "0")]
    domain: i32,

    /// Topic name to subscribe
    #[arg(short, long, default_value = "SensorTopic")]
    topic: String,
}

/// Wait for a publisher on the specified topic and extract TypeObject from discovery
fn wait_for_type_object(
    builtin_subscriber: &int2dds::subscription::subscriber::Subscriber,
    topic_name: &str,
) -> (TypeObject, String) {
    let publication_reader = builtin_subscriber
        .lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication")
        .expect("Failed to lookup DCPSPublication reader");

    let mut condition = publication_reader.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();

    loop {
        let _ = wait_set.wait(Duration { sec: 1, nanosec: 0 });
        publication_reader.get_status_changes().ok();

        if let Ok(samples) = publication_reader.read(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        ) {
            for sample in samples.iter() {
                if let Ok(pub_data) = sample.data() {
                    if pub_data.topic_name() == topic_name {
                        if let Some(type_obj) = pub_data.type_object() {
                            return (type_obj.clone(), pub_data.type_name().to_string());
                        }
                    }
                }
            }
        }
    }
}

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let args = Args::parse();

    println!("XTypes Subscriber - Type Discovery Demo");
    println!("Waiting for publisher on topic '{}'...\n", args.topic);

    // Create DomainParticipant
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(
            args.domain,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create participant");

    // Get builtin subscriber and wait for publisher with TypeObject
    let builtin_subscriber =
        participant.get_builtin_subscriber().expect("Failed to get builtin subscriber");

    let (type_object, type_name) = wait_for_type_object(&builtin_subscriber, &args.topic);

    // Create DynamicTypeSupport from discovered TypeObject
    let type_support = participant
        .create_dynamic_type_from_type_object(type_object)
        .expect("Failed to create DynamicTypeSupport from TypeObject");

    // Print discovered type information
    println!("Discovered type: {}", type_name);
    if let Some(struct_desc) = type_support.dynamic_type().as_struct() {
        for member in struct_desc.members() {
            let key_marker = if member.is_key { " [KEY]" } else { "" };
            println!("  - {}: {:?}{}", member.name, member.member_type, key_marker);
        }
    }
    println!();

    let type_support_arc = Arc::new(type_support);

    // Create topic with discovered type
    let topic = participant
        .create_topic_dynamic(
            &args.topic,
            type_support_arc.clone(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");

    // Create Subscriber and DataReader
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .expect("Failed to create subscriber");

    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        ..Default::default()
    };

    let reader = subscriber
        .create_datareader_dynamic(
            &topic,
            type_support_arc,
            reader_qos,
            None,
            StatusMask::default(),
        )
        .expect("Failed to create dynamic datareader");

    // Wait for subscription matched
    let mut match_condition = reader.get_statuscondition().unwrap().clone();
    match_condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
    let match_wait_set = WaitSet::new();
    match_wait_set.attach_condition(match_condition).unwrap();
    match_wait_set.wait(Duration::infinite()).unwrap();
    reader.get_subscription_matched_status().unwrap();

    println!("Matched with publisher. Receiving data...\n");

    // Use WaitSet to wait for data
    let mut data_condition = reader.get_statuscondition().unwrap().clone();
    data_condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap();
    let data_wait_set = WaitSet::new();
    data_wait_set.attach_condition(data_condition).unwrap();

    loop {
        if data_wait_set.wait(Duration { sec: 5, nanosec: 0 }).is_ok() {
            reader.get_status_changes().ok();

            match reader.take(
                10,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            ) {
                Ok(samples) => {
                    for sample in samples.iter() {
                        if let Ok(data) = sample.data() {
                            // Access fields DYNAMICALLY by name!
                            let sensor_id: i32 = data.get("sensor_id").unwrap_or_default();
                            let temperature: f64 = data.get("temperature").unwrap_or_default();
                            let humidity: f64 = data.get("humidity").unwrap_or_default();
                            let location: String = data.get("location").unwrap_or_default();

                            println!(
                                "[RECV] sensor_id={}, temp={:.1}°C, humidity={:.1}%, location={}",
                                sensor_id, temperature, humidity, location
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Read failed: {:?}", e);
                }
            }
        }
    }
}
