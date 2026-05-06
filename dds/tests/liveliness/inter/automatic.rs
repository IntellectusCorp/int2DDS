use std::time::Duration as StdDuration;

use int2dds::dcps::{
    core::time::Duration as DdsDuration,
    infrastructure::qos_policy::{LivelinessQosPolicy, LivelinessQosPolicyKind},
    publication::qos::DataWriterQos,
    subscription::qos::DataReaderQos,
};

use crate::helpers::{wait_for_liveliness_changed_state, Scenario};

fn qos_pair() -> (DataWriterQos, DataReaderQos) {
    let liveliness = LivelinessQosPolicy {
        kind: LivelinessQosPolicyKind::Automatic,
        lease_duration: DdsDuration::from_seconds(1),
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

    let _writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(1)).unwrap();
}

#[test]
fn unmatch_alive() {
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();

    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(1)).unwrap();

    // Delete on writer side; SEDP dispose travels to the reader's participant.
    s.publisher.delete_datawriter(writer).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 0, StdDuration::from_secs(1)).unwrap();
}
