//! A `DataReaderListener` runs on the RTPS receive thread. Nothing may be held across it that
//! the listener can re-acquire from the public API, because `std::sync::Mutex` is not reentrant
//! and the resulting hang has no timeout anywhere in the stack.
//!
//! `DataReader::get_matched_publications` and `get_matched_publication_data` both lock the
//! reader's matched-writer list -- the same list the delivery path in
//! `UserLogic::deliver_change_to_reader` locks before notifying. Calling either from
//! `on_data_available` deadlocked the receive thread on both reliability kinds.
//!
//! Each case runs the whole DDS session on a worker thread and bounds it with a channel, so a
//! regression fails in seconds instead of wedging the test binary. That bound is not optional:
//! `cargo test` has no per-test timeout at all, and nextest's is 300s per attempt.
//! The failure path uses `process::exit` rather than `panic!` on purpose -- unwinding runs
//! `Drop`, which calls `delete_contained_entities()`, which blocks on the very thread that is
//! already stuck.
//!
//! `take()` and `get_qos()` are the controls. They exercise the identical setup and the
//! identical callback but touch different locks, so if they ever hang too, the harness is
//! broken rather than the middleware.

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::{
        data_reader::DataReader,
        data_reader_listener::DataReaderListener,
        qos::DataReaderQos,
        qos::SubscriberQos,
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::time::Duration as StdDuration;

/// Generous enough that a slow board never trips it, short enough that a real deadlock is
/// reported long before nextest's 300s kill.
const DEADLINE: StdDuration = StdDuration::from_secs(20);

/// What the listener does while the receive thread is inside it.
#[derive(Clone, Copy, Debug)]
enum Probe {
    /// Locks the reader's matched-writer list. The defect under test.
    MatchedPublications,
    /// Same list, via the lookup-by-handle path.
    MatchedPublicationData,
    /// Control: touches the history caches, not the matched-writer list.
    Take,
    /// Control: touches only the QoS `ArcSwap`.
    Qos,
}

/// Signals from the listener. `Entered` proves the callback actually ran, so a fix that
/// silently stops delivering cannot masquerade as a pass.
#[derive(Debug, PartialEq, Eq)]
enum Signal {
    Entered,
    Returned,
}

struct ProbingListener {
    probe: Probe,
    tx: SyncSender<Signal>,
}

impl DataReaderListener for ProbingListener {
    type Foo = KeyedDataType;

    fn on_data_available(&self, reader: &DataReader<Self::Foo>) {
        let _ = self.tx.try_send(Signal::Entered);
        match self.probe {
            Probe::MatchedPublications => {
                let _ = reader.get_matched_publications();
            }
            Probe::MatchedPublicationData => {
                let _ = reader.get_matched_publication_data(InstanceHandle::NIL);
            }
            Probe::Take => {
                let _ = reader.take(
                    1,
                    &[SampleStateKind::ANY_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ANY_INSTANCE_STATE],
                );
            }
            Probe::Qos => {
                let _ = reader.get_qos();
            }
        }
        let _ = self.tx.try_send(Signal::Returned);
    }
}

/// Runs one publish/deliver cycle on a worker thread and reports whether the listener returned.
///
/// The worker is deliberately not joined: when the receive thread is wedged, teardown blocks
/// too, so the caller must decide the verdict from the channel alone.
fn run_probe(probe: Probe, reliability: ReliabilityQosPolicyKind) {
    let (tx, rx) = mpsc::sync_channel::<Signal>(4);

    std::thread::Builder::new()
        .name(format!("reentrancy-{probe:?}"))
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
                    "ReentrancyTopic",
                    "KeyedDataType",
                    TopicQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();

            let qos = ReliabilityQosPolicy {
                kind: reliability,
                max_blocking_time: Duration::from_millis(100),
            };

            let subscriber = participant
                .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
                .unwrap();
            let reader = subscriber
                .create_datareader::<KeyedDataType>(
                    &topic,
                    DataReaderQos { reliability: qos.clone(), ..DataReaderQos::default() },
                    Some(Arc::new(ProbingListener { probe, tx })),
                    StatusMask::DATA_AVAILABLE,
                )
                .unwrap();

            let publisher = participant
                .create_publisher(PublisherQos::default(), None, StatusMask::default())
                .unwrap();
            let writer = publisher
                .create_datawriter::<KeyedDataType>(
                    &topic,
                    DataWriterQos { reliability: qos, ..DataWriterQos::default() },
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

            writer.write(&KeyedDataType::new(1, 42), InstanceHandle::NIL).unwrap();

            // Teardown is best-effort: it blocks if the receive thread is wedged, which is
            // exactly the case this test exists to catch.
            let _ = participant.delete_contained_entities();
            let _ = factory.delete_participant(participant);
        })
        .expect("failed to spawn the DDS worker thread");

    match rx.recv_timeout(DEADLINE) {
        Ok(Signal::Entered) => {}
        Ok(Signal::Returned) => panic!("{probe:?}: got Returned before Entered"),
        Err(RecvTimeoutError::Timeout) => panic!(
            "{probe:?}/{reliability:?}: on_data_available never ran within {DEADLINE:?}, \
             so this case proves nothing about reentrancy"
        ),
        Err(RecvTimeoutError::Disconnected) => {
            panic!("{probe:?}/{reliability:?}: the DDS worker died before delivering a sample")
        }
    }

    match rx.recv_timeout(DEADLINE) {
        Ok(Signal::Returned) => {}
        Ok(Signal::Entered) => panic!("{probe:?}: unexpected second Entered"),
        Err(RecvTimeoutError::Disconnected) => {
            panic!("{probe:?}/{reliability:?}: the listener panicked instead of returning")
        }
        Err(RecvTimeoutError::Timeout) => {
            // The receive thread is stuck inside the listener. Unwinding would run Drop and
            // block on the same thread, so leave the process immediately instead.
            eprintln!(
                "DEADLOCK: {probe:?} on a {reliability:?} reader entered on_data_available \
                 and never returned within {DEADLINE:?}"
            );
            std::process::exit(101);
        }
    }
}

#[test]
fn get_matched_publications_from_listener_does_not_deadlock_best_effort() {
    run_probe(Probe::MatchedPublications, ReliabilityQosPolicyKind::BestEffort);
}

#[test]
fn get_matched_publications_from_listener_does_not_deadlock_reliable() {
    run_probe(Probe::MatchedPublications, ReliabilityQosPolicyKind::Reliable);
}

#[test]
fn get_matched_publication_data_from_listener_does_not_deadlock() {
    run_probe(Probe::MatchedPublicationData, ReliabilityQosPolicyKind::BestEffort);
}

/// Control. Touches the history caches, which the delivery path releases before notifying.
#[test]
fn take_from_listener_does_not_deadlock() {
    run_probe(Probe::Take, ReliabilityQosPolicyKind::BestEffort);
}

/// Control. Touches no delivery-path lock at all.
#[test]
fn get_qos_from_listener_does_not_deadlock() {
    run_probe(Probe::Qos, ReliabilityQosPolicyKind::BestEffort);
}
