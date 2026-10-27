// PRESENTATION QoS access_scope GROUP integration tests.

use std::sync::Arc;
use std::time::Instant;

use crate::common::*;
use int2dds::{
    core::time::Duration,
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
    },
    infrastructure::{
        qos_policy::{
            DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy, HistoryQosPolicyKind,
            PresentationQosAccessScopeKind, PresentationQosPolicy, ReliabilityQosPolicy,
            ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::{
        data_writer::DataWriter,
        publisher::Publisher,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        subscriber::Subscriber,
    },
    topic::qos::TopicQos,
    DataReaderBase,
};

mod coherent;
mod ordered;
mod ordered_coherent;

pub(crate) const ANY_SAMPLE_STATES: &[SampleStateKind] = &[SampleStateKind::ANY_SAMPLE_STATE];
pub(crate) const ANY_VIEW_STATES: &[ViewStateKind] = &[ViewStateKind::ANY_VIEW_STATE];
pub(crate) const ANY_INSTANCE_STATES: &[InstanceStateKind] =
    &[InstanceStateKind::ANY_INSTANCE_STATE];
pub(crate) const NOT_READ_SAMPLE_STATES: &[SampleStateKind] =
    &[SampleStateKind::NOT_READ_SAMPLE_STATE];

pub(crate) const MATCH_TIMEOUT: Duration = Duration { sec: 10, nanosec: 0 };
pub(crate) const POLL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
pub(crate) const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);
// Long enough for a sample that is on its way to arrive, so that an empty reader means the set is
// being held back rather than still in flight.
pub(crate) const SETTLE: std::time::Duration = std::time::Duration::from_millis(300);

pub(crate) fn group_presentation(ordered_access: bool) -> PresentationQosPolicy {
    PresentationQosPolicy {
        access_scope: PresentationQosAccessScopeKind::Group,
        coherent_access: false,
        ordered_access,
    }
}

pub(crate) fn reliable_keep_all_writer_qos() -> DataWriterQos {
    DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
        ..Default::default()
    }
}

pub(crate) fn reliable_keep_all_reader_qos() -> DataReaderQos {
    DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
        ..Default::default()
    }
}

pub(crate) fn create_participant(domain_id: i32) -> DomainParticipant {
    DomainParticipantFactory::get_instance()
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap()
}

pub(crate) fn delete_participants(
    writer_participant: DomainParticipant,
    reader_participant: DomainParticipant,
) {
    let factory = DomainParticipantFactory::get_instance();
    writer_participant.delete_contained_entities().unwrap();
    reader_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    factory.delete_participant(reader_participant).unwrap();
}

pub(crate) fn make_publisher(
    participant: &DomainParticipant,
    presentation: PresentationQosPolicy,
) -> Publisher {
    participant
        .create_publisher(
            PublisherQos { presentation, ..Default::default() },
            None,
            StatusMask::default(),
        )
        .unwrap()
}

pub(crate) fn make_writer(
    participant: &DomainParticipant,
    publisher: &Publisher,
    topic_name: &str,
) -> DataWriter<KeyedDataType> {
    make_writer_with_qos(participant, publisher, topic_name, reliable_keep_all_writer_qos())
}

