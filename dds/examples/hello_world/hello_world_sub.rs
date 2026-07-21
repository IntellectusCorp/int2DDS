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
    core::time::Duration as DdsDuration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::PARTICIPANT_QOS_DEFAULT},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SUBSCRIBER_QOS_DEFAULT},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::{qos::TOPIC_QOS_DEFAULT, type_support::DdsType},
};
use log::info;

const TOPIC_NAME: &str = "hello_world_topic";

/// Reliability is selectable on the CLI; the remaining QoS comes from the
/// spec default. Richer profiles live in the int2DDS-examples repository.
#[derive(Parser, Debug)]
#[command(
    about = "Hello World DDS Subscriber (best-effort by default, --reliable for reliable)",
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

struct SubListener;

impl DataReaderListener for SubListener {
    type Foo = HelloWorld;
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
    let subscriber =
        participant.create_subscriber(SUBSCRIBER_QOS_DEFAULT, None, StatusMask::default()).unwrap();
    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: reliability_kind,
            max_blocking_time: DdsDuration { sec: 0, nanosec: 100_000_000 },
        },
        ..Default::default()
    };
    let reader = subscriber
        .create_datareader::<HelloWorld>(
            &topic,
            reader_qos,
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
