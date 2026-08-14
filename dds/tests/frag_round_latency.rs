//! A multi-window sample costs one round trip per window, not one round trip plus fixed delays.
//!
//! A writer sends at most two thirds of a peer's advertised receive buffer toward that peer and
//! drops the rest, so a large sample completes over several rounds. Each round is: the heartbeat
//! riding the window's last datagram, the reader's NACK_FRAG, the writer's answer. Two fixed
//! delays used to sit inside that loop -- the reader's `nack_frag_response_delay` and the
//! writer's `nack_response_delay` -- and both now default to zero, because one heartbeat per
//! window leaves the debounce nothing to collapse and the merge nothing to merge.
//!
//! The test is a paired A/B on the same host, in the same process, back to back: one transfer at
//! the defaults, one with the two delays put back to their pre-window values through QoS. Paired,
//! because an absolute millisecond bound would only be measuring this board. If the delays no
//! longer govern round latency -- because they are hardcoded, or because the default is not zero
//! -- the two arms take the same time and the ratio assertion fails.
//!
//! Two backstops are narrowed in *both* arms, because either one alone can carry a transfer of
//! this size without the round loop working at all: the reader's NACK_FRAG retry chain (default
//! ten self-re-arming retries) is cut to two, and the writer's periodic heartbeat (default 2 s)
//! is pushed past the deadline. Neither fires in a healthy run of either arm -- each window's
//! heartbeat re-arms the reader before its retry comes due -- so narrowing them changes no
//! timing, it only stops them standing in for the mechanism under test.
//!
//! The receive buffer is pinned rather than inherited, because the window is two thirds of what
//! the host grants and the host is not the test's to choose: where a runner grants megabytes the
//! whole sample lands in one window, no round ever happens, and both arms measure a plain burst.
//!
//! Run with `INT2DDS_DATA_FRAG_SIZE=1344 INT2DDS_MAX_MESSAGE_SIZE=13440`, the deployment shape.

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
    },
    infrastructure::{
        qos_policy::{
            DataFragQosPolicy, HistoryQosPolicy, HistoryQosPolicyKind,
            ReaderReliabilityExtensionQosPolicy, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
            WriterReliabilityExtensionQosPolicy,
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

const FRAGMENT_SIZE: i32 = 1344;

/// Several windows against the pinned receive buffer below, so the number of rounds is large
/// enough for a per-round delay to dominate the transfer.
const PAYLOAD_BYTES: usize = 512 * 1024;

/// Asked of every socket this process opens, before the first one binds. Linux grants twice
/// this and the other platforms grant it as asked; either way a window holds far less than the
/// sample, which is the only property this test needs from it.
const SOCKET_BUFFER_BYTES: &str = "131072";

/// Generous: it only has to catch a transfer that never completes at all.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// The pre-window values, restored through QoS for the delayed arm.
const DELAYED_NACK_FRAG_RESPONSE_MS: i64 = 80;
const DELAYED_NACK_RESPONSE_MS: i64 = 100;

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct LatencyPayload {
    index: i32,
    data: Vec<u8>,
}

/// One writer/reader pair on its own topic, differing only in the two delays under test.
/// Returns how long the sample took from `write` to a complete, byte-correct `take`.
fn time_one_multi_window_sample(
    participant: &DomainParticipant,
    topic_name: &str,
    nack_frag_response_delay: Duration,
    nack_response_delay: Duration,
) -> std::time::Duration {
    let topic = participant
        .create_topic::<LatencyPayload>(
            topic_name,
            "LatencyPayload",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let reliability = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration::from_seconds(5),
    };
    // KeepAll: every round after the first is served out of the writer's cache.
    let history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: false };

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_reliability_extension = ReaderReliabilityExtensionQosPolicy {
        nack_frag_response_delay,
        nack_frag_max_retries: 2,
        ..ReaderReliabilityExtensionQosPolicy::default()
    };
    let data_reader = subscriber
        .create_datareader::<LatencyPayload>(
            &topic,
            DataReaderQos {
                reliability,
                history,
                reader_reliability_extension,
                ..DataReaderQos::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let writer_reliability_extension = WriterReliabilityExtensionQosPolicy {
        nack_response_delay,
        heartbeat_period: Duration::from_seconds(60),
        ..WriterReliabilityExtensionQosPolicy::default()
    };
    let data_writer = publisher
        .create_datawriter::<LatencyPayload>(
            &topic,
            DataWriterQos {
                reliability,
                history,
                data_frag: DataFragQosPolicy { max_size: FRAGMENT_SIZE },
                writer_reliability_extension,
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

    let payload =
        LatencyPayload { index: 0, data: (0..PAYLOAD_BYTES).map(|i| (i % 251) as u8).collect() };
    let started = Instant::now();
    data_writer.write(&payload, InstanceHandle::NIL).unwrap();

    let mut received: Option<LatencyPayload> = None;
    let deadline = started + DEADLINE;
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
        // Well under one round trip, so the poll interval is not what is being measured.
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let elapsed = started.elapsed();

    let sample = received.unwrap_or_else(|| {
        panic!("{topic_name}: a {PAYLOAD_BYTES}-byte sample never completed within {DEADLINE:?}")
    });
    assert_eq!(sample.data.len(), PAYLOAD_BYTES, "{topic_name}: reassembled to the wrong length");
    let first_bad = sample.data.iter().enumerate().find(|(i, byte)| **byte != (i % 251) as u8);
    assert!(first_bad.is_none(), "{topic_name}: byte {:?} is wrong", first_bad.map(|(i, _)| i));

    elapsed
}

#[test]
fn a_multi_window_sample_is_paced_by_the_round_trip_not_by_the_fixed_delays() {
    // Read by every listener as it binds, so it has to be set before the first participant.
    // This binary holds one test, so no other test in the process sees the change.
    std::env::set_var("INT2DDS_UDP_SOCKET_BUFFER", SOCKET_BUFFER_BYTES);

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    // The default arm runs first, so it and not the delayed arm pays whatever one-off warm-up
    // the first large transfer costs. That biases against the assertion below.
    let at_defaults = time_one_multi_window_sample(
        &participant,
        "FragRoundLatencyDefault",
        ReaderReliabilityExtensionQosPolicy::default().nack_frag_response_delay,
        WriterReliabilityExtensionQosPolicy::default().nack_response_delay,
    );
    let with_delays = time_one_multi_window_sample(
        &participant,
        "FragRoundLatencyDelayed",
        Duration::from_millis(DELAYED_NACK_FRAG_RESPONSE_MS),
        Duration::from_millis(DELAYED_NACK_RESPONSE_MS),
    );

    let _ = participant.delete_contained_entities();
    let _ = factory.delete_participant(participant);

    // Every round pays 180 ms in the delayed arm and nothing in the default one, and this sample
    // needs enough rounds that the gap is an order of magnitude. Threefold is far inside that,
    // and asserting a ratio rather than an absolute keeps the test honest on any board.
    assert!(
        at_defaults * 3 < with_delays,
        "a multi-window sample took {at_defaults:?} at the default delays and {with_delays:?} \
         with the pre-window 80ms/100ms restored. The two are too close for the delays to be \
         what paces a round: either the default is not zero, or the scheduling sites no longer \
         read the QoS, or the sample no longer spans more than one window."
    );
}
