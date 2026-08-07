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

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        status::StatusMask,
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

/// Panics the first time it is called, then behaves normally.
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

#[test]
fn a_panicking_listener_does_not_stop_delivery() {
    let (tx, rx) = mpsc::sync_channel::<usize>(1);
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
                    DataReaderQos {
                        reliability: reliability.clone(),
                        history: history.clone(),
                        ..DataReaderQos::default()
                    },
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

            // Poll for everything except the sample the panicking callback was handling: the
            // question is whether delivery continues at all, not whether that one survived.
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

            let _ = tx.send(taken);

            let _ = participant.delete_contained_entities();
            let _ = factory.delete_participant(participant);
        })
        .expect("failed to spawn the DDS worker thread");

    let taken = match rx.recv_timeout(DEADLINE) {
        Ok(taken) => taken,
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
}
