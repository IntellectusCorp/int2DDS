//! A packed DATA_FRAG must reassemble to the same bytes as an unpacked one.
//!
//! At a 1344-byte fragment size a 1 MiB sample is 781 fragments. Sending one per datagram
//! charges the receive socket buffer 781 times; packing them under the message budget sends
//! the same payload in 17. This test fixes the reassembly, not the count: it proves the
//! packed path does not lose, duplicate, or misplace a fragment.
//!
//! The payload is `i % 251` by byte position, not a repeated fill byte: 251 is prime and
//! coprime with the 1344-byte fragment size, so no fragment-aligned reorder or rotation can
//! reproduce the correct sequence by accident. A misplaced-but-fully-covering fragment run
//! changes the bytes at that range and the content assertion catches it; a repeated fill byte
//! would not have.
//!
//! Caveat: on a host whose receive buffer swallows the whole burst either path succeeds, so
//! this guards correctness rather than the loss reduction. The socket buffer is deliberately
//! shrunk below the burst so the repair path runs too.
//!
//! Env is process-global and this file rewrites it, so there is exactly one `#[test]`.

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

const PAYLOAD_BYTES: usize = 1024 * 1024;

/// Small enough that the 1 MiB burst cannot fit, so fragments really are lost and repaired.
/// Shrinking is safe on any host: rmem_max is an upper clamp, never a lower one.
const FORCED_SOCKET_BUFFER: usize = 64 * 1024;

/// Far above the sub-second reassembly seen on the development board, low enough that a
/// stalled repair fails rather than hanging the binary.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct PackedPayload {
    index: i32,
    data: Vec<u8>,
}

/// Position-varying payload: byte `i` is `i % 251`. 251 is prime and coprime with the
/// 1344-byte fragment size, so a fragment run landing at the wrong offset changes the
/// bytes there instead of accidentally reproducing the correct sequence.
fn expected_payload() -> Vec<u8> {
    (0..PAYLOAD_BYTES).map(|i| (i % 251) as u8).collect()
}

/// Byte-for-byte check against `expected_payload()`. Fails on the first mismatching index
/// instead of `assert_eq!`'s whole-vector diff, which for a 1 MiB buffer is unreadable.
fn assert_payload_matches(actual: &[u8], context: &str) {
    let expected = expected_payload();
    for (index, (a, e)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(
            a, e,
            "{context}: byte {index} does not match the expected i % 251 pattern, so a \
             fragment run landed at the wrong offset or is corrupted"
        );
    }
}

/// Sends one `PAYLOAD_BYTES` sample at `fragment_size` and returns the reassembled bytes.
fn round_trip(fragment_size: i32) -> Vec<u8> {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<PackedPayload>(
            "FragPackedTopic",
            "PackedPayload",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let reliability = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration::from_seconds(5),
    };
    // KeepAll: the sample must stay in the writer cache until every fragment is acked,
    // which is what a repair retransmits from.
    let history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: false };

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<PackedPayload>(
            &topic,
            DataReaderQos { reliability, history, ..DataReaderQos::default() },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let data_writer = publisher
        .create_datawriter::<PackedPayload>(
            &topic,
            DataWriterQos {
                reliability,
                history,
                data_frag: DataFragQosPolicy { max_size: fragment_size },
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
        .write(&PackedPayload { index: 0, data: expected_payload() }, InstanceHandle::NIL)
        .unwrap();

    let mut received: Option<PackedPayload> = None;
    let deadline = Instant::now() + DEADLINE;
    while received.is_none() && Instant::now() < deadline {
        if let Ok(samples) = data_reader.take(
            1,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples {
                if let Ok(data) = sample.data() {
                    received = Some(data);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // Tear down before asserting: a panic here unwinds into Drop, which blocks on the
    // receive thread.
    let _ = participant.delete_contained_entities();
    let _ = factory.delete_participant(participant);

    let sample = received.unwrap_or_else(|| {
        panic!(
            "a {PAYLOAD_BYTES}-byte RELIABLE sample at fragment size {fragment_size} was not \
             reassembled within {DEADLINE:?}"
        )
    });
    assert_eq!(sample.index, 0);
    sample.data
}

#[test]
fn a_packed_fragment_burst_reassembles_to_the_same_bytes() {
    int2dds::common::env::set_udp_socket_buffer_size(FORCED_SOCKET_BUFFER);
    int2dds::common::env::set_max_message_size(65_000);

    // 65000 / 65000 = 1 fragment per submessage: the unpacked shape, unchanged by this work.
    let unpacked = round_trip(65_000);
    assert_eq!(unpacked.len(), PAYLOAD_BYTES, "unpacked reassembly lost or gained bytes");
    assert_payload_matches(&unpacked, "unpacked reassembly");

    // 65000 / 1344 = 48 fragments per submessage: the packed shape.
    let packed = round_trip(1_344);
    assert_eq!(
        packed.len(),
        PAYLOAD_BYTES,
        "packed reassembly lost or gained bytes, so a fragment run was mis-sized"
    );
    assert_payload_matches(&packed, "packed reassembly");
}
