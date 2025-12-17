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
        Self {
            key: BuiltinTopicKey {
                value: [
                    participant_guid.prefix()[0] as i32,
                    participant_guid.prefix()[1] as i32,
                    participant_guid.prefix()[2] as i32,
                ],
            },
            user_data: user_data,
        }
    }

    pub fn key(&self) -> BuiltinTopicKey {
        self.key.clone()
    }

    pub fn user_data(&self) -> UserDataQosPolicy {
        self.user_data.clone()
    }
}
