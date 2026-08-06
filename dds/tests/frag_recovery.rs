//! A RELIABLE fragmented sample must survive lost DATA_FRAGs.
//!
//! Fragments are dropped at the UDP receive buffer during a burst. The reader used to
//! record a sample as received on its first fragment, which hid it from
//! `missing_changes_for_heartbeat`; the writer then acked and freed it while fragments
//! were still outstanding, so the loss could never be repaired. RELIABLE lost whole
//! samples silently.
//!
//! `frag.rs` takes once after a fixed 100 ms sleep, which measures how much arrives in
//! the first burst. RELIABLE promises eventual delivery, not delivery by a deadline, so
//! these tests poll until every sample has arrived or a generous deadline expires -- the
//! only formulation that can tell "repaired late" apart from "lost forever".

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
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
        topic::{qos::TopicQos, type_support::DdsType},
    },
};

const SAMPLE_COUNT: usize = 10;
const PAYLOAD_BYTES: usize = 1024 * 1024;
/// Repair needs several heartbeat/NACK_FRAG round trips. Well above the observed
/// settle time so a pass means "delivered", not "delivered quickly".
const REPAIR_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct LargeData {
    index: i32,
    data: Vec<u8>,
}

/// Writes `SAMPLE_COUNT` samples of `PAYLOAD_BYTES` and polls until all of them have
/// been taken or `REPAIR_DEADLINE` passes. Returns how many distinct samples arrived.
fn deliver_large_samples(fragment_size: i32) -> usize {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<LargeData>(
            "frag_recovery_topic",
            "LargeData",
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

    let reliable = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration { sec: 1, nanosec: 0 },
    };
    // KeepLast(SAMPLE_COUNT) holds every sample of the run, so nothing is evicted, while
    // matching what ROS 2 (and therefore Autoware) actually configures.
    let history = HistoryQosPolicy {
        kind: HistoryQosPolicyKind::KeepLast(SAMPLE_COUNT as i32),
        ..Default::default()
    };

    let writer_qos = DataWriterQos {
        history: history.clone(),
        reliability: reliable.clone(),
        data_frag: DataFragQosPolicy { max_size: fragment_size },
        ..Default::default()
    };
    let reader_qos = DataReaderQos { history, reliability: reliable, ..Default::default() };

    let data_writer = publisher
        .create_datawriter::<LargeData>(&topic, writer_qos, None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<LargeData>(&topic, reader_qos, None, StatusMask::default())
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

    for i in 0..SAMPLE_COUNT {
        data_writer
            .write(
                &LargeData { index: i as i32, data: vec![0; PAYLOAD_BYTES] },
                InstanceHandle::NIL,
            )
            .unwrap();
    }

    // Poll rather than sleep-once: `take` drains what has arrived so far, so a sample
    // repaired after the first poll still counts.
    let mut received = 0usize;
    let deadline = std::time::Instant::now() + REPAIR_DEADLINE;
    while received < SAMPLE_COUNT && std::time::Instant::now() < deadline {
        if let Ok(samples) = data_reader.take(
            SAMPLE_COUNT as i32,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples.iter() {
                if let Ok(data) = sample.data() {
                    assert_eq!(
                        data.data.len(),
                        PAYLOAD_BYTES,
                        "sample {} was reassembled at the wrong length",
                        data.index
                    );
                    received += 1;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();

    received
}

// The default fragment size (65000) makes each DATA_FRAG a ~65 KB datagram.
#[test]
fn test_reliable_large_sample_is_repaired_at_default_fragment_size() {
    let received = deliver_large_samples(DataFragQosPolicy::DEFAULT_SIZE);
    assert_eq!(
        received, SAMPLE_COUNT,
        "RELIABLE must deliver every sample eventually; got {received}/{SAMPLE_COUNT}",
    );
}

// An MTU-sized fragment avoids IP-level fragmentation of each DATA_FRAG, at the cost of
// many more datagrams per sample. Pinned separately so a change to the default cannot
// silently alter what this suite covers.
#[test]
fn test_reliable_large_sample_is_repaired_at_mtu_fragment_size() {
    let received = deliver_large_samples(1400);
    assert_eq!(
        received, SAMPLE_COUNT,
        "RELIABLE must deliver every sample eventually; got {received}/{SAMPLE_COUNT}",
    );
}
