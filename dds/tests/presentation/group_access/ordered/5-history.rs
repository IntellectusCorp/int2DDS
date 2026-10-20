// A KEEP_LAST reader under GROUP ordered access keeps the latest samples.

use super::*;
use int2dds::common::instance_handle::InstanceHandle;

// A keyless reader's max_samples equals its depth. The samples wait in the Subscriber while the
// access block is open and reach the reader together when it closes.
#[test]
fn group_ordered_keep_last_reader_keeps_the_latest_samples() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupOrderedKeepLast";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
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

    let subscriber = make_subscriber(&reader_participant, group_presentation(true));
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
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(1), strict: true },
                ..reliable_keep_all_reader_qos()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, MATCH_TIMEOUT)
        .expect("timed out waiting for the writer to match a reader");

    subscriber.begin_access().unwrap();

    for value in 0..5 {
        writer.write(&NoKeyDataType::new(value), InstanceHandle::NIL).unwrap();
    }

    std::thread::sleep(SETTLE);

    subscriber.end_access().unwrap();

    // Group ordered access hands out one sample per take, so taking goes on until the reader is
    // empty after the first sample arrived.
    let deadline = Instant::now() + POLL_TIMEOUT;
    let mut values = Vec::new();
    loop {
        subscriber.begin_access().unwrap();
        let taken: Vec<i16> = reader
            .take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .map(|samples| samples.iter().map(|sample| sample.data().unwrap().value).collect())
            .unwrap_or_default();
        subscriber.end_access().unwrap();

        if taken.is_empty() && !values.is_empty() {
            break;
        }

        assert!(Instant::now() < deadline, "timed out waiting for the samples");
        values.extend(taken);
        std::thread::sleep(POLL_INTERVAL);
    }

    assert_eq!(values, vec![4]);

    delete_participants(writer_participant, reader_participant);
}
