//! XTypes Subscriber Example
//!
//! This subscriber receives data WITHOUT knowing the type at compile time.
//! It uses DynamicData to inspect fields dynamically at runtime.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example xtypes_subscriber -- --domain 0
//! ```
//!
//! # How it works
//!
//! 1. Creates a TypeObject that describes the data structure (simulating discovery)
//! 2. Creates DynamicTypeSupport from the TypeObject
//! 3. Creates a DataReader<DynamicData> without compile-time type
//! 4. Accesses fields by name: `data.get::<i32>("sensor_id")`

use std::sync::Arc;

use clap::Parser;
use int2dds::{
    common::{
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
    topic::{qos::TopicQos, TypeSupport},
    xtypes::{
        CompleteStructType, CompleteStructMember, CompleteTypeObject,
        MemberFlag, TypeFlag, TypeIdentifier, TypeObject,
        TryConstructKind, ExtensibilityKind,
    },
};

#[derive(Parser, Debug)]
#[command(author, version, about = "XTypes Subscriber - receives data without compile-time type", long_about = None)]
struct Args {
    /// Domain ID
    #[arg(short, long, default_value = "0")]
    domain: i32,
}

/// Create a TypeObject that describes the SensorData structure.
///
/// In a real application, this TypeObject would be received during discovery
/// from `PublicationBuiltinTopicData::type_object()`.
///
/// Here we manually create it to simulate what discovery would provide.
fn create_sensor_data_type_object() -> TypeObject {
    let mut struct_type = CompleteStructType::new(
        TypeFlag::new(ExtensibilityKind::Final, false, false),
        "SensorData".to_string(),
        None,
    );

    // sensor_id: i32 (key field)
    struct_type.add_member(CompleteStructMember::new(
        0, // member_id
        MemberFlag::new(TryConstructKind::Discard, false, false, false, true, false), // is_key = true
        TypeIdentifier::Int32,
        "sensor_id".to_string(),
    ));

    // temperature: f64
    struct_type.add_member(CompleteStructMember::new(
        1,
        MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false),
        TypeIdentifier::Float64,
        "temperature".to_string(),
    ));

    // humidity: f64
    struct_type.add_member(CompleteStructMember::new(
        2,
        MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false),
        TypeIdentifier::Float64,
        "humidity".to_string(),
    ));

    // location: String
    struct_type.add_member(CompleteStructMember::new(
        3,
        MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false),
        TypeIdentifier::String8,
        "location".to_string(),
    ));

    TypeObject::Complete(CompleteTypeObject::Struct(struct_type))
}

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let args = Args::parse();

    println!("Receiving data WITHOUT compile-time type knowledge.");
    println!("Using DynamicData to access fields by name at runtime.\n");

    // Step 1: Create DomainParticipant
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(args.domain, DomainParticipantQos::default(), None, StatusMask::default())
        .expect("Failed to create participant");

    // Step 2: Create TypeObject (simulating what discovery would provide)
    println!("Step 1: Creating TypeObject (simulates discovery)...");
    let type_object = create_sensor_data_type_object();

    // Step 3: Create DynamicTypeSupport from TypeObject
    println!("Step 2: Creating DynamicTypeSupport from TypeObject...");
    let type_support = participant
        .create_dynamic_type_from_type_object(type_object)
        .expect("Failed to create DynamicTypeSupport");

    // Print type information
    println!("\n  Type Information:");
    println!("  ─────────────────");
    println!("  Type name: {}", type_support.get_type_name());
    if let Some(struct_desc) = type_support.dynamic_type().as_struct() {
        println!("  Members ({}):", struct_desc.members().len());
        for member in struct_desc.members() {
            let key_marker = if member.is_key { " [KEY]" } else { "" };
            println!("    - {}: {:?}{}", member.name, member.member_type, key_marker);
        }
    }
    println!();

    let type_support_arc = Arc::new(type_support);

    // Step 4: Register type and create topic
    println!("Step 3: Registering type and creating topic...");
    participant
        .register_dynamic_type(type_support_arc.clone())
        .expect("Failed to register dynamic type");

    let topic = participant
        .create_topic_dynamic(
            "SensorTopic",
            type_support_arc.clone(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");

    // Step 5: Create Subscriber and DataReader
    println!("Step 4: Creating DataReader<DynamicData>...\n");
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

    // Create DataReader for DynamicData - NO compile-time type needed!
    let reader = subscriber
        .create_datareader_dynamic(&topic, type_support_arc, reader_qos, None, StatusMask::default())
        .expect("Failed to create dynamic datareader");

    println!("═══════════════════════════════════════════════════════════════");
    println!("  Waiting for data from XTypes Publisher...");
    println!("  (Run: cargo run --example xtypes_publisher -- --domain {})", args.domain);
    println!("═══════════════════════════════════════════════════════════════\n");

    // Use WaitSet to wait for data
    let mut condition = reader.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();

    loop {
        if wait_set.wait(Duration { sec: 5, nanosec: 0 }).is_ok() {
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
                            // No compile-time SensorData struct needed!
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
