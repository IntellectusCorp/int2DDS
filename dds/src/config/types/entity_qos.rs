use serde::{Deserialize, Serialize};

use crate::{
    config::types::qos_policy::{
        DataFragQosPolicy, DataRepresentationQosPolicy, DestinationOrderQosPolicy,
        DurabilityQosPolicy, DurabilityServiceQosPolicy, GroupDataQosPolicy, HistoryQosPolicy,
        LivelinessQosPolicy, OwnershipQosPolicy, PartitionQosPolicy, PresentationQosPolicy,
        PropertyQosPolicy, ReaderReliabilityExtensionQosPolicy, ReliabilityQosPolicy,
        TopicDataQosPolicy, TypeConsistencyEnforcementQosPolicy, UserDataQosPolicy,
        WriterReliabilityExtensionQosPolicy, DEFAULT_MAX_BLOCKING_TIME,
    },
    domain,
    infrastructure::qos_policy as internal_qos_policy,
    infrastructure::qos_policy::{
        DeadlineQosPolicy, EntityFactoryQosPolicy, LatencyBudgetQosPolicy, LifespanQosPolicy,
        OwnershipStrengthQosPolicy, ReaderDataLifecycleQosPolicy, ResourceLimitsQosPolicy,
        TimeBasedFilterQosPolicy, TransportPriorityQosPolicy, WriterDataLifecycleQosPolicy,
    },
    publication, subscription, topic,
};

pub(crate) trait MergeQos {
    fn merge(&self, base: &Self) -> Self;
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct DataWriterQos {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) durability: Option<DurabilityQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) durability_service: Option<DurabilityServiceQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deadline: Option<DeadlineQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) latency_budget: Option<LatencyBudgetQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) liveliness: Option<LivelinessQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reliability: Option<ReliabilityQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) destination_order: Option<DestinationOrderQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) history: Option<HistoryQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) resource_limits: Option<ResourceLimitsQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) transport_priority: Option<TransportPriorityQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lifespan: Option<LifespanQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) user_data: Option<UserDataQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ownership: Option<OwnershipQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ownership_strength: Option<OwnershipStrengthQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) writer_data_lifecycle: Option<WriterDataLifecycleQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data_representation: Option<DataRepresentationQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) writer_reliability_extension: Option<WriterReliabilityExtensionQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data_frag: Option<DataFragQosPolicy>,
}

