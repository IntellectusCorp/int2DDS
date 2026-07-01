//! Hello World subscriber driven by XML: the participant QoS comes from a
//! `<qos_library>` profile, and the datareader's **topic and QoS come from its
//! `<data_reader>` declaration** in the `<domain_participant_library>` — the code
//! only names the reader path and the Rust type.
//!
//! Run (from the `dds` crate dir):
//!   cargo run --example hello_world_xml_sub -- -d 0

use std::sync::Arc;

#[path = "../common/shutdown.rs"]
mod shutdown;
use shutdown::{cleanup_participant, Shutdown};

use clap::Parser;
use int2dds::{
    common::{
        env::{set_console_log_level, set_log_type},
        log::{LogLevel, LogType},
    },
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::status::StatusMask,
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::SUBSCRIBER_QOS_DEFAULT,
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::type_support::DdsType,
};

/// Participant QoS profile and the datareader declaration path inside the loaded XML.
const PROFILE: &str = "HelloWorld::Sub";
const READER_PATH: &str = "PL::SubApp::sub::reader";
const PROFILE_XML: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hello_world/profiles/xml/sub_profile.xml");

#[derive(Parser, Debug)]
#[command(
    about = "Hello World DDS Subscriber (reader topic + QoS from XML)",
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

struct SubListener;

impl DataReaderListener for SubListener {
    type Foo = HelloWorldType;
    fn on_data_available(
        &self,
        reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        if let Ok(samples) = reader.take(
            1,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples.iter() {
                if let Ok(data) = sample.data() {
                    println!("Read sample: {:?}", data);
                }
            }
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
    let subscriber =
        participant.create_subscriber(SUBSCRIBER_QOS_DEFAULT, None, StatusMask::default()).unwrap();
    // The reader's topic (name, type, QoS) and its own QoS all come from the XML.
    let reader = subscriber
        .create_datareader_from_config::<HelloWorldType>(
            READER_PATH,
            Some(Arc::new(SubListener)),
            StatusMask::default(),
        )
        .unwrap();

    let rqos = reader.get_qos().unwrap();
    println!("[subscriber INFO] domain_id: {}, reader: {}", domain_id, READER_PATH);
    println!(
        "[subscriber qos] reliability: {:?}, durability: {:?}, history: {:?}",
        rqos.reliability.kind, rqos.durability.kind, rqos.history.kind
    );

    shutdown.wait();

    drop(reader);
    cleanup_participant(participant);
}
