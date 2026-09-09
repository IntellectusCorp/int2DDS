//! A panicking user listener must not take the reader down with it.
//!
//! Listener callbacks run synchronously on the RTPS receive thread, and the pure-Rust path has
//! no panic boundary -- the only `catch_unwind` in the tree guards C callbacks in the FFI
//! layer. So a `panic!` (or an `unwrap` on an empty `Option`) inside `on_data_available`
//! unwinds through the middleware, dropping whatever guards are live on the way out.
//!
//! The shipped example subscriber calls `reader.take(...)` from inside `on_data_available`, so
//! this is one ordinary user bug away rather than a theoretical case.
//!
//! Bounded on a worker thread: if delivery stops, the reader never signals again and the
//! assertion would otherwise wait forever.

use crate::common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        status::StatusMask,
        wait_set::WaitSet,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::{
        data_reader::DataReader,
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration as StdDuration;

const DEADLINE: StdDuration = StdDuration::from_secs(20);
const SAMPLES: u32 = 4;

/// Panics on the first call, then behaves normally.
struct PanicOnceListener {
    calls: Arc<AtomicUsize>,
}

impl DataReaderListener for PanicOnceListener {
    type Foo = KeyedDataType;

    fn on_data_available(&self, _reader: &DataReader<Self::Foo>) {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("deliberate listener panic: the reader must survive this");
        }
    }
}

/// Panics on every call, so no later success can repair state the first panic skipped.
struct AlwaysPanicListener {
    calls: Arc<AtomicUsize>,
}

impl DataReaderListener for AlwaysPanicListener {
    type Foo = KeyedDataType;

    fn on_data_available(&self, _reader: &DataReader<Self::Foo>) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        panic!("deliberate listener panic on every sample");
    }
}

