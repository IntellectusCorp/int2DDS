// One read through the Subscriber returns the members of one Publisher coherent set, and no
// member of another set or of no set.

use super::*;
use int2dds::{
    common::instance_handle::InstanceHandle, infrastructure::qos_policy::ResourceLimitsQosPolicy,
};

// Two sets written back to back keep their boundary: closing the first hands over exactly its own
// samples, and the second stays whole until it closes in turn.
#[test]
fn consecutive_group_coherent_sets_keep_their_boundary() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupCoherentBoundaryA";
    let topic_b = "GroupCoherentBoundaryB";

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
    publisher.end_coherent_changes().unwrap();

    // The second set is written but left open, so only the first may be handed over.
    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();

    std::thread::sleep(SETTLE);

    let entries = wait_for_entries(&subscriber, 2);
    let mut first_set: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    first_set.sort();
    assert_eq!(first_set, vec![1, 2], "the open second set must not join the first");

    publisher.end_coherent_changes().unwrap();

    let entries = wait_for_entries(&subscriber, 2);
    let mut second_set: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    second_set.sort();
    assert_eq!(second_set, vec![3, 4]);

    delete_participants(writer_participant, reader_participant);
}

// Nested begin_coherent_changes calls raise the depth of one set, so the inner
// end_coherent_changes commits nothing and the outer one commits every member.
#[test]
fn nested_coherent_changes_deliver_only_at_the_outermost_end() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentNested";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let _reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    publisher.begin_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();

    publisher.begin_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    std::thread::sleep(SETTLE);

    subscriber.begin_access().unwrap();
    let held = subscriber
        .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap();
    assert!(held.is_empty(), "the inner end must not open the set");
    subscriber.end_access().unwrap();

    publisher.end_coherent_changes().unwrap();

    let entries = wait_for_entries(&subscriber, 1);
    let mut values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    values.sort();
    assert_eq!(values, vec![1, 2]);

    delete_participants(writer_participant, reader_participant);
}

// A sample written before begin_coherent_changes carries no PID_GROUP_COHERENT_SET, so an open
// set does not delay its commit.
#[test]
fn an_out_of_set_sample_is_not_held_by_a_later_set() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentOutOfSet";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let _reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();

    publisher.begin_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();

    std::thread::sleep(SETTLE);

    let entries = wait_for_entries(&subscriber, 1);
    let out_of_set: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    assert_eq!(out_of_set, vec![1], "the open set must not hand over its member yet");

    publisher.end_coherent_changes().unwrap();

    let entries = wait_for_entries(&subscriber, 1);
    let set_member: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    assert_eq!(set_member, vec![2]);

    delete_participants(writer_participant, reader_participant);
}

// A dispose carries a key and no payload, and it takes a group position in the set like a write.
#[test]
fn a_dispose_inside_the_set_is_held_with_its_members() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentDispose";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    publisher.begin_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.dispose(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();

    std::thread::sleep(SETTLE);

    subscriber.begin_access().unwrap();
    let held = subscriber
        .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap();
    assert!(held.is_empty(), "an open set must not reach the application");
    subscriber.end_access().unwrap();

    publisher.end_coherent_changes().unwrap();

    wait_for_entries(&subscriber, 1);

    let samples =
        reader.take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES).unwrap();

    subscriber.end_access().unwrap();

    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].data().unwrap().value, 1);
    assert_eq!(
        samples[1].sample_info().instance_state,
        InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE
    );

    delete_participants(writer_participant, reader_participant);
}

// The changes of a writer matched to no reader of the Subscriber are removed from the
// Subscriber-relevant coherent set, so the position that writer took does not discard the set.
#[test]
fn a_set_is_delivered_without_the_portion_of_an_unmatched_writer() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let matched_topic = "GroupCoherentRelevantA";
    let unmatched_topic = "GroupCoherentRelevantB";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let matched_writer = make_writer(&writer_participant, &publisher, matched_topic);
    let unmatched_writer = make_writer(&writer_participant, &publisher, unmatched_topic);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let _reader = make_reader(&reader_participant, &subscriber, matched_topic);

    wait_for_match(&matched_writer);

    publisher.begin_coherent_changes().unwrap();
    matched_writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    unmatched_writer.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    let entries = wait_for_entries(&subscriber, 1);
    let values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![1]);

    delete_participants(writer_participant, reader_participant);
}

// A set above the reader's max_samples is discarded with every member, and the group cursor moves
// to its End Coherent Set so the next set is committed.
#[test]
fn a_set_larger_than_the_readers_history_is_discarded_whole_and_the_next_one_arrives() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentOverHistory";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let reader_qos = DataReaderQos {
        resource_limits: ResourceLimitsQosPolicy {
            max_samples: 2,
            max_instances: 10,
            max_samples_per_instance: 2,
        },
        ..reliable_keep_all_reader_qos()
    };
    let _reader = make_reader_with_qos(&reader_participant, &subscriber, topic_name, reader_qos);

    wait_for_match(&writer);

    // Three members against a limit of two, so the set cannot be stored at all.
    publisher.begin_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 10), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(2, 20), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(3, 30), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    std::thread::sleep(SETTLE);

    subscriber.begin_access().unwrap();
    let held = subscriber
        .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap();
    assert!(held.is_empty(), "a set over the limit must not be handed over in part");
    subscriber.end_access().unwrap();

    publisher.begin_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 40), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(2, 50), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    let entries = wait_for_entries(&subscriber, 1);
    let mut values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    values.sort();
    assert_eq!(values, vec![40, 50]);

    delete_participants(writer_participant, reader_participant);
}
