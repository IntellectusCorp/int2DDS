//! Fragmented delivery that fits in a default-sized UDP receive buffer.
//!
//! `frag.rs` writes 10 x 1 MB and needs `net.core.rmem_max` raised well past the Linux default
//! (212992 on this class of machine). Where the sysctl is not set -- a fresh box after reboot,
//! and GitHub runners, which silently clamp the 5 MiB `INT2DDS_UDP_SOCKET_BUFFER` the workflow
//! asks for -- it fails for environmental reasons and stops guarding anything.
//!
//! That leaves the DATA_FRAG reassembly path with no usable coverage on exactly the machines
//! most likely to run it. These tests shrink the fragment size through
//! `DataFragQosPolicy::max_size` instead of growing the payload, so a sample still spans many
//! fragments while the whole exchange stays far inside the default buffer.
//!
//! The RELIABLE case is the one that matters for `UserLogic::handle_heartbeat_message`: a
//! heartbeat drives `has_fragmented_changes` / `all_fragments_received` and the buffered-change
//! flush, which is where reassembled samples are handed to the reader.

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{
            DataFragQosPolicy, HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy,
            ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
    DdsType,
};
use std::time::Instant;

/// 1400-byte fragments over a 20 KiB payload gives ~15 fragments per sample: enough to exercise
/// reassembly, small enough that 4 samples in flight stay under a 208 KiB socket buffer.
const FRAGMENT_SIZE: i32 = 1400;
const PAYLOAD_BYTES: usize = 20 * 1024;
const SAMPLE_COUNT: usize = 4;
const DELIVERY_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct FragPayload {
    index: i32,
    data: Vec<u8>,
}

/// Each sample carries a distinct byte pattern so a mis-reassembled fragment shows up as
/// corrupted content, not just a missing sample.
fn payload_for(index: i32) -> FragPayload {
    FragPayload { index, data: vec![(index as u8).wrapping_add(1); PAYLOAD_BYTES] }
}

fn run_fragmented_exchange(reliability: ReliabilityQosPolicyKind) {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<FragPayload>(
            "FragSmallTopic",
            "FragPayload",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let reliability =
        ReliabilityQosPolicy { kind: reliability, max_blocking_time: Duration::from_millis(500) };
    let history = HistoryQosPolicy {
        kind: HistoryQosPolicyKind::KeepLast(SAMPLE_COUNT as i32 * 2),
        ..HistoryQosPolicy::default()
    };

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<FragPayload>(
            &topic,
            DataReaderQos {
                reliability: reliability.clone(),
                history: history.clone(),
                ..DataReaderQos::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let data_writer = publisher
        .create_datawriter::<FragPayload>(
            &topic,
            DataWriterQos {
                reliability,
                history,
                data_frag: DataFragQosPolicy { max_size: FRAGMENT_SIZE },
                ..DataWriterQos::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(5),
    )
    .expect("writer never matched the reader");
    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(5),
    )
    .expect("reader never matched the writer");

    for index in 0..SAMPLE_COUNT as i32 {
        data_writer.write(&payload_for(index), InstanceHandle::NIL).unwrap();
    }

    // Poll rather than sleep once: RELIABLE promises eventual delivery, not delivery by a
    // deadline, so a sample repaired after the first poll still counts.
    let mut received: Vec<FragPayload> = Vec::new();
    let deadline = Instant::now() + DELIVERY_DEADLINE;
    while received.len() < SAMPLE_COUNT && Instant::now() < deadline {
        if let Ok(samples) = data_reader.take(
            SAMPLE_COUNT as i32,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples {
                if let Ok(data) = sample.data() {
                    received.push(data);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let delivered = received.len();
    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();

    assert_eq!(
        delivered, SAMPLE_COUNT,
        "expected {SAMPLE_COUNT} reassembled samples within {DELIVERY_DEADLINE:?}, got {delivered}"
    );

    received.sort_by_key(|sample| sample.index);
    for (position, sample) in received.iter().enumerate() {
        assert_eq!(sample.index, position as i32, "sample index mismatch after reassembly");
        assert_eq!(
            sample.data.len(),
            PAYLOAD_BYTES,
            "sample {} was reassembled to the wrong length",
            sample.index
        );
        let expected = (sample.index as u8).wrapping_add(1);
        assert!(
            sample.data.iter().all(|byte| *byte == expected),
            "sample {} has corrupted content after reassembly",
            sample.index
        );
    }
}

/// RELIABLE drives the heartbeat path, where reassembled samples are flushed to the reader.
#[test]
fn fragmented_samples_reassemble_reliable() {
    run_fragmented_exchange(ReliabilityQosPolicyKind::Reliable);
}

#[test]
fn fragmented_samples_reassemble_best_effort() {
    run_fragmented_exchange(ReliabilityQosPolicyKind::BestEffort);
}

/// Proves the other two cases are not passing unfragmented.
///
/// They use a 20 KiB payload, which the stack would happily deliver in one datagram if
/// `DataFragQosPolicy::max_size` were ever ignored, so on their own they cannot tell
/// "reassembled correctly" from "never fragmented". This payload is larger than
/// `MAX_UDP_PACKET_BYTES` (64 KiB), so arriving at all requires DATA_FRAG.
#[test]
fn payload_larger_than_one_datagram_requires_fragmentation() {
    const OVERSIZED_BYTES: usize = 100 * 1024;

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let topic = participant
        .create_topic::<FragPayload>(
            "FragOversizedTopic",
            "FragPayload",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let reliability = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration::from_millis(500),
    };

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<FragPayload>(
            &topic,
            DataReaderQos { reliability: reliability.clone(), ..DataReaderQos::default() },
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let data_writer = publisher
        .create_datawriter::<FragPayload>(
            &topic,
            DataWriterQos {
                reliability,
                data_frag: DataFragQosPolicy { max_size: FRAGMENT_SIZE },
                ..DataWriterQos::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(5),
    )
    .expect("writer never matched the reader");
    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(5),
    )
    .expect("reader never matched the writer");

    data_writer
        .write(&FragPayload { index: 0, data: vec![0x5a; OVERSIZED_BYTES] }, InstanceHandle::NIL)
        .unwrap();

    let mut delivered = None;
    let deadline = Instant::now() + DELIVERY_DEADLINE;
    while delivered.is_none() && Instant::now() < deadline {
        if let Ok(samples) = data_reader.take(
            1,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            if let Some(sample) = samples.first() {
                delivered = sample.data().ok();
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();

    let sample = delivered.unwrap_or_else(|| {
        panic!("a {OVERSIZED_BYTES}-byte sample never arrived within {DELIVERY_DEADLINE:?}")
    });
    assert_eq!(sample.data.len(), OVERSIZED_BYTES, "oversized sample reassembled to a bad length");
    assert!(
        sample.data.iter().all(|byte| *byte == 0x5a),
        "oversized sample has corrupted content after reassembly"
    );
}
