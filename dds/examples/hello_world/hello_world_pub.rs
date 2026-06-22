use std::sync::Arc;
use std::time::Duration as StdDuration;

#[path = "../common/shutdown.rs"]
mod shutdown;
use shutdown::{cleanup_participant, Shutdown};

use int2dds::{
    common::{
        env::{init_from_env, set_console_log_level, set_log_type},
        instance_handle::InstanceHandle,
        log::{LogLevel, LogType},
    },
    domain::{domain_participant_factory::DomainParticipantFactory, qos::PARTICIPANT_QOS_DEFAULT},
    infrastructure::status::StatusMask,
    publication::{
        data_writer_listener::DataWriterListener,
        qos::{DATAWRITER_QOS_DEFAULT, PUBLISHER_QOS_DEFAULT},
    },
    topic::{qos::TOPIC_QOS_DEFAULT, type_support::DdsType},
};
use log::info;

const TOPIC_NAME: &str = "hello_world_topic";
const PUBLISH_INTERVAL: StdDuration = StdDuration::from_millis(1000);
const DEFAULT_DOMAIN: i32 = 0;

fn parse_domain_id() -> i32 {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-d" || arg == "--domain" {
            if let Some(Ok(id)) = args.next().map(|v| v.parse::<i32>()) {
                return id;
            }
        }
    }
    DEFAULT_DOMAIN
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

    fn on_offered_incompatible_qos(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        status: &int2dds::infrastructure::status::OfferedIncompatibleQosStatus,
    ) {
        info!("Offered incompatible QoS: {:?}", status);
    }
}

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);
    init_from_env();

    let domain_id = parse_domain_id();
    let shutdown = Shutdown::install();
    let factory = DomainParticipantFactory::get_instance();

    let participant = factory
        .create_participant(domain_id, PARTICIPANT_QOS_DEFAULT, None, StatusMask::default())
        .unwrap();
    let topic = participant
        .create_topic::<HelloWorldType>(
            TOPIC_NAME,
            "HelloWorldType",
            TOPIC_QOS_DEFAULT,
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        participant.create_publisher(PUBLISHER_QOS_DEFAULT, None, StatusMask::default()).unwrap();
    let writer = publisher
        .create_datawriter::<HelloWorldType>(
            &topic,
            DATAWRITER_QOS_DEFAULT,
            Some(Arc::new(PubListener)),
            StatusMask::default(),
        )
        .unwrap();

    let wqos = writer.get_qos().unwrap();
    println!(
        "[publisher INFO] domain_id: {}, hostname: {:?}, topic: {}",
        domain_id,
        hostname::get().unwrap(),
        TOPIC_NAME
    );
    println!(
        "[publisher qos] reliability: {:?}, durability: {:?}, history: {:?} ",
        wqos.reliability.kind, wqos.durability.kind, wqos.history.kind
    );

    // Wait until a subscriber matches before publishing.
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
        let data = HelloWorldType {
            index,
            message: format!("[{:?}]HelloWorld_d{}", hostname::get().unwrap(), domain_id),
        };
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
