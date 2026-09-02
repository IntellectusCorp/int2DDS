//! A fragmented sample longer than one receive window has to complete across several windows.
//!
//! A writer sends at most two thirds of a peer's advertised receive buffer toward that peer in
//! one go and drops the rest. What reaches the wire is therefore a prefix of the sample; the
//! reader learns the total fragment count from any DATA_FRAG, so the untransmitted tail already
//! reads as missing to it, and the heartbeat riding the window's last datagram is what lets it
//! ask. Every round after the first is that exchange.
//!
//! The receive buffer is pinned rather than inherited, because the window is two thirds of what
//! the host grants: a runner that grants megabytes takes the whole sample in one window, and the
//! transfer then completes without a single round. Pinned, several rounds are mandatory.
//!
//! Two backstops are turned down on purpose, because either one alone carries a transfer of this
//! size to completion whether or not a heartbeat ever arrives, and a test that cannot tell those
//! apart is not testing the window: the reader's NACK_FRAG retry chain (default ten self-re-arming
//! retries) is cut to two, and the writer's periodic heartbeat (default 2 s) is pushed out of the
//! way. What remains is the heartbeat riding each window's last datagram -- the only thing that
//! keeps the rounds coming. Both defaults are what production uses; they are narrowed here so the
//! test fails when the mechanism it names is absent.
//!
//! Run with `INT2DDS_DATA_FRAG_SIZE=1344 INT2DDS_MAX_MESSAGE_SIZE=13440`, the deployment shape.

use crate::common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
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

/// Several windows against the pinned receive buffer below, whatever `INT2DDS_MAX_MESSAGE_SIZE`
/// packs into one datagram: the window is two thirds of that buffer, so it takes several either
/// way -- at the 13440 of the deployment shape, and at the 65000 default.
const PAYLOAD_BYTES: usize = 512 * 1024;

/// Asked of every socket this process opens, before the first one binds. Linux grants twice
/// this and the other platforms grant it as asked; either way a window holds far less than the
/// sample, which is the only property this test needs from it.
const SOCKET_BUFFER_BYTES: &str = "131072";

/// A healthy multi-window transfer costs one round trip per window, and at the zero default
/// delays those rounds are tens of milliseconds in total. With the two backstops narrowed above,
/// a writer that heartbeats only on the sample's last datagram rather than each window's never
/// finishes at all, which is the gap this deadline has to be generous enough to attribute.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct WindowPayload {
    index: i32,
    data: Vec<u8>,
}

#[test]
fn a_sample_several_windows_long_completes() {
    // Read by every listener as it binds, so it has to be set before the first participant.
    // This binary holds one test, so no other test in the process sees the change.
    std::env::set_var("INT2DDS_UDP_SOCKET_BUFFER", SOCKET_BUFFER_BYTES);

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<WindowPayload>(
            "FragWindowTopic",
            "WindowPayload",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let reliability = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration::from_seconds(5),
    };
    // KeepAll: the sample must stay in the writer's cache for the rounds after the first, which
    // are served entirely out of it.
    let history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: false };

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    // Two retries, not the default ten: enough to ride out a lost heartbeat, far too few to
    // carry the eight or so rounds this sample needs.
    let reader_reliability_extension = ReaderReliabilityExtensionQosPolicy {
        nack_frag_max_retries: 2,
        ..ReaderReliabilityExtensionQosPolicy::default()
    };
    let data_reader = subscriber
        .create_datareader::<WindowPayload>(
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
    // The periodic heartbeat pushed well past the deadline, so it cannot stand in for the
    // per-window one. A healthy transfer never needs it.
    let writer_reliability_extension = WriterReliabilityExtensionQosPolicy {
        heartbeat_period: Duration::from_seconds(60),
        ..WriterReliabilityExtensionQosPolicy::default()
    };
    let data_writer = publisher
        .create_datawriter::<WindowPayload>(
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

    // A position-dependent pattern: a fragment delivered at the wrong offset, or a window that
    // re-sent from fragment 1 instead of continuing, shows up as content corruption.
    let payload =
        WindowPayload { index: 0, data: (0..PAYLOAD_BYTES).map(|i| (i % 251) as u8).collect() };
    let started = Instant::now();
    data_writer.write(&payload, InstanceHandle::NIL).unwrap();

    let mut received: Option<WindowPayload> = None;
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
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let elapsed = started.elapsed();

    let _ = participant.delete_contained_entities();
    let _ = factory.delete_participant(participant);

    let sample = received.unwrap_or_else(|| {
        panic!(
            "a {PAYLOAD_BYTES}-byte RELIABLE sample spanning several receive windows was not \
             reassembled within {DEADLINE:?}. Check that a heartbeat rides the last datagram of \
             every window, not only the last datagram of the sample -- without it the reader has \
             no trigger to ask for the next window and falls back on the periodic heartbeat."
        )
    });

    assert_eq!(sample.index, 0);
    assert_eq!(
        sample.data.len(),
        PAYLOAD_BYTES,
        "reassembled to the wrong length after {elapsed:?}, so fragments were lost or duplicated"
    );
    let first_bad = sample.data.iter().enumerate().find(|(i, byte)| **byte != (i % 251) as u8);
    assert!(
        first_bad.is_none(),
        "byte {:?} is wrong, so a window's fragments landed at the wrong offsets",
        first_bad.map(|(i, _)| i)
    );
}
