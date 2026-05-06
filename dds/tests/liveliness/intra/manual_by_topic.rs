// ManualByTopic: write() asserts only the calling writer (not its siblings
// on the same participant). In a single-writer cell the recipe matches
// manual_by_participant verbatim — the difference shows up in multi-writer
// cases reserved for follow-up work.

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

const LEASE: i32 = 1;

fn qos_pair() -> (DataWriterQos, DataReaderQos) {
    let liveliness = LivelinessQosPolicy {
        kind: LivelinessQosPolicyKind::ManualByTopic,
        lease_duration: DdsDuration::from_seconds(LEASE),
    };
    (
        DataWriterQos { liveliness: liveliness.clone(), ..Default::default() },
        DataReaderQos { liveliness, ..Default::default() },
    )
}

#[test]
fn match_alive() {
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();
}

#[test]
fn lost() {
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 1, StdDuration::from_secs(LEASE as u64 + 2))
        .unwrap();
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

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();
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
}

#[test]
fn sibling_lost_independently() {
    // MBT semantics: write() on A renews ONLY A's lease. B's lease expires
    // independently — reader must observe (alive=1, not_alive=1).
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer_a = s.create_writer(wqos.clone());
    let _writer_b = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer_a.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 2, 0, StdDuration::from_secs(2)).unwrap();

    // Keep A asserting; B must flip not_alive after its own lease elapses.
    let deadline = std::time::Instant::now() + StdDuration::from_secs(LEASE as u64 + 2);
    while std::time::Instant::now() < deadline {
        writer_a.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
        std::thread::sleep(StdDuration::from_millis(300));
    }
    wait_for_liveliness_changed_state(&reader, 1, 1, StdDuration::from_secs(1)).unwrap();
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

    s.publisher.delete_datawriter(writer).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 0, StdDuration::from_secs(2)).unwrap();
}

#[test]
fn assert_keeps_alive() {
    // writer.assert_liveliness() keeps the calling writer alive without write().
    let s = Scenario::intra();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.assert_liveliness().unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();

    for _ in 0..3 {
        std::thread::sleep(StdDuration::from_millis(500));
        writer.assert_liveliness().unwrap();
    }
    let status = reader.get_liveliness_changed_status().unwrap();
    assert_eq!((status.alive_count(), status.not_alive_count()), (1, 0));
}
