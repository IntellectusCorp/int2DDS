// Samples written through one Publisher reach the readers in group order.

use super::*;
use int2dds::common::instance_handle::InstanceHandle;

// Two writers of one Publisher share one group order, so walking the returned list one sample at
// a time restores the order the samples were written in.
#[test]
fn group_ordered_list_walks_two_readers_in_publication_order() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupOrderedWalkA";
    let topic_b = "GroupOrderedWalkB";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer_a = make_writer(&writer_participant, &publisher, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher, topic_b);

    let subscriber = make_subscriber(&reader_participant, group_presentation(true));
    let _reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let _reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    // The value carries the publication order, the key keeps each topic on one instance.
    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();

    subscriber.begin_access().unwrap();

    let entries = wait_for_entries(&subscriber, 4);
    let values: Vec<i16> = entries.iter().map(take_one).collect();

    subscriber.end_access().unwrap();

    // A list of [A, A, B, B] would have produced 1, 3, 2, 4 here.
    assert_eq!(values, vec![1, 2, 3, 4]);

    delete_participants(writer_participant, reader_participant);
}

// The single sample limit is on the returned collection, so read is bound by it as much as take.
#[test]
fn group_ordered_read_returns_one_sample_like_take() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupOrderedRead";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_presentation(true));
    let reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();

    subscriber.begin_access().unwrap();

    // Two samples in one reader, so the list names it twice.
    wait_for_entries(&subscriber, 2);

    let first = reader.read(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].data().unwrap().value, 1);

    // read leaves the sample in the cache, so the earliest one comes back again.
    let second = reader.read(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].data().unwrap().value, 1);

    subscriber.end_access().unwrap();

    delete_participants(writer_participant, reader_participant);
}

// Each Publisher issues its own group sequence numbers, so only the order within a Publisher is
// defined. The order between them is not, and this test never asserts it.
#[test]
fn two_publishers_keep_their_own_group_order() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupOrderedPublisherA";
    let topic_b = "GroupOrderedPublisherB";

    let publisher_a = make_publisher(&writer_participant, group_presentation(true));
    let publisher_b = make_publisher(&writer_participant, group_presentation(true));
    let writer_a = make_writer(&writer_participant, &publisher_a, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher_b, topic_b);

    let subscriber = make_subscriber(&reader_participant, group_presentation(true));
    let _reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let _reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    // Odd values belong to Publisher A, even values to Publisher B.
    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();

    subscriber.begin_access().unwrap();

    let entries = wait_for_entries(&subscriber, 4);
    let values: Vec<i16> = entries.iter().map(take_one).collect();

    subscriber.end_access().unwrap();

    let from_publisher_a: Vec<i16> =
        values.iter().copied().filter(|value| value % 2 == 1).collect();
    let from_publisher_b: Vec<i16> =
        values.iter().copied().filter(|value| value % 2 == 0).collect();

    assert_eq!(from_publisher_a, vec![1, 3]);
    assert_eq!(from_publisher_b, vec![2, 4]);

    delete_participants(writer_participant, reader_participant);
}

// get_datareaders reports the readers holding samples that match the given states, not every
// reader holding samples.
#[test]
fn group_ordered_get_datareaders_honours_the_state_masks() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupOrderedStateMasks";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_presentation(true));
    let reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();

    subscriber.begin_access().unwrap();

    let entries = wait_for_entries(&subscriber, 2);
    assert_eq!(entries.len(), 2);

    // Reading the earliest sample turns its sample state to READ.
    let read_samples =
        reader.read(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES).unwrap();
    assert_eq!(read_samples.len(), 1);

    let not_read_entries = subscriber
        .get_datareaders(NOT_READ_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap();
    assert_eq!(not_read_entries.len(), 1);

    subscriber.end_access().unwrap();

    delete_participants(writer_participant, reader_participant);
}
