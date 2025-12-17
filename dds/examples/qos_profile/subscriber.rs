//! QoS Profile Subscriber Example
//!
//! This example demonstrates how to use QoS profiles loaded from a JSON file
//! to create DDS entities with predefined QoS settings.
//!
//! Run with:
//! ```bash
//! cargo run --example qos_profile_subscriber
//! ```

use std::sync::Arc;

use int2dds::{
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::status::StatusMask,
    subscription::{
        data_reader_listener::DataReaderListener,
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::type_support::DdsType,
};

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct HelloWorld {
    #[dds(key)]
    id: u32,
    message: String,
}

struct SubscriberListener;

impl DataReaderListener for SubscriberListener {
    type Foo = HelloWorld;

    fn on_subscription_matched(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("[Subscriber] Publisher matched! (total: {})", status.current_count());
        } else {
            println!("[Subscriber] Publisher disconnected.");
        }
    }

    fn on_data_available(
        &self,
        reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        match reader.take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            Ok(samples) => {
                for sample in samples.iter() {
                    if let Ok(data) = sample.data() {
                        println!("[Subscriber] Received: {:?}", data);
                    }
                }
            }
            Err(e) => eprintln!("[Subscriber] Take failed: {:?}", e),
        }
    }
}

fn main() {
    let domain_id = 0;

    // Get the DomainParticipantFactory singleton
    let factory = DomainParticipantFactory::get_instance();

    // Load QoS profiles from JSON file
    let profile_path = "dds/examples/qos_profile/qos_profiles.json";
    if let Err(e) = factory.load_profiles(&[profile_path]) {
        eprintln!("Failed to load QoS profiles: {:?}", e);
        eprintln!("Make sure to run from the project root directory.");
        return;
    }
    println!("[Subscriber] QoS profiles loaded from: {}", profile_path);

    // Create DomainParticipant with default QoS
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .expect("Failed to create participant");

    // Create Topic using QoS from profile
    let topic = participant
        .create_topic_with_profile::<HelloWorld>(
            "HelloWorldTopic",
            "HelloWorld",
            "HelloWorldLibrary::ReliableProfile",
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");
    println!("[Subscriber] Topic created with ReliableProfile QoS");

    // Create Subscriber using QoS from profile
    let subscriber = participant
        .create_subscriber_with_profile(
            "HelloWorldLibrary::ReliableProfile",
            None,
            StatusMask::default(),
        )
        .expect("Failed to create subscriber");
    println!("[Subscriber] Subscriber created with ReliableProfile QoS");

    // Create DataReader using QoS from profile
    let reader = subscriber
        .create_datareader_with_profile::<HelloWorld>(
            &topic,
            "HelloWorldLibrary::ReliableProfile",
            Some(Arc::new(SubscriberListener)),
            StatusMask::default(),
        )
        .expect("Failed to create datareader");
    println!("[Subscriber] DataReader created with ReliableProfile QoS");

    // Print the applied QoS settings
    let qos = reader.get_qos().expect("Failed to get QoS");
    println!("[Subscriber] DataReader QoS:");
    println!("  - Reliability: {:?}", qos.reliability.kind);
    println!("  - History: {:?}", qos.history.kind);
    println!("  - Durability: {:?}", qos.durability.kind);

    println!("\n[Subscriber] Waiting for data...");
    println!("Press Ctrl+C to stop.\n");

    // Keep the application running
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
