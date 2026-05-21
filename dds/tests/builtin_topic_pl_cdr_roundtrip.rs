//! Round-trip tests for the 4 Mutable builtin discovery topic data types.
//!
//! Verifies that PL_CDR encoding/decoding produces the spec-compliant
//! RTPS Table 9.18 wire format and that DdsType::serialize is byte-identical
//! to the dedicated to_serialized_data path.

use int2dds::{
    common::builtin::topic::{
        participant_builtin_topic_data::ParticipantBuiltinTopicData,
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        topic_builtin_topic_data::TopicBuiltinTopicData,
    },
    core::time::Duration,
    infrastructure::qos_policy::{
        ReliabilityQosPolicy, ReliabilityQosPolicyKind, UserDataQosPolicy,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    rtps::common::{guid::Guid, locator::Locator},
    subscription::qos::{DataReaderQos, SubscriberQos},
    topic::{qos::TopicQos, type_support::DdsType},
};

// ---------- helpers ----------

/// PL_CDR_LE encapsulation prefix in big-endian-on-wire bytes.
const PL_CDR_LE_HEADER: [u8; 4] = [0x00, 0x03, 0x00, 0x00];

fn make_guid(seed: u8) -> Guid {
    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = seed.wrapping_add(i as u8);
    }
    Guid::from_bytes(bytes)
}

fn locator(port: u32, addr_seed: u8) -> Locator {
    let mut addr = [0u8; 16];
    for (i, b) in addr.iter_mut().enumerate() {
        *b = addr_seed.wrapping_add(i as u8);
    }
    Locator::new(1, port, addr)
}

/// Count how many times the little-endian PID (2 bytes) appears at a
/// parameter-list start position (i.e. 4-byte aligned, after the 4-byte
/// encapsulation header, before sentinel).
fn count_pid_occurrences(payload: &[u8], pid_le: u16) -> usize {
    assert!(payload.len() >= 4, "payload too short for encap header");
    let mut pos = 4usize;
    let mut count = 0usize;
    while pos + 4 <= payload.len() {
        let pid = u16::from_le_bytes([payload[pos], payload[pos + 1]]);
        let len = u16::from_le_bytes([payload[pos + 2], payload[pos + 3]]) as usize;
        // PID_SENTINEL = 0x0001 ends the list.
        if pid == 0x0001 {
            break;
        }
        if pid == pid_le {
            count += 1;
        }
        pos += 4 + len;
        // 4-byte alignment for next parameter (already aligned because len is u16
        // and PlCdrSerializer pads each parameter to 4-byte boundary, but be safe).
        pos = pos.div_ceil(4) * 4;
    }
    count
}

fn assert_starts_with_pl_cdr_le(payload: &[u8]) {
    assert!(payload.len() >= 4, "payload too short");
    assert_eq!(&payload[..4], &PL_CDR_LE_HEADER, "expected PL_CDR_LE encapsulation header");
}

// ---------- Publication ----------

fn sample_publication() -> PublicationBuiltinTopicData {
    let mut p = PublicationBuiltinTopicData::new(
        &DataWriterQos::default(),
        &PublisherQos::default(),
        &TopicQos::default(),
    );
    p.set_endpoint_guid(make_guid(0x11));
    p.set_topic_name("test/pub_topic".into());
    p.set_type_name("PubType".into());
    p.add_unicast_locator(locator(7400, 0x20));
    p.add_unicast_locator(locator(7401, 0x21));
    p
}

#[test]
fn publication_pl_cdr_roundtrip() {
    let original = sample_publication();
    let bytes = original.to_serialized_data().to_vec();
    assert_starts_with_pl_cdr_le(&bytes);

    let parsed = PublicationBuiltinTopicData::from_serialized_data(&bytes)
        .expect("PL_CDR parse should succeed");
    assert_eq!(parsed.endpoint_guid(), original.endpoint_guid());
    assert_eq!(parsed.topic_name(), original.topic_name());
    assert_eq!(parsed.type_name(), original.type_name());
    assert_eq!(parsed.unicast_locator_list(), original.unicast_locator_list());
}

#[test]
fn publication_dds_serialize_emits_pl_cdr() {
    let original = sample_publication();
    let dds_bytes = DdsType::serialize(&original).expect("serialize").to_vec();
    assert_starts_with_pl_cdr_le(&dds_bytes);

    // Bytes from DdsType::serialize must round-trip via the PL_CDR helper.
    let parsed = PublicationBuiltinTopicData::from_serialized_data(&dds_bytes)
        .expect("DdsType bytes are valid PL_CDR");
    assert_eq!(parsed.topic_name(), original.topic_name());
    assert_eq!(parsed.type_name(), original.type_name());

    // And they must be byte-identical to the PL_CDR helper output.
    let helper_bytes = original.to_serialized_data().to_vec();
    assert_eq!(
        dds_bytes, helper_bytes,
        "DdsType::serialize must produce the same bytes as to_serialized_data"
    );
}

