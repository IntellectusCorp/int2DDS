//! Quality of Service (QoS) policies for domain participants.
//!
//! This module defines the QoS policies that control the behavior of `DomainParticipant`
//! and `DomainParticipantFactory` entities.
//!
//! # Available QoS Policies
//!
//! - **UserData**: Application-specific data attached to the participant
//! - **EntityFactory**: Controls whether created entities are automatically enabled
//!
//! The default QoS can be accessed via `PARTICIPANT_QOS_DEFAULT` or `DomainParticipantQos::default()`.

use const_default::ConstDefault;

use crate::{
    core::error::{DdsError, DdsResult},
    infrastructure::qos_policy::{
        EntityFactoryQosPolicy, PropertyQosPolicy, Qos, UserDataQosPolicy,
    },
};

pub const PARTICIPANT_QOS_DEFAULT: DomainParticipantQos = DomainParticipantQos::DEFAULT;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DomainParticipantQos {
    pub user_data: UserDataQosPolicy,
    pub entity_factory: EntityFactoryQosPolicy,
    pub property: PropertyQosPolicy,
}

impl ConstDefault for DomainParticipantQos {
    const DEFAULT: Self = Self {
        user_data: UserDataQosPolicy::DEFAULT,
        entity_factory: EntityFactoryQosPolicy::DEFAULT,
        property: PropertyQosPolicy::DEFAULT,
    };
}

impl Qos for DomainParticipantQos {
    fn check_unsupported_policies(&self) -> DdsResult<()> {
        if self.user_data != UserDataQosPolicy::default() {
            return Err(DdsError::Unsupported);
        }

        Ok(())
    }
}

impl DomainParticipantQos {}

#[derive(Debug, Default, ConstDefault, Clone, PartialEq, Eq)]
pub struct DomainParticipantFactoryQos {
    pub entity_factory: EntityFactoryQosPolicy,
}

impl Qos for DomainParticipantFactoryQos {
    fn autoenable_created_entities(&self) -> bool {
        self.entity_factory.autoenable_created_entities
    }
}

impl DomainParticipantFactoryQos {}
