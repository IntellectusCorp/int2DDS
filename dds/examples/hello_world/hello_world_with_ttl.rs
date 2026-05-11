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
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{
            DeadlineQosPolicy, DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy,
            HistoryQosPolicyKind, OwnershipQosPolicy, OwnershipQosPolicyKind,
            OwnershipStrengthQosPolicy, PartitionQosPolicy, PropertyQosPolicy,
            ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::{
        data_writer_listener::DataWriterListener,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::{qos::TopicQos, type_support::DdsType},
};
use log::info;

const MULTICAST_TTL: u8 = 64;

#[derive(Parser, Debug)]
#[command(about = "Hello World DDS Example (multicast TTL = 64)", disable_help_flag = true)]
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

fn build_participant_qos() -> DomainParticipantQos {
    let mut property = PropertyQosPolicy::default();
    property.set_multicast_ttl(MULTICAST_TTL);
    DomainParticipantQos { property, ..Default::default() }
}

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
    let qos = build_qos(args, true);
    let shutdown = Shutdown::install();

    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(args.domain, build_participant_qos(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<HelloWorldType>(
            &args.topic,
            "HelloWorld",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher_qos = PublisherQos { partition: qos.partition.clone(), ..Default::default() };
    let publisher =
        participant.create_publisher(publisher_qos, None, StatusMask::default()).unwrap();

    let writer_qos = DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: qos.reliability,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        durability: DurabilityQosPolicy { kind: qos.durability },
        history: HistoryQosPolicy { kind: qos.history },
        deadline: DeadlineQosPolicy { period: qos.deadline },
        ownership: OwnershipQosPolicy { kind: qos.ownership_kind },
        ownership_strength: OwnershipStrengthQosPolicy { value: qos.ownership_strength },
        ..Default::default()
    };

    let listener = PubListener;
    let writer = publisher
        .create_datawriter::<HelloWorldType>(
            &topic,
            writer_qos,
            Some(Arc::new(listener)),
            StatusMask::default(),
        )
        .unwrap();

    println!(
        "********* [publisher INFO] domain_id: {:?}, hostname: {:?}, reliability: {:?}, topic: {}, multicast_ttl: {}",
        args.domain,
        hostname::get().unwrap(),
        qos.reliability,
        args.topic,
        MULTICAST_TTL,
    );
    let deadline_str = if qos.deadline == Duration::infinite() {
        "INFINITE".to_string()
    } else {
        format!("{:?}", qos.deadline)
    };
    println!(
        "********* [publisher qos info] interval: {}ms, reliability: {:?}, durability: {:?}, \
         history: {:?}, deadline: {}, ownership: {:?}(strength: {}), partition: {:?}",
        args.interval,
        qos.reliability,
        qos.durability,
        qos.history,
        deadline_str,
        qos.ownership_kind,
        qos.ownership_strength,
        qos.partition.name,
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

    let reliability_str = match qos.reliability {
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
            args.domain,
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
    let qos = build_qos(args, false);
    let shutdown = Shutdown::install();

    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(args.domain, build_participant_qos(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<HelloWorldType>(
            &args.topic,
            "HelloWorld",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber_qos = SubscriberQos { partition: qos.partition.clone(), ..Default::default() };
    let subscriber =
        participant.create_subscriber(subscriber_qos, None, StatusMask::default()).unwrap();

    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: qos.reliability,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        durability: DurabilityQosPolicy { kind: qos.durability },
        history: HistoryQosPolicy { kind: qos.history },
        deadline: DeadlineQosPolicy { period: qos.deadline },
        ownership: OwnershipQosPolicy { kind: qos.ownership_kind },
        ..Default::default()
    };

    let (sender, _receiver) = sync_channel(0);
    let read_listener = SubListener { sender };
    let _reader = subscriber
        .create_datareader::<HelloWorldType>(
            &topic,
            reader_qos.clone(),
            Some(Arc::new(read_listener)),
            StatusMask::default(),
        )
        .unwrap();

    println!(
        "********* [subscriber INFO] domain_id: {:?}, hostname: {:?}, reliability: {:?}, topic: {}, multicast_ttl: {}",
        args.domain,
        hostname::get().unwrap(),
        qos.reliability,
        args.topic,
        MULTICAST_TTL,
    );
    let deadline_str = if qos.deadline == Duration::infinite() {
        "INFINITE".to_string()
    } else {
        format!("{:?}", qos.deadline)
    };
    println!(
        "********* [subscriber qos info] reliability: {:?}, durability: {:?}, \
         history: {:?}, ownership_kind: {:?}, deadline: {}, partition: {:?}",
        qos.reliability,
        qos.durability,
        qos.history,
        qos.ownership_kind,
        deadline_str,
        qos.partition.name,
    );

    shutdown.wait();

    drop(_reader);
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
