use std::sync::{
    mpsc::{sync_channel, SyncSender},
    Arc,
};

use clap::{Parser, ValueEnum};
use int2dds::{
    common::{
        env::{set_console_log_level, set_log_type},
        instance_handle::InstanceHandle,
        log::{LogLevel, LogType},
    },
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
        wait_set::WaitSet,
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
use speedy::{Readable, Writable};

#[derive(Clone, ValueEnum, Debug)]
enum Role {
    Pub,
    Sub,
}

#[derive(Clone, ValueEnum, Debug)]
enum Reliability {
    BestEffort,
    Reliable,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Hello World DDS Example", long_about = None)]
struct Args {
    /// Role: pub or sub
    #[arg(short, long)]
    role: Role,

    /// Domain ID (e.g., 30, 31, 32, 33 for pub; 40, 41, 42, 43 for sub)
    #[arg(short, long)]
    domain: i32,

    /// Reliability QoS: best-effort or reliable
    #[arg(short = 'q', long, default_value = "best-effort")]
    reliability: Reliability,
}

#[derive(DdsType, Readable, Writable)]
#[dds_type(crate_path = "int2dds")]
struct HelloWorldType {
    index: u32,
    message: String,
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
}

fn run_publisher(domain_id: i32, reliability: Reliability) {
    let participant_qos = DomainParticipantQos::default();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, participant_qos, None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<HelloWorldType>(
            "hello_world_topic",
            "HelloWorld",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let reliability_kind = match reliability {
        Reliability::BestEffort => ReliabilityQosPolicyKind::BestEffort,
        Reliability::Reliable => ReliabilityQosPolicyKind::Reliable,
    };

    let writer_qos = DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: reliability_kind,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
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

    info!(
        "[publisher INFO] domain_id: {:?}, hostname: {:?}, reliability: {:?}",
        domain_id,
        hostname::get().unwrap(),
        reliability_kind
    );

    let mut condition = writer.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();
    wait_set.wait(Duration::infinite()).unwrap();
    writer.get_publication_matched_status().unwrap();

    let reliability_str = match reliability_kind {
        ReliabilityQosPolicyKind::BestEffort => "best_effort",
        ReliabilityQosPolicyKind::Reliable => "reliable",
    };

    let mut i = 0;
    loop {
        let data = HelloWorldType {
            index: i,
            message: format!(
                "[{:?}]HelloWorld_{}_d{}",
                hostname::get().unwrap(),
                reliability_str,
                domain_id
            ),
        };
        writer.write(&data, InstanceHandle::NIL).unwrap();
        info!("Published {:?}", data);
        std::thread::sleep(std::time::Duration::from_millis(1000));
        i += 1;
    }
}

fn run_subscriber(domain_id: i32, reliability: Reliability) {
    let participant_qos = DomainParticipantQos::default();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, participant_qos, None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<HelloWorldType>(
            "hello_world_topic",
            "HelloWorld",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let reliability_kind = match reliability {
        Reliability::BestEffort => ReliabilityQosPolicyKind::BestEffort,
        Reliability::Reliable => ReliabilityQosPolicyKind::Reliable,
    };

    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: reliability_kind,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        ..Default::default()
    };

    let (sender, _receiver) = sync_channel(0);
    let read_listener = SubListener { sender };
    let _reader = subscriber
        .create_datareader::<HelloWorldType>(
            &topic,
            reader_qos,
            Some(Arc::new(read_listener)),
            StatusMask::default(),
        )
        .unwrap();

    println!(
        "[subscriber INFO] domain_id: {:?}, hostname: {:?}, reliability: {:?}",
        domain_id,
        hostname::get().unwrap(),
        reliability_kind
    );

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn main() {
    set_log_type(LogType::Console);
    set_console_log_level(LogLevel::Info);

    let args = Args::parse();

    match args.role {
        Role::Pub => run_publisher(args.domain, args.reliability),
        Role::Sub => run_subscriber(args.domain, args.reliability),
    }
}
