//! QoS Profile Publisher Example
//!
//! This example demonstrates how to use QoS profiles loaded from a JSON file
//! to create DDS entities with predefined QoS settings.
//!
//! Run with:
//! ```bash
//! cargo run --example qos_profile_publisher
//! ```

use std::sync::Arc;

use int2dds::{
    common::instance_handle::InstanceHandle,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::status::StatusMask,
    publication::data_writer_listener::DataWriterListener,
    topic::type_support::DdsType,
};

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct HelloWorld {
    #[dds(key)]
    id: u32,
    message: String,
}

struct PublisherListener;

impl DataWriterListener for PublisherListener {
    type Foo = HelloWorld;

    fn on_publication_matched(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        status: &int2dds::infrastructure::status::PublicationMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("[Publisher] Subscriber matched! (total: {})", status.current_count());
        } else {
            println!("[Publisher] Subscriber disconnected.");
        }
    }
}

fn main() {
    let domain_id = 0;

    // Get the DomainParticipantFactory singleton
    let factory = DomainParticipantFactory::get_instance();

    // Load QoS profiles from JSON file
    // The path is relative to where the executable is run from
    let profile_path = "dds/examples/qos_profile/qos_profiles.json";
    if let Err(e) = factory.load_profiles(&[profile_path]) {
        eprintln!("Failed to load QoS profiles: {:?}", e);
        eprintln!("Make sure to run from the project root directory.");
        return;
    }
    println!("[Publisher] QoS profiles loaded from: {}", profile_path);

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
    println!("[Publisher] Topic created with ReliableProfile QoS");

    // Create Publisher using QoS from profile
    let publisher = participant
        .create_publisher_with_profile(
            "HelloWorldLibrary::ReliableProfile",
            None,
            StatusMask::default(),
        )
        .expect("Failed to create publisher");
    println!("[Publisher] Publisher created with ReliableProfile QoS");

    // Create DataWriter using QoS from profile
    let writer = publisher
        .create_datawriter_with_profile::<HelloWorld>(
            &topic,
            "HelloWorldLibrary::ReliableProfile",
            Some(Arc::new(PublisherListener)),
            StatusMask::default(),
        )
        .expect("Failed to create datawriter");
    println!("[Publisher] DataWriter created with ReliableProfile QoS");

    // Print the applied QoS settings
    let qos = writer.get_qos().expect("Failed to get QoS");
    println!("[Publisher] DataWriter QoS:");
    println!("  - Reliability: {:?}", qos.reliability.kind);
    println!("  - History: {:?}", qos.history.kind);
    println!("  - Durability: {:?}", qos.durability.kind);

    println!("\n[Publisher] Publishing messages...");
    println!("Press Ctrl+C to stop.\n");

    // Publish data
    for i in 0.. {
        let data = HelloWorld { id: i, message: format!("Hello from QoS Profile example! #{}", i) };

        match writer.write(&data, InstanceHandle::NIL) {
            Ok(_) => println!("[Publisher] Sent: {:?}", data),
            Err(e) => eprintln!("[Publisher] Write failed: {:?}", e),
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
