//! int2dds Hello World Subscriber Example (Rust)
//!
//! This example demonstrates how to use int2dds with IDL-generated types.

use std::sync::Arc;

use int2dds::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
        wait_set::WaitSet,
    },
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
};

mod hello_world;
use hello_world::HelloWorld;

struct SubListener;

impl DataReaderListener for SubListener {
    type Foo = HelloWorld;

    fn on_subscription_matched(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Publisher matched!");
        } else {
            println!("No publishers.");
        }
    }

    fn on_data_available(
        &self,
        reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        if let Ok(samples) = reader.take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples.iter() {
                if let Ok(data) = sample.data() {
                    println!("[{}] Received: {}", data.index, data.message);
                }
            }
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
    println!("int2dds IDL Hello World Subscriber (Rust)");
    println!("Domain: {}, QoS: {}", domain_id, reliability_str);
    println!("------------------------------------");

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

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .expect("Failed to create subscriber");

    let reliability_kind = if use_reliable {
        ReliabilityQosPolicyKind::Reliable
    } else {
        ReliabilityQosPolicyKind::BestEffort
    };

    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: reliability_kind,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        ..Default::default()
    };

    let listener = SubListener;
    let _reader = subscriber
        .create_datareader::<HelloWorld>(
            &topic,
            reader_qos,
            Some(Arc::new(listener)),
            StatusMask::default(),
        )
        .expect("Failed to create datareader");

    println!("Subscriber ready. Waiting for publisher...");

    let mut condition = _reader.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();
    wait_set.wait(Duration::infinite()).unwrap();

    let status = _reader.get_subscription_matched_status().unwrap();
    println!(
        "Publisher matched! (total: {}, current: {})",
        status.total_count(),
        status.current_count()
    );
    println!("Waiting for messages...\n");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
