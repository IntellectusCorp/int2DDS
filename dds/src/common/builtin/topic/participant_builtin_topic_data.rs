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
