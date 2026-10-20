// A Subscriber requesting less than GROUP + coherent_access commits members without their set.

use super::*;
use int2dds::{
    common::instance_handle::InstanceHandle, infrastructure::qos_policy::PresentationQosPolicy,
};

// The Publisher offers GROUP + coherent_access in every test below. A weaker offer would make the
// requested value the compatible one and the test would pass without the downgrade.

// With coherent_access false there is no Subscriber coherent set, so each member is committed on
// arrival. The access block is still required, because the requested scope is GROUP.
#[test]
fn group_without_coherent_access_sees_members_before_the_set_closes() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentNoCoherentAccess";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_presentation(false));
    let reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    publisher.begin_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();

    // The collection names the reader once for any number of samples, so the poll counts the
    // reader's samples. Every attempt opens its own access block.
    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        subscriber.begin_access().unwrap();
        let held = reader
            .read(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .map(|samples| samples.len())
            .unwrap_or(0);

        if held == 2 {
            break;
        }

        subscriber.end_access().unwrap();
        assert!(
            Instant::now() < deadline,
            "timed out waiting for 2 samples, the reader held {}",
            held
        );
        std::thread::sleep(POLL_INTERVAL);
    }

    let values: Vec<i16> = reader
        .take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap()
        .iter()
        .map(|sample| sample.data().unwrap().value)
        .collect();

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![1, 2]);

    publisher.end_coherent_changes().unwrap();

    delete_participants(writer_participant, reader_participant);
}

// Under TOPIC scope there is one Subscriber coherent set per writer, so each writer's portion is
// committed on its own and read without an access block.
#[test]
fn topic_scope_subscriber_gets_one_coherent_set_per_writer() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupCoherentTopicScopeA";
    let topic_b = "GroupCoherentTopicScopeB";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer_a = make_writer(&writer_participant, &publisher, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher, topic_b);

    let topic_coherent = PresentationQosPolicy {
        access_scope: PresentationQosAccessScopeKind::Topic,
        coherent_access: true,
        ordered_access: false,
    };
    let subscriber = make_subscriber(&reader_participant, topic_coherent);
    let reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    wait_for_samples(&reader_a, 2);
    wait_for_samples(&reader_b, 1);

    let values_a: Vec<i16> = reader_a
        .take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap()
        .iter()
        .map(|sample| sample.data().unwrap().value)
        .collect();
    let values_b: Vec<i16> = reader_b
        .take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap()
        .iter()
        .map(|sample| sample.data().unwrap().value)
        .collect();

    assert_eq!(values_a, vec![1, 3]);
    assert_eq!(values_b, vec![2]);

    delete_participants(writer_participant, reader_participant);
}

// Under INSTANCE scope begin_coherent_changes changes nothing the Subscriber may read, and no
// access block is required.
#[test]
fn instance_scope_subscriber_sees_members_before_the_set_closes() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentInstanceScope";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, PresentationQosPolicy::default());
    let reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    publisher.begin_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();

    wait_for_samples(&reader, 2);

    let values: Vec<i16> = reader
        .take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
        .unwrap()
        .iter()
        .map(|sample| sample.data().unwrap().value)
        .collect();

    assert_eq!(values, vec![1, 2]);

    publisher.end_coherent_changes().unwrap();

    delete_participants(writer_participant, reader_participant);
}

// With ordered_access alone the members pass the group order one at a time and are committed on
// arrival, and the End Coherent Set takes its group position without being committed.
#[test]
fn group_ordered_access_only_sees_members_before_the_set_closes() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupCoherentOrderedOnlyA";
    let topic_b = "GroupCoherentOrderedOnlyB";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(true));
    let writer_a = make_writer(&writer_participant, &publisher, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher, topic_b);

    let subscriber = make_subscriber(&reader_participant, group_presentation(true));
    let _reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let _reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();

    // The set is still open, so a Subscriber held to it would list nothing here.
    let entries = wait_for_entries(&subscriber, 2);
    let open_set_values: Vec<i16> = entries.iter().map(take_one).collect();

    subscriber.end_access().unwrap();

    assert_eq!(open_set_values, vec![1, 2]);

    publisher.end_coherent_changes().unwrap();

    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();
    publisher.end_coherent_changes().unwrap();

    // The End Coherent Set of the first set sits between the two sets, and it is no sample.
    let entries = wait_for_entries(&subscriber, 2);
    let second_set_values: Vec<i16> = entries.iter().map(take_one).collect();

    subscriber.end_access().unwrap();

    assert_eq!(second_set_values, vec![3, 4]);

    delete_participants(writer_participant, reader_participant);
}
