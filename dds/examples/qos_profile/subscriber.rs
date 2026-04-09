//! QoS Profile Subscriber Example
//!
//! Companion to `qos_profile_publisher`. See that file's module docs for the
//! full explanation of how `DDS_QOS_PROFILE` + `is_default_profile` makes the
//! `_QOS_DEFAULT` sentinels resolve to the JSON-defined QoS automatically.
//!
//! Run with (from the project root):
//! ```bash
//! # Linux / macOS
//! DDS_QOS_PROFILE=dds/examples/qos_profile/qos_profiles.json \
//!     cargo run --example qos_profile_subscriber -- --int2dds-domain-id 7
//!
//! # Windows (PowerShell)
//! $env:DDS_QOS_PROFILE="dds/examples/qos_profile/qos_profiles.json"
//! cargo run --example qos_profile_subscriber -- --int2dds-domain-id 7
//! ```
//!
//! The `--int2dds-domain-id` CLI flag (or `DDS_DOMAIN_ID` env var) selects the
//! DDS domain. Defaults to 0.

use std::sync::Arc;

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
    // DEFAULT_DOMAIN_ID is a sentinel: int2dds resolves it from `DDS_DOMAIN_ID`
    // env var or `--int2dds-domain-id <ID>` CLI flag. Defaults to 0 when unset.
    let domain_id = DEFAULT_DOMAIN_ID;

    // Auto-loads QoS profiles from DDS_QOS_PROFILE and parses CLI flags on first access.
    let factory = DomainParticipantFactory::get_instance();

    println!(
        "\n[Subscriber] domain_id ={}, env_domain_id={:?} \n\n",
        domain_id,
        std::env::var("DDS_DOMAIN_ID")
    );

    if std::env::var("DDS_QOS_PROFILE").is_err() {
        eprintln!("[Subscriber] WARNING: DDS_QOS_PROFILE is not set.");
        eprintln!("            Falling back to spec-default QoS (BEST_EFFORT).");
        eprintln!("            Set DDS_QOS_PROFILE=dds/examples/qos_profile/qos_profiles.json");
    }

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

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
