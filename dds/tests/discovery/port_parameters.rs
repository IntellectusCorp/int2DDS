//! Participant ports driven by the port parameter properties.
//!
//! The port formula `PB + DG * domain + offset + PG * participant_id` is read
//! from the participant QoS. Two participants only meet when both evaluate it
//! to the same ports, a pinned participant id is never searched around, and
//! the id search stops at the last id whose ports still belong to the domain.

use crate::common::*;
use int2dds::{
    core::time::Duration,
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
    },
    infrastructure::{qos_policy::PropertyQosPolicy, status::StatusMask},
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::qos::{DataReaderQos, SubscriberQos},
};

fn participant_qos(properties: &[(&str, &str)]) -> DomainParticipantQos {
    let mut property = PropertyQosPolicy::default();
    for (name, value) in properties {
        property.add_property(*name, *value, false);
    }
    DomainParticipantQos { property, ..Default::default() }
}

fn delete(participant: DomainParticipant) {
    participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
}

#[test]
fn participants_with_the_same_port_parameters_discover_each_other() {
    let factory = DomainParticipantFactory::get_instance();
    // Keeps every port below the Windows dynamic range, whatever domain comes up
    let domain = next_domain_id() % 100;
    let shared_parameters = [
        ("int2dds.transport.port_base", "20000"),
        ("int2dds.transport.domain_id_gain", "250"),
        ("int2dds.transport.participant_id_gain", "2"),
        ("int2dds.transport.discovery_multicast_offset", "0"),
        ("int2dds.transport.discovery_unicast_offset", "10"),
        ("int2dds.transport.user_multicast_offset", "1"),
        ("int2dds.transport.user_unicast_offset", "11"),
    ];

    let publishing = factory
        .create_participant(
            domain,
            participant_qos(&shared_parameters),
            None,
            StatusMask::default(),
        )
        .expect("publishing participant");
    let subscribing = factory
        .create_participant(
            domain,
            participant_qos(&shared_parameters),
            None,
            StatusMask::default(),
        )
        .expect("subscribing participant");
    let data_writer =
        create_datawriter(&publishing, PublisherQos::default(), DataWriterQos::default());
    let data_reader =
        create_datareader(&subscribing, SubscriberQos::default(), DataReaderQos::default());

    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(5),
    )
    .expect("the writer matches the reader over the moved ports");
    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(5),
    )
    .expect("the reader matches the writer over the moved ports");

    drop((data_writer, data_reader));
    delete(publishing);
    delete(subscribing);
}

#[test]
fn a_pinned_participant_id_fails_when_its_ports_are_taken() {
    let factory = DomainParticipantFactory::get_instance();
    let domain = next_domain_id();

    // Takes the domain's first free id, which is 0 unless another process holds it.
    // Either way id 0 is taken once this participant exists.
    let searching = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .expect("searching participant");

    let pinned_to_taken_id = factory.create_participant(
        domain,
        participant_qos(&[("int2dds.participant_id", "0")]),
        None,
        StatusMask::default(),
    );
    assert!(pinned_to_taken_id.is_err(), "a pinned id whose ports are taken must not be moved");

    let pinned_to_free_id = factory
        .create_participant(
            domain,
            participant_qos(&[("int2dds.participant_id", "7")]),
            None,
            StatusMask::default(),
        )
        .expect("a pinned id whose ports are free");

    delete(pinned_to_free_id);
    delete(searching);
}

#[test]
fn the_participant_id_search_stops_at_the_end_of_the_domain_block() {
    let factory = DomainParticipantFactory::get_instance();
    let domain = next_domain_id();

    // The domain block is DG = 20 ports wide. The user unicast port of id p is
    // offset d3 + PG * p = 11 + 2 * p into it, so the last id that stays inside is
    // (20 - 1 - 11) / 2 = 4, which leaves ids 0..=4 and makes the sixth creation fail.
    let small_block = [
        ("int2dds.transport.port_base", "30000"),
        ("int2dds.transport.domain_id_gain", "20"),
        ("int2dds.transport.participant_id_gain", "2"),
    ];
    let create = |domain: i32| {
        factory.create_participant(
            domain,
            participant_qos(&small_block),
            None,
            StatusMask::default(),
        )
    };

    // Another process may hold some of these ports, so fewer than five can succeed.
    let mut created = Vec::new();
    let exhausted = loop {
        match create(domain) {
            Ok(participant) => created.push(participant),
            Err(error) => break error,
        }
        assert!(created.len() <= 5, "id 5 would take the next domain's ports");
    };
    // The searched range in the error proves the bound was 4, however many ids
    // another process already held.
    assert!(exhausted.to_string().contains("participant ids 0..=4 are all in use"), "{exhausted}");

    let next_domain = create(domain + 1).expect("the next domain's block is untouched");

    delete(next_domain);
    for participant in created {
        delete(participant);
    }
}
