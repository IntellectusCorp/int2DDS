//! QoS Profile Publisher Example
//!
//! Demonstrates QoS profile auto-loading via `DDS_QOS_PROFILE`.
//! Entities are created with `_QOS_DEFAULT` sentinels — the loaded profile
//! (`ReliableProfile`) is applied automatically.
//!
//! ```bash
//! DDS_QOS_PROFILE=dds/examples/qos_profile/qos_profiles.json \
//!     cargo run --example qos_profile_publisher
//! ```

use std::sync::Arc;

use int2dds::{
    common::{env::DEFAULT_DOMAIN_ID, instance_handle::InstanceHandle},
    core::time::Duration,
    dcps::infrastructure::wait_set::WaitSet,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::PARTICIPANT_QOS_DEFAULT},
    infrastructure::status::StatusMask,
    publication::{
        data_writer_listener::DataWriterListener,
        qos::{DATAWRITER_QOS_DEFAULT, PUBLISHER_QOS_DEFAULT},
    },
    topic::{qos::TOPIC_QOS_DEFAULT, type_support::DdsType},
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
    let domain_id = DEFAULT_DOMAIN_ID;
    let factory = DomainParticipantFactory::get_instance();

    if std::env::var("DDS_QOS_PROFILE").is_err() {
        eprintln!("[Publisher] WARNING: DDS_QOS_PROFILE is not set.");
        eprintln!("           Set DDS_QOS_PROFILE=dds/examples/qos_profile/qos_profiles.json");
    }

    println!("[Publisher] domain_id = {}", domain_id);

    let participant = factory
        .create_participant(domain_id, PARTICIPANT_QOS_DEFAULT, None, StatusMask::default())
        .expect("Failed to create participant");

    let topic = participant
        .create_topic::<HelloWorld>(
            "HelloWorldTopic",
            "HelloWorld",
            TOPIC_QOS_DEFAULT,
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");

    let publisher = participant
        .create_publisher(PUBLISHER_QOS_DEFAULT, None, StatusMask::default())
        .expect("Failed to create publisher");

    let writer = publisher
        .create_datawriter::<HelloWorld>(
            &topic,
            DATAWRITER_QOS_DEFAULT,
            Some(Arc::new(PublisherListener)),
            StatusMask::default(),
        )
        .expect("Failed to create datawriter");

    let qos = writer.get_qos().expect("Failed to get QoS");
    println!("[Publisher] DataWriter QoS in effect:");
    println!("  - Reliability: {:?}", qos.reliability.kind);
    println!("  - Durability:  {:?}", qos.durability.kind);
    println!("  - History:     {:?} (depth = {:?})", qos.history.kind, qos.history.depth());

    println!("\n[Publisher] Publishing messages... (Ctrl+C to stop)\n");

    let mut condition = writer.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();
    wait_set.wait(Duration::infinite()).unwrap();
    writer.get_publication_matched_status().unwrap();

    for i in 0.. {
        let data = HelloWorld { id: i, message: format!("Hello from QoS Profile example! #{}", i) };

        match writer.write(&data, InstanceHandle::NIL) {
            Ok(_) => println!("[Publisher] Sent: {:?}", data),
            Err(e) => eprintln!("[Publisher] Write failed: {:?}", e),
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
