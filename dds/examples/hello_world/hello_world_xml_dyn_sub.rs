//! Hello World subscriber built entirely from XML in one call: the whole tree
//! (participant + subscriber + datareader + topic) is declared in the
//! `<domain_participant_library>` and instantiated by `create_participant_from_config`.
//! The type comes from `<types>`, so data is received as `DynamicData`.
//!
//! Run (from the `dds` crate dir):
//!   cargo run --example hello_world_xml_dyn_sub

use std::time::Duration as StdDuration;

#[path = "../common/shutdown.rs"]
mod shutdown;
use shutdown::{cleanup_participant, Shutdown};

use int2dds::{
    common::{
        env::{set_console_log_level, set_log_type},
        log::{LogLevel, LogType},
    },
    domain::domain_participant_factory::DomainParticipantFactory,
    subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
};

const CONFIG_PATH: &str = "PL::SubApp";
const READER_NAME: &str = "sub::reader";
const PROFILE_XML: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hello_world/profiles/xml/sub_profile.xml");

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let shutdown = Shutdown::install();
    let factory = DomainParticipantFactory::get_instance();

    factory.load_profiles(&[PROFILE_XML]).expect("failed to load XML config");

    // One call builds participant + subscriber + datareader + topic from the XML tree.
    let configured = factory.create_participant_from_config(CONFIG_PATH).unwrap();
    let participant = configured.participant.clone();
    let reader = configured.datareader(READER_NAME).expect("datareader not found in config");

    println!(
        "[subscriber INFO] built from config '{}', reader '{}' (DynamicData)",
        CONFIG_PATH, READER_NAME
    );

    while !shutdown.is_stopped() {
        if let Ok(samples) = reader.take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples.iter() {
                if let Ok(data) = sample.data() {
                    let index: u32 = data.get("index").unwrap_or_default();
                    let message: String = data.get("message").unwrap_or_default();
                    println!("Read sample: index={}, message={:?}", index, message);
                }
            }
        }
        if shutdown.wait_timeout(StdDuration::from_millis(200)) {
            break;
        }
    }

    drop(reader);
    drop(configured);
    cleanup_participant(participant);
}
