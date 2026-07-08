use std::thread;

use crate::common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, PresentationQosAccessScopeKind,
                PresentationQosPolicy, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
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

// A TOPIC+coherent reader sees none of the set members while the set is open,
// then all of them (and nothing else) after end_coherent_changes.
#[test]
fn coherent_set_is_withheld_until_end_coherent_changes() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let reliable = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
    };
    let keep_all = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true };
    let topic_coherent = PresentationQosPolicy {
        access_scope: PresentationQosAccessScopeKind::Topic,
        coherent_access: true,
        ordered_access: false,
    };

    // Publisher must offer TOPIC + coherent so a TOPIC+coherent reader is QoS-compatible.
    let writer = create_datawriter(
        &participant,
        PublisherQos { presentation: topic_coherent, ..Default::default() },
        DataWriterQos { reliability: reliable, history: keep_all, ..Default::default() },
    );
    let reader = create_datareader(
        &participant,
        SubscriberQos { presentation: topic_coherent, ..Default::default() },
        DataReaderQos { reliability: reliable, history: keep_all, ..Default::default() },
    );

    wait_for_reader_status(&reader, StatusMask::SUBSCRIPTION_MATCHED, Duration::from_seconds(2))
        .unwrap();
    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, Duration::from_seconds(2))
        .unwrap();

    let publisher = writer.get_publisher().unwrap();
    publisher.begin_coherent_changes().unwrap();
    for (key, value) in [(1, 10), (2, 20), (3, 30)] {
        writer.write(&KeyedDataType::new(key, value), InstanceHandle::NIL).unwrap();
    }

    // Members must stay invisible while the set is open.
    thread::sleep(std::time::Duration::from_millis(500));
    let early = reader.read(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ALIVE_INSTANCE_STATE],
    );
    assert!(
        matches!(early, Err(DdsError::NoData)),
        "coherent members visible before end_coherent_changes: {:?}",
        early.map(|s| s.len())
    );

    publisher.end_coherent_changes().unwrap();

    wait_for_reader_status(&reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(2)).unwrap();
    thread::sleep(std::time::Duration::from_millis(300));

    // The whole set becomes available at once; the end marker must not surface.
    let samples = reader
        .read(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )
        .unwrap();
    let mut values: Vec<i16> = samples.iter().map(|s| s.data().unwrap().value).collect();
    values.sort_unstable();
    assert_eq!(values, vec![10, 20, 30]);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
