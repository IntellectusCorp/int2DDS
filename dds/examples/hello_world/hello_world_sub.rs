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
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::PARTICIPANT_QOS_DEFAULT},
    infrastructure::status::StatusMask,
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DATAREADER_QOS_DEFAULT, SUBSCRIBER_QOS_DEFAULT},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::{qos::TOPIC_QOS_DEFAULT, type_support::DdsType},
};
use log::{info, warn};

const TOPIC_NAME: &str = "hello_world_topic";

/// The domain id is the only CLI option; everything else is fixed.
#[derive(Parser, Debug)]
#[command(
    about = "Hello World DDS Subscriber (QoS via profile or spec default)",
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

fn deadline_str(period: Duration) -> String {
    if period == Duration::infinite() {
        "INFINITE".to_string()
    } else {
        format!("{:?}", period)
    }
}

/// Logs whether the effective QoS came from a profile or spec defaults.
fn log_qos_source(factory: &DomainParticipantFactory) {
    if std::env::var("DDS_QOS_PROFILE").is_err() {
        info!("No DDS_QOS_PROFILE set; using spec-default QoS");
    } else if let Some(profile) = factory.default_profile_path() {
        info!("Using QoS profile: {}", profile);
    } else {
        warn!(
            "DDS_QOS_PROFILE is set but no profile resolved (bad path / parse error / \
             no default profile selected); using spec-default QoS"
        );
    }
}

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let domain_id = Args::parse().domain;
    let shutdown = Shutdown::install();
    let factory = DomainParticipantFactory::get_instance();
    log_qos_source(factory);

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
        "********* [subscriber INFO] domain_id: {}, hostname: {:?}, topic: {}",
        domain_id,
        hostname::get().unwrap(),
        TOPIC_NAME
    );
    println!(
        "********* [subscriber qos] reliability: {:?}, durability: {:?}, history: {:?}, \
         deadline: {}, ownership: {:?}",
        rqos.reliability.kind,
        rqos.durability.kind,
        rqos.history.kind,
        deadline_str(rqos.deadline.period),
        rqos.ownership.kind,
    );

    shutdown.wait();

    drop(reader);
    cleanup_participant(participant);
}
