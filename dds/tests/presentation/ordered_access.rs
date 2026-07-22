use std::thread;

use crate::common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
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

// OA_10 (dds-rtps) reduced: a TOPIC + ordered_access reader presents samples across instances in
// topic-wide publication order, while a default INSTANCE reader presents them grouped per instance.
#[test]
fn topic_ordered_reader_gets_publication_order_instance_reader_gets_blocks() {
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
    let topic_ordered = PresentationQosPolicy {
        access_scope: PresentationQosAccessScopeKind::Topic,
        coherent_access: false,
        ordered_access: true,
    };

    // Publisher must offer TOPIC + ordered so a TOPIC+ordered reader is QoS-compatible.
    let publisher_qos = PublisherQos { presentation: topic_ordered, ..Default::default() };
    let writer_qos =
        DataWriterQos { reliability: reliable, history: keep_all, ..Default::default() };
    let writer = create_datawriter(&participant, publisher_qos, writer_qos);

    // Reader A: TOPIC + ordered -> topic-wide DESTINATION_ORDER across instances.
    let reader_topic = create_datareader(
        &participant,
        SubscriberQos { presentation: topic_ordered, ..Default::default() },
        DataReaderQos { reliability: reliable, history: keep_all, ..Default::default() },
    );

    // Reader B: default INSTANCE presentation -> per-instance blocks.
    let reader_instance = create_datareader(
        &participant,
        SubscriberQos::default(),
        DataReaderQos { reliability: reliable, history: keep_all, ..Default::default() },
    );

    wait_for_reader_status(
        &reader_topic,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(2),
    )
    .unwrap();
    wait_for_reader_status(
        &reader_instance,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(2),
    )
    .unwrap();
    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, Duration::from_seconds(2))
        .unwrap();

    // Interleave two instances in publication order: k1=10, k2=20, k1=11, k2=21.
    for (key, value) in [(1, 10), (2, 20), (1, 11), (2, 21)] {
        writer.write(&KeyedDataType::new(key, value), InstanceHandle::NIL).unwrap();
    }

    wait_for_reader_status(&reader_topic, StatusMask::DATA_AVAILABLE, Duration::from_seconds(2))
        .unwrap();
    wait_for_reader_status(&reader_instance, StatusMask::DATA_AVAILABLE, Duration::from_seconds(2))
        .unwrap();
    thread::sleep(std::time::Duration::from_millis(300));

    let topic_samples = reader_topic
        .read(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )
        .unwrap();
    let instance_samples = reader_instance
        .read(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )
        .unwrap();

    let topic_values: Vec<i16> = topic_samples.iter().map(|s| s.data().unwrap().value).collect();
    let instance_keys: Vec<i16> = instance_samples.iter().map(|s| s.data().unwrap().key).collect();

    // TOPIC+ordered: samples across instances in publication order.
    assert_eq!(topic_values, vec![10, 20, 11, 21]);
    // INSTANCE: each instance's samples grouped (not interleaved); block order is handle-defined.
    assert!(
        instance_keys == vec![1, 1, 2, 2] || instance_keys == vec![2, 2, 1, 1],
        "instance-scope keys should be grouped by instance, got {:?}",
        instance_keys
    );
}
