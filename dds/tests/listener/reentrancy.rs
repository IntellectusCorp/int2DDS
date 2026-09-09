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

use crate::common::*;
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
///
/// Teardown is driven by the caller rather than by the worker. The worker owns the reader, the
/// reader owns the listener, and the listener owns the only `Signal` sender -- so a worker that
/// tore down as soon as `write` returned would race the receive thread for the sample and drop
/// the sender out from under a caller that had not decided anything yet.
fn run_probe(probe: Probe, reliability: ReliabilityQosPolicyKind) {
    let (tx, rx) = mpsc::sync_channel::<Signal>(4);
    // Keeps the channel connected for as long as the worker lives. Without it, a sample that is
    // never delivered reports as `Disconnected` ("the worker died") instead of the timeout that
    // actually describes what happened.
    let keepalive = tx.clone();
    // Caller -> worker: the verdict is in, tearing down is now safe.
    let (teardown_tx, teardown_rx) = mpsc::channel::<()>();
    // Worker -> caller: teardown finished, so the next test in this binary does not start
    // against a participant that is still shutting down.
    let (torn_down_tx, torn_down_rx) = mpsc::sync_channel::<()>(1);

    std::thread::Builder::new()
        .name(format!("reentrancy-{probe:?}"))
        .spawn(move || {
            let _keepalive = keepalive;
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
                    DataReaderQos { reliability: qos, ..DataReaderQos::default() },
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

            // `write` returns once the sample is handed to the send path; the listener runs
            // later, on the receive thread. Block until the caller has its verdict rather than
            // racing that delivery. `Err` means the caller already gave up and dropped its
            // sender, which is just as good a cue to clean up.
            let _ = teardown_rx.recv_timeout(DEADLINE);

            // Teardown is best-effort: it blocks if the receive thread is wedged, which is
            // exactly the case this test exists to catch.
            let _ = participant.delete_contained_entities();
            let _ = factory.delete_participant(participant);
            let _ = torn_down_tx.try_send(());
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

    // Verdict is in, so the worker may drop the reader now. Give teardown a bounded chance to
    // finish; if it does not, the caller still returns and the harness moves on.
    let _ = teardown_tx.send(());
    let _ = torn_down_rx.recv_timeout(DEADLINE);
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
