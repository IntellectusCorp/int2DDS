// Losing a writer or a reader releases the samples that were waiting on it.

use super::*;
use int2dds::common::instance_handle::InstanceHandle;

// One reader of the Subscriber goes away while the other keeps receiving, and the group order of
// what is left stays intact.
#[test]
fn deleting_one_reader_leaves_the_other_receiving() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupOrderedSurvivorA";
    let topic_b = "GroupOrderedSurvivorB";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer_a = make_writer(&writer_participant, &publisher, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher, topic_b);

    let subscriber = make_subscriber(&reader_participant, group_presentation(true));
    let reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();

    subscriber.begin_access().unwrap();
    wait_for_entries(&subscriber, 2);
    subscriber.end_access().unwrap();

    subscriber.delete_datareader(reader_b).unwrap();

    writer_a.write(&KeyedDataType::new(1, 3), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 4), InstanceHandle::NIL).unwrap();

    subscriber.begin_access().unwrap();

    // Reader A keeps its first sample and gains the third. The deleted reader's share is gone.
    let entries = wait_for_entries(&subscriber, 2);
    let values: Vec<i16> = entries.iter().map(take_one).collect();

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![1, 3]);
    drop(reader_a);

    delete_participants(writer_participant, reader_participant);
}
