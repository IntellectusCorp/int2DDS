//! DDS 1.4 section 2.2.4.1: a plain communication status has its StatusChangedFlag reset to FALSE
//! when the listener that consumed it returns.
//!
//! The flag used to be set TRUE *after* the listener had already run and drained the status, so an
//! entity carrying both a listener and a StatusCondition was left permanently triggered. A
//! WaitSet on that condition then returned immediately, forever, from its own immediate check --
//! a spin, not a wake-up -- and every `get_*_status()` reported zero change because the listener
//! had already taken it.
//!
//! SUBSCRIPTION_MATCHED is used because it is driven by discovery rather than by sample delivery.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use int2dds::{
    core::{error::DdsError, time::Duration},
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{status::StatusMask, wait_set::WaitSet},
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::{
        data_reader::DataReader,
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos},
    },
    topic::qos::TopicQos,
    DdsType,
};

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct HelloWorldType {
    index: u32,
    message: String,
}

struct CountingListener {
    matched: Arc<AtomicUsize>,
}

impl DataReaderListener for CountingListener {
    type Foo = HelloWorldType;

    fn on_subscription_matched(
        &self,
        _reader: &DataReader<Self::Foo>,
        _status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
    ) {
        self.matched.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn a_consumed_status_does_not_leave_the_condition_triggered() {
    let domain_id = crate::common::next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<HelloWorldType>(
            "ListenerStatusResetTopic",
            "HelloWorldType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let matched = Arc::new(AtomicUsize::new(0));
    let reader = subscriber
        .create_datareader::<HelloWorldType>(
            &topic,
            DataReaderQos::default(),
            Some(Arc::new(CountingListener { matched: matched.clone() })),
            StatusMask::SUBSCRIPTION_MATCHED,
        )
        .unwrap();

    let condition = reader.get_statuscondition().unwrap();
    condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let _writer = publisher
        .create_datawriter::<HelloWorldType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Give discovery time to match the pair and run the listener.
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(
        matched.load(Ordering::SeqCst) >= 1,
        "the listener never ran, so this test proves nothing about the reset"
    );

    // The listener consumed the status, so nothing is left for a condition-driven waiter. Before
    // the fix this returned Ok immediately on every call.
    assert_eq!(
        wait_set.wait(Duration::from_millis(500)).unwrap_err(),
        DdsError::Timeout,
        "the condition is still triggered after its listener consumed the status"
    );

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
