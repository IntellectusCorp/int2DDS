//! QoS Profile Publisher Example
//!
//! Demonstrates int2dds' QoS profile auto-loading via the `DDS_QOS_PROFILE`
//! environment variable, combined with the `is_default_profile` marker in the
//! JSON file. With both in place, the user code creates entities using the
//! standard `_QOS_DEFAULT` sentinels — no `_with_profile` calls required.
//!
//! Run with (from the project root):
//! ```bash
//! # Linux / macOS — env var
//! DDS_QOS_PROFILE=dds/examples/qos_profile/qos_profiles.json \
//!     cargo run --example qos_profile_publisher -- --int2dds-domain-id 7
//!
//! # Windows (PowerShell)
//! $env:DDS_QOS_PROFILE="dds/examples/qos_profile/qos_profiles.json"
//! cargo run --example qos_profile_publisher -- --int2dds-domain-id 7
//! ```
//!
//! The `--int2dds-domain-id` CLI flag (or `DDS_DOMAIN_ID` env var) selects the
//! DDS domain. Without it, the domain defaults to 0.
//!
//! Resolution order applied by int2dds when `DATAWRITER_QOS_DEFAULT` (and
//! sibling sentinels) is passed to `create_*`:
//!   1. QoS registered via `set_default_*_qos()`
//!   2. The default profile (env `DDS_DEFAULT_QOS_PROFILE`, then the first
//!      profile marked `is_default_profile: true` in the loaded JSON)
//!   3. The OMG DDS spec default
//!
//! In this example, `qos_profiles.json` marks `ReliableProfile` as the default,
//! so the DataWriter ends up with RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(10).

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
    // DEFAULT_DOMAIN_ID is a sentinel: int2dds resolves it from the
    // `DDS_DOMAIN_ID` env var (or `--int2dds-domain-id <ID>` CLI flag,
    // which `init_from_env()` translates into the same env var).
    // Defaults to 0 when neither is set.
    let domain_id = DEFAULT_DOMAIN_ID;

    // The factory's get_instance() auto-loads QoS profiles from DDS_QOS_PROFILE
    // and parses CLI flags via init_from_env() on first access.
    let factory = DomainParticipantFactory::get_instance();

    if std::env::var("DDS_QOS_PROFILE").is_err() {
        eprintln!("[Publisher] WARNING: DDS_QOS_PROFILE is not set.");
        eprintln!("           Falling back to spec-default QoS (BEST_EFFORT).");
        eprintln!("           Set DDS_QOS_PROFILE=dds/examples/qos_profile/qos_profiles.json");
    }

    println!(
        "\n[Publisher] domain_id ={}, env_domain_id={:?}\n\n",
        domain_id,
        std::env::var("DDS_DOMAIN_ID")
    );

    // All entities below use the standard `_QOS_DEFAULT` sentinels. When a
    // default profile is loaded (via DDS_QOS_PROFILE + is_default_profile),
    // int2dds transparently substitutes the profile's QoS at create time.
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

    // Inspect the QoS that was actually applied — proves the profile took effect.
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
