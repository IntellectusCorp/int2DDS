//! Reliable delivery must survive the multicast send path. Samples the group carries and
//! samples only unicast can carry have to arrive as one uninterrupted run on every Reader.

mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{
            domain_participant::DomainParticipant,
            domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
        },
        infrastructure::{
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, ReaderMulticastExtensionQosPolicy,
                ReliabilityQosPolicy, ReliabilityQosPolicyKind,
            },
            status::StatusMask,
        },
        publication::{
            data_writer::DataWriter,
            qos::{DataWriterQos, PublisherQos},
        },
        subscription::{
            data_reader::DataReader,
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::{qos::TopicQos, type_support::DdsType},
    },
};

const GROUP_ADDRESS: &str = "239.255.13.7";
const OTHER_GROUP_ADDRESS: &str = "239.255.13.8";
const SAMPLE_COUNT: i16 = 10;

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct LargeData {
    index: i32,
    data: Vec<u8>,
}

fn reliable_writer_qos() -> DataWriterQos {
    DataWriterQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(64),
            ..Default::default()
        },
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        ..Default::default()
    }
}

fn reliable_group_reader_qos() -> DataReaderQos {
    reader_qos_on_group(GROUP_ADDRESS)
}

fn best_effort_writer_qos() -> DataWriterQos {
    DataWriterQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(64),
            ..Default::default()
        },
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::BestEffort,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        ..Default::default()
    }
}

fn best_effort_reader_qos_on_group(group_address: &str) -> DataReaderQos {
    DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::BestEffort,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        ..reader_qos_on_group(group_address)
    }
}

fn reader_qos_on_group(group_address: &str) -> DataReaderQos {
    DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(64),
            ..Default::default()
        },
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        reader_multicast_extension: ReaderMulticastExtensionQosPolicy {
            group_address: Some(group_address.to_string()),
        },
        ..Default::default()
    }
}

/// A Reader reporting a match says nothing about the Writer having built its ReaderProxy yet.
/// Writing before that would drop the sample for reasons that have nothing to do with the group.
fn wait_for_writer_match_count<Foo: DdsType>(data_writer: &DataWriter<Foo>, expected: i32) {
    for _ in 0..200 {
        if data_writer.get_publication_matched_status().unwrap().current_count() >= expected {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    panic!("Writer never reached {} matched Reader(s)", expected);
}

/// Collect what a Reader has taken, retrying until the expected count arrives or the budget runs
/// out. Repair travels over unicast after the group send, so a single take can be early.
fn take_values(data_reader: &DataReader<KeyedDataType>, expected: usize) -> Vec<i16> {
    let mut values: Vec<i16> = Vec::new();

    for _ in 0..40 {
        let samples = data_reader
            .take(
                64,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap_or_default();

        values.extend(samples.iter().filter_map(|sample| sample.data().ok().map(|d| d.value)));

        if values.len() >= expected {
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    values
}

fn wait_until_matched<Foo: DdsType>(data_reader: &DataReader<Foo>) {
    wait_for_reader_status(
        data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(5),
    )
    .unwrap();
}

fn cleanup(factory: &DomainParticipantFactory, participants: Vec<DomainParticipant>) {
    for participant in participants {
        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }
}

#[test]
fn two_reliable_readers_on_one_group_receive_every_sample() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let writer_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer =
        create_datawriter(&writer_participant, PublisherQos::default(), reliable_writer_qos());
    let first_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        reliable_group_reader_qos(),
    );
    let second_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        reliable_group_reader_qos(),
    );

    wait_until_matched(&first_reader);
    wait_until_matched(&second_reader);
    wait_for_writer_match_count(&data_writer, 2);

    for value in 1..=SAMPLE_COUNT {
        data_writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL).unwrap();
    }

    let expected: Vec<i16> = (1..=SAMPLE_COUNT).collect();
    assert_eq!(
        take_values(&first_reader, expected.len()),
        expected,
        "the first group member must receive every sample in order"
    );
    assert_eq!(
        take_values(&second_reader, expected.len()),
        expected,
        "the second group member must receive the same run from the same datagrams"
    );

    cleanup(factory, vec![writer_participant, reader_participant]);
}

/// BestEffort has no repair, so a group Reader is reachable over multicast alone. That makes this
/// the only case where the receive path is actually load-bearing: under Reliable the unicast
/// repair would deliver the same run even if every group datagram were discarded.
#[test]
fn best_effort_readers_on_separate_groups_receive_over_multicast_alone() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let writer_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer =
        create_datawriter(&writer_participant, PublisherQos::default(), best_effort_writer_qos());
    let first_group_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        best_effort_reader_qos_on_group(GROUP_ADDRESS),
    );
    let second_group_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        best_effort_reader_qos_on_group(OTHER_GROUP_ADDRESS),
    );

    wait_until_matched(&first_group_reader);
    wait_until_matched(&second_group_reader);
    wait_for_writer_match_count(&data_writer, 2);

    for value in 1..=SAMPLE_COUNT {
        data_writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let expected: Vec<i16> = (1..=SAMPLE_COUNT).collect();
    assert_eq!(
        take_values(&first_group_reader, expected.len()),
        expected,
        "a BestEffort Reader must take the datagrams its own group carried"
    );
    assert_eq!(
        take_values(&second_group_reader, expected.len()),
        expected,
        "the Reader on the other group must take its own group's datagrams, not be starved by them"
    );

    cleanup(factory, vec![writer_participant, reader_participant]);
}

