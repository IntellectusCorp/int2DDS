// GROUP access_scope with both ordered_access and coherent_access.

use super::*;
use int2dds::common::instance_handle::InstanceHandle;

// The set is held until it closes, and what comes out then is still in the order the Publisher
// wrote it, one sample per take.
#[test]
fn a_closed_group_coherent_set_walks_in_publication_order() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupOrderedCoherentA";
    let topic_b = "GroupOrderedCoherentB";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(true));
    let writer_a = make_writer(&writer_participant, &publisher, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher, topic_b);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(true));
    let _reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let _reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    // The value carries the publication order, the key keeps each topic on one instance.
    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();

    std::thread::sleep(SETTLE);

    subscriber.begin_access().unwrap();
    let held = subscriber.get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES);
    assert!(held.unwrap().is_empty(), "an open set must not reach the application");
    subscriber.end_access().unwrap();

    publisher.end_coherent_changes().unwrap();

    // With ordered_access the collection is a list, so the four samples name their readers four
    // times instead of twice.
    let entries = wait_for_entries(&subscriber, 4);
    let mut values = Vec::new();
    for entry in &entries {
        let reader = entry
            .as_any()
            .downcast_ref::<DataReader<KeyedDataType>>()
            .expect("the entry is a KeyedDataType reader");

        // Under GROUP ordered access one call hands out one sample, which is what makes walking
        // the list restore the order the samples were written in.
        let samples =
            reader.take(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES).unwrap();
        assert_eq!(samples.len(), 1);

        values.push(samples[0].data().unwrap().value);
    }

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![1, 2, 3, 4]);

    delete_participants(writer_participant, reader_participant);
}

// Two committed sets are sorted by group sequence number against each other, not only within one
// set. Taking the first set before the second is committed would pass without that sorting.
#[test]
fn two_sets_walk_as_one_list_in_publication_order() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupOrderedCoherentTwoSetsA";
    let topic_b = "GroupOrderedCoherentTwoSetsB";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(true));
    let writer_a = make_writer(&writer_participant, &publisher, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher, topic_b);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(true));
    let _reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let _reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    // Both sets are closed before anything is taken, so the walk crosses the set boundary.
    let entries = wait_for_entries(&subscriber, 4);
    let values: Vec<i16> = entries.iter().map(take_one).collect();

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![1, 2, 3, 4]);

    delete_participants(writer_participant, reader_participant);
}

// Samples carrying no PID_GROUP_COHERENT_SET are sorted by group sequence number together with
// the members of a set.
#[test]
fn samples_outside_and_inside_a_set_walk_in_one_order() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupOrderedCoherentMixedA";
    let topic_b = "GroupOrderedCoherentMixedB";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(true));
    let writer_a = make_writer(&writer_participant, &publisher, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher, topic_b);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(true));
    let _reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let _reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();

    publisher.begin_coherent_changes().unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    writer_b.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();

    let entries = wait_for_entries(&subscriber, 4);
    let values: Vec<i16> = entries.iter().map(take_one).collect();

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![1, 2, 3, 4]);

    delete_participants(writer_participant, reader_participant);
}
