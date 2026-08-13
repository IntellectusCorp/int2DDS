//! The batched multi-reader path (`send_unsent_changes_of_stateful_writer`'s batched-group
//! branch) must actually run for two readers on one participant, not merely deliver bytes some
//! other path could equally well have delivered.
//!
//! Both the batched arm and the per-reader deferred arm resolve `reader_id = EntityId::UNKNOWN`
//! traffic to the exact same matched-reader set (`find_readers_matched_with_remote_writer`), so
//! "both readers received the sample byte-for-byte" is true whichever arm ran and proves
//! nothing about which one did. A delivery-only test here would be the same class of mistake
//! `frag_small_payload.rs` made. So this test also captures the `debug!` line unique to the
//! batched DATA_FRAG arm -- `"Batched DATA_FRAG sn={} to {} readers behind one participant"`,
//! present only at that call site, with the plain-DATA counterpart using a different string --
//! and asserts it was emitted with a reader count of 2.
//!
//! This does NOT assert heartbeat placement, `increase_heartbeat_count()` call counts, or that
//! exactly one datagram went out instead of two: none of that is observable through the DDS
//! API, and no assertion is written that cannot fail. Byte-correct reassembly on both readers
//! proves "the batched burst plus any repair" delivered correctly; it does not by itself prove
//! the batched burst alone emitted every fragment.
//!
//! Modeled on `frag_packed_submessage.rs` (payload generator, byte-check helper) and
//! `presentation/coherent_access.rs` (one participant hosting two readers).
//!
//! Env is process-global and this file rewrites it, so there is exactly one `#[test]`.
//!
//! Also a regression guard for the shared-buffer fan-out defect: on a host with a small
//! `net.core.rmem_max`, this 131 KB burst genuinely overruns the kernel receive buffer, so
//! completion here often rides on a real per-reader NACK_FRAG repair race rather than the
//! batched burst alone. Before the fix, whichever reader's repair happened to complete the
//! shared reassembly buffer first was delivered the sample while the buffer was destroyed,
//! permanently stranding the other reader. The byte-for-byte checks on both readers below
//! catch that regardless of which arm ran or how much repair it took to get there.

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{
            DataFragQosPolicy, HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy,
            ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
    DdsType,
};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 131,072 bytes at a 1344-byte fragment size is 98 fragments; at a 65,000-byte message budget
/// (48 fragments/submessage) that packs into three DATA_FRAG submessages: (1,48) (49,48) (97,2).
const PAYLOAD_BYTES: usize = 131_072;

/// Far above the sub-second delivery seen on the development board, low enough that a stalled
/// burst fails rather than hanging the binary.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct BatchedPayload {
    index: i32,
    data: Vec<u8>,
}

/// Position-varying payload: byte `i` is `i % 251`. Same generator as
/// `frag_packed_submessage.rs`: 251 is prime and coprime with the 1344-byte fragment size, so a
/// fragment run landing at the wrong offset changes the bytes there instead of accidentally
/// reproducing the correct sequence.
fn expected_payload() -> Vec<u8> {
    (0..PAYLOAD_BYTES).map(|i| (i % 251) as u8).collect()
}

/// Byte-for-byte check against `expected_payload()`. Fails on the first mismatching index
/// instead of `assert_eq!`'s whole-vector diff.
fn assert_payload_matches(actual: &[u8], context: &str) {
    let expected = expected_payload();
    for (index, (a, e)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(
            a, e,
            "{context}: byte {index} does not match the expected i % 251 pattern, so a \
             fragment run landed at the wrong offset or is corrupted"
        );
    }
}

