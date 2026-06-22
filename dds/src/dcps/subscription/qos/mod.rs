//! Quality of Service (QoS) policies for subscribers and data readers.
//!
//! This module defines the QoS policies that control the behavior of `Subscriber` and
//! `DataReader` entities.
//!
//! # Subscriber QoS Policies
//!
//! Subscribers support policies like Partition, Presentation, GroupData, and EntityFactory.
//!
//! # DataReader QoS Policies
//!
//! DataReaders support a comprehensive set of policies including:
//! - **Reliability**: RELIABLE or BEST_EFFORT delivery expectations
//! - **Durability**: Minimum durability level required from matched writers (thus how much historical data can be retained and delivered to this reader)
//! - **History**: Number of samples to keep
//! - **Deadline**: Maximum time between received samples
//! - **Liveliness**: Writer activity monitoring
//! - **TypeConsistencyEnforcement**: Type consistency enforcement for DDS-XTypes
//! - **ReaderReliabilityExtension**: int2DDS extension for reader reliability options (heartbeat response, preemptive ACKNACK)
//! - And many more...
//!
//! Default QoS can be accessed via `SUBSCRIBER_QOS_DEFAULT` and `DATAREADER_QOS_DEFAULT`.

use crate::{
    core::{
        error::{DdsError, DdsResult},
        time::Duration,
        types::LENGTH_UNLIMITED,
    },
    infrastructure::qos_kind::QosKind,
    infrastructure::qos_policy::{
        DataRepresentationQosPolicy, DeadlineQosPolicy, DestinationOrderQosPolicy,
        DurabilityQosPolicy, EntityFactoryQosPolicy, GroupDataQosPolicy, HistoryQosPolicy,
        LatencyBudgetQosPolicy, LivelinessQosPolicy, OwnershipQosPolicy, PartitionQosPolicy,
        PresentationQosPolicy, Qos, ReaderDataLifecycleQosPolicy,
        ReaderReliabilityExtensionQosPolicy, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        ResourceLimitsQosPolicy, TimeBasedFilterQosPolicy, TypeConsistencyEnforcementQosPolicy,
        UserDataQosPolicy,
    },
};
use const_default::ConstDefault;

pub const SUBSCRIBER_QOS_DEFAULT: QosKind<SubscriberQos> = QosKind::Default;
pub const DATAREADER_QOS_DEFAULT: QosKind<DataReaderQos> = QosKind::Default;
// todo # define DATAREADER_QOS_USE_TOPIC_QOS

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataReaderQos {
    pub durability: DurabilityQosPolicy,
    pub deadline: DeadlineQosPolicy,
    pub latency_budget: LatencyBudgetQosPolicy,
    pub liveliness: LivelinessQosPolicy,
    pub reliability: ReliabilityQosPolicy,
    pub destination_order: DestinationOrderQosPolicy,
    pub history: HistoryQosPolicy,
    pub resource_limits: ResourceLimitsQosPolicy,
    pub user_data: UserDataQosPolicy,
    pub ownership: OwnershipQosPolicy,
    pub time_based_filter: TimeBasedFilterQosPolicy,
    pub reader_data_lifecycle: ReaderDataLifecycleQosPolicy,
    pub data_representation: DataRepresentationQosPolicy,
    pub type_consistency_enforcement: TypeConsistencyEnforcementQosPolicy,
    pub reader_reliability_extension: ReaderReliabilityExtensionQosPolicy,
}

impl Default for DataReaderQos {
    fn default() -> Self {
        Self {
            durability: DurabilityQosPolicy::default(),
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
            user_data: UserDataQosPolicy::default(),
            ownership: OwnershipQosPolicy::default(),
            time_based_filter: TimeBasedFilterQosPolicy::default(),
            reader_data_lifecycle: ReaderDataLifecycleQosPolicy::default(),
            data_representation: DataRepresentationQosPolicy::default(),
            type_consistency_enforcement: TypeConsistencyEnforcementQosPolicy::default(),
            reader_reliability_extension: ReaderReliabilityExtensionQosPolicy::default(),
        }
    }
}

impl ConstDefault for DataReaderQos {
    const DEFAULT: Self = Self {
        durability: DurabilityQosPolicy::DEFAULT,
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
        user_data: UserDataQosPolicy::DEFAULT,
        ownership: OwnershipQosPolicy::DEFAULT,
        time_based_filter: TimeBasedFilterQosPolicy::DEFAULT,
        reader_data_lifecycle: ReaderDataLifecycleQosPolicy::DEFAULT,
        data_representation: DataRepresentationQosPolicy::DEFAULT,
        type_consistency_enforcement: TypeConsistencyEnforcementQosPolicy::DEFAULT,
        reader_reliability_extension: ReaderReliabilityExtensionQosPolicy::DEFAULT,
    };
}

impl Qos for DataReaderQos {
    fn check_unsupported_policies(&self) -> DdsResult<()> {
        if self.user_data != UserDataQosPolicy::default()
            // || self.durability.kind == DurabilityQosPolicyKind::Transient
            // || self.durability.kind == DurabilityQosPolicyKind::Persistent
            || self.latency_budget != LatencyBudgetQosPolicy::default()
        // || self.liveliness != LivelinessQosPolicy::default()
        // || self.ownership != OwnershipQosPolicy::default()
        // || self.reader_data_lifecycle != ReaderDataLifecycleQosPolicy::default()
        {
            return Err(DdsError::Unsupported);
        }

        // ReaderReliabilityExtensionQosPolicy unsupported fields check
        let ext_default = ReaderReliabilityExtensionQosPolicy::DEFAULT;
        if self.reader_reliability_extension.heartbeat_suppression_duration
            != ext_default.heartbeat_suppression_duration
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
            || self.destination_order != new_qos.destination_order
            || self.data_representation != new_qos.data_representation
            || self.reader_reliability_extension != new_qos.reader_reliability_extension
        {
            return Err(DdsError::ImmutablePolicy);
        }

        Ok(())
    }

    fn is_consistent(&self) -> DdsResult<()> {
        if self.resource_limits.max_samples_per_instance != LENGTH_UNLIMITED
            && self.history.depth() > Some(self.resource_limits.max_samples_per_instance)
            || self.resource_limits.max_samples < self.resource_limits.max_samples_per_instance
            || self.time_based_filter.minimum_separation > self.deadline.period
        {
            return Err(DdsError::InconsistentPolicy);
        }

        Ok(())
    }
}

impl DataReaderQos {}

#[derive(Debug, Default, ConstDefault, Clone, PartialEq, Eq)]
pub struct SubscriberQos {
    pub presentation: PresentationQosPolicy,
    pub partition: PartitionQosPolicy,
    pub group_data: GroupDataQosPolicy,
    pub entity_factory: EntityFactoryQosPolicy,
}

impl Qos for SubscriberQos {
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

impl SubscriberQos {}
