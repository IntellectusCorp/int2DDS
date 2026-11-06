//! Quality of Service (QoS) policies for domain participants.
//!
//! This module defines the QoS policies that control the behavior of `DomainParticipant`
//! and `DomainParticipantFactory` entities.
//!
//! # Available QoS Policies
//!
//! - **UserData**: Application-specific data attached to the participant
//! - **EntityFactory**: Controls whether created entities are automatically enabled
//! - **Property**: Name-value pairs for implementation settings such as transport selection
//!
//! `PARTICIPANT_QOS_DEFAULT` selects the configured default QoS; `DomainParticipantQos::default()`
//! is the spec default value.

use const_default::ConstDefault;

use crate::infrastructure::{
    qos_kind::QosKind,
    qos_policy::{EntityFactoryQosPolicy, PropertyQosPolicy, Qos, UserDataQosPolicy},
};

pub const PARTICIPANT_QOS_DEFAULT: QosKind<DomainParticipantQos> = QosKind::Default;

#[derive(Debug, Default, ConstDefault, Clone, PartialEq, Eq)]
pub struct DomainParticipantQos {
    pub user_data: UserDataQosPolicy,
    pub entity_factory: EntityFactoryQosPolicy,
    pub property: PropertyQosPolicy,
}

impl Qos for DomainParticipantQos {
    fn autoenable_created_entities(&self) -> bool {
        self.entity_factory.autoenable_created_entities
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
