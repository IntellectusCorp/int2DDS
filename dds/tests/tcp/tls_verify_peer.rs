//! Two participants that disagree on verify_peer still exchange samples over TLS.

use std::time::Duration as StdDuration;

use crate::common::*;
use crate::tls_reload::{free_tcp_ports, reaches_reader, tls_node, Ca};
use int2dds::dcps::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::qos::{DataReaderQos, SubscriberQos},
};

fn without_peer_verification(mut qos: DomainParticipantQos) -> DomainParticipantQos {
    qos.property.remove_property("int2dds.tls.crl_file");
    qos.property.add_property("int2dds.tls.verify_peer", "false", false);
    qos
}

fn samples_flow_when_only_one_side_verifies(writer_verifies: bool) {
    let dir = tempfile::tempdir().unwrap();
    let ca = Ca::new();
    let (writer_port, reader_port) = free_tcp_ports();
    let (writer_qos, _) = tls_node(dir.path(), &ca, "writer", 2, writer_port, reader_port);
    let (reader_qos, _) = tls_node(dir.path(), &ca, "reader", 3, reader_port, writer_port);
    let (writer_qos, reader_qos) = if writer_verifies {
        (writer_qos, without_peer_verification(reader_qos))
    } else {
        (without_peer_verification(writer_qos), reader_qos)
    };

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let writer_participant =
        factory.create_participant(domain_id, writer_qos, None, StatusMask::default()).unwrap();
    let reader_participant =
        factory.create_participant(domain_id, reader_qos, None, StatusMask::default()).unwrap();

    let best_effort = ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::BestEffort,
        max_blocking_time: Duration { sec: 0, nanosec: 0 },
    };
    let data_writer = create_datawriter(
        &writer_participant,
        PublisherQos::default(),
        DataWriterQos { reliability: best_effort.clone(), ..Default::default() },
    );
    let data_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        DataReaderQos { reliability: best_effort, ..Default::default() },
    );

    let flowed = reaches_reader(&data_writer, &data_reader, 1, StdDuration::from_secs(30));

    writer_participant.delete_contained_entities().unwrap();
    reader_participant.delete_contained_entities().unwrap();
    factory.delete_participant(writer_participant).unwrap();
    factory.delete_participant(reader_participant).unwrap();

    assert!(flowed, "no sample reached the reader (writer verifies peers: {writer_verifies})");
}

#[test]
fn a_writer_without_peer_verification_reaches_a_reader_with_it() {
    samples_flow_when_only_one_side_verifies(false);
}

#[test]
fn a_writer_with_peer_verification_reaches_a_reader_without_it() {
    samples_flow_when_only_one_side_verifies(true);
}
