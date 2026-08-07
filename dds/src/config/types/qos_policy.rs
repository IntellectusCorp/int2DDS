//! DDS standard QoS policy types for JSON/XML serialization.
//!
//! These types conform to the OMG DDS-JSON specification and provide
//! conversion to/from internal QoS types in [`crate::infrastructure::qos_policy`].

use crate::{
    core::{
        time::Duration,
        types::{deserialize_i32_or_unlimited, serialize_i32_or_unlimited, LENGTH_UNLIMITED},
    },
    infrastructure::qos_policy,
};
use log::error;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct HistoryQosPolicy {
    #[serde(default)]
    pub(crate) kind: HistoryQosPolicyKind,
    #[serde(deserialize_with = "deserialize_i32_or_unlimited")]
    #[serde(serialize_with = "serialize_i32_or_unlimited")]
    #[serde(default = "default_depth")]
    pub(crate) depth: i32,
    // Only meaningful for a Volatile KeepAll writer; mirrors qos_policy default of true
    #[serde(default = "default_strict")]
    pub(crate) strict: bool,
}

impl Default for HistoryQosPolicy {
    fn default() -> Self {
        Self {
            kind: HistoryQosPolicyKind::default(),
            depth: default_depth(),
            strict: default_strict(),
        }
    }
}

fn default_depth() -> i32 {
    1
}

fn default_strict() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum HistoryQosPolicyKind {
    #[default]
    KeepLastHistoryQos,
    KeepAllHistoryQos,
}

impl From<HistoryQosPolicy> for qos_policy::HistoryQosPolicy {
    fn from(external: HistoryQosPolicy) -> Self {
        match external.kind {
            HistoryQosPolicyKind::KeepLastHistoryQos => Self {
                kind: qos_policy::HistoryQosPolicyKind::KeepLast(external.depth),
                strict: external.strict,
            },
            HistoryQosPolicyKind::KeepAllHistoryQos => {
                Self { kind: qos_policy::HistoryQosPolicyKind::KeepAll, strict: external.strict }
            }
        }
    }
}

