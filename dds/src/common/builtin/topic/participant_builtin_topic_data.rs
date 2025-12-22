//! Participant builtin topic data for discovery.
//!
//! This module defines the `ParticipantBuiltinTopicData` structure that represents
//! discovered domain participant information in the DDS discovery protocol.

use crate::{infrastructure::qos_policy::UserDataQosPolicy, rtps::common::guid::Guid};

use super::builtin_topic_key::BuiltinTopicKey;

pub struct ParticipantBuiltinTopicData {
    key: BuiltinTopicKey,
    user_data: UserDataQosPolicy,
}

impl ParticipantBuiltinTopicData {
    pub(crate) fn new(participant_guid: Guid, user_data: UserDataQosPolicy) -> Self {
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
}
