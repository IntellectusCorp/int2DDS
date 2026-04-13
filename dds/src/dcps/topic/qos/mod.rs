//! Quality of Service (QoS) policies for topics.
//!
//! This module defines the QoS policies that control the behavior of `Topic` entities.
//! Topic QoS policies define characteristics that are inherited by DataWriters and
//! DataReaders created for the topic (unless overridden).
//!
//! # Available QoS Policies
//!
//! Topics support a wide range of QoS policies including:
//! - **TopicData**: Application-specific data attached to the topic
//! - **Durability**: Whether late-joining readers receive historical data
//! - **DurabilityService**: Configuration for durability service
//! - **Deadline**: Maximum time between data updates
//! - **LatencyBudget**: Hint for transport latency optimization
//! - **Liveliness**: How writers assert they are alive
//! - **Reliability**: RELIABLE or BEST_EFFORT delivery
//! - **DestinationOrder**: Ordering of samples (by reception or source timestamp)
//! - **History**: Number of samples to keep (KEEP_LAST vs KEEP_ALL)
//! - **ResourceLimits**: Memory usage limits
//! - **TransportPriority**: Priority hint for the transport layer
//! - **Lifespan**: Maximum validity time for samples
//! - **Ownership**: Shared or exclusive ownership mode
//!
//! Default QoS can be accessed via `TOPIC_QOS_DEFAULT` or `TopicQos::default()`.

use const_default::ConstDefault;

use crate::{
    core::{
        error::{DdsError, DdsResult},
        time::Duration,
        types::LENGTH_UNLIMITED,
    },
    infrastructure::qos_kind::QosKind,
    infrastructure::qos_policy::{
        DataRepresentationQosPolicy, DeadlineQosPolicy, DestinationOrderQosPolicy,
        DurabilityQosPolicy, DurabilityServiceQosPolicy, HistoryQosPolicy, LatencyBudgetQosPolicy,
        LifespanQosPolicy, LivelinessQosPolicy, OwnershipQosPolicy, Qos, ReliabilityQosPolicy,
        ReliabilityQosPolicyKind, ResourceLimitsQosPolicy, TopicDataQosPolicy,
        TransportPriorityQosPolicy,
    },
};

pub const TOPIC_QOS_DEFAULT: QosKind<TopicQos> = QosKind::Default;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicQos {
    pub topic_data: TopicDataQosPolicy,
    pub durability: DurabilityQosPolicy,
    pub durability_service: DurabilityServiceQosPolicy,
    pub deadline: DeadlineQosPolicy,
    pub latency_budget: LatencyBudgetQosPolicy,
    pub liveliness: LivelinessQosPolicy,
    pub reliability: ReliabilityQosPolicy,
    pub destination_order: DestinationOrderQosPolicy,
    pub history: HistoryQosPolicy,
    pub resource_limits: ResourceLimitsQosPolicy,
    pub transport_priority: TransportPriorityQosPolicy,
    pub lifespan: LifespanQosPolicy,
    pub ownership: OwnershipQosPolicy,
    pub data_representation: DataRepresentationQosPolicy,
}

impl Default for TopicQos {
    fn default() -> Self {
        Self {
            topic_data: TopicDataQosPolicy::default(),
            durability: DurabilityQosPolicy::default(),
            durability_service: DurabilityServiceQosPolicy::default(),
            deadline: DeadlineQosPolicy::default(),
            latency_budget: LatencyBudgetQosPolicy::default(),
            liveliness: LivelinessQosPolicy::default(),
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::BestEffort,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            destination_order: DestinationOrderQosPolicy::default(),
            history: HistoryQosPolicy::default(),
            resource_limits: ResourceLimitsQosPolicy::default(),
            transport_priority: TransportPriorityQosPolicy::default(),
            lifespan: LifespanQosPolicy::default(),
            ownership: OwnershipQosPolicy::default(),
            data_representation: DataRepresentationQosPolicy::default(),
        }
    }
}

impl ConstDefault for TopicQos {
    const DEFAULT: Self = Self {
        topic_data: TopicDataQosPolicy::DEFAULT,
        durability: DurabilityQosPolicy::DEFAULT,
        durability_service: DurabilityServiceQosPolicy::DEFAULT,
        deadline: DeadlineQosPolicy::DEFAULT,
        latency_budget: LatencyBudgetQosPolicy::DEFAULT,
        liveliness: LivelinessQosPolicy::DEFAULT,
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::BestEffort,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        destination_order: DestinationOrderQosPolicy::DEFAULT,
        history: HistoryQosPolicy::DEFAULT,
        resource_limits: ResourceLimitsQosPolicy::DEFAULT,
        transport_priority: TransportPriorityQosPolicy::DEFAULT,
        lifespan: LifespanQosPolicy::DEFAULT,
        ownership: OwnershipQosPolicy::DEFAULT,
        data_representation: DataRepresentationQosPolicy::DEFAULT,
    };
}

impl Qos for TopicQos {
    fn check_unsupported_policies(&self) -> DdsResult<()> {
        if self.topic_data != TopicDataQosPolicy::default()
            || self.durability_service != DurabilityServiceQosPolicy::default()
            || self.latency_budget != LatencyBudgetQosPolicy::default()
            || self.transport_priority != TransportPriorityQosPolicy::default()
        {
            return Err(DdsError::Unsupported);
        }

        Ok(())
    }

    fn check_immutable_change(&self, new_qos: &Self) -> DdsResult<()> {
        if self.durability != new_qos.durability
            || self.ownership != new_qos.ownership
            || self.reliability != new_qos.reliability
            || self.liveliness != new_qos.liveliness
            || self.history != new_qos.history
            || self.resource_limits != new_qos.resource_limits
            || self.durability_service != new_qos.durability_service
            || self.destination_order != new_qos.destination_order
            || self.data_representation != new_qos.data_representation
        {
            return Err(DdsError::ImmutablePolicy);
        }

        Ok(())
    }

    fn is_consistent(&self) -> DdsResult<()> {
        if self.resource_limits.max_samples_per_instance != LENGTH_UNLIMITED
            && self.history.depth() > Some(self.resource_limits.max_samples_per_instance)
            || self.resource_limits.max_samples < self.resource_limits.max_samples_per_instance
        {
            return Err(DdsError::InconsistentPolicy);
        }

        Ok(())
    }
}

impl TopicQos {}
