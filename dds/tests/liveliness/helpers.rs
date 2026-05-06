// Test helpers for the liveliness matrix.
//
// Just two pieces, deliberately:
//   1. Scenario  — bundles participant(s)/topic(s)/publisher/subscriber.
//      Intra and inter setups diverge enough (one vs two participants,
//      one vs two topic handles) that inlining the boilerplate per cell
//      buries the actual transition under setup noise.
//   2. wait_for_reader_state — polls get_liveliness_changed_status() against
//      a target (alive, not_alive). We poll instead of using a WaitSet:
//      LIVELINESS_CHANGED is edge-trigger and latches on first observation,
//      so a forgotten drain in one phase produces a stale-flag race in the
//      next. Polling is level-checked and idempotent; that race is what
//      drove this whole refactor.
//
// QoS construction stays inline at call sites — the test should read like
// a recipe, not a sequence of helper calls.

use std::time::Instant;

use int2dds::dcps::{
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
    },
    infrastructure::status::{LivelinessChangedStatus, StatusMask},
    publication::{
        data_writer::DataWriter,
        publisher::Publisher,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
        subscriber::Subscriber,
    },
    topic::{qos::TopicQos, topic::Topic},
};

use crate::common::*;

pub struct Scenario {
    pub publisher: Publisher,
    pub subscriber: Subscriber,
    writer_topic: Topic,
    reader_topic: Topic,
}

impl Scenario {
    // Single participant — exercises intra-participant unmatch
    // (cleanup_remote_writer in participant.rs).
    pub fn intra() -> Self {
        let participant = make_participant();
        let topic = make_topic(&participant);
        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        Self { publisher, subscriber, writer_topic: topic.clone(), reader_topic: topic }
    }

    // Two participants on the same domain — exercises the SEDP dispose path.
    pub fn inter() -> Self {
        let domain_id = next_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let writer_participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let reader_participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let writer_topic = make_topic(&writer_participant);
        let reader_topic = make_topic(&reader_participant);
        let publisher = writer_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let subscriber = reader_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        Self { publisher, subscriber, writer_topic, reader_topic }
    }

    pub fn create_writer(&self, qos: DataWriterQos) -> DataWriter<KeyedDataType> {
        self.publisher
            .create_datawriter::<KeyedDataType>(
                &self.writer_topic,
                qos,
                None,
                StatusMask::default(),
            )
            .unwrap()
    }

    pub fn create_reader(&self, qos: DataReaderQos) -> DataReader<KeyedDataType> {
        self.subscriber
            .create_datareader::<KeyedDataType>(
                &self.reader_topic,
                qos,
                None,
                StatusMask::default(),
            )
            .unwrap()
    }
}

fn make_participant() -> DomainParticipant {
    let domain_id = next_domain_id();
    DomainParticipantFactory::get_instance()
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap()
}

fn make_topic(participant: &DomainParticipant) -> Topic {
    participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

// Poll until the reader's liveliness counts hit the target.
// Returns the matching status on success, an explanatory string on timeout.
pub fn wait_for_liveliness_changed_state(
    reader: &DataReader<KeyedDataType>,
    target_alive: i32,
    target_not_alive: i32,
    deadline: std::time::Duration,
) -> Result<LivelinessChangedStatus, String> {
    let start = Instant::now();
    loop {
        let status = reader.get_liveliness_changed_status().unwrap();
        if status.alive_count() == target_alive && status.not_alive_count() == target_not_alive {
            return Ok(status);
        }
        if start.elapsed() >= deadline {
            return Err(format!(
                "wait_for_reader_state timed out: want (alive={}, not_alive={}), \
                 last seen (alive={}, not_alive={})",
                target_alive,
                target_not_alive,
                status.alive_count(),
                status.not_alive_count()
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