impl MergeQos for DataWriterQos {
    fn merge(&self, base: &Self) -> Self {
        Self {
            base_name: self.base_name.clone(),
            durability: self.durability.clone().or(base.durability.clone()),
            durability_service: self.durability_service.clone().or(base.durability_service.clone()),
            deadline: self.deadline.or(base.deadline),
            latency_budget: self.latency_budget.or(base.latency_budget),
            liveliness: self.liveliness.clone().or(base.liveliness.clone()),
            reliability: self.reliability.clone().or(base.reliability.clone()),
            destination_order: self.destination_order.clone().or(base.destination_order.clone()),
            history: self.history.clone().or(base.history.clone()),
            resource_limits: self.resource_limits.or(base.resource_limits),
            transport_priority: self.transport_priority.or(base.transport_priority),
            lifespan: self.lifespan.or(base.lifespan),
            user_data: self.user_data.clone().or(base.user_data.clone()),
            ownership: self.ownership.clone().or(base.ownership.clone()),
            ownership_strength: self.ownership_strength.or(base.ownership_strength),
            writer_data_lifecycle: self.writer_data_lifecycle.or(base.writer_data_lifecycle),
            data_representation: self
                .data_representation
                .clone()
                .or(base.data_representation.clone()),
            writer_reliability_extension: self
                .writer_reliability_extension
                .clone()
                .or(base.writer_reliability_extension.clone()),
            data_frag: self.data_frag.clone().or(base.data_frag.clone()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DataWriterQosNamed {
    pub(crate) name: String, // essential

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic_filter: Option<String>,

    #[serde(flatten)]
    pub(crate) qos: DataWriterQos,
}

pub(crate) type DataWriterQosSeq = Vec<DataWriterQosNamed>;

impl From<DataWriterQos> for publication::qos::DataWriterQos {
    fn from(external: DataWriterQos) -> Self {
        let mut qos = Self::default();

        if let Some(durability) = external.durability {
            qos.durability = durability.into();
        }

        if let Some(durability_service) = external.durability_service {
            qos.durability_service = durability_service.into();
        }

        if let Some(deadline) = external.deadline {
            qos.deadline = deadline;
        }

        if let Some(latency_budget) = external.latency_budget {
            qos.latency_budget = latency_budget;
        }

        if let Some(liveliness) = external.liveliness {
            qos.liveliness = liveliness.into();
        }

        if let Some(reliability) = external.reliability {
            if reliability.has_any_field() {
                // DataWriter default: Reliable
                let kind = reliability
                    .into_internal_kind()
                    .unwrap_or(internal_qos_policy::ReliabilityQosPolicyKind::Reliable);
                let max_blocking_time =
                    reliability.max_blocking_time.unwrap_or(DEFAULT_MAX_BLOCKING_TIME);
                qos.reliability =
                    internal_qos_policy::ReliabilityQosPolicy { kind, max_blocking_time };
            }
        }

        if let Some(destination_order) = external.destination_order {
            qos.destination_order = destination_order.into();
        }

        if let Some(history) = external.history {
            qos.history = history.into();
        }

        if let Some(resource_limits) = external.resource_limits {
            qos.resource_limits = resource_limits;
        }

        if let Some(transport_priority) = external.transport_priority {
            qos.transport_priority = transport_priority;
        }

        if let Some(lifespan) = external.lifespan {
            qos.lifespan = lifespan;
        }

        if let Some(user_data) = external.user_data {
            qos.user_data = user_data.into();
        }

        if let Some(ownership) = external.ownership {
            qos.ownership = ownership.into();
        }

        if let Some(ownership_strength) = external.ownership_strength {
            qos.ownership_strength = ownership_strength;
        }

        if let Some(writer_data_lifecycle) = external.writer_data_lifecycle {
            qos.writer_data_lifecycle = writer_data_lifecycle;
        }

        if let Some(data_representation) = external.data_representation {
            qos.data_representation = data_representation.into();
        }

        if let Some(writer_reliability_extension) = external.writer_reliability_extension {
            qos.writer_reliability_extension = writer_reliability_extension.into();
        }

        if let Some(data_frag) = external.data_frag {
            qos.data_frag = data_frag.into();
        }

        qos
    }
}

impl From<publication::qos::DataWriterQos> for DataWriterQos {
    fn from(internal: publication::qos::DataWriterQos) -> Self {
        Self {
            base_name: None,
            durability: Some(internal.durability.into()),
            durability_service: Some(internal.durability_service.into()),
            deadline: Some(internal.deadline),
            latency_budget: Some(internal.latency_budget),
            liveliness: Some(internal.liveliness.into()),
            reliability: Some(internal.reliability.into()),
            destination_order: Some(internal.destination_order.into()),
            history: Some(internal.history.into()),
            resource_limits: Some(internal.resource_limits),
            transport_priority: Some(internal.transport_priority),
            lifespan: Some(internal.lifespan),
            user_data: Some(internal.user_data.into()),
            ownership: Some(internal.ownership.into()),
            ownership_strength: Some(internal.ownership_strength),
            writer_data_lifecycle: Some(internal.writer_data_lifecycle),
            data_representation: Some(internal.data_representation.into()),
            writer_reliability_extension: Some(internal.writer_reliability_extension.into()),
            data_frag: Some(internal.data_frag.into()),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct DataReaderQos {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) durability: Option<DurabilityQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deadline: Option<DeadlineQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) latency_budget: Option<LatencyBudgetQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) liveliness: Option<LivelinessQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reliability: Option<ReliabilityQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) destination_order: Option<DestinationOrderQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) history: Option<HistoryQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) resource_limits: Option<ResourceLimitsQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) user_data: Option<UserDataQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ownership: Option<OwnershipQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) time_based_filter: Option<TimeBasedFilterQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reader_data_lifecycle: Option<ReaderDataLifecycleQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reader_reliability_extension: Option<ReaderReliabilityExtensionQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data_representation: Option<DataRepresentationQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) type_consistency_enforcement: Option<TypeConsistencyEnforcementQosPolicy>,
}