/// Two groups reach the same Participant through two sockets, and the receive path decides per
/// Reader which of them may take a datagram. Getting that decision wrong is silent: too strict
/// drops the group's own samples, too loose lets a Reader run ahead on another group's stream.
#[test]
fn readers_on_separate_groups_each_receive_every_sample() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let writer_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer =
        create_datawriter(&writer_participant, PublisherQos::default(), reliable_writer_qos());
    let first_group_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        reader_qos_on_group(GROUP_ADDRESS),
    );
    let second_group_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        reader_qos_on_group(OTHER_GROUP_ADDRESS),
    );

    wait_until_matched(&first_group_reader);
    wait_until_matched(&second_group_reader);
    wait_for_writer_match_count(&data_writer, 2);

    for value in 1..=SAMPLE_COUNT {
        data_writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL).unwrap();
    }

    let expected: Vec<i16> = (1..=SAMPLE_COUNT).collect();
    assert_eq!(
        take_values(&first_group_reader, expected.len()),
        expected,
        "a Reader must receive the run its own group carried"
    );
    assert_eq!(
        take_values(&second_group_reader, expected.len()),
        expected,
        "a Reader on the other group must receive the same run from its own datagrams"
    );

    cleanup(factory, vec![writer_participant, reader_participant]);
}

#[test]
fn a_reader_joining_behind_the_group_is_caught_up_over_unicast() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let writer_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer =
        create_datawriter(&writer_participant, PublisherQos::default(), reliable_writer_qos());
    let early_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        reliable_group_reader_qos(),
    );

    wait_until_matched(&early_reader);
    wait_for_writer_match_count(&data_writer, 1);

    // The group runs ahead while it has one member, so its start sequence number is already
    // above where the second member will stand.
    for value in 1..=SAMPLE_COUNT {
        data_writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL).unwrap();
    }

    // Let the group datagrams above drain before the late member exists. A group datagram names
    // no destination Reader, so the receiver hands it to whichever Readers are matched at the
    // moment it is processed - one still queued when the late member joins would reach it.
    std::thread::sleep(std::time::Duration::from_millis(500));

    let late_reader = create_datareader(
        &reader_participant,
        SubscriberQos::default(),
        reliable_group_reader_qos(),
    );
    wait_until_matched(&late_reader);
    wait_for_writer_match_count(&data_writer, 2);

    for value in (SAMPLE_COUNT + 1)..=(SAMPLE_COUNT * 2) {
        data_writer.write(&KeyedDataType::new(1, value), InstanceHandle::NIL).unwrap();
    }

    assert_eq!(
        take_values(&early_reader, (SAMPLE_COUNT * 2) as usize),
        (1..=SAMPLE_COUNT * 2).collect::<Vec<i16>>(),
        "the member that was there all along must see one uninterrupted run"
    );

    // Volatile durability: the late member owns nothing written before it matched, and the
    // group must not replay that history to it.
    assert_eq!(
        take_values(&late_reader, SAMPLE_COUNT as usize),
        ((SAMPLE_COUNT + 1)..=SAMPLE_COUNT * 2).collect::<Vec<i16>>(),
        "the late member must receive every sample written after it joined, and no earlier one"
    );

    cleanup(factory, vec![writer_participant, reader_participant]);
}

#[test]
fn fragmented_samples_reach_every_group_member() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let writer_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_topic = writer_participant
        .create_topic::<LargeData>(
            "multicast_large_topic",
            "LargeData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let reader_topic = reader_participant
        .create_topic::<LargeData>(
            "multicast_large_topic",
            "LargeData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher = writer_participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let data_writer = publisher
        .create_datawriter::<LargeData>(
            &writer_topic,
            reliable_writer_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let readers: Vec<DataReader<LargeData>> = (0..2)
        .map(|_| {
            let subscriber = reader_participant
                .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
                .unwrap();
            subscriber
                .create_datareader::<LargeData>(
                    &reader_topic,
                    reliable_group_reader_qos(),
                    None,
                    StatusMask::default(),
                )
                .unwrap()
        })
        .collect();

    for reader in &readers {
        wait_until_matched(reader);
    }
    wait_for_writer_match_count(&data_writer, 2);

    // A fragmented sample cannot travel as one group datagram, so it falls back to a unicast
    // send per member. Delivery has to stay complete across that switch.
    for index in 0..3 {
        data_writer
            .write(&LargeData { index, data: vec![7; 256 * 1024] }, InstanceHandle::NIL)
            .unwrap();
    }

    for (position, reader) in readers.iter().enumerate() {
        let mut indices: Vec<i32> = Vec::new();

        for _ in 0..60 {
            let samples = reader
                .take(
                    16,
                    &[SampleStateKind::ANY_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ANY_INSTANCE_STATE],
                )
                .unwrap_or_default();

            indices.extend(samples.iter().filter_map(|sample| sample.data().ok().map(|d| d.index)));

            if indices.len() >= 3 {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        assert_eq!(
            indices,
            vec![0, 1, 2],
            "group member {} must receive every fragmented sample",
            position
        );
    }

    cleanup(factory, vec![writer_participant, reader_participant]);
}