/// Polls `data_reader` until it has taken one sample or `DEADLINE` (from now) elapses.
fn wait_for_one_sample(data_reader: &DataReader<BatchedPayload>) -> Option<BatchedPayload> {
    let mut received = None;
    let deadline = Instant::now() + DEADLINE;
    while received.is_none() && Instant::now() < deadline {
        if let Ok(samples) = data_reader.take(
            1,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples {
                if let Ok(data) = sample.data() {
                    received = Some(data);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    received
}

/// Records only the log lines naming the batched DATA_FRAG arm specifically -- the plain-DATA
/// batched arm logs a different string ("Batched DATA sn=..." without `_FRAG`), so this cannot
/// be fooled by the topic failing to fragment at all.
struct BatchedFragLogCapture {
    lines: Arc<Mutex<Vec<String>>>,
}

impl log::Log for BatchedFragLogCapture {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let line = record.args().to_string();
        if line.contains("Batched DATA_FRAG") {
            self.lines.lock().unwrap().push(line);
        }
    }

    fn flush(&self) {}
}

#[test]
fn two_readers_on_one_participant_reach_the_batched_data_frag_arm() {
    // First statement, unconditionally: if a logger is already installed in this process this
    // must fail loudly, not silently fall back to a no-op logger and lose the whole proof.
    let captured_lines = Arc::new(Mutex::new(Vec::new()));
    log::set_boxed_logger(Box::new(BatchedFragLogCapture { lines: captured_lines.clone() }))
        .expect("no logger should already be installed in this test binary");
    log::set_max_level(log::LevelFilter::Debug);

    // Pinned explicitly: built-in defaults are not this test's business, and depending on them
    // is exactly the failure mode that let frag_small_payload.rs validate the wrong thing.
    int2dds::common::env::set_max_message_size(65_000);

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<BatchedPayload>(
            "FragBatchedTopic",
            "BatchedPayload",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let reliability = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration::from_seconds(5),
    };
    // KeepAll: the sample must stay in the writer cache until every fragment is acked.
    let history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: false };

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    // Two readers behind the SAME participant prefix: send_unsent_changes_of_stateful_writer
    // groups reader proxies by (participant prefix, pending SN, locator list), and only a
    // group of two or more takes the batched path instead of the single-reader path. Whether
    // they share a Subscriber is irrelevant -- Subscriber is not part of the grouping key.
    let reader_a = subscriber
        .create_datareader::<BatchedPayload>(
            &topic,
            DataReaderQos { reliability, history, ..DataReaderQos::default() },
            None,
            StatusMask::default(),
        )
        .unwrap();
    let reader_b = subscriber
        .create_datareader::<BatchedPayload>(
            &topic,
            DataReaderQos { reliability, history, ..DataReaderQos::default() },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let data_writer = publisher
        .create_datawriter::<BatchedPayload>(
            &topic,
            DataWriterQos {
                reliability,
                history,
                data_frag: DataFragQosPolicy { max_size: 1_344 },
                ..DataWriterQos::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Poll get_matched_subscriptions() rather than waiting on a PUBLICATION_MATCHED
    // StatusCondition: the WaitSet fires on the *first* match, so a write right after could
    // still see only one reader proxy. get_matched_subscriptions().len() == 2 is the condition
    // send_unsent_changes_of_stateful_writer itself iterates, not an approximation of it.
    let match_deadline = Instant::now() + DEADLINE;
    loop {
        let matched = data_writer.get_matched_subscriptions().unwrap().len();
        if matched >= 2 {
            break;
        }
        assert!(
            Instant::now() < match_deadline,
            "writer matched only {matched} of 2 expected readers within {DEADLINE:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // Asserted precondition, not just a loop exit: the write below is only meaningful if
    // exactly this holds at the moment it happens.
    assert_eq!(
        data_writer.get_matched_subscriptions().unwrap().len(),
        2,
        "writer must see exactly the two readers this test created"
    );

    // The writer and reader sides of SEDP matching complete independently: the writer seeing
    // both readers does not guarantee either reader's OWN writer_proxies entry -- the thing
    // find_readers_matched_with_remote_writer checks on the receive side -- is populated yet.
    // Wait for both readers to independently confirm their own match before writing.
    for (tag, reader) in [("A", &reader_a), ("B", &reader_b)] {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let matched = reader.get_subscription_matched_status().unwrap().current_count();
            if matched >= 1 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "reader {tag} never confirmed its own match with the writer within {DEADLINE:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    // Exactly one write: a second sample arriving before both readers are grouped together
    // would turn the pending plan into a multi-change catch-up, which always takes the
    // deferred arm regardless of reader count.
    data_writer
        .write(&BatchedPayload { index: 0, data: expected_payload() }, InstanceHandle::NIL)
        .unwrap();

    let received_a = wait_for_one_sample(&reader_a);
    let received_b = wait_for_one_sample(&reader_b);

    // A second take on each reader must find nothing: exactly one sample was published, so
    // exactly one may have arrived.
    let extra_a = reader_a.take(
        1,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );
    let extra_b = reader_b.take(
        1,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );

    // Tear down before asserting: a panic here unwinds into Drop, which blocks on the
    // receive thread.
    let _ = participant.delete_contained_entities();
    let _ = factory.delete_participant(participant);

    let sample_a = received_a
        .unwrap_or_else(|| panic!("reader A never received the batched burst within {DEADLINE:?}"));
    let sample_b = received_b
        .unwrap_or_else(|| panic!("reader B never received the batched burst within {DEADLINE:?}"));

    assert!(extra_a.is_err(), "reader A took a second sample; only one was ever published");
    assert!(extra_b.is_err(), "reader B took a second sample; only one was ever published");

    assert_eq!(sample_a.index, 0);
    assert_eq!(sample_a.data.len(), PAYLOAD_BYTES, "reader A: reassembly lost or gained bytes");
    assert_payload_matches(&sample_a.data, "reader A");

    assert_eq!(sample_b.index, 0);
    assert_eq!(sample_b.data.len(), PAYLOAD_BYTES, "reader B: reassembly lost or gained bytes");
    assert_payload_matches(&sample_b.data, "reader B");

    // The decisive assertion: the batched DATA_FRAG arm actually ran, addressing both readers
    // in one burst, rather than the deferred per-reader arm delivering the same bytes twice.
    let lines = captured_lines.lock().unwrap();
    assert!(
        lines.iter().any(|line| line.contains("to 2 readers behind one participant")),
        "the batched DATA_FRAG log line was never emitted with a count of 2 readers; captured: \
         {lines:?}. Either the two readers were never grouped together (fell through to the \
         per-reader path) or the payload never fragmented at all (the plain-DATA batched arm \
         would have logged a different string)."
    );
}
