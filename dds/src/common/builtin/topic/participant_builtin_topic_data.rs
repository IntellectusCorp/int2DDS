//! Participant builtin topic data for discovery.
//!
//! This module defines the `ParticipantBuiltinTopicData` structure that represents
//! discovered domain participant information in the DDS discovery protocol.

use crate::{
    infrastructure::qos_policy::UserDataQosPolicy,
    rtps::common::{guid::Guid, types::SerializedData},
    topic::type_support::DdsType,
};

use super::builtin_topic_key::BuiltinTopicKey;
use crate::serialize::DeserializerReader;

#[derive(DdsType)]
#[dds_type(crate_path = "crate", no_default, extensibility = "Mutable")]
pub struct ParticipantBuiltinTopicData {
    #[dds(non_serialized)]
    key: BuiltinTopicKey,
    #[dds(id = 0x002c)] // PidUserData
    user_data: UserDataQosPolicy,
}

impl ParticipantBuiltinTopicData {
    pub fn new(participant_guid: Guid, user_data: UserDataQosPolicy) -> Self {
        let prefix = participant_guid.prefix();
        Self {
            key: BuiltinTopicKey {
                value: [
                    i32::from_be_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]),
                    i32::from_be_bytes([prefix[4], prefix[5], prefix[6], prefix[7]]),
                    i32::from_be_bytes([prefix[8], prefix[9], prefix[10], prefix[11]]),
                ],
            },
            user_data,
        }
    }

    pub fn key(&self) -> BuiltinTopicKey {
        self.key.clone()
    }

    pub fn user_data(&self) -> UserDataQosPolicy {
        self.user_data.clone()
    }

    pub fn from_serialized_data(data: &[u8]) -> Result<Self, String> {
        use crate::serialize::pl_cdr::ParsedBuiltinTopicData;

        let parsed = ParsedBuiltinTopicData::from_serialized_data(data)?;
        let mut participant_data =
            Self { key: BuiltinTopicKey::default(), user_data: UserDataQosPolicy::default() };
        if let Some(key) = parsed.participant_key {
            participant_data.key = key;
        }
        if let Some(user_data) = parsed.user_data {
            participant_data.user_data = user_data;
        }
        Ok(participant_data)
    }

    pub fn to_serialized_data(&self) -> SerializedData {
        use crate::serialize::pl_cdr::ParsedBuiltinTopicData;

        let parsed = ParsedBuiltinTopicData::from_participant_topic_data(self);
        parsed.to_serialized_data()
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use crate::{
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
        xtypes::{
            EquivalenceHash, MinimalTypeObject, TypeIdentifier, TypeIdentifierWithDependencies,
            TypeIdentifierWithSize, TypeInformation, TypeObject,
        },
    };
    const PL_CDR_LE_HEADER: [u8; 4] = [0x00, 0x03, 0x00, 0x00];
    fn make_guid(seed: u8) -> Guid {
        let mut bytes = [0u8; 16];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        Guid::from_bytes(bytes)
    }
    fn assert_starts_with_pl_cdr_le(payload: &[u8]) {
        assert!(payload.len() >= 4, "payload too short");
        assert_eq!(&payload[..4], &PL_CDR_LE_HEADER, "expected PL_CDR_LE encapsulation header");
    }
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
}
