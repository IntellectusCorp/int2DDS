//! Quality of Service (QoS) policies for publishers and data writers.
//!
//! This module defines the QoS policies that control the behavior of `Publisher` and
//! `DataWriter` entities.
//!
//! # Publisher QoS Policies
//!
//! Publishers support policies like Partition, Presentation, GroupData, and EntityFactory.
//!
//! # DataWriter QoS Policies
//!
//! DataWriters support a comprehensive set of policies including:
//! - **Reliability**: RELIABLE or BEST_EFFORT delivery
//! - **Durability**: Defines the persistence of historical data for late-joining subscribers
//! - **History**: Number of samples to keep
//! - **Deadline**: Maximum allowed time between data writes
//! - **Liveliness**: Mechanism for asserting the writer's activity
//! - **Ownership**: Shared or exclusive ownership of instances
//! - And many more...
//!
//! Default QoS can be accessed via `PUBLISHER_QOS_DEFAULT` and `DATAWRITER_QOS_DEFAULT`.

use crate::{
    core::{
        error::{DdsError, DdsResult},
        time::Duration,
        types::LENGTH_UNLIMITED,
    },
    infrastructure::qos_policy::{
        DataRepresentationQosPolicy, DeadlineQosPolicy, DestinationOrderQosPolicy,
        DurabilityQosPolicy, DurabilityServiceQosPolicy, EntityFactoryQosPolicy,
        GroupDataQosPolicy, HistoryQosPolicy, LatencyBudgetQosPolicy, LifespanQosPolicy,
        LivelinessQosPolicy, OwnershipQosPolicy, OwnershipStrengthQosPolicy, PartitionQosPolicy,
        PresentationQosPolicy, Qos, ReliabilityExtensionQosPolicy, ReliabilityQosPolicy,
        ReliabilityQosPolicyKind, ResourceLimitsQosPolicy, TransportPriorityQosPolicy,
        UserDataQosPolicy, WriterDataLifecycleQosPolicy,
    },
};
use const_default::ConstDefault;

pub const PUBLISHER_QOS_DEFAULT: PublisherQos = PublisherQos::DEFAULT;
pub const DATAWRITER_QOS_DEFAULT: DataWriterQos = DataWriterQos::DEFAULT;
// todo # define DATAWRITER_QOS_USE_TOPIC_QOS

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataWriterQos {
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
    pub user_data: UserDataQosPolicy,
    pub ownership: OwnershipQosPolicy,
    pub ownership_strength: OwnershipStrengthQosPolicy,
    pub writer_data_lifecycle: WriterDataLifecycleQosPolicy,
    pub data_representation: DataRepresentationQosPolicy,
    pub reliability_extension: ReliabilityExtensionQosPolicy,
}

impl Default for DataWriterQos {
    fn default() -> Self {
        Self {
            durability: DurabilityQosPolicy::default(),
            durability_service: DurabilityServiceQosPolicy::default(),
            deadline: DeadlineQosPolicy::default(),
            latency_budget: LatencyBudgetQosPolicy::default(),
            liveliness: LivelinessQosPolicy::default(),
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            destination_order: DestinationOrderQosPolicy::default(),
            history: HistoryQosPolicy::default(),
            resource_limits: ResourceLimitsQosPolicy::default(),
            transport_priority: TransportPriorityQosPolicy::default(),
            lifespan: LifespanQosPolicy::default(),
            user_data: UserDataQosPolicy::default(),
            ownership: OwnershipQosPolicy::default(),
            ownership_strength: OwnershipStrengthQosPolicy::default(),
            writer_data_lifecycle: WriterDataLifecycleQosPolicy::default(),
            data_representation: DataRepresentationQosPolicy::default(),
            reliability_extension: ReliabilityExtensionQosPolicy::default(),
        }
    }
}

impl ConstDefault for DataWriterQos {
    const DEFAULT: Self = Self {
        durability: DurabilityQosPolicy::DEFAULT,
        durability_service: DurabilityServiceQosPolicy::DEFAULT,
        deadline: DeadlineQosPolicy::DEFAULT,
        latency_budget: LatencyBudgetQosPolicy::DEFAULT,
        liveliness: LivelinessQosPolicy::DEFAULT,
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        destination_order: DestinationOrderQosPolicy::DEFAULT,
        history: HistoryQosPolicy::DEFAULT,
        resource_limits: ResourceLimitsQosPolicy::DEFAULT,
        transport_priority: TransportPriorityQosPolicy::DEFAULT,
        lifespan: LifespanQosPolicy::DEFAULT,
        user_data: UserDataQosPolicy::DEFAULT,
        ownership: OwnershipQosPolicy::DEFAULT,
        ownership_strength: OwnershipStrengthQosPolicy::DEFAULT,
        writer_data_lifecycle: WriterDataLifecycleQosPolicy::DEFAULT,
        data_representation: DataRepresentationQosPolicy::DEFAULT,
        reliability_extension: ReliabilityExtensionQosPolicy::DEFAULT,
    };
}

impl Qos for DataWriterQos {
    fn check_unsupported_policies(&self) -> DdsResult<()> {
        if self.user_data != UserDataQosPolicy::default()
            // || self.durability.kind == DurabilityQosPolicyKind::Transient
            // || self.durability.kind == DurabilityQosPolicyKind::Persistent
            // || self.durability_service != DurabilityServiceQosPolicy::default()
            || self.latency_budget != LatencyBudgetQosPolicy::default()
            // || self.liveliness != LivelinessQosPolicy::default()
            || self.transport_priority != TransportPriorityQosPolicy::default()
        // || self.lifespan != LifespanQosPolicy::default()
        // || self.ownership != OwnershipQosPolicy::default()
        // || self.ownership_strength != OwnershipStrengthQosPolicy::default()
        // || self.writer_data_lifecycle != WriterDataLifecycleQosPolicy::default()
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

impl DataWriterQos {}

#[derive(Debug, Default, ConstDefault, Clone, PartialEq, Eq)]
pub struct PublisherQos {
    pub presentation: PresentationQosPolicy,
    pub partition: PartitionQosPolicy,
    pub group_data: GroupDataQosPolicy,
    pub entity_factory: EntityFactoryQosPolicy,
}

impl Qos for PublisherQos {
    fn check_unsupported_policies(&self) -> DdsResult<()> {
        if self.presentation != PresentationQosPolicy::default()
            // || self.partition != PartitionQosPolicy::default()
            || self.group_data != GroupDataQosPolicy::default()
        {
            return Err(DdsError::Unsupported);
        }

        Ok(())
    }

    fn check_immutable_change(&self, new_qos: &Self) -> DdsResult<()> {
        if self.presentation != new_qos.presentation {
            return Err(DdsError::ImmutablePolicy);
        }

        Ok(())
    }

    fn autoenable_created_entities(&self) -> bool {
        self.entity_factory.autoenable_created_entities
    }
}

impl PublisherQos {}
