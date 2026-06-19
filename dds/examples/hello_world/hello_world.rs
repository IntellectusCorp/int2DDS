use std::sync::{
    mpsc::{sync_channel, SyncSender},
    Arc,
};
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
    infrastructure::{
        qos_policy::{
            DeadlineQosPolicy, DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy,
            HistoryQosPolicyKind, OwnershipQosPolicy, OwnershipQosPolicyKind,
            OwnershipStrengthQosPolicy, PartitionQosPolicy, ReliabilityQosPolicy,
            ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::{
        data_writer_listener::DataWriterListener,
        qos::{DataWriterQos, PublisherQos, DATAWRITER_QOS_DEFAULT, PUBLISHER_QOS_DEFAULT},
    },
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos, DATAREADER_QOS_DEFAULT, SUBSCRIBER_QOS_DEFAULT},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::{
        qos::{TopicQos, TOPIC_QOS_DEFAULT},
        type_support::DdsType,
    },
};
use log::{info, warn};

#[derive(Parser, Debug)]
#[command(about = "Hello World DDS Example", disable_help_flag = true)]
struct Args {
    /// Print help
    #[arg(long, action = clap::ArgAction::Help)]
    help: Option<bool>,

    /// Run as publisher
    #[arg(short = 'P', long, conflicts_with = "subscriber")]
    publisher: bool,

    /// Run as subscriber
    #[arg(short = 'S', long, conflicts_with = "publisher")]
    subscriber: bool,

    /// Topic name
    #[arg(short = 'T', long, default_value = "hello_world_topic")]
    topic: String,

    /// Domain ID
    #[arg(short = 'd', long, default_value_t = 0)]
    domain: i32,

    /// Publish interval in ms [publisher only]
    #[arg(short = 'i', long, default_value_t = 1000)]
    interval: u64,

    /// Use transient-local durability [default: volatile]
    #[arg(short = 't', long = "transient-local")]
    transient_local: bool,

    /// Use reliable reliability [default: best-effort]
    #[arg(short = 'r', long)]
    reliable: bool,

    /// Deadline period in ms [default: infinite]
    #[arg(short = 'f', long)]
    deadline: Option<u64>,

    /// Ownership exclusive [subscriber: -o, publisher: -o <strength>]
    #[arg(short = 'o', long, num_args = 0..=1, default_missing_value = "0")]
    ownership: Option<i32>,

    /// Partition name
    #[arg(short = 'p', long)]
    partition: Option<String>,

    /// History depth: 0 = keep-all, N = keep-last N
    #[arg(short = 'k', long, default_value_t = 1)]
    keep: i32,

    /// Message size in bytes [publisher only, pads message to this size]
    #[arg(short = 's', long)]
    size: Option<usize>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "final")]
struct HelloWorldType {
    index: u32,
    message: String,
}

struct QosConfig {
    reliability: ReliabilityQosPolicyKind,
    durability: DurabilityQosPolicyKind,
    history: HistoryQosPolicyKind,
    deadline: Duration,
    ownership_kind: OwnershipQosPolicyKind,
    ownership_strength: i32,
    partition: PartitionQosPolicy,
}

fn ms_to_duration(ms: u64) -> Duration {
    Duration { sec: (ms / 1000) as i32, nanosec: ((ms % 1000) * 1_000_000) as u32 }
}

fn deadline_str(period: Duration) -> String {
    if period == Duration::infinite() {
        "INFINITE".to_string()
    } else {
        format!("{:?}", period)
    }
}

/// True when QoS should come from an env-loaded profile (default-sentinel mode).
fn profile_mode() -> bool {
    std::env::var("DDS_QOS_PROFILE").is_ok()
}

/// Logs the QoS source after factory init (the log facade is active by then).
/// In profile mode, warns when `DDS_QOS_PROFILE` is set but no profile resolved
/// (bad path / parse error / no default profile selected) — entities then fall
/// back to spec-default QoS.
fn log_qos_source(factory: &DomainParticipantFactory) {
    if !profile_mode() {
        info!("Profile was not set; running without a QoS profile (CLI args / UDP)");
    } else if factory.default_profile_path().is_none() {
        warn!(
            "DDS_QOS_PROFILE is set but no profile resolved (bad path / parse error / \
             no default profile selected); using spec-default QoS"
        );
    }
}

