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
    core::time::Duration as DdsDuration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::PARTICIPANT_QOS_DEFAULT},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::{
        data_writer_listener::DataWriterListener,
        qos::{DataWriterQos, PUBLISHER_QOS_DEFAULT},
    },
    topic::{qos::TOPIC_QOS_DEFAULT, type_support::DdsType},
};
use log::info;

const TOPIC_NAME: &str = "hello_world_topic";
const PUBLISH_INTERVAL: StdDuration = StdDuration::from_millis(1000);

/// Reliability is selectable on the CLI; the remaining QoS comes from the
/// spec default. Richer profiles live in the int2DDS-examples repository.
#[derive(Parser, Debug)]
#[command(
    about = "Hello World DDS Publisher (best-effort by default, --reliable for reliable)",
    disable_help_flag = true
)]
struct Args {
    /// Print help
    #[arg(long, action = clap::ArgAction::Help)]
    help: Option<bool>,

    /// Domain ID
    #[arg(short = 'd', long, default_value_t = 0)]
    domain: i32,

    /// Use RELIABLE reliability (default is BEST_EFFORT)
    #[arg(long, default_value_t = false)]
    reliable: bool,
}

// HelloWorld type generated from idl/input/HelloWorld.idl by int2dds-idl.
#[derive(DdsType)]
pub struct HelloWorld {
    pub index: u32,
    pub message: String,
}

struct PubListener;

impl DataWriterListener for PubListener {
    type Foo = HelloWorld;
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

    let args = Args::parse();
    let domain_id = args.domain;
    let reliability_kind = if args.reliable {
        ReliabilityQosPolicyKind::Reliable
    } else {
        ReliabilityQosPolicyKind::BestEffort
    };
    let shutdown = Shutdown::install();
    let factory = DomainParticipantFactory::get_instance();

    let participant = factory
        .create_participant(domain_id, PARTICIPANT_QOS_DEFAULT, None, StatusMask::default())
        .unwrap();
    let topic = participant
        .create_topic::<HelloWorld>(
            TOPIC_NAME,
            "HelloWorld",
            TOPIC_QOS_DEFAULT,
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        participant.create_publisher(PUBLISHER_QOS_DEFAULT, None, StatusMask::default()).unwrap();
    let writer_qos = DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: reliability_kind,
            max_blocking_time: DdsDuration { sec: 0, nanosec: 100_000_000 },
        },
        ..Default::default()
    };
    let writer = publisher
        .create_datawriter::<HelloWorld>(
            &topic,
            writer_qos,
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
        let data = HelloWorld {
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