pub(crate) fn make_writer_with_qos(
    participant: &DomainParticipant,
    publisher: &Publisher,
    topic_name: &str,
    qos: DataWriterQos,
) -> DataWriter<KeyedDataType> {
    let topic = participant
        .create_topic::<KeyedDataType>(
            topic_name,
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    publisher.create_datawriter::<KeyedDataType>(&topic, qos, None, StatusMask::default()).unwrap()
}

pub(crate) fn make_subscriber(
    participant: &DomainParticipant,
    presentation: PresentationQosPolicy,
) -> Subscriber {
    participant
        .create_subscriber(
            SubscriberQos { presentation, ..Default::default() },
            None,
            StatusMask::default(),
        )
        .unwrap()
}

pub(crate) fn make_reader(
    participant: &DomainParticipant,
    subscriber: &Subscriber,
    topic_name: &str,
) -> DataReader<KeyedDataType> {
    make_reader_with_qos(participant, subscriber, topic_name, reliable_keep_all_reader_qos())
}

pub(crate) fn make_reader_with_qos(
    participant: &DomainParticipant,
    subscriber: &Subscriber,
    topic_name: &str,
    qos: DataReaderQos,
) -> DataReader<KeyedDataType> {
    let topic = participant
        .create_topic::<KeyedDataType>(
            topic_name,
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    subscriber.create_datareader::<KeyedDataType>(&topic, qos, None, StatusMask::default()).unwrap()
}

pub(crate) fn wait_for_match(writer: &DataWriter<KeyedDataType>) {
    wait_for_writer_status(writer, StatusMask::PUBLICATION_MATCHED, MATCH_TIMEOUT)
        .expect("timed out waiting for the writer to match a reader");
}

// Waits on the writer side, so that a write after it actually has somewhere to go.
pub(crate) fn wait_for_matched_readers(writer: &DataWriter<KeyedDataType>, expected_readers: i32) {
    let deadline = Instant::now() + POLL_TIMEOUT;

    loop {
        let matched = writer
            .get_publication_matched_status()
            .map(|status| status.current_count())
            .unwrap_or(0);

        if matched >= expected_readers {
            return;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for {} matched readers, the writer saw {}",
            expected_readers,
            matched
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

// Polls until the Subscriber lists the expected number of entries and returns them with the
// access block open. Nothing reaches a reader while a block is open, so every attempt opens its own.
pub(crate) fn wait_for_entries(
    subscriber: &Subscriber,
    expected_entries: usize,
) -> Vec<Arc<dyn DataReaderBase<Qos = DataReaderQos>>> {
    let deadline = Instant::now() + POLL_TIMEOUT;

    loop {
        subscriber.begin_access().unwrap();

        let entries = subscriber
            .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .unwrap();

        if entries.len() == expected_entries {
            return entries;
        }

        subscriber.end_access().unwrap();

        assert!(
            Instant::now() < deadline,
            "timed out waiting for {} entries, the collection held {}",
            expected_entries,
            entries.len()
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

// Polls until the reader holds the expected number of samples, without consuming any.
pub(crate) fn wait_for_samples(reader: &DataReader<KeyedDataType>, expected_samples: usize) {
    let deadline = Instant::now() + POLL_TIMEOUT;

    loop {
        let samples = reader
            .read(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .map(|samples| samples.len())
            .unwrap_or(0);

        if samples == expected_samples {
            return;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for {} samples, the reader held {}",
            expected_samples,
            samples
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

// Every value the entry's reader holds. Without ordered_access one call empties it.
pub(crate) fn take_all(entry: &Arc<dyn DataReaderBase<Qos = DataReaderQos>>) -> Vec<i16> {
    let reader = entry
        .as_any()
        .downcast_ref::<DataReader<KeyedDataType>>()
        .expect("the entry is a KeyedDataType reader");

    reader
        .take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .map(|samples| samples.iter().map(|sample| sample.data().unwrap().value).collect())
        .unwrap_or_default()
}

pub(crate) fn take_one(entry: &Arc<dyn DataReaderBase<Qos = DataReaderQos>>) -> i16 {
    let reader = entry
        .as_any()
        .downcast_ref::<DataReader<KeyedDataType>>()
        .expect("the entry is a KeyedDataType reader");

    let samples = reader.take(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES).unwrap();
    assert_eq!(samples.len(), 1, "group ordered access hands out one sample per call");

    samples[0].data().unwrap().value
}

pub(crate) fn group_coherent_presentation(ordered_access: bool) -> PresentationQosPolicy {
    PresentationQosPolicy {
        access_scope: PresentationQosAccessScopeKind::Group,
        coherent_access: true,
        ordered_access,
    }
}

pub(crate) fn transient_local_writer_qos() -> DataWriterQos {
    DataWriterQos {
        durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal },
        ..reliable_keep_all_writer_qos()
    }
}

pub(crate) fn transient_local_reader_qos() -> DataReaderQos {
    DataReaderQos {
        durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal },
        ..reliable_keep_all_reader_qos()
    }
}