impl MergeQos for DataReaderQos {
    fn merge(&self, base: &Self) -> Self {
        Self {
            base_name: self.base_name.clone(),
            durability: self.durability.clone().or(base.durability.clone()),
            deadline: self.deadline.or(base.deadline),
            latency_budget: self.latency_budget.or(base.latency_budget),
            liveliness: self.liveliness.clone().or(base.liveliness.clone()),
            reliability: self.reliability.clone().or(base.reliability.clone()),
            destination_order: self.destination_order.clone().or(base.destination_order.clone()),
            history: self.history.clone().or(base.history.clone()),
            resource_limits: self.resource_limits.or(base.resource_limits),
            user_data: self.user_data.clone().or(base.user_data.clone()),
            ownership: self.ownership.clone().or(base.ownership.clone()),
            time_based_filter: self.time_based_filter.or(base.time_based_filter),
            reader_data_lifecycle: self.reader_data_lifecycle.or(base.reader_data_lifecycle),
            reader_reliability_extension: self
                .reader_reliability_extension
                .clone()
                .or(base.reader_reliability_extension.clone()),
            data_representation: self
                .data_representation
                .clone()
                .or(base.data_representation.clone()),
            type_consistency_enforcement: self
                .type_consistency_enforcement
                .clone()
                .or(base.type_consistency_enforcement.clone()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DataReaderQosNamed {
    pub(crate) name: String, // essential

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic_filter: Option<String>,

    #[serde(flatten)]
    pub(crate) qos: DataReaderQos,
}

pub(crate) type DataReaderQosSeq = Vec<DataReaderQosNamed>;

impl From<DataReaderQos> for subscription::qos::DataReaderQos {
    fn from(external: DataReaderQos) -> Self {
        let mut qos = Self::default();

        if let Some(durability) = external.durability {
            qos.durability = durability.into();
        }

        if let Some(deadline) = external.deadline {
            qos.deadline = deadline;
        }

        if let Some(latency_budget) = external.latency_budget {
            qos.latency_budget = latency_budget;
        }

        if let Some(liveliness) = external.liveliness {
            qos.liveliness = liveliness.into();
        }

        if let Some(reliability) = external.reliability {
            if reliability.has_any_field() {
                // DataReader default: BestEffort
                let kind = reliability
                    .into_internal_kind()
                    .unwrap_or(internal_qos_policy::ReliabilityQosPolicyKind::BestEffort);
                let max_blocking_time =
                    reliability.max_blocking_time.unwrap_or(DEFAULT_MAX_BLOCKING_TIME);
                qos.reliability =
                    internal_qos_policy::ReliabilityQosPolicy { kind, max_blocking_time };
            }
        }

        if let Some(destination_order) = external.destination_order {
            qos.destination_order = destination_order.into();
        }

        if let Some(history) = external.history {
            qos.history = history.into();
        }

        if let Some(resource_limits) = external.resource_limits {
            qos.resource_limits = resource_limits;
        }

        if let Some(user_data) = external.user_data {
            qos.user_data = user_data.into();
        }

        if let Some(ownership) = external.ownership {
            qos.ownership = ownership.into();
        }

        if let Some(time_based_filter) = external.time_based_filter {
            qos.time_based_filter = time_based_filter;
        }

        if let Some(reader_data_lifecycle) = external.reader_data_lifecycle {
            qos.reader_data_lifecycle = reader_data_lifecycle;
        }

        if let Some(reader_reliability_extension) = external.reader_reliability_extension {
            qos.reader_reliability_extension = reader_reliability_extension.into();
        }

        if let Some(data_representation) = external.data_representation {
            qos.data_representation = data_representation.into();
        }

        if let Some(type_consistency_enforcement) = external.type_consistency_enforcement {
            qos.type_consistency_enforcement = type_consistency_enforcement.into();
        }

        qos
    }
}

impl From<subscription::qos::DataReaderQos> for DataReaderQos {
    fn from(internal: subscription::qos::DataReaderQos) -> Self {
        Self {
            base_name: None,
            durability: Some(internal.durability.into()),
            deadline: Some(internal.deadline),
            latency_budget: Some(internal.latency_budget),
            liveliness: Some(internal.liveliness.into()),
            reliability: Some(internal.reliability.into()),
            destination_order: Some(internal.destination_order.into()),
            history: Some(internal.history.into()),
            resource_limits: Some(internal.resource_limits),
            user_data: Some(internal.user_data.into()),
            ownership: Some(internal.ownership.into()),
            time_based_filter: Some(internal.time_based_filter),
            reader_data_lifecycle: Some(internal.reader_data_lifecycle),
            reader_reliability_extension: Some(internal.reader_reliability_extension.into()),
            data_representation: Some(internal.data_representation.into()),
            type_consistency_enforcement: Some(internal.type_consistency_enforcement.into()),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct TopicQos {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic_data: Option<TopicDataQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) durability: Option<DurabilityQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) durability_service: Option<DurabilityServiceQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deadline: Option<DeadlineQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) latency_budget: Option<LatencyBudgetQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) liveliness: Option<LivelinessQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reliability: Option<ReliabilityQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) destination_order: Option<DestinationOrderQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) history: Option<HistoryQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) resource_limits: Option<ResourceLimitsQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) transport_priority: Option<TransportPriorityQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lifespan: Option<LifespanQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ownership: Option<OwnershipQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data_representation: Option<DataRepresentationQosPolicy>,
}

