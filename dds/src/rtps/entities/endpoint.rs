//! Endpoint trait for RTPS readers and writers.
//!
//! This module defines the `Endpoint` trait for RTPS DataReaders and DataWriters,
//! which are the fundamental communication endpoints in the RTPS protocol. Endpoints
//! have topic kinds, reliability levels, and locator lists.

#![allow(dead_code)]
#![allow(unused_variables)]

use crate::{
    infrastructure::qos_policy::ReliabilityQosPolicyKind,
    rtps::{
        common::{entity_id::EntityId, locator::Locator, types::TopicKind},
        entities::entity::Entity,
    },
};

/*
pub struct Endpoint {
    topic_kind: TopicKind,
    reliability_level: ReliabilityKind,
    unicast_locator_list: LocatorList,
    multicast_locator_list: LocatorList,
    endpoint_id: EntityId,
}

impl Entity for Endpoint {
    fn guid(&self) -> Guid {}
}
*/

pub(crate) trait Endpoint: Entity {
    fn topic_kind(&self) -> TopicKind;
    fn reliability_level(&self) -> ReliabilityQosPolicyKind;
    fn unicast_locator_list(&self) -> Vec<Locator>;
    fn multicast_locator_list(&self) -> Vec<Locator>;
    fn endpoint_id(&self) -> EntityId;
}
