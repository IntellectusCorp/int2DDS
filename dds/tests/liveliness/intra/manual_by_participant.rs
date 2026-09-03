// ManualByParticipant: write() on any writer of the participant asserts
// liveliness for the whole participant. In a one-writer cell the distinction
// versus ManualByTopic doesn't show up — that's exercised separately when
// multi-writer cases are added later.

use std::time::Duration as StdDuration;

use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration as DdsDuration,
        infrastructure::qos_policy::{LivelinessQosPolicy, LivelinessQosPolicyKind},
        publication::qos::DataWriterQos,
        subscription::qos::DataReaderQos,
    },
};

use crate::common::KeyedDataType;
use crate::helpers::{wait_for_liveliness_changed_state, Scenario};

const LEASE: i32 = 1; // seconds

fn qos_pair() -> (DataWriterQos, DataReaderQos) {
    let liveliness = LivelinessQosPolicy {
        kind: LivelinessQosPolicyKind::ManualByParticipant,
        lease_duration: DdsDuration::from_seconds(LEASE),
    };
    (
        DataWriterQos { liveliness, ..Default::default() },
        DataReaderQos { liveliness, ..Default::default() },
    )
}

#[test]
fn match_alive() {
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    // Manual qos: alive only after the writer actively asserts.
    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();

    s.teardown();
}

#[test]
fn lost() {
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();

    // After lease expires without re-assertion the reader must transition
    // to (alive=0, not_alive=1).
    wait_for_liveliness_changed_state(&reader, 0, 1, StdDuration::from_secs(LEASE as u64 + 2))
        .unwrap();

    s.teardown();
}

#[test]
fn recovered() {
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 1, StdDuration::from_secs(LEASE as u64 + 2))
        .unwrap();

    // Re-assertion after lost must flip back to alive without leaking the
    // not_alive count (Lost→Recovered, not Lost+Match).
    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();

    s.teardown();
}

#[test]
fn unmatch_alive() {
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();

    s.publisher.delete_datawriter(writer).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 0, StdDuration::from_secs(2)).unwrap();

    s.teardown();
}

#[test]
fn sibling_kept_alive() {
    // MBP semantics: write() on any writer of the participant renews lease
    // for ALL MBP writers of that participant. Two writers, only A writes —
    // both must remain alive.
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer_a = s.create_writer(wqos.clone());
    let _writer_b = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer_a.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 2, 0, StdDuration::from_secs(2)).unwrap();

    for _ in 0..2 {
        std::thread::sleep(StdDuration::from_millis(500));
        writer_a.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    }
    let status = reader.get_liveliness_changed_status().unwrap();
    assert_eq!((status.alive_count(), status.not_alive_count()), (2, 0));

    s.teardown();
}

#[test]
fn unmatch_not_alive() {
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 1, StdDuration::from_secs(LEASE as u64 + 2))
        .unwrap();

    // Deleting an already-not-alive writer must drop not_alive_count to 0.
    s.publisher.delete_datawriter(writer).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 0, StdDuration::from_secs(2)).unwrap();

    s.teardown();
}

#[test]
fn assert_keeps_alive() {
    // participant.assert_liveliness() keeps MBP writers alive without write().
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let _writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    s.writer_participant.assert_liveliness().unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();

    for _ in 0..3 {
        std::thread::sleep(StdDuration::from_millis(500));
        s.writer_participant.assert_liveliness().unwrap();
    }
    let status = reader.get_liveliness_changed_status().unwrap();
    assert_eq!((status.alive_count(), status.not_alive_count()), (1, 0));

    s.teardown();
}
