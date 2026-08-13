//! Reader ↔ Writer inside one Participant over TCP.
//!
//! Both endpoints address each other by locator, and that locator is the
//! participant's own listener. The frame must still reach the Reader, and the
//! reliable handshake back to the Writer must complete, without the transport
//! dialling itself.

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, PropertyQosPolicy, ReliabilityQosPolicy,
                ReliabilityQosPolicyKind,
            },
            status::StatusMask,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
    },
};

/// Pure-TCP participant QoS on a pinned port. `initial_peers` is mandatory for
/// TCP but irrelevant here: everything this test exchanges stays inside the one
/// participant, so it points at a port nobody answers.
fn tcp_qos(bind_port: u16) -> DomainParticipantQos {
    let mut property = PropertyQosPolicy::default();
    property.add_property("int2dds.transport", "tcp", false);
    property.add_property("int2dds.initial_peers", "127.0.0.1:1", false);
    property.set_tcp_bind_port(bind_port);
    DomainParticipantQos { property, ..Default::default() }
}

#[test]
fn reliable_writer_reaches_a_reader_in_the_same_participant_over_tcp() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, tcp_qos(17420), None, StatusMask::default())
        .expect("TCP participant binds the pinned listen port");

    let writer_qos = DataWriterQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        ..Default::default()
    };
    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        ..Default::default()
    };

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);
    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(5),
    )
    .expect("reader matches the writer in its own participant");
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(5),
    )
    .expect("writer matches the reader in its own participant");

    data_writer.write(&KeyedDataType::new(1, 42), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(5))
        .expect("user data must reach a reader in the same participant");
    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].data().unwrap().value, 42);

    // A reliable sample the reader acknowledged leaves nothing pending, which
    // only holds if the ACKNACK travelled back the same way.
    data_writer.wait_for_acknowledgments(Duration::from_seconds(5)).unwrap();

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