#[test]
fn a_panicking_listener_does_not_stop_delivery() {
    let (tx, rx) = mpsc::sync_channel::<(usize, bool)>(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = Arc::clone(&calls);

    std::thread::Builder::new()
        .name("listener-panic-recovery".into())
        .spawn(move || {
            let domain_id = next_domain_id();
            let factory = DomainParticipantFactory::get_instance();
            let participant = factory
                .create_participant(
                    domain_id,
                    DomainParticipantQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();
            let topic = participant
                .create_topic::<KeyedDataType>(
                    "ListenerPanicTopic",
                    "KeyedDataType",
                    TopicQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();

            let reliability = ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(500),
            };
            let history = HistoryQosPolicy {
                kind: HistoryQosPolicyKind::KeepLast(SAMPLES as i32 * 2),
                ..HistoryQosPolicy::default()
            };

            let subscriber = participant
                .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
                .unwrap();
            let reader = subscriber
                .create_datareader::<KeyedDataType>(
                    &topic,
                    DataReaderQos { reliability, history, ..DataReaderQos::default() },
                    Some(Arc::new(PanicOnceListener { calls })),
                    StatusMask::DATA_AVAILABLE,
                )
                .unwrap();

            let publisher = participant
                .create_publisher(PublisherQos::default(), None, StatusMask::default())
                .unwrap();
            let writer = publisher
                .create_datawriter::<KeyedDataType>(
                    &topic,
                    DataWriterQos { reliability, history, ..DataWriterQos::default() },
                    None,
                    StatusMask::default(),
                )
                .unwrap();

            wait_for_writer_status(
                &writer,
                StatusMask::PUBLICATION_MATCHED,
                Duration::from_seconds(5),
            )
            .expect("writer never matched the reader");
            wait_for_reader_status(
                &reader,
                StatusMask::SUBSCRIPTION_MATCHED,
                Duration::from_seconds(5),
            )
            .expect("reader never matched the writer");

            for key in 0..SAMPLES as i16 {
                writer.write(&KeyedDataType::new(key, key), InstanceHandle::NIL).unwrap();
                std::thread::sleep(StdDuration::from_millis(50));
            }

            // A WaitSet, not a poll: containing the panic must not cost the DATA_AVAILABLE
            // StatusChangedFlag. Polling `take()` cannot see that -- the samples are in the
            // cache either way -- so a boundary that swallows the unwind before the flag is
            // published would look healthy here while every WaitSet waiter blocked forever.
            let status_condition = reader.get_statuscondition().unwrap().clone();
            status_condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap();
            let wait_set = WaitSet::new();
            wait_set.attach_condition(status_condition).unwrap();
            let woken = wait_set.wait(Duration::from_seconds(5)).is_ok();

            let mut taken = 0usize;
            let poll_deadline = std::time::Instant::now() + StdDuration::from_secs(10);
            while taken < SAMPLES as usize && std::time::Instant::now() < poll_deadline {
                if let Ok(samples) = reader.take(
                    SAMPLES as i32,
                    &[SampleStateKind::ANY_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ANY_INSTANCE_STATE],
                ) {
                    taken += samples.len();
                }
                std::thread::sleep(StdDuration::from_millis(20));
            }

            let _ = tx.send((taken, woken));

            let _ = participant.delete_contained_entities();
            let _ = factory.delete_participant(participant);
        })
        .expect("failed to spawn the DDS worker thread");

    let (taken, woken) = match rx.recv_timeout(DEADLINE) {
        Ok(report) => report,
        Err(RecvTimeoutError::Timeout) => {
            eprintln!("the DDS worker never reported within {DEADLINE:?}");
            std::process::exit(101);
        }
        Err(RecvTimeoutError::Disconnected) => {
            panic!("the DDS worker died before reporting")
        }
    };

    assert!(
        observed_calls.load(Ordering::SeqCst) >= 1,
        "the listener never ran, so this test proves nothing about panic recovery"
    );
    assert_eq!(
        taken, SAMPLES as usize,
        "only {taken} of {SAMPLES} samples arrived after the listener panicked once, so the \
         panic left the reader unable to deliver"
    );
    assert!(
        woken,
        "samples arrived but DATA_AVAILABLE never triggered, so containing the panic cost the \
         status flag and every WaitSet waiter blocks forever"
    );
}

/// Containing the panic must not cost the DATA_AVAILABLE `StatusChangedFlag`.
///
/// `handle_data_available_status` runs the listeners first and publishes the flag last, so an
/// unwind that is caught at the callback boundary skips the publish entirely. Delivery still
/// looks healthy to anyone polling `take()` -- the samples are in the cache -- while every
/// `WaitSet::wait` blocks forever with nothing but a log line to explain it.
///
/// DDS 1.4 2.2.4.1 puts the flag transition before the listener, and the sibling
/// `handle_liveliness_changed_status` already implements that order.
///
/// The listener panics on every sample on purpose: with a panic-once listener the next
/// successful delivery republishes the flag and hides the defect.
#[test]
fn a_contained_listener_panic_still_publishes_the_status_flag() {
    let (tx, rx) = mpsc::sync_channel::<bool>(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = Arc::clone(&calls);

    std::thread::Builder::new()
        .name("listener-panic-status-flag".into())
        .spawn(move || {
            let domain_id = next_domain_id();
            let factory = DomainParticipantFactory::get_instance();
            let participant = factory
                .create_participant(
                    domain_id,
                    DomainParticipantQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();
            let topic = participant
                .create_topic::<KeyedDataType>(
                    "ListenerPanicFlagTopic",
                    "KeyedDataType",
                    TopicQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();

            let subscriber = participant
                .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
                .unwrap();
            let reader = subscriber
                .create_datareader::<KeyedDataType>(
                    &topic,
                    DataReaderQos::default(),
                    Some(Arc::new(AlwaysPanicListener { calls })),
                    StatusMask::DATA_AVAILABLE,
                )
                .unwrap();

            let publisher = participant
                .create_publisher(PublisherQos::default(), None, StatusMask::default())
                .unwrap();
            let writer = publisher
                .create_datawriter::<KeyedDataType>(
                    &topic,
                    DataWriterQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();

            wait_for_writer_status(
                &writer,
                StatusMask::PUBLICATION_MATCHED,
                Duration::from_seconds(5),
            )
            .expect("writer never matched the reader");

            // Attach before writing so the waiter cannot miss the transition.
            let status_condition = reader.get_statuscondition().unwrap().clone();
            status_condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap();
            let wait_set = WaitSet::new();
            wait_set.attach_condition(status_condition).unwrap();

            writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();

            let woken = wait_set.wait(Duration::from_seconds(5)).is_ok();
            let _ = tx.send(woken);

            let _ = participant.delete_contained_entities();
            let _ = factory.delete_participant(participant);
        })
        .expect("failed to spawn the DDS worker thread");

    let woken = match rx.recv_timeout(DEADLINE) {
        Ok(woken) => woken,
        Err(RecvTimeoutError::Timeout) => {
            eprintln!("the DDS worker never reported within {DEADLINE:?}");
            std::process::exit(101);
        }
        Err(RecvTimeoutError::Disconnected) => panic!("the DDS worker died before reporting"),
    };

    assert!(
        observed_calls.load(Ordering::SeqCst) >= 1,
        "the listener never ran, so this test proves nothing about the status flag"
    );
    assert!(
        woken,
        "the listener panicked and DATA_AVAILABLE was never published, so containing the panic \
         left every WaitSet waiter blocked"
    );
}
