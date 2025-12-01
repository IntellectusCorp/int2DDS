//! Topic builtin topic data for discovery.
//!
//! This module defines the `TopicBuiltinTopicData` structure that represents
//! discovered Topic information in the DDS discovery protocol.

use crate::{
    infrastructure::qos_policy::{
        DeadlineQosPolicy, DestinationOrderQosPolicy, DurabilityQosPolicy,
        DurabilityServiceQosPolicy, HistoryQosPolicy, LatencyBudgetQosPolicy, LifespanQosPolicy,
        LivelinessQosPolicy, OwnershipQosPolicy, ReliabilityQosPolicy, ResourceLimitsQosPolicy,
        TopicDataQosPolicy, TransportPriorityQosPolicy,
    },
    rtps::common::guid::Guid,
    topic::qos::TopicQos,
};

use super::builtin_topic_key::BuiltinTopicKey;

#[derive(Debug)]
pub struct TopicBuiltinTopicData {
    key: BuiltinTopicKey,
    name: String,
    type_name: String,
    durability: DurabilityQosPolicy,
    durability_service: DurabilityServiceQosPolicy,
    deadline: DeadlineQosPolicy,
    latency_budget: LatencyBudgetQosPolicy,
    liveliness: LivelinessQosPolicy,
    reliability: ReliabilityQosPolicy,
    transport_priority: TransportPriorityQosPolicy,
    lifespan: LifespanQosPolicy,
    destination_order: DestinationOrderQosPolicy,
    history: HistoryQosPolicy,
    resource_limits: ResourceLimitsQosPolicy,
    ownership: OwnershipQosPolicy,
    topic_data: TopicDataQosPolicy,
}

impl TopicBuiltinTopicData {
    #[allow(dead_code)]
    pub(crate) fn new(topic_guid: Guid, name: String, type_name: String, qos: TopicQos) -> Self {
        Self {
            key: BuiltinTopicKey {
                value: [
                    topic_guid.prefix()[0] as i32,
                    topic_guid.prefix()[1] as i32,
                    topic_guid.prefix()[2] as i32,
                ],
            },
            name,
            type_name,
            durability: qos.durability,
            durability_service: qos.durability_service,
            deadline: qos.deadline,
            latency_budget: qos.latency_budget,
            liveliness: qos.liveliness,
            reliability: qos.reliability,
            transport_priority: qos.transport_priority,
            lifespan: qos.lifespan,
            destination_order: qos.destination_order,
            history: qos.history,
            resource_limits: qos.resource_limits,
            ownership: qos.ownership,
            topic_data: qos.topic_data,
        }
    }
    pub fn key(&self) -> BuiltinTopicKey {
        self.key.clone()
    }
    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn type_name(&self) -> String {
        self.type_name.clone()
    }
    pub fn durability(&self) -> DurabilityQosPolicy {
        self.durability
    }
    pub fn durability_service(&self) -> DurabilityServiceQosPolicy {
        self.durability_service
    }
    pub fn deadline(&self) -> DeadlineQosPolicy {
        self.deadline
    }
    pub fn latency_budget(&self) -> LatencyBudgetQosPolicy {
        self.latency_budget
    }
    pub fn liveliness(&self) -> LivelinessQosPolicy {
        self.liveliness
    }
    pub fn reliability(&self) -> ReliabilityQosPolicy {
        self.reliability
    }
    pub fn transport_priority(&self) -> TransportPriorityQosPolicy {
        self.transport_priority
    }
    pub fn lifespan(&self) -> LifespanQosPolicy {
        self.lifespan
    }
    pub fn destination_order(&self) -> DestinationOrderQosPolicy {
        self.destination_order
    }
    pub fn history(&self) -> HistoryQosPolicy {
        self.history
    }
    pub fn resource_limits(&self) -> ResourceLimitsQosPolicy {
        self.resource_limits
    }
    pub fn ownership(&self) -> OwnershipQosPolicy {
        self.ownership
    }
    pub fn topic_data(&self) -> TopicDataQosPolicy {
        self.topic_data.clone()
    }
}
