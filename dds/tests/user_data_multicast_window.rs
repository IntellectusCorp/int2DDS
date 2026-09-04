//! A fragmented sample longer than the group's send window has to complete on every member.
//!
//! The group is bounded by the smallest window among its members, so a sample larger than that
//! goes out as a multicast prefix per window; each member then asks for the rest over unicast
//! with NACK_FRAG and is repaired there. Pinning the receive buffer is what forces several
//! windows: a host that grants megabytes takes the sample in one.
//!
//! One test per binary, because the buffer size is read from the environment by every socket
//! this process opens.

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
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            data_reader::DataReader,
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::{qos::TopicQos, type_support::DdsType},
    },
};

const GROUP_ADDRESS: &str = "239.255.13.9";
const SOCKET_BUFFER_BYTES: &str = "131072";
const PAYLOAD_BYTES: usize = 512 * 1024;
const SAMPLE_COUNT: i32 = 2;

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct LargeData {
    index: i32,
    data: Vec<u8>,
}

fn reliable() -> ReliabilityQosPolicy {
    ReliabilityQosPolicy {
        kind: ReliabilityQosPolicyKind::Reliable,
        max_blocking_time: Duration { sec: 5, nanosec: 0 },
    }
}

fn keep_all() -> HistoryQosPolicy {
    HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: false }
}

fn create_large_topic(participant: &DomainParticipant) -> int2dds::dcps::topic::topic::Topic {
    participant
        .create_topic::<LargeData>(
            "multicast_window_topic",
            "LargeData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

fn create_group_reader(participant: &DomainParticipant) -> DataReader<LargeData> {
    let topic = create_large_topic(participant);
    participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datareader::<LargeData>(
            &topic,
            DataReaderQos {
                history: keep_all(),
                reliability: reliable(),
                reader_multicast_extension: ReaderMulticastExtensionQosPolicy {
                    group_address: Some(GROUP_ADDRESS.to_string()),
                },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap()
}

fn take_indices(reader: &DataReader<LargeData>, expected: usize) -> Vec<i32> {
    let mut indices: Vec<i32> = Vec::new();
    for _ in 0..200 {
        let samples = reader
            .take(
                16,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap_or_default();
        for sample in samples.iter() {
            let data = sample.data().unwrap();
            assert_eq!(data.data.len(), PAYLOAD_BYTES, "sample {} arrived short", data.index);
            assert!(data.data.iter().all(|&b| b == 7), "sample {} arrived corrupt", data.index);
            indices.push(data.index);
        }
        if indices.len() >= expected {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    indices
}

#[test]
fn a_sample_longer_than_the_group_window_completes_on_every_member() {
    // Read by every listener as it binds, so it has to be set before the first participant.
    std::env::set_var("INT2DDS_UDP_SOCKET_BUFFER", SOCKET_BUFFER_BYTES);

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let writer_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let first_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let second_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_topic = create_large_topic(&writer_participant);
    let data_writer = writer_participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datawriter::<LargeData>(
            &writer_topic,
            DataWriterQos { history: keep_all(), reliability: reliable(), ..Default::default() },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let readers =
        [create_group_reader(&first_participant), create_group_reader(&second_participant)];
    for reader in &readers {
        wait_for_reader_status(reader, StatusMask::SUBSCRIPTION_MATCHED, Duration::from_seconds(5))
            .unwrap();
    }
    for _ in 0..200 {
        if data_writer.get_publication_matched_status().unwrap().current_count() >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    for index in 0..SAMPLE_COUNT {
        data_writer
            .write(&LargeData { index, data: vec![7; PAYLOAD_BYTES] }, InstanceHandle::NIL)
            .unwrap();
    }

    for (position, reader) in readers.iter().enumerate() {
        assert_eq!(
            take_indices(reader, SAMPLE_COUNT as usize),
            (0..SAMPLE_COUNT).collect::<Vec<i32>>(),
            "group member {} must complete every multi-window sample",
            position
        );
    }

    for participant in [writer_participant, first_participant, second_participant] {
        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }
}
