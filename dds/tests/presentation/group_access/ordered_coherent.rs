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
    // Nothing reaches a reader while an access block is open, so every attempt opens its own.
    let deadline = Instant::now() + POLL_TIMEOUT;
    let entries = loop {
        subscriber.begin_access().unwrap();
        let entries = subscriber
            .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .unwrap();

        if entries.len() == 4 {
            break entries;
        }

        subscriber.end_access().unwrap();
        assert!(
            Instant::now() < deadline,
            "timed out waiting for 4 entries, the collection held {}",
            entries.len()
        );
        std::thread::sleep(POLL_INTERVAL);
    };
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
