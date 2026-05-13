//! QoS Profile Subscriber Example
//!
//! Companion to `qos_profile_publisher`. Uses `_QOS_DEFAULT` sentinels
//! with the profile loaded via `DDS_QOS_PROFILE`.
//!
//! ```bash
//! DDS_QOS_PROFILE=dds/examples/qos_profile/qos_profiles.json \
//!     cargo run --example qos_profile_subscriber
//! ```

use std::sync::Arc;

#[path = "../common/shutdown.rs"]
mod shutdown;
use shutdown::{cleanup_participant, Shutdown};

use int2dds::{
    common::env::DEFAULT_DOMAIN_ID,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::PARTICIPANT_QOS_DEFAULT},
    infrastructure::status::StatusMask,
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DATAREADER_QOS_DEFAULT, SUBSCRIBER_QOS_DEFAULT},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
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
    let shutdown = Shutdown::install();

    let domain_id = DEFAULT_DOMAIN_ID;
    let factory = DomainParticipantFactory::get_instance();

    if std::env::var("DDS_QOS_PROFILE").is_err() {
        eprintln!("[Subscriber] WARNING: DDS_QOS_PROFILE is not set.");
        eprintln!("             Set DDS_QOS_PROFILE=dds/examples/qos_profile/qos_profiles.json");
    }

    println!("[Subscriber] domain_id = {}", domain_id);

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

    let subscriber = participant
        .create_subscriber(SUBSCRIBER_QOS_DEFAULT, None, StatusMask::default())
        .expect("Failed to create subscriber");

    let reader = subscriber
        .create_datareader::<HelloWorld>(
            &topic,
            DATAREADER_QOS_DEFAULT,
            Some(Arc::new(SubscriberListener)),
            StatusMask::default(),
        )
        .expect("Failed to create datareader");

    let qos = reader.get_qos().expect("Failed to get QoS");
    println!("[Subscriber] DataReader QoS in effect:");
    println!("  - Reliability: {:?}", qos.reliability.kind);
    println!("  - Durability:  {:?}", qos.durability.kind);
    println!("  - History:     {:?} (depth = {:?})", qos.history.kind, qos.history.depth());

    println!("\n[Subscriber] Waiting for data... (Ctrl+C to stop)\n");

    shutdown.wait();

    drop(reader);
    cleanup_participant(participant);
}
