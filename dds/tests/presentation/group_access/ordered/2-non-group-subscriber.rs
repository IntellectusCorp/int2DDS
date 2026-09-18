// A Subscriber that does not request GROUP access scope is untouched by the group rules.

use super::*;
use int2dds::{
    common::instance_handle::InstanceHandle, infrastructure::qos_policy::PresentationQosPolicy,
};

// The Publisher offers GROUP with ordered_access in both tests below. A weaker offer would leave
// nothing for the requesting side to switch off, and the tests would pass for the wrong reason.

// INSTANCE access scope reads with no access block and with no single sample limit, even though
// the samples arrive carrying group sequence numbers.
#[test]
fn instance_scope_subscriber_reads_without_an_access_block() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "InstanceScopeNoBlock";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, PresentationQosPolicy::default());
    let reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();

    wait_for_samples(&reader, 2);

    let taken = reader.take(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES).unwrap();

    assert_eq!(taken.len(), 2);
    assert_eq!(taken[0].data().unwrap().value, 1);
    assert_eq!(taken[1].data().unwrap().value, 2);

    delete_participants(writer_participant, reader_participant);
}

// GROUP without ordered_access still needs the access block, but the collection is a set and no
// single sample limit applies.
#[test]
fn group_without_ordered_access_takes_every_sample() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupUnorderedSet";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_presentation(false));
    let reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();

    subscriber.begin_access().unwrap();

    wait_for_samples(&reader, 2);

    // Two samples in one reader, and the set names it once.
    let entries = subscriber
        .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap();
    assert_eq!(entries.len(), 1);

    let taken = reader.take(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES).unwrap();

    subscriber.end_access().unwrap();

    assert_eq!(taken.len(), 2);
    assert_eq!(taken[0].data().unwrap().value, 1);
    assert_eq!(taken[1].data().unwrap().value, 2);

    delete_participants(writer_participant, reader_participant);
}

// TOPIC + ordered_access still merges the instance buckets into publication order when the
// Publisher offers GROUP. No access block is opened here and take hands back every sample.
#[test]
fn topic_scope_subscriber_keeps_topic_order_from_a_group_publisher() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "TopicScopeFromGroupPublisher";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let topic_ordered = PresentationQosPolicy {
        access_scope: PresentationQosAccessScopeKind::Topic,
        coherent_access: false,
        ordered_access: true,
    };
    let topic_subscriber = make_subscriber(&reader_participant, topic_ordered);
    let topic_reader = make_reader(&reader_participant, &topic_subscriber, topic_name);

    let instance_subscriber =
        make_subscriber(&reader_participant, PresentationQosPolicy::default());
    let instance_reader = make_reader(&reader_participant, &instance_subscriber, topic_name);

    wait_for_matched_readers(&writer, 2);

    // Two instances written alternately, so an unordered read shows them as two blocks.
    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();

    wait_for_samples(&topic_reader, 4);
    wait_for_samples(&instance_reader, 4);

    let topic_values: Vec<i16> = topic_reader
        .take(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap()
        .iter()
        .map(|sample| sample.data().unwrap().value)
        .collect();
    let instance_values: Vec<i16> = instance_reader
        .take(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap()
        .iter()
        .map(|sample| sample.data().unwrap().value)
        .collect();

    assert_eq!(topic_values, vec![1, 2, 3, 4]);
    assert_eq!(instance_values, vec![1, 3, 2, 4]);

    delete_participants(writer_participant, reader_participant);
}