/// Builds entity QoS from the CLI flags. Used only when `profile_mode()` is false.
fn build_qos(args: &Args, is_publisher: bool) -> QosConfig {
    let reliability = if args.reliable {
        ReliabilityQosPolicyKind::Reliable
    } else {
        ReliabilityQosPolicyKind::BestEffort
    };
    let durability = if args.transient_local {
        DurabilityQosPolicyKind::TransientLocal
    } else {
        DurabilityQosPolicyKind::Volatile
    };
    let history = if args.keep == 0 {
        HistoryQosPolicyKind::KeepAll
    } else {
        HistoryQosPolicyKind::KeepLast(args.keep)
    };
    let deadline = match args.deadline {
        Some(ms) => ms_to_duration(ms),
        None => Duration::infinite(),
    };
    let (ownership_kind, ownership_strength) = match args.ownership {
        Some(s) => {
            if is_publisher && s <= 0 {
                eprintln!("Error: publisher ownership strength must be greater than 0 (use -o <strength>)");
                std::process::exit(1);
            }
            (OwnershipQosPolicyKind::Exclusive, s)
        }
        None => (OwnershipQosPolicyKind::Shared, 0),
    };
    let partition = match &args.partition {
        Some(name) => PartitionQosPolicy { name: vec![name.clone()] },
        None => PartitionQosPolicy::default(),
    };
    QosConfig {
        reliability,
        durability,
        history,
        deadline,
        ownership_kind,
        ownership_strength,
        partition,
    }
}

// Transport (and any participant QoS) can also be injected directly in code,
// instead of loading a profile via DDS_QOS_PROFILE. The block below is the
// code-level equivalent of profiles/tcp/profile.json — pass its result to
// create_participant to bind TCP without a profile file.
/// ```no_run
/// fn build_participant_qos(is_publisher: bool) -> DomainParticipantQos {
///    let (bind_port, peer_port) = if is_publisher { (7400, 7401) } else { (7401, 7400) };
///    let mut property = PropertyQosPolicy::default();
///    property.add_property("int2dds.transport", "tcp", false);
///    property.set_tcp_bind_port(bind_port);
///    property.add_property("int2dds.initial_peers", format!("127.0.0.1:{peer_port}"), false);
///    DomainParticipantQos { property, ..Default::default() }
/// }
/// ```

// Publisher Listener
struct PubListener;

