//! Hello World publisher built entirely from XML in one call: the whole tree
//! (participant + publisher + datawriter + topic) is declared in the
//! `<domain_participant_library>` and instantiated by `create_participant_from_config`.
//! The type comes from `<types>`, so data is published as `DynamicData`.
//!
//! Run (from the `dds` crate dir):
//!   cargo run --example hello_world_xml_dyn_pub

use std::sync::Arc;
use std::time::Duration as StdDuration;

#[path = "../common/shutdown.rs"]
mod shutdown;
use shutdown::{cleanup_participant, Shutdown};

use int2dds::{
    common::{
        env::{set_console_log_level, set_log_type},
        instance_handle::InstanceHandle,
        log::{LogLevel, LogType},
    },
    domain::domain_participant_factory::DomainParticipantFactory,
};
use log::info;

const PUBLISH_INTERVAL: StdDuration = StdDuration::from_millis(1000);

const CONFIG_PATH: &str = "PL::PubApp";
const WRITER_NAME: &str = "pub::writer";
const TYPE_NAME: &str = "HelloWorldType";
const PROFILE_XML: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hello_world/profiles/xml/pub_profile.xml");

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let shutdown = Shutdown::install();
    let factory = DomainParticipantFactory::get_instance();

    factory.load_profiles(&[PROFILE_XML]).expect("failed to load XML config");

    // One call builds participant + publisher + datawriter + topic from the XML tree.
    let configured = factory.create_participant_from_config(CONFIG_PATH).unwrap();
    let participant = configured.participant.clone();
    let writer = configured.datawriter(WRITER_NAME).expect("datawriter not found in config");
    let type_support = Arc::new(factory.get_dynamic_type_support(TYPE_NAME).unwrap());

    println!(
        "[publisher INFO] built from config '{}', writer '{}' (DynamicData)",
        CONFIG_PATH, WRITER_NAME
    );

    while !shutdown.is_stopped() {
        if writer.get_publication_matched_status().unwrap().current_count() > 0 {
            break;
        }
        if shutdown.wait_timeout(StdDuration::from_millis(100)) {
            drop(writer);
            drop(configured);
            cleanup_participant(participant);
            return;
        }
    }

    let mut index = 1u32;
    while !shutdown.is_stopped() {
        let mut data = type_support.create_data();
        data.set("index", index).unwrap();
        data.set("message", format!("HelloWorld_dyn_{}", index)).unwrap();
        writer.write(&data, InstanceHandle::NIL).unwrap();
        info!("Published index={}", index);
        if shutdown.wait_timeout(PUBLISH_INTERVAL) {
            break;
        }
        index += 1;
    }

    drop(writer);
    drop(configured);
    cleanup_participant(participant);
}
