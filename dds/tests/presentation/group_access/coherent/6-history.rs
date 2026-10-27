// A group coherent set the reader cannot hold in full is discarded whole, and the samples around
// it are still delivered.

use super::*;
use int2dds::{
    common::instance_handle::InstanceHandle, infrastructure::qos_policy::ResourceLimitsQosPolicy,
    topic::type_support::DdsType,
};

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct LargeData {
    index: i32,
    data: Vec<u8>,
}

const LARGE_SAMPLE_BYTES: usize = 12 * 1024 * 1024;
const LARGE_DATA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

// A keyless reader's max_samples equals its depth. The sample after the set shows the stream went
// on past it.
#[test]
fn group_coherent_set_larger_than_keep_last_depth_is_discarded() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentKeepLastDepth";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer_topic = writer_participant
        .create_topic::<NoKeyDataType>(
            topic_name,
            NoKeyDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let writer = publisher
        .create_datawriter::<NoKeyDataType>(
            &writer_topic,
            reliable_keep_all_writer_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let reader_topic = reader_participant
        .create_topic::<NoKeyDataType>(
            topic_name,
            NoKeyDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let reader = subscriber
        .create_datareader::<NoKeyDataType>(
            &reader_topic,
            DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(2), strict: true },
                ..reliable_keep_all_reader_qos()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, MATCH_TIMEOUT)
        .expect("timed out waiting for the writer to match a reader");

    // Three members exceed the depth of 2.
    publisher.begin_coherent_changes().unwrap();
    for value in [1, 2, 3] {
        writer.write(&NoKeyDataType::new(value), InstanceHandle::NIL).unwrap();
    }
    publisher.end_coherent_changes().unwrap();
    writer.write(&NoKeyDataType::new(4), InstanceHandle::NIL).unwrap();

    let deadline = Instant::now() + POLL_TIMEOUT;
    let values = loop {
        subscriber.begin_access().unwrap();
        let values: Vec<i16> = reader
            .take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .map(|samples| samples.iter().map(|sample| sample.data().unwrap().value).collect())
            .unwrap_or_default();
        subscriber.end_access().unwrap();

        if !values.is_empty() {
            break values;
        }

        assert!(Instant::now() < deadline, "timed out waiting for the sample after the set");
        std::thread::sleep(POLL_INTERVAL);
    };

    assert_eq!(values, vec![4]);

    delete_participants(writer_participant, reader_participant);
}

#[test]
fn group_coherent_set_larger_than_keep_all_max_samples_is_discarded() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentKeepAllMaxSamples";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let _reader = make_reader_with_qos(
        &reader_participant,
        &subscriber,
        topic_name,
        DataReaderQos {
            resource_limits: ResourceLimitsQosPolicy {
                max_samples: 2,
                max_instances: 10,
                max_samples_per_instance: 2,
            },
            ..reliable_keep_all_reader_qos()
        },
    );

    wait_for_match(&writer);

    // One member per instance, so only max_samples is exceeded. The sample after the set shows
    // the stream went on past it.
    publisher.begin_coherent_changes().unwrap();
    for key in [1, 2, 3] {
        writer.write(&KeyedDataType::new(key, key), InstanceHandle::NIL).unwrap();
    }
    publisher.end_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(4, 4), InstanceHandle::NIL).unwrap();

    let entries = wait_for_entries(&subscriber, 1);
    let values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![4]);

    delete_participants(writer_participant, reader_participant);
}

// The set and the sample after it reach the reader together when the access block closes. Only
// the set is discarded.
#[test]
fn group_coherent_set_larger_than_keep_last_depth_leaves_the_sample_after_it() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentKeepLastDepthThenSample";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let _reader = make_reader_with_qos(
        &reader_participant,
        &subscriber,
        topic_name,
        DataReaderQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(2), strict: true },
            ..reliable_keep_all_reader_qos()
        },
    );

    wait_for_match(&writer);

    subscriber.begin_access().unwrap();

    publisher.begin_coherent_changes().unwrap();
    for value in [1, 2, 3] {
        writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL).unwrap();
    }
    publisher.end_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 4), InstanceHandle::NIL).unwrap();

    std::thread::sleep(SETTLE);

    subscriber.end_access().unwrap();

    let entries = wait_for_entries(&subscriber, 1);
    let values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![4]);

    delete_participants(writer_participant, reader_participant);
}

// Three 12 MiB members exceed the 32 MiB one remote Publisher holds for a reader. The small sample
// after the set shows the stream went on past it.
#[test]
fn group_coherent_set_larger_than_the_publisher_buffer_is_discarded() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentPublisherBuffer";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer_topic = writer_participant
        .create_topic::<LargeData>(
            topic_name,
            "LargeData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let writer = publisher
        .create_datawriter::<LargeData>(
            &writer_topic,
            reliable_keep_all_writer_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let reader_topic = reader_participant
        .create_topic::<LargeData>(
            topic_name,
            "LargeData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let reader = subscriber
        .create_datareader::<LargeData>(
            &reader_topic,
            reliable_keep_all_reader_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, MATCH_TIMEOUT)
        .expect("timed out waiting for the writer to match a reader");

    publisher.begin_coherent_changes().unwrap();
    for index in 0..3 {
        writer
            .write(&LargeData { index, data: vec![0; LARGE_SAMPLE_BYTES] }, InstanceHandle::NIL)
            .unwrap();
    }
    publisher.end_coherent_changes().unwrap();
    writer.write(&LargeData { index: 3, data: Vec::new() }, InstanceHandle::NIL).unwrap();

    let deadline = Instant::now() + LARGE_DATA_TIMEOUT;
    let indices = loop {
        subscriber.begin_access().unwrap();
        let indices: Vec<i32> = reader
            .take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .map(|samples| samples.iter().map(|sample| sample.data().unwrap().index).collect())
            .unwrap_or_default();
        subscriber.end_access().unwrap();

        if !indices.is_empty() {
            break indices;
        }

        assert!(Instant::now() < deadline, "timed out waiting for the sample after the set");
        std::thread::sleep(POLL_INTERVAL);
    };

    assert_eq!(indices, vec![3]);

    delete_participants(writer_participant, reader_participant);
}