impl From<qos_policy::HistoryQosPolicy> for HistoryQosPolicy {
    fn from(internal: qos_policy::HistoryQosPolicy) -> Self {
        match internal.kind {
            qos_policy::HistoryQosPolicyKind::KeepLast(depth) => Self {
                kind: HistoryQosPolicyKind::KeepLastHistoryQos,
                depth,
                strict: internal.strict,
            },
            qos_policy::HistoryQosPolicyKind::KeepAll => Self {
                kind: HistoryQosPolicyKind::KeepAllHistoryQos,
                depth: 1,
                strict: internal.strict,
            },
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct OwnershipQosPolicy {
    pub(crate) kind: OwnershipQosPolicyKind,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum OwnershipQosPolicyKind {
    #[default]
    SharedOwnershipQos,
    ExclusiveOwnershipQos,
}

impl From<OwnershipQosPolicy> for qos_policy::OwnershipQosPolicy {
    fn from(external: OwnershipQosPolicy) -> Self {
        match external.kind {
            OwnershipQosPolicyKind::SharedOwnershipQos => {
                Self { kind: qos_policy::OwnershipQosPolicyKind::Shared }
            }
            OwnershipQosPolicyKind::ExclusiveOwnershipQos => {
                Self { kind: qos_policy::OwnershipQosPolicyKind::Exclusive }
            }
        }
    }
}

impl From<qos_policy::OwnershipQosPolicy> for OwnershipQosPolicy {
    fn from(internal: qos_policy::OwnershipQosPolicy) -> Self {
        match internal.kind {
            qos_policy::OwnershipQosPolicyKind::Shared => {
                Self { kind: OwnershipQosPolicyKind::SharedOwnershipQos }
            }
            qos_policy::OwnershipQosPolicyKind::Exclusive => {
                Self { kind: OwnershipQosPolicyKind::ExclusiveOwnershipQos }
            }
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct PresentationQosPolicy {
    pub(crate) access_scope: PresentationQosAccessScopeKind,
    pub(crate) coherent_access: bool,
    pub(crate) ordered_access: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub(crate) enum PresentationQosAccessScopeKind {
    #[default]
    InstancePresentationQos,
    TopicPresentationQos,
    GroupPresentationQos,
}

impl From<PresentationQosPolicy> for qos_policy::PresentationQosPolicy {
    fn from(external: PresentationQosPolicy) -> Self {
        match external.access_scope {
            PresentationQosAccessScopeKind::InstancePresentationQos => Self {
                access_scope: qos_policy::PresentationQosAccessScopeKind::Instance,
                coherent_access: external.coherent_access,
                ordered_access: external.ordered_access,
            },
            PresentationQosAccessScopeKind::TopicPresentationQos => Self {
                access_scope: qos_policy::PresentationQosAccessScopeKind::Topic,
                coherent_access: external.coherent_access,
                ordered_access: external.ordered_access,
            },
            PresentationQosAccessScopeKind::GroupPresentationQos => Self {
                access_scope: qos_policy::PresentationQosAccessScopeKind::Group,
                coherent_access: external.coherent_access,
                ordered_access: external.ordered_access,
            },
        }
    }
}

impl From<qos_policy::PresentationQosPolicy> for PresentationQosPolicy {
    fn from(internal: qos_policy::PresentationQosPolicy) -> Self {
        match internal.access_scope {
            qos_policy::PresentationQosAccessScopeKind::Instance => Self {
                access_scope: PresentationQosAccessScopeKind::InstancePresentationQos,
                coherent_access: internal.coherent_access,
                ordered_access: internal.ordered_access,
            },
            qos_policy::PresentationQosAccessScopeKind::Topic => Self {
                access_scope: PresentationQosAccessScopeKind::TopicPresentationQos,
                coherent_access: internal.coherent_access,
                ordered_access: internal.ordered_access,
            },
            qos_policy::PresentationQosAccessScopeKind::Group => Self {
                access_scope: PresentationQosAccessScopeKind::GroupPresentationQos,
                coherent_access: internal.coherent_access,
                ordered_access: internal.ordered_access,
            },
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct UserDataQosPolicy {
    pub(crate) value: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct PropertyQosPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<Vec<PropertyEntry>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PropertyEntry {
    pub(crate) name: String,
    pub(crate) value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) propagate: Option<bool>,
}

impl From<PropertyQosPolicy> for qos_policy::PropertyQosPolicy {
    fn from(external: PropertyQosPolicy) -> Self {
        let mut internal = Self::default();
        if let Some(entries) = external.value {
            for e in entries {
                internal.add_property(e.name, e.value, e.propagate.unwrap_or(true));
            }
        }
        internal
    }
}

impl From<qos_policy::PropertyQosPolicy> for PropertyQosPolicy {
    fn from(internal: qos_policy::PropertyQosPolicy) -> Self {
        let entries: Vec<PropertyEntry> = internal
            .value
            .into_iter()
            .map(|p| PropertyEntry { name: p.name, value: p.value, propagate: Some(p.propagate) })
            .collect();
        Self { value: if entries.is_empty() { None } else { Some(entries) } }
    }
}

impl From<UserDataQosPolicy> for qos_policy::UserDataQosPolicy {
    fn from(external: UserDataQosPolicy) -> Self {
        Self { value: external.value.into() }
    }
}

impl From<qos_policy::UserDataQosPolicy> for UserDataQosPolicy {
    fn from(internal: qos_policy::UserDataQosPolicy) -> Self {
        Self {
            value: String::from_utf8(internal.value).unwrap_or_else(|e| {
                error!("Failed to convert to UTF-8: {}", e);
                String::new()
            }),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct TopicDataQosPolicy {
    pub(crate) value: String,
}

impl From<TopicDataQosPolicy> for qos_policy::TopicDataQosPolicy {
    fn from(external: TopicDataQosPolicy) -> Self {
        Self { value: external.value.into() }
    }
}

impl From<qos_policy::TopicDataQosPolicy> for TopicDataQosPolicy {
    fn from(internal: qos_policy::TopicDataQosPolicy) -> Self {
        Self {
            value: String::from_utf8(internal.value).unwrap_or_else(|e| {
                error!("Failed to convert to UTF-8: {}", e);
                String::new()
            }),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct GroupDataQosPolicy {
    pub(crate) value: String,
}

impl From<GroupDataQosPolicy> for qos_policy::GroupDataQosPolicy {
    fn from(external: GroupDataQosPolicy) -> Self {
        Self { value: external.value.into() }
    }
}

impl From<qos_policy::GroupDataQosPolicy> for GroupDataQosPolicy {
    fn from(internal: qos_policy::GroupDataQosPolicy) -> Self {
        Self {
            value: String::from_utf8(internal.value).unwrap_or_else(|e| {
                error!("Failed to convert to UTF-8: {}", e);
                String::new()
            }),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct PartitionQosPolicy {
    pub(crate) name: StringSeq,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct StringSeq {
    pub(crate) element: Vec<String>,
}

impl From<PartitionQosPolicy> for qos_policy::PartitionQosPolicy {
    fn from(external: PartitionQosPolicy) -> Self {
        Self { name: external.name.element }
    }
}

impl From<qos_policy::PartitionQosPolicy> for PartitionQosPolicy {
    fn from(internal: qos_policy::PartitionQosPolicy) -> Self {
        Self { name: StringSeq { element: internal.name } }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct ReliabilityQosPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<ReliabilityQosPolicyKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_blocking_time: Option<Duration>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ReliabilityQosPolicyKind {
    BestEffortReliabilityQos,
    ReliableReliabilityQos,
}

/// Default max_blocking_time: 100ms
pub(crate) const DEFAULT_MAX_BLOCKING_TIME: Duration = Duration { sec: 0, nanosec: 100_000_000 };

impl ReliabilityQosPolicy {
    /// Returns true if either kind or max_blocking_time is specified.
    pub(crate) fn has_any_field(&self) -> bool {
        self.kind.is_some() || self.max_blocking_time.is_some()
    }

    /// Converts kind to internal representation if present.
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn into_internal_kind(&self) -> Option<qos_policy::ReliabilityQosPolicyKind> {
        self.kind.as_ref().map(|k| match k {
            ReliabilityQosPolicyKind::BestEffortReliabilityQos => {
                qos_policy::ReliabilityQosPolicyKind::BestEffort
            }
            ReliabilityQosPolicyKind::ReliableReliabilityQos => {
                qos_policy::ReliabilityQosPolicyKind::Reliable
            }
        })
    }
}

impl From<qos_policy::ReliabilityQosPolicy> for ReliabilityQosPolicy {
    fn from(internal: qos_policy::ReliabilityQosPolicy) -> Self {
        let kind = match internal.kind {
            qos_policy::ReliabilityQosPolicyKind::BestEffort => {
                ReliabilityQosPolicyKind::BestEffortReliabilityQos
            }
            qos_policy::ReliabilityQosPolicyKind::Reliable => {
                ReliabilityQosPolicyKind::ReliableReliabilityQos
            }
        };
        Self { kind: Some(kind), max_blocking_time: Some(internal.max_blocking_time) }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LivelinessQosPolicy {
    pub(crate) kind: LivelinessQosPolicyKind,
    pub(crate) lease_duration: Duration,
}

impl Default for LivelinessQosPolicy {
    fn default() -> Self {
        Self {
            kind: LivelinessQosPolicyKind::default(),
            lease_duration: Duration {
                sec: Duration::INFINITE_SEC,
                nanosec: Duration::INFINITE_NSEC,
            },
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub(crate) enum LivelinessQosPolicyKind {
    #[default]
    AutomaticLivelinessQos,
    ManualByParticipantLivelinessQos,
    ManualByTopicLivelinessQos,
}

impl From<LivelinessQosPolicy> for qos_policy::LivelinessQosPolicy {
    fn from(external: LivelinessQosPolicy) -> Self {
        match external.kind {
            LivelinessQosPolicyKind::AutomaticLivelinessQos => Self {
                kind: qos_policy::LivelinessQosPolicyKind::Automatic,
                lease_duration: external.lease_duration,
            },
            LivelinessQosPolicyKind::ManualByParticipantLivelinessQos => Self {
                kind: qos_policy::LivelinessQosPolicyKind::ManualByParticipant,
                lease_duration: external.lease_duration,
            },
            LivelinessQosPolicyKind::ManualByTopicLivelinessQos => Self {
                kind: qos_policy::LivelinessQosPolicyKind::ManualByTopic,
                lease_duration: external.lease_duration,
            },
        }
    }
}

impl From<qos_policy::LivelinessQosPolicy> for LivelinessQosPolicy {
    fn from(internal: qos_policy::LivelinessQosPolicy) -> Self {
        match internal.kind {
            qos_policy::LivelinessQosPolicyKind::Automatic => Self {
                kind: LivelinessQosPolicyKind::AutomaticLivelinessQos,
                lease_duration: internal.lease_duration,
            },
            qos_policy::LivelinessQosPolicyKind::ManualByParticipant => Self {
                kind: LivelinessQosPolicyKind::ManualByParticipantLivelinessQos,
                lease_duration: internal.lease_duration,
            },
            qos_policy::LivelinessQosPolicyKind::ManualByTopic => Self {
                kind: LivelinessQosPolicyKind::ManualByTopicLivelinessQos,
                lease_duration: internal.lease_duration,
            },
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct DurabilityQosPolicy {
    pub(crate) kind: DurabilityQosPolicyKind,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub(crate) enum DurabilityQosPolicyKind {
    #[default]
    VolatileDurabilityQos,
    TransientLocalDurabilityQos,
    TransientDurabilityQos,
    PersistentDurabilityQos,
}

impl From<DurabilityQosPolicy> for qos_policy::DurabilityQosPolicy {
    fn from(external: DurabilityQosPolicy) -> Self {
        match external.kind {
            DurabilityQosPolicyKind::VolatileDurabilityQos => {
                Self { kind: qos_policy::DurabilityQosPolicyKind::Volatile }
            }
            DurabilityQosPolicyKind::TransientLocalDurabilityQos => {
                Self { kind: qos_policy::DurabilityQosPolicyKind::TransientLocal }
            }
            DurabilityQosPolicyKind::TransientDurabilityQos => {
                Self { kind: qos_policy::DurabilityQosPolicyKind::Transient }
            }
            DurabilityQosPolicyKind::PersistentDurabilityQos => {
                Self { kind: qos_policy::DurabilityQosPolicyKind::Persistent }
            }
        }
    }
}

impl From<qos_policy::DurabilityQosPolicy> for DurabilityQosPolicy {
    fn from(internal: qos_policy::DurabilityQosPolicy) -> Self {
        match internal.kind {
            qos_policy::DurabilityQosPolicyKind::Volatile => {
                Self { kind: DurabilityQosPolicyKind::VolatileDurabilityQos }
            }
            qos_policy::DurabilityQosPolicyKind::TransientLocal => {
                Self { kind: DurabilityQosPolicyKind::TransientLocalDurabilityQos }
            }
            qos_policy::DurabilityQosPolicyKind::Transient => {
                Self { kind: DurabilityQosPolicyKind::TransientDurabilityQos }
            }
            qos_policy::DurabilityQosPolicyKind::Persistent => {
                Self { kind: DurabilityQosPolicyKind::PersistentDurabilityQos }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DurabilityServiceQosPolicy {
    pub(crate) service_cleanup_delay: Duration,
    pub(crate) history_kind: HistoryQosPolicyKind,
    #[serde(deserialize_with = "deserialize_i32_or_unlimited")]
    #[serde(serialize_with = "serialize_i32_or_unlimited")]
    pub(crate) max_samples: i32,
    #[serde(deserialize_with = "deserialize_i32_or_unlimited")]
    #[serde(serialize_with = "serialize_i32_or_unlimited")]
    pub(crate) max_instances: i32,
    #[serde(deserialize_with = "deserialize_i32_or_unlimited")]
    #[serde(serialize_with = "serialize_i32_or_unlimited")]
    pub(crate) max_samples_per_instance: i32,
    #[serde(deserialize_with = "deserialize_i32_or_unlimited")]
    #[serde(serialize_with = "serialize_i32_or_unlimited")]
    pub(crate) history_depth: i32,
}

impl Default for DurabilityServiceQosPolicy {
    fn default() -> Self {
        Self {
            service_cleanup_delay: Duration::default(),
            history_kind: HistoryQosPolicyKind::KeepLastHistoryQos,
            max_instances: LENGTH_UNLIMITED,
            max_samples: LENGTH_UNLIMITED,
            max_samples_per_instance: LENGTH_UNLIMITED,
            history_depth: 1,
        }
    }
}

impl From<DurabilityServiceQosPolicy> for qos_policy::DurabilityServiceQosPolicy {
    fn from(external: DurabilityServiceQosPolicy) -> Self {
        match external.history_kind {
            HistoryQosPolicyKind::KeepLastHistoryQos => Self {
                history_kind: qos_policy::HistoryQosPolicyKind::KeepLast(external.history_depth),
                service_cleanup_delay: external.service_cleanup_delay,
                max_instances: external.max_instances,
                max_samples: external.max_samples,
                max_samples_per_instance: external.max_samples_per_instance,
            },
            HistoryQosPolicyKind::KeepAllHistoryQos => Self {
                history_kind: qos_policy::HistoryQosPolicyKind::KeepAll,
                service_cleanup_delay: external.service_cleanup_delay,
                max_instances: external.max_instances,
                max_samples: external.max_samples,
                max_samples_per_instance: external.max_samples_per_instance,
            },
        }
    }
}

impl From<qos_policy::DurabilityServiceQosPolicy> for DurabilityServiceQosPolicy {
    fn from(internal: qos_policy::DurabilityServiceQosPolicy) -> Self {
        match internal.history_kind {
            qos_policy::HistoryQosPolicyKind::KeepLast(history_depth) => Self {
                history_kind: HistoryQosPolicyKind::KeepLastHistoryQos,
                history_depth,
                service_cleanup_delay: internal.service_cleanup_delay,
                max_instances: internal.max_instances,
                max_samples: internal.max_samples,
                max_samples_per_instance: internal.max_samples_per_instance,
            },
            qos_policy::HistoryQosPolicyKind::KeepAll => Self {
                history_kind: HistoryQosPolicyKind::KeepAllHistoryQos,
                history_depth: 1,
                service_cleanup_delay: internal.service_cleanup_delay,
                max_instances: internal.max_instances,
                max_samples: internal.max_samples,
                max_samples_per_instance: internal.max_samples_per_instance,
            },
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct DestinationOrderQosPolicy {
    pub(crate) kind: DestinationOrderQosPolicyKind,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) enum DestinationOrderQosPolicyKind {
    #[default]
    #[serde(rename = "BY_RECEPTION_TIMESTAMP_DESTINATIONORDER_QOS")]
    ByReceptionTimestampDestinationOrderQos,
    #[serde(rename = "BY_SOURCE_TIMESTAMP_DESTINATIONORDER_QOS")]
    BySourceTimestampDestinationOrderQos,
}

impl From<DestinationOrderQosPolicy> for qos_policy::DestinationOrderQosPolicy {
    fn from(external: DestinationOrderQosPolicy) -> Self {
        match external.kind {
            DestinationOrderQosPolicyKind::ByReceptionTimestampDestinationOrderQos => {
                Self { kind: qos_policy::DestinationOrderQosPolicyKind::ByReceptionTimestamp }
            }
            DestinationOrderQosPolicyKind::BySourceTimestampDestinationOrderQos => {
                Self { kind: qos_policy::DestinationOrderQosPolicyKind::BySourceTimestamp }
            }
        }
    }
}

impl From<qos_policy::DestinationOrderQosPolicy> for DestinationOrderQosPolicy {
    fn from(internal: qos_policy::DestinationOrderQosPolicy) -> Self {
        match internal.kind {
            qos_policy::DestinationOrderQosPolicyKind::ByReceptionTimestamp => Self {
                kind: DestinationOrderQosPolicyKind::ByReceptionTimestampDestinationOrderQos,
            },
            qos_policy::DestinationOrderQosPolicyKind::BySourceTimestamp => {
                Self { kind: DestinationOrderQosPolicyKind::BySourceTimestampDestinationOrderQos }
            }
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct LifespanReferenceQosPolicy {
    pub(crate) kind: LifespanReferenceQosPolicyKind,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) enum LifespanReferenceQosPolicyKind {
    #[default]
    #[serde(rename = "BY_SOURCE")]
    BySourceTimestampLifespanReferenceQos,
    #[serde(rename = "BY_RECEPTION")]
    ByReceptionTimestampLifespanReferenceQos,
}

impl From<LifespanReferenceQosPolicy> for qos_policy::LifespanReferenceQosPolicy {
    fn from(external: LifespanReferenceQosPolicy) -> Self {
        match external.kind {
            LifespanReferenceQosPolicyKind::BySourceTimestampLifespanReferenceQos => {
                Self { kind: qos_policy::LifespanReferenceQosPolicyKind::BySourceTimestamp }
            }
            LifespanReferenceQosPolicyKind::ByReceptionTimestampLifespanReferenceQos => {
                Self { kind: qos_policy::LifespanReferenceQosPolicyKind::ByReceptionTimestamp }
            }
        }
    }
}

impl From<qos_policy::LifespanReferenceQosPolicy> for LifespanReferenceQosPolicy {
    fn from(internal: qos_policy::LifespanReferenceQosPolicy) -> Self {
        match internal.kind {
            qos_policy::LifespanReferenceQosPolicyKind::BySourceTimestamp => {
                Self { kind: LifespanReferenceQosPolicyKind::BySourceTimestampLifespanReferenceQos }
            }
            qos_policy::LifespanReferenceQosPolicyKind::ByReceptionTimestamp => Self {
                kind: LifespanReferenceQosPolicyKind::ByReceptionTimestampLifespanReferenceQos,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WriterReliabilityExtensionQosPolicy {
    pub(crate) disable_piggyback_heartbeat: bool,
    pub(crate) heartbeat_period: Duration,
    pub(crate) initial_heartbeat_delay: Duration,
    pub(crate) push_mode: bool,
    pub(crate) nack_suppression_duration: Duration,
    pub(crate) nack_response_delay: Duration,
}

impl Default for WriterReliabilityExtensionQosPolicy {
    fn default() -> Self {
        qos_policy::WriterReliabilityExtensionQosPolicy::default().into()
    }
}

impl From<WriterReliabilityExtensionQosPolicy> for qos_policy::WriterReliabilityExtensionQosPolicy {
    fn from(external: WriterReliabilityExtensionQosPolicy) -> Self {
        Self {
            disable_piggyback_heartbeat: external.disable_piggyback_heartbeat,
            heartbeat_period: external.heartbeat_period,
            initial_heartbeat_delay: external.initial_heartbeat_delay,
            push_mode: external.push_mode,
            nack_suppression_duration: external.nack_suppression_duration,
            nack_response_delay: external.nack_response_delay,
        }
    }
}

impl From<qos_policy::WriterReliabilityExtensionQosPolicy> for WriterReliabilityExtensionQosPolicy {
    fn from(internal: qos_policy::WriterReliabilityExtensionQosPolicy) -> Self {
        Self {
            disable_piggyback_heartbeat: internal.disable_piggyback_heartbeat,
            heartbeat_period: internal.heartbeat_period,
            initial_heartbeat_delay: internal.initial_heartbeat_delay,
            push_mode: internal.push_mode,
            nack_suppression_duration: internal.nack_suppression_duration,
            nack_response_delay: internal.nack_response_delay,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DataFragQosPolicy {
    pub(crate) max_size: i32,
}

impl Default for DataFragQosPolicy {
    fn default() -> Self {
        Self { max_size: qos_policy::DataFragQosPolicy::UNSET }
    }
}

impl From<DataFragQosPolicy> for qos_policy::DataFragQosPolicy {
    fn from(external: DataFragQosPolicy) -> Self {
        Self { max_size: external.max_size }
    }
}

impl From<qos_policy::DataFragQosPolicy> for DataFragQosPolicy {
    fn from(internal: qos_policy::DataFragQosPolicy) -> Self {
        Self { max_size: internal.max_size }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ReaderReliabilityExtensionQosPolicy {
    pub(crate) heartbeat_response_delay: Duration,
    pub(crate) heartbeat_suppression_duration: Duration,
    pub(crate) preemptive_acknack_delay: Duration,
}

impl Default for ReaderReliabilityExtensionQosPolicy {
    fn default() -> Self {
        Self {
            heartbeat_response_delay: Duration { sec: 0, nanosec: 10_000_000 },
            heartbeat_suppression_duration: Duration { sec: 0, nanosec: 0 },
            preemptive_acknack_delay: Duration { sec: 0, nanosec: 80_000_000 },
        }
    }
}

impl From<ReaderReliabilityExtensionQosPolicy> for qos_policy::ReaderReliabilityExtensionQosPolicy {
    fn from(external: ReaderReliabilityExtensionQosPolicy) -> Self {
        Self {
            heartbeat_response_delay: external.heartbeat_response_delay,
            heartbeat_suppression_duration: external.heartbeat_suppression_duration,
            preemptive_acknack_delay: external.preemptive_acknack_delay,
        }
    }
}

impl From<qos_policy::ReaderReliabilityExtensionQosPolicy> for ReaderReliabilityExtensionQosPolicy {
    fn from(internal: qos_policy::ReaderReliabilityExtensionQosPolicy) -> Self {
        Self {
            heartbeat_response_delay: internal.heartbeat_response_delay,
            heartbeat_suppression_duration: internal.heartbeat_suppression_duration,
            preemptive_acknack_delay: internal.preemptive_acknack_delay,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ReaderMulticastExtensionQosPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group_address: Option<String>,
}

impl From<ReaderMulticastExtensionQosPolicy> for qos_policy::ReaderMulticastExtensionQosPolicy {
    fn from(external: ReaderMulticastExtensionQosPolicy) -> Self {
        Self { group_address: external.group_address }
    }
}

impl From<qos_policy::ReaderMulticastExtensionQosPolicy> for ReaderMulticastExtensionQosPolicy {
    fn from(internal: qos_policy::ReaderMulticastExtensionQosPolicy) -> Self {
        Self { group_address: internal.group_address }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DataRepresentationQosPolicy {
    pub(crate) value: Vec<DataRepresentationId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum DataRepresentationId {
    #[default]
    XcdrDataRepresentation,
    XmlDataRepresentation,
    Xcdr2DataRepresentation,
}

impl From<DataRepresentationQosPolicy> for qos_policy::DataRepresentationQosPolicy {
    fn from(external: DataRepresentationQosPolicy) -> Self {
        Self { value: external.value.into_iter().map(|id| id.into()).collect() }
    }
}

impl From<qos_policy::DataRepresentationQosPolicy> for DataRepresentationQosPolicy {
    fn from(internal: qos_policy::DataRepresentationQosPolicy) -> Self {
        Self { value: internal.value.into_iter().map(|id| id.into()).collect() }
    }
}

impl From<DataRepresentationId> for qos_policy::DataRepresentationId {
    fn from(external: DataRepresentationId) -> Self {
        match external {
            DataRepresentationId::XcdrDataRepresentation => Self::XcdrDataRepresentation,
            DataRepresentationId::XmlDataRepresentation => Self::XmlDataRepresentation,
            DataRepresentationId::Xcdr2DataRepresentation => Self::Xcdr2DataRepresentation,
        }
    }
}

impl From<qos_policy::DataRepresentationId> for DataRepresentationId {
    fn from(internal: qos_policy::DataRepresentationId) -> Self {
        match internal {
            qos_policy::DataRepresentationId::XcdrDataRepresentation => {
                Self::XcdrDataRepresentation
            }
            qos_policy::DataRepresentationId::XmlDataRepresentation => Self::XmlDataRepresentation,
            qos_policy::DataRepresentationId::Xcdr2DataRepresentation => {
                Self::Xcdr2DataRepresentation
            }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum TypeConsistencyKind {
    DisallowTypeCoercion,
    #[default]
    AllowTypeCoercion,
}

impl From<TypeConsistencyKind> for qos_policy::TypeConsistencyKind {
    fn from(external: TypeConsistencyKind) -> Self {
        match external {
            TypeConsistencyKind::DisallowTypeCoercion => Self::DisallowTypeCoercion,
            TypeConsistencyKind::AllowTypeCoercion => Self::AllowTypeCoercion,
        }
    }
}

impl From<qos_policy::TypeConsistencyKind> for TypeConsistencyKind {
    fn from(internal: qos_policy::TypeConsistencyKind) -> Self {
        match internal {
            qos_policy::TypeConsistencyKind::DisallowTypeCoercion => Self::DisallowTypeCoercion,
            qos_policy::TypeConsistencyKind::AllowTypeCoercion => Self::AllowTypeCoercion,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TypeConsistencyEnforcementQosPolicy {
    pub(crate) kind: TypeConsistencyKind,
    pub(crate) ignore_sequence_bounds: bool,
    pub(crate) ignore_string_bounds: bool,
    pub(crate) ignore_member_names: bool,
    pub(crate) prevent_type_widening: bool,
    pub(crate) force_type_validation: bool,
}

impl From<TypeConsistencyEnforcementQosPolicy> for qos_policy::TypeConsistencyEnforcementQosPolicy {
    fn from(external: TypeConsistencyEnforcementQosPolicy) -> Self {
        Self {
            kind: external.kind.into(),
            ignore_sequence_bounds: external.ignore_sequence_bounds,
            ignore_string_bounds: external.ignore_string_bounds,
            ignore_member_names: external.ignore_member_names,
            prevent_type_widening: external.prevent_type_widening,
            force_type_validation: external.force_type_validation,
        }
    }
}

impl From<qos_policy::TypeConsistencyEnforcementQosPolicy> for TypeConsistencyEnforcementQosPolicy {
    fn from(internal: qos_policy::TypeConsistencyEnforcementQosPolicy) -> Self {
        Self {
            kind: internal.kind.into(),
            ignore_sequence_bounds: internal.ignore_sequence_bounds,
            ignore_string_bounds: internal.ignore_string_bounds,
            ignore_member_names: internal.ignore_member_names,
            prevent_type_widening: internal.prevent_type_widening,
            force_type_validation: internal.force_type_validation,
        }
    }
}