impl MergeQos for TopicQos {
    fn merge(&self, base: &Self) -> Self {
        Self {
            base_name: self.base_name.clone(),
            topic_data: self.topic_data.clone().or(base.topic_data.clone()),
            durability: self.durability.clone().or(base.durability.clone()),
            durability_service: self.durability_service.clone().or(base.durability_service.clone()),
            deadline: self.deadline.or(base.deadline),
            latency_budget: self.latency_budget.or(base.latency_budget),
            liveliness: self.liveliness.clone().or(base.liveliness.clone()),
            reliability: self.reliability.clone().or(base.reliability.clone()),
            destination_order: self.destination_order.clone().or(base.destination_order.clone()),
            history: self.history.clone().or(base.history.clone()),
            resource_limits: self.resource_limits.or(base.resource_limits),
            transport_priority: self.transport_priority.or(base.transport_priority),
            lifespan: self.lifespan.or(base.lifespan),
            ownership: self.ownership.clone().or(base.ownership.clone()),
            data_representation: self
                .data_representation
                .clone()
                .or(base.data_representation.clone()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct TopicQosNamed {
    pub(crate) name: String, // essential

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic_filter: Option<String>,

    #[serde(flatten)]
    pub(crate) qos: TopicQos,
}

pub(crate) type TopicQosSeq = Vec<TopicQosNamed>;

impl From<TopicQos> for topic::qos::TopicQos {
    fn from(external: TopicQos) -> Self {
        let mut qos = Self::default();

        if let Some(topic_data) = external.topic_data {
            qos.topic_data = topic_data.into();
        }

        if let Some(durability) = external.durability {
            qos.durability = durability.into();
        }

        if let Some(durability_service) = external.durability_service {
            qos.durability_service = durability_service.into();
        }

        if let Some(deadline) = external.deadline {
            qos.deadline = deadline;
        }

        if let Some(latency_budget) = external.latency_budget {
            qos.latency_budget = latency_budget;
        }

        if let Some(liveliness) = external.liveliness {
            qos.liveliness = liveliness.into();
        }

        if let Some(reliability) = external.reliability {
            if reliability.has_any_field() {
                // Topic default: BestEffort
                let kind = reliability
                    .into_internal_kind()
                    .unwrap_or(internal_qos_policy::ReliabilityQosPolicyKind::BestEffort);
                let max_blocking_time =
                    reliability.max_blocking_time.unwrap_or(DEFAULT_MAX_BLOCKING_TIME);
                qos.reliability =
                    internal_qos_policy::ReliabilityQosPolicy { kind, max_blocking_time };
            }
        }

        if let Some(destination_order) = external.destination_order {
            qos.destination_order = destination_order.into();
        }

        if let Some(history) = external.history {
            qos.history = history.into();
        }

        if let Some(resource_limits) = external.resource_limits {
            qos.resource_limits = resource_limits;
        }

        if let Some(transport_priority) = external.transport_priority {
            qos.transport_priority = transport_priority;
        }

        if let Some(lifespan) = external.lifespan {
            qos.lifespan = lifespan;
        }

        if let Some(ownership) = external.ownership {
            qos.ownership = ownership.into();
        }

        if let Some(data_representation) = external.data_representation {
            qos.data_representation = data_representation.into();
        }

        qos
    }
}

impl From<topic::qos::TopicQos> for TopicQos {
    fn from(internal: topic::qos::TopicQos) -> Self {
        Self {
            base_name: None,
            topic_data: Some(internal.topic_data.into()),
            durability: Some(internal.durability.into()),
            durability_service: Some(internal.durability_service.into()),
            deadline: Some(internal.deadline),
            latency_budget: Some(internal.latency_budget),
            liveliness: Some(internal.liveliness.into()),
            reliability: Some(internal.reliability.into()),
            destination_order: Some(internal.destination_order.into()),
            history: Some(internal.history.into()),
            resource_limits: Some(internal.resource_limits),
            transport_priority: Some(internal.transport_priority),
            lifespan: Some(internal.lifespan),
            ownership: Some(internal.ownership.into()),
            data_representation: Some(internal.data_representation.into()),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct SubscriberQos {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) presentation: Option<PresentationQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) partition: Option<PartitionQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group_data: Option<GroupDataQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) entity_factory: Option<EntityFactoryQosPolicy>,
}

impl MergeQos for SubscriberQos {
    fn merge(&self, base: &Self) -> Self {
        Self {
            base_name: self.base_name.clone(),
            presentation: self.presentation.clone().or(base.presentation.clone()),
            partition: self.partition.clone().or(base.partition.clone()),
            group_data: self.group_data.clone().or(base.group_data.clone()),
            entity_factory: self.entity_factory.or(base.entity_factory),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SubscriberQosNamed {
    pub(crate) name: String, // essential

    #[serde(flatten)]
    pub(crate) qos: SubscriberQos,
}

pub(crate) type SubscriberQosSeq = Vec<SubscriberQosNamed>;

impl From<SubscriberQos> for subscription::qos::SubscriberQos {
    fn from(external: SubscriberQos) -> Self {
        let mut qos = Self::default();

        if let Some(presentation) = external.presentation {
            qos.presentation = presentation.into();
        }

        if let Some(partition) = external.partition {
            qos.partition = partition.into();
        }

        if let Some(group_data) = external.group_data {
            qos.group_data = group_data.into();
        }

        if let Some(entity_factory) = external.entity_factory {
            qos.entity_factory = entity_factory;
        }

        qos
    }
}

impl From<subscription::qos::SubscriberQos> for SubscriberQos {
    fn from(internal: subscription::qos::SubscriberQos) -> Self {
        Self {
            base_name: None,
            presentation: Some(internal.presentation.into()),
            partition: Some(internal.partition.into()),
            group_data: Some(internal.group_data.into()),
            entity_factory: Some(internal.entity_factory),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct PublisherQos {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) presentation: Option<PresentationQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) partition: Option<PartitionQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group_data: Option<GroupDataQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) entity_factory: Option<EntityFactoryQosPolicy>,
}

impl MergeQos for PublisherQos {
    fn merge(&self, base: &Self) -> Self {
        Self {
            base_name: self.base_name.clone(),
            presentation: self.presentation.clone().or(base.presentation.clone()),
            partition: self.partition.clone().or(base.partition.clone()),
            group_data: self.group_data.clone().or(base.group_data.clone()),
            entity_factory: self.entity_factory.or(base.entity_factory),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PublisherQosNamed {
    pub(crate) name: String, // essential

    #[serde(flatten)]
    pub(crate) qos: PublisherQos,
}

pub(crate) type PublisherQosSeq = Vec<PublisherQosNamed>;

impl From<PublisherQos> for publication::qos::PublisherQos {
    fn from(external: PublisherQos) -> Self {
        let mut qos = Self::default();

        if let Some(presentation) = external.presentation {
            qos.presentation = presentation.into();
        }

        if let Some(partition) = external.partition {
            qos.partition = partition.into();
        }

        if let Some(group_data) = external.group_data {
            qos.group_data = group_data.into();
        }

        if let Some(entity_factory) = external.entity_factory {
            qos.entity_factory = entity_factory;
        }

        qos
    }
}

impl From<publication::qos::PublisherQos> for PublisherQos {
    fn from(internal: publication::qos::PublisherQos) -> Self {
        Self {
            base_name: None,
            presentation: Some(internal.presentation.into()),
            partition: Some(internal.partition.into()),
            group_data: Some(internal.group_data.into()),
            entity_factory: Some(internal.entity_factory),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct DomainParticipantQos {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) user_data: Option<UserDataQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) entity_factory: Option<EntityFactoryQosPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) property: Option<PropertyQosPolicy>,
}

/// Merge two `PropertyQosPolicy` mirrors by `name`: parent's keys are kept and
/// child's keys override on collision. This matches XML inheritance semantics
/// and is required because the simple `.or()` fallback would drop all parent
/// properties as soon as the child sets a single one.
fn merge_property_policies(
    parent: PropertyQosPolicy,
    child: PropertyQosPolicy,
) -> PropertyQosPolicy {
    let mut merged = parent;
    if let Some(child_entries) = child.value {
        let merged_entries = merged.value.get_or_insert_with(Vec::new);
        for entry in child_entries {
            if let Some(slot) = merged_entries.iter_mut().find(|e| e.name == entry.name) {
                *slot = entry;
            } else {
                merged_entries.push(entry);
            }
        }
    }
    merged
}

impl MergeQos for DomainParticipantQos {
    fn merge(&self, base: &Self) -> Self {
        Self {
            base_name: self.base_name.clone(),
            user_data: self.user_data.clone().or(base.user_data.clone()),
            entity_factory: self.entity_factory.or(base.entity_factory),
            property: match (self.property.clone(), base.property.clone()) {
                (Some(child), Some(parent)) => Some(merge_property_policies(parent, child)),
                (Some(child), None) => Some(child),
                (None, parent) => parent,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DomainParticipantQosNamed {
    pub(crate) name: String, // essential

    #[serde(flatten)]
    pub(crate) qos: DomainParticipantQos,
}

pub(crate) type DomainParticipantQosSeq = Vec<DomainParticipantQosNamed>;

impl From<DomainParticipantQos> for domain::qos::DomainParticipantQos {
    fn from(external: DomainParticipantQos) -> Self {
        let mut qos = Self::default();

        if let Some(user_data) = external.user_data {
            qos.user_data = user_data.into();
        }

        if let Some(entity_factory) = external.entity_factory {
            qos.entity_factory = entity_factory;
        }

        if let Some(property) = external.property {
            qos.property = property.into();
        }

        qos
    }
}

impl From<domain::qos::DomainParticipantQos> for DomainParticipantQos {
    fn from(internal: domain::qos::DomainParticipantQos) -> Self {
        Self {
            base_name: None,
            user_data: Some(internal.user_data.into()),
            entity_factory: Some(internal.entity_factory),
            property: Some(internal.property.into()),
        }
    }
}

#[cfg(test)]
mod property_qos_config_tests {
    use super::*;
    use crate::config::types::qos_policy::PropertyEntry;

    fn entry(name: &str, value: &str, propagate: Option<bool>) -> PropertyEntry {
        PropertyEntry { name: name.into(), value: value.into(), propagate }
    }

    #[test]
    fn property_round_trips_internal_external_internal() {
        // Covers external→internal (with propagate default = true), internal→external,
        // and the empty-policy edge case in one go.
        let mut internal = internal_qos_policy::PropertyQosPolicy::default();
        internal.add_property("int2dds.transport.UDPv4.multicast_ttl", "32", false);
        internal.add_property("vendor.us.int2.example", "abc", true);

        let external: PropertyQosPolicy = internal.clone().into();
        let back: internal_qos_policy::PropertyQosPolicy = external.into();
        assert_eq!(back, internal);

        let empty_external: PropertyQosPolicy =
            internal_qos_policy::PropertyQosPolicy::default().into();
        assert!(empty_external.value.is_none(), "empty internal serializes to None");

        let propagate_default: internal_qos_policy::PropertyQosPolicy =
            PropertyQosPolicy { value: Some(vec![entry("k", "v", None)]) }.into();
        assert!(propagate_default.value[0].propagate, "missing propagate => true");
    }

    #[test]
    fn participant_qos_merge_unions_and_overrides_by_name() {
        // Covers both the helper and the MergeQos branch in one fixture.
        let parent_qos = DomainParticipantQos {
            property: Some(PropertyQosPolicy {
                value: Some(vec![entry("a", "1", None), entry("b", "2", None)]),
            }),
            ..Default::default()
        };
        let child_qos = DomainParticipantQos {
            property: Some(PropertyQosPolicy {
                value: Some(vec![entry("b", "9", Some(false)), entry("c", "3", None)]),
            }),
            ..Default::default()
        };

        let merged_entries = child_qos.merge(&parent_qos).property.unwrap().value.unwrap();
        assert_eq!(merged_entries.len(), 3);
        assert_eq!(merged_entries[0].value, "1");
        assert_eq!(merged_entries[1].value, "9", "child overrides parent by name");
        assert_eq!(merged_entries[1].propagate, Some(false));
        assert_eq!(merged_entries[2].name, "c");

        // Parent-only and child-only fallthrough branches.
        assert!(DomainParticipantQos::default().merge(&parent_qos).property.is_some());
    }

    #[test]
    fn participant_qos_round_trips_through_json() {
        let json = r#"{
            "property": {
                "value": [
                    { "name": "int2dds.transport.UDPv4.multicast_ttl", "value": "32", "propagate": false },
                    { "name": "vendor.us.int2.example", "value": "abc" }
                ]
            }
        }"#;
        let parsed: DomainParticipantQos = serde_json::from_str(json).expect("parse");
        let internal: domain::qos::DomainParticipantQos = parsed.into();
        assert_eq!(
            internal.property.find_property("int2dds.transport.UDPv4.multicast_ttl"),
            Some("32")
        );

        let external: DomainParticipantQos = internal.clone().into();
        let internal2: domain::qos::DomainParticipantQos = external.into();
        assert_eq!(internal2, internal);
    }
}
