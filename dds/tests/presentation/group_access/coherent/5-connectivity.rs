// Deleting a reader while a set is open removes that reader's members from the set and commits
// the set to the readers that remain.

use super::*;
use int2dds::common::instance_handle::InstanceHandle;

#[test]
fn deleting_one_reader_leaves_the_set_for_the_other() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_a = "GroupCoherentSurvivorA";
    let topic_b = "GroupCoherentSurvivorB";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer_a = make_writer(&writer_participant, &publisher, topic_a);
    let writer_b = make_writer(&writer_participant, &publisher, topic_b);

    let subscriber = make_subscriber(&reader_participant, group_coherent_presentation(false));
    let reader_a = make_reader(&reader_participant, &subscriber, topic_a);
    let reader_b = make_reader(&reader_participant, &subscriber, topic_b);

    wait_for_match(&writer_a);
    wait_for_match(&writer_b);

    publisher.begin_coherent_changes().unwrap();
    writer_a.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer_b.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();

    subscriber.delete_datareader(reader_b).unwrap();

    publisher.end_coherent_changes().unwrap();

    let entries = wait_for_entries(&subscriber, 1);
    let values: Vec<i16> = entries.iter().flat_map(take_all).collect();

    subscriber.end_access().unwrap();

    assert_eq!(values, vec![1]);
    drop(reader_a);

    delete_participants(writer_participant, reader_participant);
}
