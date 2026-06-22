use std::sync::Arc;

#[path = "../common/shutdown.rs"]
mod shutdown;
use shutdown::{cleanup_participant, Shutdown};

use int2dds::{
    common::{
        env::{init_from_env, set_console_log_level, set_log_type},
        log::{LogLevel, LogType},
    },
    domain::{domain_participant_factory::DomainParticipantFactory, qos::PARTICIPANT_QOS_DEFAULT},
    infrastructure::status::StatusMask,
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DATAREADER_QOS_DEFAULT, SUBSCRIBER_QOS_DEFAULT},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::{qos::TOPIC_QOS_DEFAULT, type_support::DdsType},
};
use log::info;

const TOPIC_NAME: &str = "hello_world_topic";
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

struct SubListener;

impl DataReaderListener for SubListener {
    type Foo = HelloWorldType;
    fn on_subscription_matched(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Publisher matched!");
        } else {
            println!("No publishers.");
        }
    }

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

    fn on_requested_incompatible_qos(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::RequestedIncompatibleQosStatus,
    ) {
        info!("Requested incompatible QoS: {:?}", status);
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
    let subscriber =
        participant.create_subscriber(SUBSCRIBER_QOS_DEFAULT, None, StatusMask::default()).unwrap();
    let reader = subscriber
        .create_datareader::<HelloWorldType>(
            &topic,
            DATAREADER_QOS_DEFAULT,
            Some(Arc::new(SubListener)),
            StatusMask::default(),
        )
        .unwrap();

    let rqos = reader.get_qos().unwrap();
    println!(
        "[subscriber INFO] domain_id: {}, hostname: {:?}, topic: {}",
        domain_id,
        hostname::get().unwrap(),
        TOPIC_NAME
    );
    println!(
        "[subscriber qos] reliability: {:?}, durability: {:?}, history: {:?}",
        rqos.reliability.kind, rqos.durability.kind, rqos.history.kind
    );

    shutdown.wait();

    drop(reader);
    cleanup_participant(participant);
}
