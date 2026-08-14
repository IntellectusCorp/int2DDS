//! Fragmented delivery under XCDR2, which is the only representation that takes
//! the in-place fragment decode path.
//!
//! The other `frag_*` tests all write the default representation (XCDR1), so the
//! payload arrives as classic CDR and is read by the classic chained route. The
//! XCDR2 route is separate code and had no end-to-end coverage: a panic dropped
//! into `Xcdr2Deserializer::new_chained` left every one of them passing. This
//! writes XCDR2 so the fragments go through that route, and checks the sample
//! comes back byte-identical.
//!
//! Both a keyless and a keyed topic, because only the keyless one reaches the
//! route. A fragmented sample carries no KeyHash, so a keyed reader falls back
//! to computing the key from the data — `fallback_instance_handle` deserializes
//! `data_value()` while committing the change, which materializes the payload
//! before the application ever asks for it. Measured, not assumed: a panic in
//! `Xcdr2Deserializer::new_chained` fires for the keyless test and not the keyed
//! one. The keyed case is here to hold that line, so that if key extraction
//! later reads across the chunks instead, this says which test should start
//! covering the route.
//!
//! What these cannot catch: the derive route takes the in-place result only if
//! it decodes, so a chained reader that *errors* falls back to the materialized
//! buffer and every assertion below still holds — verified by corrupting the
//! body offset, which left both tests green. That fallback is the point of the
//! design, but it means the guard against a broken chained reader is the kernel's
//! own vectors, which call it with nothing to fall back to. These pin the two
//! things those cannot see: that the route is reached at all, and that what it
//! produces survives the whole pub/sub path.

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                DataRepresentationId, DataRepresentationQosPolicy, HistoryQosPolicy,
                HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
            },
            status::StatusMask,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::{qos::TopicQos, type_support::DdsType},
    },
};

/// Big enough to fragment several times over. The members after the bulk one are
/// what a stitching mistake shows up in: they are read at offsets that depend on
/// the sequence length being read correctly first.
#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct LargeXcdr2Data {
    index: i32,
    payload: Vec<u8>,
    label: String,
    checksum: u64,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct LargeKeyedData {
    #[dds(key)]
    id: i32,
    payload: Vec<u8>,
    label: String,
}

const PAYLOAD_LEN: usize = 1024 * 1024;
const SAMPLES: i32 = 4;

fn payload_for(index: i32) -> Vec<u8> {
    (0..PAYLOAD_LEN).map(|i| (i as u8).wrapping_add(index as u8)).collect()
}

fn xcdr2() -> DataRepresentationQosPolicy {
    DataRepresentationQosPolicy { value: vec![DataRepresentationId::Xcdr2DataRepresentation] }
}

fn writer_qos() -> DataWriterQos {
    DataWriterQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        data_representation: xcdr2(),
        ..Default::default()
    }
}

fn reader_qos() -> DataReaderQos {
    DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        data_representation: xcdr2(),
        ..Default::default()
    }
}

/// Poll until `want` samples arrive or the deadline passes. Reliable repair of
/// several MiB takes however long it takes; a fixed sleep turns a slow run into
/// a failure.
fn collect<T: DdsType + 'static>(
    reader: &int2dds::dcps::subscription::data_reader::DataReader<T>,
    want: usize,
) -> Vec<T> {
    let mut collected = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while collected.len() < want && std::time::Instant::now() < deadline {
        if let Ok(samples) = reader.take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples {
                if let Ok(data) = sample.data() {
                    collected.push(data);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    collected
}

#[test]
fn xcdr2_fragments_decode_to_the_written_sample() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<LargeXcdr2Data>(
            "frag_xcdr2_topic",
            "LargeXcdr2Data",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer = publisher
        .create_datawriter::<LargeXcdr2Data>(&topic, writer_qos(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<LargeXcdr2Data>(&topic, reader_qos(), None, StatusMask::default())
        .unwrap();

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(5),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(5),
    )
    .unwrap();

    for i in 0..SAMPLES {
        data_writer
            .write(
                &LargeXcdr2Data {
                    index: i,
                    payload: payload_for(i),
                    label: format!("sample-{i}"),
                    checksum: 0x0123_4567_89AB_CDEF ^ i as u64,
                },
                InstanceHandle::NIL,
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let mut collected = collect(&data_reader, SAMPLES as usize);
    collected.sort_by_key(|d| d.index);

    assert_eq!(collected.len(), SAMPLES as usize, "not every XCDR2 sample was reassembled in 60 s");
    for (i, data) in collected.iter().enumerate() {
        let i = i as i32;
        assert_eq!(data.index, i);
        // Compare the whole buffer, not just its length: stitching the chunks
        // wrong yields the right count of bytes in the wrong order.
        assert_eq!(data.payload, payload_for(i), "payload of sample {i}");
        // Read after the bulk member, so its offset depends on that one.
        assert_eq!(data.label, format!("sample-{i}"));
        assert_eq!(data.checksum, 0x0123_4567_89AB_CDEF ^ i as u64);
    }

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn keyed_xcdr2_fragments_decode_to_the_written_sample() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<LargeKeyedData>(
            "frag_xcdr2_keyed_topic",
            "LargeKeyedData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer = publisher
        .create_datawriter::<LargeKeyedData>(&topic, writer_qos(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<LargeKeyedData>(&topic, reader_qos(), None, StatusMask::default())
        .unwrap();

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(5),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(5),
    )
    .unwrap();

    // One instance, several samples. Every one of them is materialized at
    // reception to recover the key; none takes the in-place route.
    for i in 0..SAMPLES {
        data_writer
            .write(
                &LargeKeyedData { id: 7, payload: payload_for(i), label: format!("keyed-{i}") },
                InstanceHandle::NIL,
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let collected = collect(&data_reader, SAMPLES as usize);

    assert_eq!(collected.len(), SAMPLES as usize, "not every keyed sample was reassembled in 60 s");
    for data in &collected {
        assert_eq!(data.id, 7);
        let i: i32 = data.label.strip_prefix("keyed-").expect("label survived").parse().unwrap();
        assert_eq!(data.payload, payload_for(i), "payload of sample {i}");
    }

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
