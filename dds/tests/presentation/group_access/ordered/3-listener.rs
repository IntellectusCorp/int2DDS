// Inside on_data_on_readers the access rules of GROUP access scope do not apply.

use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Mutex;

use super::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::error::DdsError,
    domain::domain_participant_listener::DomainParticipantListener,
    publication::publisher_listener::PublisherListener,
    subscription::{
        data_reader_listener::DataReaderListener, subscriber_listener::SubscriberListener,
    },
    topic::topic_listener::TopicListener,
};

// Reads inside on_data_on_readers and reports what it saw. The callback never opens an access
// block, so a report at all means the exemption held.
struct ReadingSubscriberListener {
    report: SyncSender<(usize, usize)>,
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
                .take(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES)
                .map(|samples| samples.len())
                .unwrap_or(0);
        }

        let _ = self.report.try_send((entries.len(), taken));
    }
}

// Holds the callback open until the test thread has had its turn, so the two threads overlap.
struct BlockingSubscriberListener {
    entered: SyncSender<()>,
    release: Mutex<Option<Receiver<()>>>,
}

impl SubscriberListener for BlockingSubscriberListener {
    fn on_data_on_readers(&self, _subscriber: &Subscriber) {
        let _ = self.entered.try_send(());

        if let Ok(mut guard) = self.release.lock() {
            if let Some(release) = guard.take() {
                let _ = release.recv_timeout(POLL_TIMEOUT);
            }
        }
    }
}

// Overrides no callback, so every status reaches the participant with the default bodies.
struct DefaultParticipantListener;

impl TopicListener for DefaultParticipantListener {}
impl SubscriberListener for DefaultParticipantListener {}
impl PublisherListener for DefaultParticipantListener {}
impl DomainParticipantListener for DefaultParticipantListener {}

struct ReportingReaderListener {
    report: SyncSender<()>,
}

impl DataReaderListener for ReportingReaderListener {
    type Foo = KeyedDataType;

    fn on_data_available(&self, _reader: &DataReader<KeyedDataType>) {
        let _ = self.report.try_send(());
    }
}

// The callback reads with no access block, gets a set rather than a list, and is not held to one
// sample per call.
#[test]
fn group_ordered_listener_reads_without_an_access_block() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupOrderedListenerReads";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let (report, reports) = sync_channel(4);
    let subscriber = reader_participant
        .create_subscriber(
            SubscriberQos { presentation: group_presentation(true), ..Default::default() },
            Some(Arc::new(ReadingSubscriberListener { report })),
            StatusMask::default(),
        )
        .unwrap();
    let _reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    writer.write(&KeyedDataType::new(1, 2), InstanceHandle::NIL).unwrap();

    // Each arrival drives one callback, and the samples of that moment are read there.
    let mut entries_seen = 0;
    let mut taken = 0;
    while taken < 2 {
        let (entries, just_taken) =
            reports.recv_timeout(POLL_TIMEOUT).expect("the callback reported nothing");
        entries_seen = entries_seen.max(entries);
        taken += just_taken;
    }

    // One reader, named once however many samples it holds.
    assert_eq!(entries_seen, 1);
    assert_eq!(taken, 2);

    delete_participants(writer_participant, reader_participant);
}

// The exemption follows the callback stack, so another thread reading the same Subscriber at the
// same moment still needs its own access block.
#[test]
fn group_ordered_listener_exemption_does_not_reach_another_thread() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic_name = "GroupOrderedListenerThread";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let (entered, entries) = sync_channel(1);
    let (release, released) = sync_channel(1);
    let subscriber = reader_participant
        .create_subscriber(
            SubscriberQos { presentation: group_presentation(true), ..Default::default() },
            Some(Arc::new(BlockingSubscriberListener {
                entered,
                release: Mutex::new(Some(released)),
            })),
            StatusMask::default(),
        )
        .unwrap();
    let reader = make_reader(&reader_participant, &subscriber, topic_name);

    wait_for_match(&writer);

    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();

    entries.recv_timeout(POLL_TIMEOUT).expect("the callback never started");

    // The callback is still running on the receive thread while this runs on the test thread.
    assert!(matches!(
        reader.take(10, ANY_SAMPLE_STATES, ANY_VIEW_STATES, ANY_INSTANCE_STATES),
        Err(DdsError::PreconditionNotMet)
    ));

    let _ = release.try_send(());

    delete_participants(writer_participant, reader_participant);
}

// A participant listener installed with the default mask leaves on_data_available to the reader
// listener.
#[test]
fn participant_listener_with_the_default_mask_does_not_swallow_on_data_available() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = DomainParticipantFactory::get_instance()
        .create_participant(
            domain_id,
            DomainParticipantQos::default(),
            Some(Arc::new(DefaultParticipantListener)),
            StatusMask::default(),
        )
        .unwrap();

    let topic_name = "GroupOrderedParticipantListener";

    let publisher = make_publisher(&writer_participant, group_presentation(true));
    let writer = make_writer(&writer_participant, &publisher, topic_name);

    let subscriber = make_subscriber(&reader_participant, group_presentation(true));
    let topic = reader_participant
        .create_topic::<KeyedDataType>(
            topic_name,
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let (report, reports) = sync_channel(4);
    let _reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reliable_keep_all_reader_qos(),
            Some(Arc::new(ReportingReaderListener { report })),
            StatusMask::default(),
        )
        .unwrap();

    wait_for_match(&writer);

    writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();

    reports.recv_timeout(POLL_TIMEOUT).expect("the reader listener never got on_data_available");

    delete_participants(writer_participant, reader_participant);
}
