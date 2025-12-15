use serde::{Deserialize, Serialize};

use crate::{
    config::types::qos_policy::{
        DestinationOrderQosPolicy, DurabilityQosPolicy, DurabilityServiceQosPolicy,
        GroupDataQosPolicy, HistoryQosPolicy, LivelinessQosPolicy, OwnershipQosPolicy,
        PartitionQosPolicy, PresentationQosPolicy, ReliabilityQosPolicy, TopicDataQosPolicy,
        UserDataQosPolicy,
    },
    domain,
    infrastructure::qos_policy::{
        DeadlineQosPolicy, EntityFactoryQosPolicy, LatencyBudgetQosPolicy, LifespanQosPolicy,
        OwnershipStrengthQosPolicy, ReaderDataLifecycleQosPolicy, ResourceLimitsQosPolicy,
        TimeBasedFilterQosPolicy, TransportPriorityQosPolicy, WriterDataLifecycleQosPolicy,
    },
    publication, subscription, topic,
};

#[allow(dead_code)]
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
            qos.reliability = reliability.into();
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
            qos.reliability = reliability.into();
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
            qos.reliability = reliability.into();
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
}

impl MergeQos for DomainParticipantQos {
    fn merge(&self, base: &Self) -> Self {
        Self {
            base_name: self.base_name.clone(),
            user_data: self.user_data.clone().or(base.user_data.clone()),
            entity_factory: self.entity_factory.or(base.entity_factory),
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

        qos
    }
}

impl From<domain::qos::DomainParticipantQos> for DomainParticipantQos {
    fn from(internal: domain::qos::DomainParticipantQos) -> Self {
        Self {
            base_name: None,
            user_data: Some(internal.user_data.into()),
            entity_factory: Some(internal.entity_factory),
        }
    }
}
