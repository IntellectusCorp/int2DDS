// Automatic liveliness — the stack asserts on its own cadence, so only
// `match` and `unmatch_alive` are reachable inside a single test. The lost /
// recovered / unmatch_not_alive cells live under manual_by_* qos.

use std::time::Duration as StdDuration;

use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration as DdsDuration,
        domain::domain_participant_factory::DomainParticipantFactory,
        infrastructure::qos_policy::{
            LivelinessQosPolicy, LivelinessQosPolicyKind, WriterDataLifecycleQosPolicy,
        },
        publication::qos::DataWriterQos,
        subscription::qos::DataReaderQos,
    },
};

use crate::common::KeyedDataType;
use crate::helpers::{
    wait_for_liveliness_changed_state, wait_for_no_writers_sample,
    wait_for_publication_matched_count, wait_for_subscription_matched_count, Scenario,
};

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

#[test]
fn unmatch_propagation_to_reader() {
    int2dds::common::env::set_log_type(int2dds::common::log::LogType::File);
    int2dds::common::env::set_file_log_level(int2dds::common::log::LogLevel::Debug);

    // matched_count, liveliness counts, and a NOT_ALIVE_NO_WRITERS sample
    // must all reflect the unmatch. autodispose=false for NO_WRITERS.
    let s = Scenario::inter();
    let (mut wqos, rqos) = qos_pair();
    wqos.writer_data_lifecycle =
        WriterDataLifecycleQosPolicy { autodispose_unregistered_instances: false };

    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();
    wait_for_subscription_matched_count(&reader, 1, StdDuration::from_secs(2)).unwrap();
    wait_for_publication_matched_count(&writer, 1, StdDuration::from_secs(2)).unwrap();

    // Instance must exist before NO_WRITERS can surface.
    writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    std::thread::sleep(StdDuration::from_millis(500));

    s.publisher.delete_datawriter(writer).unwrap();
    std::thread::sleep(StdDuration::from_millis(500));

    wait_for_subscription_matched_count(&reader, 0, StdDuration::from_secs(3)).unwrap();
    wait_for_liveliness_changed_state(&reader, 0, 0, StdDuration::from_secs(3)).unwrap();
    wait_for_no_writers_sample(&reader, StdDuration::from_secs(3)).unwrap();
}

#[test]
fn unmatch_via_delete_participant() {
    // int2dds::common::env::set_log_type(int2dds::common::log::LogType::File);
    // int2dds::common::env::set_file_log_level(int2dds::common::log::LogLevel::Debug);

    // Dropping the writer participant must unmatch via SEDP dispose.
    let s = Scenario::inter();
    let (wqos, rqos) = qos_pair();
    let writer = s.create_writer(wqos);
    let reader = s.create_reader(rqos);

    wait_for_publication_matched_count(&writer, 1, StdDuration::from_secs(2)).unwrap();
    wait_for_subscription_matched_count(&reader, 1, StdDuration::from_secs(2)).unwrap();
    wait_for_liveliness_changed_state(&reader, 1, 0, StdDuration::from_secs(2)).unwrap();

    std::thread::sleep(StdDuration::from_millis(500));

    s.writer_participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance()
        .delete_participant(s.writer_participant.clone())
        .unwrap();

    wait_for_liveliness_changed_state(&reader, 0, 0, StdDuration::from_secs(5)).unwrap();
}