#[test]
fn publication_pid_topic_name_present() {
    let p = sample_publication();
    let bytes = p.to_serialized_data().to_vec();
    // PidTopicName = 0x0005 must appear exactly once.
    assert_eq!(count_pid_occurrences(&bytes, 0x0005), 1);
    // PidEndpointGuid = 0x005a must appear exactly once.
    assert_eq!(count_pid_occurrences(&bytes, 0x005a), 1);
}

#[test]
fn publication_with_locators_emits_one_pid_per_locator() {
    let p = sample_publication();
    let bytes = p.to_serialized_data().to_vec();
    // PidUnicastLocator = 0x002f, one per locator added (2).
    assert_eq!(count_pid_occurrences(&bytes, 0x002f), 2);
}

// ---------- Subscription ----------

fn sample_subscription() -> SubscriptionBuiltinTopicData {
    let qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 0 },
        },
        ..Default::default()
    };
    let mut s =
        SubscriptionBuiltinTopicData::new(&qos, &SubscriberQos::default(), &TopicQos::default());
    s.set_endpoint_guid(make_guid(0x33));
    s.set_topic_name("test/sub_topic".into());
    s.set_type_name("SubType".into());
    s.add_multicast_locator(locator(7402, 0x30));
    s
}

#[test]
fn subscription_pl_cdr_roundtrip() {
    let original = sample_subscription();
    let bytes = original.to_serialized_data().to_vec();
    assert_starts_with_pl_cdr_le(&bytes);

    let parsed = SubscriptionBuiltinTopicData::from_serialized_data(&bytes)
        .expect("PL_CDR parse should succeed");
    assert_eq!(parsed.endpoint_guid(), original.endpoint_guid());
    assert_eq!(parsed.topic_name(), original.topic_name());
    assert_eq!(parsed.type_name(), original.type_name());
    assert_eq!(parsed.reliability().kind, original.reliability().kind);
    assert_eq!(parsed.multicast_locator_list(), original.multicast_locator_list());
}

#[test]
fn subscription_dds_serialize_emits_pl_cdr() {
    let original = sample_subscription();
    let dds_bytes = DdsType::serialize(&original).expect("serialize").to_vec();
    assert_starts_with_pl_cdr_le(&dds_bytes);

    let helper_bytes = original.to_serialized_data().to_vec();
    assert_eq!(
        dds_bytes, helper_bytes,
        "DdsType::serialize must produce the same bytes as to_serialized_data"
    );
}

#[test]
fn subscription_pid_topic_name_present() {
    let s = sample_subscription();
    let bytes = s.to_serialized_data().to_vec();
    assert_eq!(count_pid_occurrences(&bytes, 0x0005), 1);
    assert_eq!(count_pid_occurrences(&bytes, 0x005a), 1);
    // PidMulticastLocator = 0x0030
    assert_eq!(count_pid_occurrences(&bytes, 0x0030), 1);
}

// ---------- Participant ----------

fn sample_participant() -> ParticipantBuiltinTopicData {
    ParticipantBuiltinTopicData::new(
        make_guid(0x55),
        UserDataQosPolicy { value: vec![0xDE, 0xAD, 0xBE, 0xEF] },
    )
}

#[test]
fn participant_pl_cdr_roundtrip() {
    let original = sample_participant();
    let bytes = original.to_serialized_data().to_vec();
    assert_starts_with_pl_cdr_le(&bytes);

    let parsed = ParticipantBuiltinTopicData::from_serialized_data(&bytes)
        .expect("PL_CDR parse should succeed");
    assert_eq!(parsed.user_data().value, original.user_data().value);
}

// ---------- Topic ----------

fn sample_topic() -> TopicBuiltinTopicData {
    TopicBuiltinTopicData::new(
        make_guid(0x77),
        "test/topic".into(),
        "TopicType".into(),
        TopicQos::default(),
    )
}

#[test]
fn topic_pl_cdr_roundtrip() {
    let original = sample_topic();
    let bytes = original.to_serialized_data().to_vec();
    assert_starts_with_pl_cdr_le(&bytes);

    let parsed =
        TopicBuiltinTopicData::from_serialized_data(&bytes).expect("PL_CDR parse should succeed");
    assert_eq!(parsed.name(), original.name());
    assert_eq!(parsed.type_name(), original.type_name());
    assert_eq!(parsed.reliability().kind, original.reliability().kind);
    assert_eq!(parsed.durability().kind, original.durability().kind);
}

#[test]
fn topic_pid_name_present() {
    let t = sample_topic();
    let bytes = t.to_serialized_data().to_vec();
    // PidTopicName = 0x0005, PidTypeName = 0x0007
    assert_eq!(count_pid_occurrences(&bytes, 0x0005), 1);
    assert_eq!(count_pid_occurrences(&bytes, 0x0007), 1);
    // PidHistory = 0x0040, PidResourceLimits = 0x0041 — Topic-specific.
    assert_eq!(count_pid_occurrences(&bytes, 0x0040), 1);
    assert_eq!(count_pid_occurrences(&bytes, 0x0041), 1);
}
