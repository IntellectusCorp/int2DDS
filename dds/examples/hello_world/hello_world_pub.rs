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
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::PARTICIPANT_QOS_DEFAULT},
    infrastructure::status::StatusMask,
    publication::{
        data_writer_listener::DataWriterListener,
        qos::{DATAWRITER_QOS_DEFAULT, PUBLISHER_QOS_DEFAULT},
    },
    topic::{qos::TOPIC_QOS_DEFAULT, type_support::DdsType},
};
use log::{info, warn};

const TOPIC_NAME: &str = "hello_world_topic";
const PUBLISH_INTERVAL: StdDuration = StdDuration::from_millis(1000);

/// The domain id is the only CLI option; everything else is fixed.
#[derive(Parser, Debug)]
#[command(
    about = "Hello World DDS Publisher (QoS via profile or spec default)",
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

    fn on_offered_incompatible_qos(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        status: &int2dds::infrastructure::status::OfferedIncompatibleQosStatus,
    ) {
        info!("Offered incompatible QoS: {:?}", status);
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
        "********* [publisher INFO] domain_id: {}, hostname: {:?}, topic: {}",
        domain_id,
        hostname::get().unwrap(),
        TOPIC_NAME
    );
    println!(
        "********* [publisher qos] reliability: {:?}, durability: {:?}, history: {:?}, \
         deadline: {}, ownership: {:?}(strength: {})",
        wqos.reliability.kind,
        wqos.durability.kind,
        wqos.history.kind,
        deadline_str(wqos.deadline.period),
        wqos.ownership.kind,
        wqos.ownership_strength.value,
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
