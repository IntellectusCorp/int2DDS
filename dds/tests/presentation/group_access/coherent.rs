// GROUP access_scope with coherent_access alone. Tests that need ordered_access as well live
// in ordered_coherent.rs.

use super::*;
use int2dds::common::instance_handle::InstanceHandle;

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
    // Nothing reaches a reader while an access block is open, so every attempt opens its own.
    let deadline = Instant::now() + POLL_TIMEOUT;
    let entries = loop {
        subscriber.begin_access().unwrap();
        let entries = subscriber
            .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .unwrap();

        if entries.len() == 2 {
            break entries;
        }

        subscriber.end_access().unwrap();
        assert!(
            Instant::now() < deadline,
            "timed out waiting for 2 entries, the collection held {}",
            entries.len()
        );
        std::thread::sleep(POLL_INTERVAL);
    };
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

    // Nothing reaches a reader while an access block is open, so every attempt opens its own.
    let deadline = Instant::now() + POLL_TIMEOUT;
    let entries = loop {
        subscriber.begin_access().unwrap();
        let entries = subscriber
            .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .unwrap();

        if entries.len() == 2 {
            break entries;
        }

        subscriber.end_access().unwrap();
        assert!(
            Instant::now() < deadline,
            "timed out waiting for 2 entries, the collection held {}",
            entries.len()
        );
        std::thread::sleep(POLL_INTERVAL);
    };
    let mut values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    values.sort();
    assert_eq!(values, vec![1, 2]);

    delete_participants(writer_participant, reader_participant);
}

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

    // Nothing reaches a reader while an access block is open, so every attempt opens its own.
    let deadline = Instant::now() + POLL_TIMEOUT;
    let entries = loop {
        subscriber.begin_access().unwrap();
        let entries = subscriber
            .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .unwrap();

        if entries.len() == 2 {
            break entries;
        }

        subscriber.end_access().unwrap();
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the first set, the collection held {}",
            entries.len()
        );
        std::thread::sleep(POLL_INTERVAL);
    };
    let mut first_set: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    first_set.sort();
    assert_eq!(first_set, vec![1, 2], "the open second set must not join the first");

    publisher.end_coherent_changes().unwrap();

    let deadline = Instant::now() + POLL_TIMEOUT;
    let entries = loop {
        subscriber.begin_access().unwrap();
        let entries = subscriber
            .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .unwrap();

        if entries.len() == 2 {
            break entries;
        }

        subscriber.end_access().unwrap();
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the second set, the collection held {}",
            entries.len()
        );
        std::thread::sleep(POLL_INTERVAL);
    };
    let mut second_set: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    second_set.sort();
    assert_eq!(second_set, vec![3, 4]);

    delete_participants(writer_participant, reader_participant);
}
