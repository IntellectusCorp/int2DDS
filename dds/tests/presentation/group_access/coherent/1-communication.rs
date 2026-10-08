// end_coherent_changes makes a group coherent set readable through the Subscriber.

use super::*;
use int2dds::{common::instance_handle::InstanceHandle, topic::type_support::DdsType};

// A payload above the fragment size, so the writer sends the set member as DataFrag submessages.
#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct LargeKeyedData {
    #[dds(key)]
    key: i16,
    payload: Vec<u8>,
}

const LARGE_PAYLOAD_LEN: usize = 1024 * 1024;

// One coherent set spans both writers of the Publisher. Neither reader may show its sample while
// the set is open, and both show them once end_coherent_changes closes it.
#[test]
fn a_group_coherent_set_reaches_both_readers_only_once_it_is_closed() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupCoherentClosedA";
    let topic_b = "GroupCoherentClosedB";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer_a = make_writer(&writer_participant, &publisher, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher, topic_b);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let _reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let _reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();

    std::thread::sleep(SETTLE);

    // Under GROUP access scope the application asks the Subscriber which readers have data rather
    // than reaching for a reader of its own.
    subscriber.begin_access().unwrap();
    let held = subscriber
        .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap();
    assert!(held.is_empty(), "an open set must not reach the application");
    subscriber.end_access().unwrap();

    publisher.end_coherent_changes().unwrap();

    // Without ordered_access the collection is a set, so each reader appears once however many
    // samples it holds. Ordered access would have named them four times.
    let entries = wait_for_entries(&subscriber, 2);
    let mut values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    values.sort();
    assert_eq!(values, vec![1, 2, 3, 4]);

    delete_participants(writer_participant, reader_participant);
}

// The whole set is written and closed before the Subscriber exists. TRANSIENT_LOCAL writers resend
// it on match, and the set still arrives whole.
#[test]
fn a_late_reader_receives_a_group_coherent_set_closed_before_it_matched() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupCoherentLateA";
    let topic_b = "GroupCoherentLateB";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer_a = make_writer_with_qos(
        &writer_participant,
        &publisher,
        topic_a,
        transient_local_writer_qos(),
    );
    let writer_b = make_writer_with_qos(
        &writer_participant,
        &publisher,
        topic_b,
        transient_local_writer_qos(),
    );

    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let _reader_a = make_reader_with_qos(
        &reader_participant,
        &subscriber,
        topic_a,
        transient_local_reader_qos(),
    );
    let _reader_b = make_reader_with_qos(
        &reader_participant,
        &subscriber,
        topic_b,
        transient_local_reader_qos(),
    );

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    let entries = wait_for_entries(&subscriber, 2);
    let mut values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    values.sort();
    assert_eq!(values, vec![1, 2]);

    delete_participants(writer_participant, reader_participant);
}

// A fragmented member carries PID_GROUP_SEQ_NUM, PID_COHERENT_SET and PID_GROUP_COHERENT_SET on
// its first DataFrag only, and reassembly restores them on the CacheChange.
#[test]
fn a_fragmented_sample_inside_a_set_arrives_whole() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentFragment";

    let writer_topic = writer_participant
        .create_topic::<LargeKeyedData>(
            topic_name,
            &LargeKeyedData::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = publisher
        .create_datawriter::<LargeKeyedData>(
            &writer_topic,
            reliable_keep_all_writer_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let reader_topic = reader_participant
        .create_topic::<LargeKeyedData>(
            topic_name,
            &LargeKeyedData::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let reader = subscriber
        .create_datareader::<LargeKeyedData>(
            &reader_topic,
            reliable_keep_all_reader_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, MATCH_TIMEOUT)
        .expect("timed out waiting for the writer to match a reader");

    publisher.begin_coherent_changes().unwrap();
    writer
        .write(&LargeKeyedData { key: 1, payload: vec![7; LARGE_PAYLOAD_LEN] }, InstanceHandle::NIL)
        .unwrap();
    publisher.end_coherent_changes().unwrap();

    wait_for_entries(&subscriber, 1);

    let samples =
        reader.take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES).unwrap();

    subscriber.end_access().unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].data().unwrap().payload.len(), LARGE_PAYLOAD_LEN);

    delete_participants(writer_participant, reader_participant);
}

// A set with no member still consumes a group sequence number for its End Coherent Set, and the
// next sample is committed once the Subscriber releases that position.
#[test]
fn an_empty_set_does_not_block_the_next_sample() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentEmptySet";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let _reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    publisher.begin_coherent_changes().unwrap();
    publisher.end_coherent_changes().unwrap();

    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();

    let entries = wait_for_entries(&subscriber, 1);
    let values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![1]);

    delete_participants(writer_participant, reader_participant);
}
