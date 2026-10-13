// on_data_on_readers runs after end_coherent_changes commits the group coherent set, not before.

use std::sync::mpsc::{sync_channel, SyncSender};

use super::*;
use int2dds::{
    common::instance_handle::InstanceHandle, subscription::subscriber_listener::SubscriberListener,
};

// Reads inside on_data_on_readers and reports how many samples it took.
struct ReadingSubscriberListener {
    report: SyncSender<usize>,
}

impl SubscriberListener for ReadingSubscriberListener {
    fn on_data_on_readers(&self, subscriber: &Subscriber) {
        let entries = subscriber
            .get_datareaders(ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
            .expect("get_datareaders inside the callback");

        let mut taken = 0;
        for entry in entries.iter() {
            let reader = entry
                .as_any()
                .downcast_ref::<DataReader<KeyedDataType>>()
                .expect("the entry is a KeyedDataType reader");
            taken += reader
                .take(i32::MAX, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
                .map(|samples| samples.len())
                .unwrap_or(0);
        }

        let _ = self.report.try_send(taken);
    }
}

#[test]
fn the_data_on_readers_callback_does_not_fire_while_the_set_is_open() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupCoherentListener";

    let publisher = make_publisher(&writer_participant, group_coherent_presentation(false));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let (report, reports) = sync_channel(4);
    let subscriber = reader_participant
        .create_subscriber(
            SubscriberQos {
                presentation: group_coherent_presentation(false),
                ..Default::default()
            },
            Some(Arc::new(ReadingSubscriberListener { report })),
            StatusMask::default(),
        )
        .unwrap();
    let _reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    publisher.begin_coherent_changes().unwrap();
    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();

    assert!(reports.recv_timeout(SETTLE).is_err(), "an open set must not notify the Subscriber");

    publisher.end_coherent_changes().unwrap();

    // The callback takes the committed set, and it calls read and take with no access block.
    let mut taken = 0;
    while taken < 2 {
        taken += reports.recv_timeout(POLL_TIMEOUT).expect("the callback reported nothing");
    }

    assert_eq!(taken, 2);

    delete_participants(writer_participant, reader_participant);
}
