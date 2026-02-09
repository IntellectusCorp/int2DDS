//! int2dds Hello World Publisher Example (Rust)
//!
//! This example demonstrates how to use int2dds with IDL-generated types.

use std::sync::Arc;

use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
        wait_set::WaitSet,
    },
    publication::{
        data_writer_listener::DataWriterListener,
        qos::{DataWriterQos, PublisherQos},
    },
};

mod hello_world;
use hello_world::HelloWorld;

struct PubListener;

impl DataWriterListener for PubListener {
    type Foo = HelloWorld;

    fn on_publication_matched(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        status: &int2dds::infrastructure::status::PublicationMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Subscriber matched!");
        } else {
            println!("No subscribers.");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut domain_id = 0i32;
    let mut use_reliable = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--reliable" => use_reliable = true,
            "--domain" if i + 1 < args.len() => {
                i += 1;
                domain_id = args[i].parse().unwrap_or(0);
            }
            _ => {}
        }
        i += 1;
    }

    let reliability_str = if use_reliable { "RELIABLE" } else { "BEST_EFFORT" };
    println!("int2dds IDL Hello World Publisher (Rust)");
    println!("Domain: {}, QoS: {}", domain_id, reliability_str);
    println!("-----------------------------------");

    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .expect("Failed to create participant");

    let topic = participant
        .create_topic::<HelloWorld>(
            "hello_world_topic",
            "HelloWorld",
            Default::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");

    let publisher = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .expect("Failed to create publisher");

    let reliability_kind = if use_reliable {
        ReliabilityQosPolicyKind::Reliable
    } else {
        ReliabilityQosPolicyKind::BestEffort
    };

    let writer_qos = DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: reliability_kind,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        ..Default::default()
    };

    let listener = PubListener;
    let writer = publisher
        .create_datawriter::<HelloWorld>(
            &topic,
            writer_qos,
            Some(Arc::new(listener)),
            StatusMask::default(),
        )
        .expect("Failed to create datawriter");

    println!("Publisher ready. Waiting for subscriber...");

    let mut condition = writer.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();
    wait_set.wait(Duration::infinite()).unwrap();

    let status = writer.get_publication_matched_status().unwrap();
    println!(
        "Subscriber matched! (total: {}, current: {})",
        status.total_count(),
        status.current_count()
    );
    println!("Starting to send messages...\n");

    let mut index = 0u32;
    loop {
        index += 1;
        let data = HelloWorld { index, message: format!("Hello World from Rust! [{}]", index) };

        match writer.write(&data, InstanceHandle::NIL) {
            Ok(_) => println!("[{}] Sent: {}", index, data.message),
            Err(e) => eprintln!("Failed to write: {:?}", e),
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
