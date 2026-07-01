//! Hello World publisher driven by XML: the participant QoS comes from a
//! `<qos_library>` profile, and the datawriter's **topic and QoS come from its
//! `<data_writer>` declaration** in the `<domain_participant_library>` — the code
//! only names the writer path and the Rust type.
//!
//! Run (from the `dds` crate dir):
//!   cargo run --example hello_world_xml_pub -- -d 0

use std::sync::Arc;
use std::time::Duration as StdDuration;

#[path = "../common/shutdown.rs"]
mod shutdown;
use shutdown::{cleanup_participant, Shutdown};

use clap::Parser;
use int2dds::{
    common::{
        env::{set_console_log_level, set_log_type},
        instance_handle::InstanceHandle,
        log::{LogLevel, LogType},
    },
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::status::StatusMask,
    publication::{data_writer_listener::DataWriterListener, qos::PUBLISHER_QOS_DEFAULT},
    topic::type_support::DdsType,
};
use log::info;

const PUBLISH_INTERVAL: StdDuration = StdDuration::from_millis(1000);

/// Participant QoS profile and the datawriter declaration path inside the loaded XML.
const PROFILE: &str = "HelloWorld::Pub";
const WRITER_PATH: &str = "PL::PubApp::pub::writer";
const PROFILE_XML: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hello_world/profiles/xml/pub_profile.xml");

#[derive(Parser, Debug)]
#[command(
    about = "Hello World DDS Publisher (writer topic + QoS from XML)",
    disable_help_flag = true
)]
struct Args {
    /// Print help
    #[arg(long, action = clap::ArgAction::Help)]
    help: Option<bool>,

    /// Domain ID
    #[arg(short = 'd', long, default_value_t = 0)]
    domain: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "final")]
struct HelloWorldType {
    index: u32,
    message: String,
}

struct PubListener;

impl DataWriterListener for PubListener {
    type Foo = HelloWorldType;
    fn on_publication_matched(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        status: &int2dds::infrastructure::status::PublicationMatchedStatus,
    ) {
        if status.current_count() > 0 {
            info!("Subscriber matched!");
        } else {
            info!("No subscribers.");
        }
    }
}

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let domain_id = Args::parse().domain;
    let shutdown = Shutdown::install();
    let factory = DomainParticipantFactory::get_instance();

    factory.load_profiles(&[PROFILE_XML]).expect("failed to load XML config");

    let participant = factory
        .create_participant_with_profile(domain_id, PROFILE, None, StatusMask::default())
        .unwrap();
    let publisher =
        participant.create_publisher(PUBLISHER_QOS_DEFAULT, None, StatusMask::default()).unwrap();
    // The writer's topic (name, type, QoS) and its own QoS all come from the XML.
    let writer = publisher
        .create_datawriter_from_config::<HelloWorldType>(
            WRITER_PATH,
            Some(Arc::new(PubListener)),
            StatusMask::default(),
        )
        .unwrap();

    let wqos = writer.get_qos().unwrap();
    println!("[publisher INFO] domain_id: {}, writer: {}", domain_id, WRITER_PATH);
    println!(
        "[publisher qos] reliability: {:?}, durability: {:?}, history: {:?}",
        wqos.reliability.kind, wqos.durability.kind, wqos.history.kind
    );

    while !shutdown.is_stopped() {
        if writer.get_publication_matched_status().unwrap().current_count() > 0 {
            break;
        }
        if shutdown.wait_timeout(StdDuration::from_millis(100)) {
            drop(writer);
            cleanup_participant(participant);
            return;
        }
    }

    let mut index = 1;
    while !shutdown.is_stopped() {
        let data = HelloWorldType { index, message: format!("HelloWorld_d{}", domain_id) };
        writer.write(&data, InstanceHandle::NIL).unwrap();
        info!("Published {:?}", data);
        if shutdown.wait_timeout(PUBLISH_INTERVAL) {
            break;
        }
        index += 1;
    }

    drop(writer);
    cleanup_participant(participant);
}