impl DataWriterListener for PubListener {
    type Foo = HelloWorldType;
    fn on_publication_matched(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        status: &int2dds::infrastructure::status::PublicationMatchedStatus,
    ) {
        if status.current_count() > 0 {
            info!("Subscriber matched! Sending start signal...");
        } else {
            info!("No subscribers. Sending stop signal...");
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

// Subscriber Listener
struct SubListener {
    sender: SyncSender<bool>,
}

impl DataReaderListener for SubListener {
    type Foo = HelloWorldType;
    fn on_subscription_matched(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Publisher matched! Sending start signal...");
            let _ = self.sender.try_send(true);
        } else {
            println!("No Publishers. Sending stop signal...");
            let _ = self.sender.try_send(false);
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
                let sample = sample.data().unwrap();
                println!("Read sample: {:?}", sample);
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

fn run_publisher(args: &Args) {
    let shutdown = Shutdown::install();
    let factory = DomainParticipantFactory::get_instance();
    log_qos_source(factory);
    let participant = factory
        .create_participant(args.domain, PARTICIPANT_QOS_DEFAULT, None, StatusMask::default())
        .unwrap();

    let topic = if profile_mode() {
        participant.create_topic::<HelloWorldType>(
            &args.topic,
            "HelloWorldType",
            TOPIC_QOS_DEFAULT,
            None,
            StatusMask::default(),
        )
    } else {
        participant.create_topic::<HelloWorldType>(
            &args.topic,
            "HelloWorldType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
    }
    .unwrap();

    let (_publisher, writer) = if profile_mode() {
        let publisher = participant
            .create_publisher(PUBLISHER_QOS_DEFAULT, None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic,
                DATAWRITER_QOS_DEFAULT,
                Some(Arc::new(PubListener)),
                StatusMask::default(),
            )
            .unwrap();
        (publisher, writer)
    } else {
        let qos = build_qos(args, true);
        let publisher = participant
            .create_publisher(
                PublisherQos { partition: qos.partition.clone(), ..Default::default() },
                None,
                StatusMask::default(),
            )
            .unwrap();
        let writer_qos = DataWriterQos {
            reliability: ReliabilityQosPolicy {
                kind: qos.reliability,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            durability: DurabilityQosPolicy { kind: qos.durability },
            history: HistoryQosPolicy { kind: qos.history, strict: true },
            deadline: DeadlineQosPolicy { period: qos.deadline },
            ownership: OwnershipQosPolicy { kind: qos.ownership_kind },
            ownership_strength: OwnershipStrengthQosPolicy { value: qos.ownership_strength },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic,
                writer_qos,
                Some(Arc::new(PubListener)),
                StatusMask::default(),
            )
            .unwrap();
        (publisher, writer)
    };

    // Report the effective QoS (works in both modes).
    let wqos = writer.get_qos().unwrap();
    println!(
        "********* [publisher INFO] domain_id: {}, hostname: {:?}, topic: {}",
        args.domain,
        hostname::get().unwrap(),
        args.topic
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

    while !shutdown.is_stopped() {
        let status = writer.get_publication_matched_status().unwrap();
        if status.current_count() > 0 {
            break;
        }
        if shutdown.wait_timeout(StdDuration::from_millis(100)) {
            drop(writer);
            cleanup_participant(participant);
            return;
        }
    }

    let reliability_str = match wqos.reliability.kind {
        ReliabilityQosPolicyKind::BestEffort => "best_effort",
        ReliabilityQosPolicyKind::Reliable => "reliable",
    };

    let mut i = 1;
    let period = StdDuration::from_millis(args.interval);
    while !shutdown.is_stopped() {
        let mut message = format!(
            "[{:?}]HelloWorld_{}_d{}",
            hostname::get().unwrap(),
            reliability_str,
            args.domain
        );
        if let Some(target) = args.size {
            if message.len() < target {
                message.extend(std::iter::repeat('x').take(target - message.len()));
            } else if message.len() > target {
                message.truncate(target);
            }
        }
        let data = HelloWorldType { index: i, message };
        writer.write(&data, InstanceHandle::NIL).unwrap();
        info!("Published {:?}", data);
        if shutdown.wait_timeout(period) {
            break;
        }
        i += 1;
    }

    drop(writer);
    cleanup_participant(participant);
}

fn run_subscriber(args: &Args) {
    let shutdown = Shutdown::install();
    let factory = DomainParticipantFactory::get_instance();
    log_qos_source(factory);

    let participant = factory
        .create_participant(args.domain, PARTICIPANT_QOS_DEFAULT, None, StatusMask::default())
        .unwrap();

    let topic = if profile_mode() {
        participant.create_topic::<HelloWorldType>(
            &args.topic,
            "HelloWorldType",
            TOPIC_QOS_DEFAULT,
            None,
            StatusMask::default(),
        )
    } else {
        participant.create_topic::<HelloWorldType>(
            &args.topic,
            "HelloWorldType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
    }
    .unwrap();

    let (sender, _receiver) = sync_channel(0);
    let (_subscriber, reader) = if profile_mode() {
        let subscriber = participant
            .create_subscriber(SUBSCRIBER_QOS_DEFAULT, None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                DATAREADER_QOS_DEFAULT,
                Some(Arc::new(SubListener { sender })),
                StatusMask::default(),
            )
            .unwrap();
        (subscriber, reader)
    } else {
        let qos = build_qos(args, false);
        let subscriber = participant
            .create_subscriber(
                SubscriberQos { partition: qos.partition.clone(), ..Default::default() },
                None,
                StatusMask::default(),
            )
            .unwrap();
        let reader_qos = DataReaderQos {
            reliability: ReliabilityQosPolicy {
                kind: qos.reliability,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            durability: DurabilityQosPolicy { kind: qos.durability },
            history: HistoryQosPolicy { kind: qos.history, strict: true },
            deadline: DeadlineQosPolicy { period: qos.deadline },
            ownership: OwnershipQosPolicy { kind: qos.ownership_kind },
            ..Default::default()
        };
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                reader_qos,
                Some(Arc::new(SubListener { sender })),
                StatusMask::default(),
            )
            .unwrap();
        (subscriber, reader)
    };

    let rqos = reader.get_qos().unwrap();
    println!(
        "********* [subscriber INFO] domain_id: {}, hostname: {:?}, topic: {}",
        args.domain,
        hostname::get().unwrap(),
        args.topic
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

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let args = Args::parse();

    if !args.publisher && !args.subscriber {
        eprintln!("Error: either -P (--publisher) or -S (--subscriber) is required");
        std::process::exit(1);
    }

    if args.publisher {
        run_publisher(&args);
    } else {
        run_subscriber(&args);
    }
}
