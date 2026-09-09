//! Fragment repair must make progress on its own, not once per heartbeat period.
//!
//! Every DATA_FRAG carries a piggybacked HEARTBEAT, and the reader schedules its NACK_FRAG from
//! that heartbeat. The reader drops a heartbeat whose `count` did not advance -- and it drops it
//! before scheduling the timer -- so a repair round that reuses one count is invisible. The only
//! remaining stimulus is then the periodic heartbeat, and repair proceeds one round per
//! `heartbeat_period` (2 s by default).
//!
//! Measured on the development board with a 1 MiB sample at 1400-byte fragments: 18-22 s with
//! the reused count, 0.15-0.17 s once each retransmitted fragment carries a fresh one. The
//! deadline below sits between those.
//!
//! Caveat: this only exercises repair where the burst actually loses fragments. A 1 MiB burst
//! overruns a default-sized socket receive buffer, which is the case on this board and on CI
//! (the workflow asks for 5 MiB but the kernel clamps to `net.core.rmem_max`). On a host with a
//! genuinely large receive buffer the sample may arrive intact and the test passes without
//! testing anything.

use crate::common::*;
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

/// One fragment per datagram, so a 1 MiB sample is ~750 of them arriving back to back.
const FRAGMENT_SIZE: i32 = 1400;
const PAYLOAD_BYTES: usize = 1024 * 1024;

/// Far above the 0.17 s a self-driving repair takes, far below the 18 s a heartbeat-gated one
/// needs. Wide enough that a slow or loaded machine cannot trip it by being slow alone.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct RepairPayload {
    index: i32,
    data: Vec<u8>,
}

#[test]
fn repair_completes_without_waiting_for_the_periodic_heartbeat() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<RepairPayload>(
            "FragRepairTopic",
            "RepairPayload",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let reliability = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration::from_seconds(5),
    };
    // KeepAll: the sample must stay in the writer's cache until every fragment is acked, which is
    // what repair retransmits from.
    let history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: false };

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<RepairPayload>(
            &topic,
            DataReaderQos { reliability, history, ..DataReaderQos::default() },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    // Default heartbeat_period (2 s) on purpose: the point is that repair does not depend on it.
    let data_writer = publisher
        .create_datawriter::<RepairPayload>(
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

    // A fixed byte pattern so a mis-reassembled fragment shows up as corruption rather than as a
    // sample that merely arrived.
    let payload = RepairPayload { index: 0, data: vec![0xA5; PAYLOAD_BYTES] };
    let started = Instant::now();
    data_writer.write(&payload, InstanceHandle::NIL).unwrap();

    let mut received: Option<RepairPayload> = None;
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
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let elapsed = started.elapsed();

    let _ = participant.delete_contained_entities();
    let _ = factory.delete_participant(participant);

    let sample = received.unwrap_or_else(|| {
        panic!(
            "a {PAYLOAD_BYTES}-byte RELIABLE sample was not reassembled within {DEADLINE:?}. \
             Repair is waiting on the periodic heartbeat again -- check that every retransmitted \
             DATA_FRAG carries a heartbeat whose count advanced."
        )
    });

    assert_eq!(sample.index, 0);
    assert_eq!(
        sample.data.len(),
        PAYLOAD_BYTES,
        "reassembled to the wrong length after {elapsed:?}, so fragments were lost or duplicated"
    );
    assert!(
        sample.data.iter().all(|byte| *byte == 0xA5),
        "reassembled to the right length but the wrong content, so fragments landed at the wrong \
         offsets"
    );
}
