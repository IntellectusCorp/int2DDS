use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
        wait_set::WaitSet,
    },
    publication::{
        data_writer::DataWriter,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
    DdsType,
};

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct HelloWorldType {
    index: u32,
    message: String,
}

fn create_no_key_datareader(data_reader_qos: DataReaderQos) -> DataReader<HelloWorldType> {
    let domain_participant_factory = DomainParticipantFactory::get_instance();
    let domain_participant = domain_participant_factory
        .create_participant(99, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = domain_participant
        .create_topic::<HelloWorldType>(
            "TestTopic",
            "HelloWorldType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = domain_participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let reader = subscriber
        .create_datareader::<HelloWorldType>(&topic, data_reader_qos, None, StatusMask::default())
        .unwrap();

    reader
}

fn create_no_key_datawriter(data_writer_qos: DataWriterQos) -> DataWriter<HelloWorldType> {
    let domain_participant_factory = DomainParticipantFactory::get_instance();
    let domain_participant = domain_participant_factory
        .create_participant(99, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = domain_participant
        .create_topic::<HelloWorldType>(
            "TestTopic",
            "HelloWorldType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher = domain_participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();

    let writer = publisher
        .create_datawriter::<HelloWorldType>(&topic, data_writer_qos, None, StatusMask::default())
        .unwrap();

    writer
}

fn besteffort_pubsub_roundtrip(c: &mut Criterion) {
    let data_reader = create_no_key_datareader(DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::BestEffort,
            max_blocking_time: Duration::from_millis(100),
        },
        ..DataReaderQos::default()
    });
    let data_writer = create_no_key_datawriter(DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::BestEffort,
            max_blocking_time: Duration::from_millis(100),
        },
        ..DataWriterQos::default()
    });

    let writer_condition = data_writer.get_statuscondition().unwrap().clone();
    writer_condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(writer_condition).unwrap();
    wait_set.wait(Duration::infinite()).unwrap();
    data_writer.get_publication_matched_status().unwrap();

    let reader_condition = data_reader.get_statuscondition().unwrap().clone();
    reader_condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(reader_condition).unwrap();
    wait_set.wait(Duration::infinite()).unwrap();
    data_reader.get_subscription_matched_status().unwrap();

    let data = HelloWorldType { index: 0, message: "HelloWorld".to_string() };

    let mut group = c.benchmark_group("--besteffort_pubsub");

    group.bench_function("besteffort_pubsub", |b| {
        b.iter(|| {
            // write
            data_writer.write(&data, InstanceHandle::NIL).unwrap();

            loop {
                if let Ok(samples) = data_reader.take(
                    1,
                    &[SampleStateKind::ANY_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ANY_INSTANCE_STATE],
                ) {
                    if let Some(sample) = samples.first() {
                        return black_box(sample.data().unwrap());
                    }
                }
            }
        });
    });
}

criterion_group!(benches, besteffort_pubsub_roundtrip);
criterion_main!(benches);
