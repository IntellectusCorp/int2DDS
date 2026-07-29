mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::status::StatusMask,
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
    },
};

/// `get_key_value_serialized` and `lookup_instance_serialized` round-trip on the
/// read side: the serialized key retrieved for a live instance handle looks the
/// same handle back up, an unknown key resolves to NIL, and an unknown handle errors.
#[test]
fn reader_get_key_value_and_lookup_instance_roundtrip() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer = create_datawriter(&participant, PublisherQos::default(), DataWriterQos::default());
    let reader =
        create_datareader(&participant, SubscriberQos::default(), DataReaderQos::default());

    wait_for_reader_status(&reader, StatusMask::SUBSCRIPTION_MATCHED, Duration::from_seconds(2))
        .unwrap();
    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, Duration::from_seconds(2))
        .unwrap();

    writer.write(&KeyedDataType::new(42, 7), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(2)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));

    let samples = reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )
        .unwrap();
    assert_eq!(samples.len(), 1);

    let handle = samples[0].sample_info().instance_handle;
    assert_ne!(handle, InstanceHandle::NIL);

    // get_key_value_serialized returns bytes that look the same handle back up.
    let key_bytes = reader.get_key_value_serialized(handle).unwrap();
    assert!(!key_bytes.is_empty());
    assert_eq!(reader.lookup_instance_serialized(&key_bytes).unwrap(), handle);

    // Unknown key resolves to NIL rather than erroring.
    assert_eq!(
        reader.lookup_instance_serialized(&[0xDE, 0xAD, 0xBE, 0xEF]).unwrap(),
        InstanceHandle::NIL
    );

    // Unknown handle is a BadParameter.
    assert!(reader.get_key_value_serialized(InstanceHandle::new([0xAB; 16])).is_err());

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
