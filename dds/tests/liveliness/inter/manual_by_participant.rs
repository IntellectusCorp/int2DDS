use std::time::Duration as StdDuration;

use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration as DdsDuration,
        domain::domain_participant_factory::DomainParticipantFactory,
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
        kind: LivelinessQosPolicyKind::ManualByParticipant,
        lease_duration: DdsDuration::from_seconds(LEASE),
    };
    (
        DataWriterQos { liveliness: liveliness.clone(), ..Default::default() },
        DataReaderQos { liveliness, ..Default::default() },
    )
}

#[test]
fn match_alive() {
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(1)).unwrap();
}

#[test]
fn lost() {
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(1)).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 1, StdDuration::from_secs(LEASE as u64 + 3))
        .unwrap();
}

#[test]
fn recovered() {
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(1)).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 1, StdDuration::from_secs(LEASE as u64 + 3))
        .unwrap();

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(3)).unwrap();
}

#[test]
fn unmatch_alive() {
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(1)).unwrap();

    s.publisher.delete_datawriter(writer).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 0, StdDuration::from_secs(1)).unwrap();
}

#[test]
fn unmatch_not_alive() {
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(1)).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 1, StdDuration::from_secs(LEASE as u64 + 3))
        .unwrap();

    s.publisher.delete_datawriter(writer).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 0, StdDuration::from_secs(1)).unwrap();
}

#[test]
fn sibling_kept_alive() {
    // MBP: write() on any sibling on the writer participant renews the whole
    // participant. Only A writes — B must stay alive across multiple lease
    // windows.
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();
    let writer_a = s.create_writer(wqos.clone());
    let _writer_b = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer_a.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 2, 0, StdDuration::from_secs(5)).unwrap();

    for _ in 0..2 {
        std::thread::sleep(StdDuration::from_millis(500));
        writer_a.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
        let status = reader.get_liveliness_changed_status().unwrap();
        assert_eq!(
            (status.alive_count(), status.not_alive_count()),
            (2, 0),
            "sibling B must never flip not_alive while A asserts"
        );
    }
}

#[test]
fn assert_keeps_alive() {
    // participant.assert_liveliness() keeps MBP writers alive without write().
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();
    let _writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    s.writer_participant.assert_liveliness().unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(5)).unwrap();

    for _ in 0..3 {
        std::thread::sleep(StdDuration::from_millis(500));
        s.writer_participant.assert_liveliness().unwrap();
    }
    let status = reader.get_liveliness_changed_status().unwrap();
    assert_eq!((status.alive_count(), status.not_alive_count()), (1, 0));
}

#[test]
fn unmatch_via_delete_participant() {
    // Dropping the writer participant must unmatch via SEDP dispose.
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(5)).unwrap();

    s.writer_participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance()
        .delete_participant(s.writer_participant.clone())
        .unwrap();

    wait_for_liveliness_changed_state(&reader, 0, 0, StdDuration::from_secs(5)).unwrap();
}
