//! Quality of Service (QoS) policies for DDS entities.
//!
//! This module defines all QoS policy types that control the behavior of DDS entities.
//! QoS policies determine aspects such as reliability, durability, history depth,
//! deadlines, liveliness, resource limits, and more.
//!
//! QoS policies are configured when creating entities and can be modified (if mutable)
//! using `set_qos()` methods.
//!
//! # Supported QoS Policies
//!
//! | Policy | Description | Applicable to |
//! |--------|-------------|---------------|
//! | [`DurabilityQosPolicy`] | Historical data for late joiners (Volatile, TransientLocal) | DataWriter, DataReader, Topic |
//! | [`DeadlineQosPolicy`] | Maximum time between updates | DataWriter, DataReader, Topic |
//! | [`OwnershipQosPolicy`] | Instance ownership (Shared, Exclusive) | DataWriter, DataReader, Topic |
//! | [`OwnershipStrengthQosPolicy`] | Exclusive ownership priority | DataWriter |
//! | [`LivelinessQosPolicy`] | Writer aliveness assertion | DataWriter, DataReader, Topic |
//! | [`PartitionQosPolicy`] | Logical communication groups | Publisher, Subscriber |
//! | [`ReliabilityQosPolicy`] | BestEffort or Reliable delivery | DataWriter, DataReader, Topic |
//! | [`DestinationOrderQosPolicy`] | Sample ordering strategy | DataWriter, DataReader, Topic |
//! | [`HistoryQosPolicy`] | Sample storage (KeepLast, KeepAll) | DataWriter, DataReader, Topic |
//! | [`ResourceLimitsQosPolicy`] | Memory limits for samples/instances | DataWriter, DataReader, Topic |
//! | [`EntityFactoryQosPolicy`] | Manual entity enabling | DomainParticipantFactory, DomainParticipant, Publisher, Subscriber |
//! | [`LifespanQosPolicy`] | Sample expiration duration | DataWriter, Topic |
//! | [`DataRepresentationQosPolicy`] | Data encoding (XCDR1, XCDR2) | DataWriter, DataReader, Topic |
//!
//! # Unsupported QoS Policies
//!
//! The following QoS policies are defined for compatibility but not yet implemented:
//!
//! | Policy | Description | Applicable to |
//! |--------|-------------|---------------|
//! | [`UserDataQosPolicy`] | Arbitrary user data attached to entity | DomainParticipant, DataWriter, DataReader |
//! | [`PresentationQosPolicy`] | Coherent/ordered access for change groups | Publisher, Subscriber |
//! | [`LatencyBudgetQosPolicy`] | Acceptable delivery delay hint | DataWriter, DataReader, Topic |
//! | [`TransportPriorityQosPolicy`] | Transport priority for delivery | DataWriter, Topic |
//! | [`TimeBasedFilterQosPolicy`] | Minimum separation between samples | DataReader |
//! | [`WriterDataLifecycleQosPolicy`] | Auto-disposal of unregistered instances | DataWriter |
//! | [`ReaderDataLifecycleQosPolicy`] | Auto-purge of disposed samples | DataReader |
//! | [`TopicDataQosPolicy`] | Arbitrary data attached to Topic | Topic |
//! | [`GroupDataQosPolicy`] | Arbitrary data attached to Publisher/Subscriber | Publisher, Subscriber |
//! | [`DurabilityServiceQosPolicy`] | Transient/Persistent service config | DataWriter, Topic |

use const_default::ConstDefault;
use serde::{Deserialize, Serialize};
use speedy::{Readable, Writable};

use crate::{
    core::{
        error::DdsResult,
        time::Duration,
        types::{deserialize_i32_or_unlimited, serialize_i32_or_unlimited, LENGTH_UNLIMITED},
    },
    serialize::cdr::serializer::primitive::PrimitiveSerialize,
    topic::type_support::DdsType,
};

pub trait QosPolicy {
    fn name(&self) -> &str;
}
pub(crate) trait Qos: Default + ConstDefault {
    fn check_unsupported_policies(&self) -> DdsResult<()> {
        Ok(())
    }

    fn check_immutable_change(&self, _new_qos: &Self) -> DdsResult<()> {
        Ok(())
    }

    fn is_consistent(&self) -> DdsResult<()> {
        Ok(())
    }

    fn autoenable_created_entities(&self) -> bool {
        false
    }
} // for Parameters

const USERDATA_QOS_POLICY_NAME: &str = "UserData";
const DURABILITY_QOS_POLICY_NAME: &str = "Durability";
const PRESENTATION_QOS_POLICY_NAME: &str = "Presentation";
const DEADLINE_QOS_POLICY_NAME: &str = "Deadline";
const LATENCYBUDGET_QOS_POLICY_NAME: &str = "LatencyBudget";
const OWNERSHIP_QOS_POLICY_NAME: &str = "Ownership";
const OWNERSHIP_STRENGTH_QOS_POLICY_NAME: &str = "OwnershipStrength";
const LIVELINESS_QOS_POLICY_NAME: &str = "Liveliness";
const TIMEBASEDFILTER_QOS_POLICY_NAME: &str = "TimeBasedFilter";
const PARTITION_QOS_POLICY_NAME: &str = "Partition";
const RELIABILITY_QOS_POLICY_NAME: &str = "Reliability";
const DESTINATIONORDER_QOS_POLICY_NAME: &str = "DestinationOrder";
const HISTORY_QOS_POLICY_NAME: &str = "History";
const RESOURCELIMITS_QOS_POLICY_NAME: &str = "ResourceLimits";
const ENTITYFACTORY_QOS_POLICY_NAME: &str = "EntityFactory";
const WRITERDATALIFECYCLE_QOS_POLICY_NAME: &str = "WriterDataLifecycle";
const READERDATALIFECYCLE_QOS_POLICY_NAME: &str = "ReaderDataLifecycle";
const TOPICDATA_QOS_POLICY_NAME: &str = "TopicData";
const TRANSPORTPRIORITY_QOS_POLICY_NAME: &str = "TransportPriority";
const GROUPDATA_QOS_POLICY_NAME: &str = "GroupData";
const LIFESPAN_QOS_POLICY_NAME: &str = "Lifespan";
const DURABILITYSERVICE_QOS_POLICY_NAME: &str = "DurabilityService";
const DATAREPRESENTATION_QOS_POLICY_NAME: &str = "DataRepresentation";
const TYPECONSISTENCYENFORCEMENT_QOS_POLICY_NAME: &str = "TypeConsistencyEnforcement";
const RELIABILITY_EXTENSION_QOS_POLICY_NAME: &str = "ReliabilityExtension";

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Readable, Writable)]
pub enum QosPolicyId {
    #[default]
    Invalid = 0,
    UserData = 1,
    Durability = 2,
    Presentation = 3,
    Deadline = 4,
    LatencyBudget = 5,
    Ownership = 6,
    OwnershipStrength = 7,
    Liveliness = 8,
    TimeBasedFilter = 9,
    Partition = 10,
    Reliability = 11,
    DestinationOrder = 12,
    History = 13,
    ResourceLimits = 14,
    EntityFactory = 15,
    WriterDataLifecycle = 16,
    ReaderDataLifecycle = 17,
    TopicData = 18,
    GroupData = 19,
    TransportPriority = 20,
    Lifespan = 21,
    DurabilityService = 22,
    DataRepresentation = 23,
    TypeConsistencyEnforcement = 24,
}

impl QosPolicyId {
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }

    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(QosPolicyId::Invalid),
            1 => Some(QosPolicyId::UserData),
            2 => Some(QosPolicyId::Durability),
            3 => Some(QosPolicyId::Presentation),
            4 => Some(QosPolicyId::Deadline),
            5 => Some(QosPolicyId::LatencyBudget),
            6 => Some(QosPolicyId::Ownership),
            7 => Some(QosPolicyId::OwnershipStrength),
            8 => Some(QosPolicyId::Liveliness),
            9 => Some(QosPolicyId::TimeBasedFilter),
            10 => Some(QosPolicyId::Partition),
            11 => Some(QosPolicyId::Reliability),
            12 => Some(QosPolicyId::DestinationOrder),
            13 => Some(QosPolicyId::History),
            14 => Some(QosPolicyId::ResourceLimits),
            15 => Some(QosPolicyId::EntityFactory),
            16 => Some(QosPolicyId::WriterDataLifecycle),
            17 => Some(QosPolicyId::ReaderDataLifecycle),
            18 => Some(QosPolicyId::TopicData),
            19 => Some(QosPolicyId::GroupData),
            20 => Some(QosPolicyId::TransportPriority),
            21 => Some(QosPolicyId::Lifespan),
            22 => Some(QosPolicyId::DurabilityService),
            23 => Some(QosPolicyId::DataRepresentation),
            24 => Some(QosPolicyId::TypeConsistencyEnforcement),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            QosPolicyId::Invalid => "Invalid",
            QosPolicyId::UserData => USERDATA_QOS_POLICY_NAME,
            QosPolicyId::Durability => DURABILITY_QOS_POLICY_NAME,
            QosPolicyId::Presentation => PRESENTATION_QOS_POLICY_NAME,
            QosPolicyId::Deadline => DEADLINE_QOS_POLICY_NAME,
            QosPolicyId::LatencyBudget => LATENCYBUDGET_QOS_POLICY_NAME,
            QosPolicyId::Ownership => OWNERSHIP_QOS_POLICY_NAME,
            QosPolicyId::OwnershipStrength => OWNERSHIP_STRENGTH_QOS_POLICY_NAME,
            QosPolicyId::Liveliness => LIVELINESS_QOS_POLICY_NAME,
            QosPolicyId::TimeBasedFilter => TIMEBASEDFILTER_QOS_POLICY_NAME,
            QosPolicyId::Partition => PARTITION_QOS_POLICY_NAME,
            QosPolicyId::Reliability => RELIABILITY_QOS_POLICY_NAME,
            QosPolicyId::DestinationOrder => DESTINATIONORDER_QOS_POLICY_NAME,
            QosPolicyId::History => HISTORY_QOS_POLICY_NAME,
            QosPolicyId::ResourceLimits => RESOURCELIMITS_QOS_POLICY_NAME,
            QosPolicyId::EntityFactory => ENTITYFACTORY_QOS_POLICY_NAME,
            QosPolicyId::WriterDataLifecycle => WRITERDATALIFECYCLE_QOS_POLICY_NAME,
            QosPolicyId::ReaderDataLifecycle => READERDATALIFECYCLE_QOS_POLICY_NAME,
            QosPolicyId::TopicData => TOPICDATA_QOS_POLICY_NAME,
            QosPolicyId::GroupData => GROUPDATA_QOS_POLICY_NAME,
            QosPolicyId::TransportPriority => TRANSPORTPRIORITY_QOS_POLICY_NAME,
            QosPolicyId::Lifespan => LIFESPAN_QOS_POLICY_NAME,
            QosPolicyId::DurabilityService => DURABILITYSERVICE_QOS_POLICY_NAME,
            QosPolicyId::DataRepresentation => DATAREPRESENTATION_QOS_POLICY_NAME,
            QosPolicyId::TypeConsistencyEnforcement => TYPECONSISTENCYENFORCEMENT_QOS_POLICY_NAME,
        }
    }
}

/// Specifies the history storage strategy for samples.
///
/// - `KeepLast(depth)`: Store only the last `depth` samples per instance
/// - `KeepAll`: Store all samples until resource limits are reached
#[derive(DdsType, PartialEq, Copy, Eq)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub enum HistoryQosPolicyKind {
    /// Keep only the last N samples per instance, where N is the depth value.
    KeepLast(i32),
    /// Keep all samples until resource limits are reached.
    KeepAll,
}

impl Default for HistoryQosPolicyKind {
    fn default() -> Self {
        Self::KeepLast(1)
    }
}

impl ConstDefault for HistoryQosPolicyKind {
    const DEFAULT: Self = HistoryQosPolicyKind::KeepLast(1);
}

impl HistoryQosPolicyKind {
    pub fn depth(&self) -> Option<i32> {
        match self {
            HistoryQosPolicyKind::KeepLast(depth) => Some(*depth),
            HistoryQosPolicyKind::KeepAll => None,
        }
    }
}

/// Controls how many samples are stored per instance.
///
/// This policy controls the behavior of the middleware when the value of an instance
/// changes before it is finally communicated to some of its existing DataReaders.
///
/// # Default
/// `KeepLast(1)` - Only the most recent sample per instance is kept.
///
/// # Example
/// ```no_run
/// use int2dds::{
///     infrastructure::{
///         qos_policy::{HistoryQosPolicy, HistoryQosPolicyKind, ResourceLimitsQosPolicy},
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // Keep last 10 samples per instance
/// let writer_qos_keep_last = DataWriterQos {
///     history: HistoryQosPolicy {
///         kind: HistoryQosPolicyKind::KeepLast(10),
///     },
///     ..Default::default()
/// };
///
/// // Keep all samples with resource limits
/// let _writer_qos_keep_all = DataWriterQos {
///     history: HistoryQosPolicy {
///         kind: HistoryQosPolicyKind::KeepAll,
///     },
///     resource_limits: ResourceLimitsQosPolicy {
///         max_samples: 1000,
///         max_instances: 100,
///         max_samples_per_instance: 100,
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos_keep_last, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, Copy, Eq)]
#[dds_type(crate_path = "crate")]
pub struct HistoryQosPolicy {
    /// The history storage strategy.
    pub kind: HistoryQosPolicyKind,
}

impl ConstDefault for HistoryQosPolicy {
    const DEFAULT: Self = Self { kind: HistoryQosPolicyKind::DEFAULT };
}

impl QosPolicy for HistoryQosPolicy {
    fn name(&self) -> &str {
        HISTORY_QOS_POLICY_NAME
    }
}

impl HistoryQosPolicy {
    pub fn depth(&self) -> Option<i32> {
        self.kind.depth()
    }
}

/// Specifies the maximum duration for which data samples are valid.
///
/// Expired samples are automatically removed by the middleware. This is useful for
/// time-sensitive data that becomes stale after a certain period.
///
/// # Default
/// `Duration::INFINITE` - Samples never expire.
///
/// # Example
/// ```no_run
/// use int2dds::{
///     core::time::Duration,
///     infrastructure::{
///         qos_policy::LifespanQosPolicy,
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // Samples expire after 60 seconds
/// let writer_qos = DataWriterQos {
///     lifespan: LifespanQosPolicy {
///         duration: Duration::from_seconds(60),
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate", no_default)]
pub struct LifespanQosPolicy {
    /// Maximum validity duration for samples.
    #[serde(default)]
    pub duration: Duration,
}
impl Default for LifespanQosPolicy {
    fn default() -> Self {
        Self {
            duration: Duration { sec: Duration::INFINITE_SEC, nanosec: Duration::INFINITE_NSEC },
        }
    }
}

impl ConstDefault for LifespanQosPolicy {
    const DEFAULT: Self = Self {
        duration: Duration { sec: Duration::INFINITE_SEC, nanosec: Duration::INFINITE_NSEC },
    };
}

impl QosPolicy for LifespanQosPolicy {
    fn name(&self) -> &str {
        LIFESPAN_QOS_POLICY_NAME
    }
}

/// Specifies the ownership strategy for instances.
///
/// - `Shared`: Multiple DataWriters can update the same instance (default)
/// - `Exclusive`: Only the DataWriter with highest strength owns the instance
#[derive(DdsType, PartialEq, Default, Copy, Eq)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub enum OwnershipQosPolicyKind {
    /// Multiple DataWriters can update the same instance simultaneously.
    #[default]
    Shared,
    /// Only one DataWriter (with highest ownership strength) can update the instance.
    Exclusive,
}

impl ConstDefault for OwnershipQosPolicyKind {
    const DEFAULT: Self = OwnershipQosPolicyKind::Shared;
}

impl OwnershipQosPolicyKind {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Shared),
            1 => Some(Self::Exclusive),
            _ => None,
        }
    }
}

/// Controls instance ownership when multiple DataWriters write to the same instance.
///
/// This QoS policy is RxO (requested/offered) and immutable after entity creation.
///
/// # Default
/// `Shared` - Multiple writers can update the same instance.
///
/// # Example
/// ```no_run
/// use int2dds::{
///     infrastructure::{
///         qos_policy::{OwnershipQosPolicy, OwnershipQosPolicyKind},
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
///     subscription::qos::{DataReaderQos, SubscriberQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // Exclusive ownership - only the strongest writer owns an instance
/// let writer_qos = DataWriterQos {
///     ownership: OwnershipQosPolicy {
///         kind: OwnershipQosPolicyKind::Exclusive,
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
///     .unwrap();
///
/// // DataReader must also use Exclusive ownership to match
/// let subscriber = participant
///     .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// let reader_qos = DataReaderQos {
///     ownership: OwnershipQosPolicy {
///         kind: OwnershipQosPolicyKind::Exclusive,
///     },
///     ..Default::default()
/// };
///
/// let _reader = subscriber
///     .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, ConstDefault, Copy, Eq)]
#[dds_type(crate_path = "crate")]
pub struct OwnershipQosPolicy {
    /// The ownership strategy.
    pub kind: OwnershipQosPolicyKind,
}

impl QosPolicy for OwnershipQosPolicy {
    fn name(&self) -> &str {
        OWNERSHIP_QOS_POLICY_NAME
    }
}

/// Specifies the strength value for Exclusive ownership arbitration.
///
/// When `OwnershipQosPolicy::Exclusive` is used, the DataWriter with the
/// highest ownership strength value wins ownership of the instance.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// `0`
#[derive(DdsType, ConstDefault, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate")]
#[serde(default)]
pub struct OwnershipStrengthQosPolicy {
    /// The ownership strength value. Higher values win ownership.
    pub value: i32,
}
impl QosPolicy for OwnershipStrengthQosPolicy {
    fn name(&self) -> &str {
        OWNERSHIP_STRENGTH_QOS_POLICY_NAME
    }
}

/// Controls automatic disposal of instances when unregistered by DataWriter.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// `autodispose_unregistered_instances: true`
#[derive(DdsType, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate", no_default)]
pub struct WriterDataLifecycleQosPolicy {
    /// Whether to automatically dispose instances when unregistered.
    #[serde(default)]
    pub autodispose_unregistered_instances: bool,
}

impl Default for WriterDataLifecycleQosPolicy {
    fn default() -> Self {
        Self { autodispose_unregistered_instances: true }
    }
}

impl ConstDefault for WriterDataLifecycleQosPolicy {
    const DEFAULT: Self = Self { autodispose_unregistered_instances: true };
}

impl QosPolicy for WriterDataLifecycleQosPolicy {
    fn name(&self) -> &str {
        WRITERDATALIFECYCLE_QOS_POLICY_NAME
    }
}

/// Controls automatic purging of samples from disposed or no-writer instances.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// Both delays are `Duration::INFINITE` - samples are never automatically purged.
#[derive(DdsType, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate", no_default)]
pub struct ReaderDataLifecycleQosPolicy {
    /// Delay before purging samples from instances with no writers.
    #[serde(default)]
    pub autopurge_nowriter_samples_delay: Duration,
    /// Delay before purging samples from disposed instances.
    #[serde(default)]
    pub autopurge_disposed_samples_delay: Duration,
}

impl Default for ReaderDataLifecycleQosPolicy {
    fn default() -> Self {
        Self {
            autopurge_nowriter_samples_delay: Duration {
                sec: Duration::INFINITE_SEC,
                nanosec: Duration::INFINITE_NSEC,
            },
            autopurge_disposed_samples_delay: Duration {
                sec: Duration::INFINITE_SEC,
                nanosec: Duration::INFINITE_NSEC,
            },
        }
    }
}

impl ConstDefault for ReaderDataLifecycleQosPolicy {
    const DEFAULT: Self = Self {
        autopurge_nowriter_samples_delay: Duration {
            sec: Duration::INFINITE_SEC,
            nanosec: Duration::INFINITE_NSEC,
        },
        autopurge_disposed_samples_delay: Duration {
            sec: Duration::INFINITE_SEC,
            nanosec: Duration::INFINITE_NSEC,
        },
    };
}

impl QosPolicy for ReaderDataLifecycleQosPolicy {
    fn name(&self) -> &str {
        READERDATALIFECYCLE_QOS_POLICY_NAME
    }
}

/// Specifies the access scope for coherent and ordered access.
///
/// **Note**: This QoS policy is currently unsupported.
#[derive(DdsType, PartialEq, Default, Copy, Eq, PartialOrd, Ord)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub enum PresentationQosAccessScopeKind {
    /// Changes are coherent/ordered at instance level.
    #[default]
    Instance,
    /// Changes are coherent/ordered at topic level.
    Topic,
    /// Changes are coherent/ordered at group level.
    Group,
}

impl ConstDefault for PresentationQosAccessScopeKind {
    const DEFAULT: Self = PresentationQosAccessScopeKind::Instance;
}

impl PresentationQosAccessScopeKind {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Instance),
            1 => Some(Self::Topic),
            2 => Some(Self::Group),
            _ => None,
        }
    }
}

/// Controls coherent access and ordered access for groups of changes.
///
/// This QoS policy is RxO (requested/offered) and immutable after entity creation.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// - `access_scope`: Instance
/// - `coherent_access`: false
/// - `ordered_access`: false
#[derive(DdsType, ConstDefault, Copy, Eq)]
#[dds_type(crate_path = "crate")]
pub struct PresentationQosPolicy {
    /// The scope for coherent/ordered access.
    pub access_scope: PresentationQosAccessScopeKind,
    /// Whether coherent access is enabled.
    pub coherent_access: bool,
    /// Whether ordered access is enabled.
    pub ordered_access: bool,
}

impl QosPolicy for PresentationQosPolicy {
    fn name(&self) -> &str {
        PRESENTATION_QOS_POLICY_NAME
    }
}

/// Specifies transport priority for data delivery.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// `0`
#[derive(DdsType, ConstDefault, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate")]
#[serde(default)]
pub struct TransportPriorityQosPolicy {
    /// The transport priority value.
    pub value: i32,
}

impl QosPolicy for TransportPriorityQosPolicy {
    fn name(&self) -> &str {
        TRANSPORTPRIORITY_QOS_POLICY_NAME
    }
}

/// Attaches arbitrary user data to an entity for application-specific purposes.
///
/// The data is propagated via discovery and can be used for application-level
/// identification or configuration sharing.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// Empty byte vector.
#[derive(DdsType, ConstDefault, Eq)]
#[dds_type(crate_path = "crate")]
pub struct UserDataQosPolicy {
    /// Arbitrary user-defined data.
    pub value: Vec<u8>,
}

impl QosPolicy for UserDataQosPolicy {
    fn name(&self) -> &str {
        USERDATA_QOS_POLICY_NAME
    }
}

/// Attaches arbitrary data to a Topic, propagated via discovery.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// Empty byte vector.
#[derive(DdsType, ConstDefault, Eq)]
#[dds_type(crate_path = "crate")]
pub struct TopicDataQosPolicy {
    /// Arbitrary topic-specific data.
    pub value: Vec<u8>,
}

impl QosPolicy for TopicDataQosPolicy {
    fn name(&self) -> &str {
        TOPICDATA_QOS_POLICY_NAME
    }
}

/// Attaches arbitrary data to Publisher/Subscriber, propagated via discovery.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// Empty byte vector.
#[derive(DdsType, ConstDefault, Eq)]
#[dds_type(crate_path = "crate")]
pub struct GroupDataQosPolicy {
    /// Arbitrary group-specific data.
    pub value: Vec<u8>,
}

impl QosPolicy for GroupDataQosPolicy {
    fn name(&self) -> &str {
        GROUPDATA_QOS_POLICY_NAME
    }
}

/// Specifies the acceptable delay for data delivery.
///
/// This is a hint to the middleware for optimization purposes. The middleware
/// may use this to batch messages or delay delivery within the budget.
///
/// This QoS policy is RxO (requested/offered).
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// `Duration::ZERO`
#[derive(DdsType, ConstDefault, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate")]
#[serde(default)]
pub struct LatencyBudgetQosPolicy {
    /// Maximum acceptable delay for data delivery.
    pub duration: Duration,
}

impl QosPolicy for LatencyBudgetQosPolicy {
    fn name(&self) -> &str {
        LATENCYBUDGET_QOS_POLICY_NAME
    }
}

/// Specifies the maximum expected time between data updates.
///
/// If the deadline is missed, the corresponding listener callback
/// (`on_offered_deadline_missed` or `on_requested_deadline_missed`) is triggered.
///
/// This QoS policy is RxO (requested/offered).
///
/// # Default
/// `Duration::INFINITE` - No deadline.
///
/// # Example
/// ```no_run
/// use int2dds::{
///     core::time::Duration,
///     infrastructure::{
///         qos_policy::DeadlineQosPolicy,
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
///     subscription::qos::{DataReaderQos, SubscriberQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// // Publisher side: commit to publishing every 1 second
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// let writer_qos = DataWriterQos {
///     deadline: DeadlineQosPolicy {
///         period: Duration::from_seconds(1),
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
///     .unwrap();
///
/// // Subscriber side: expect updates every 2 seconds (more lenient)
/// let subscriber = participant
///     .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// let reader_qos = DataReaderQos {
///     deadline: DeadlineQosPolicy {
///         period: Duration::from_seconds(2),
///     },
///     ..Default::default()
/// };
///
/// let _reader = subscriber
///     .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate", no_default)]
#[serde(default)]
pub struct DeadlineQosPolicy {
    /// Maximum expected period between data updates.
    pub period: Duration,
}

impl Default for DeadlineQosPolicy {
    fn default() -> Self {
        Self { period: Duration { sec: Duration::INFINITE_SEC, nanosec: Duration::INFINITE_NSEC } }
    }
}

impl ConstDefault for DeadlineQosPolicy {
    const DEFAULT: Self =
        Self { period: Duration { sec: Duration::INFINITE_SEC, nanosec: Duration::INFINITE_NSEC } };
}

impl QosPolicy for DeadlineQosPolicy {
    fn name(&self) -> &str {
        DEADLINE_QOS_POLICY_NAME
    }
}

/// Filters samples based on minimum separation time between updates.
///
/// DataReader will only receive samples at the specified minimum interval,
/// filtering out samples that arrive too quickly.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// `Duration::ZERO` - No filtering.
#[derive(DdsType, ConstDefault, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate")]
#[serde(default)]
pub struct TimeBasedFilterQosPolicy {
    /// Minimum time between received samples.
    pub minimum_separation: Duration,
}

impl QosPolicy for TimeBasedFilterQosPolicy {
    fn name(&self) -> &str {
        TIMEBASEDFILTER_QOS_POLICY_NAME
    }
}

/// Controls whether created entities are automatically enabled.
///
/// When `autoenable_created_entities` is false, entities must be manually
/// enabled by calling `enable()` before they can participate in communication.
///
/// # Default
/// `autoenable_created_entities: true`
///
/// # Example
/// ```no_run
/// use int2dds::{
///     infrastructure::{
///         qos_policy::EntityFactoryQosPolicy,
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // Disable auto-enable for this publisher's created entities
/// let mut publisher_qos = publisher.get_qos().unwrap();
/// publisher_qos.entity_factory = EntityFactoryQosPolicy {
///     autoenable_created_entities: false,
/// };
/// publisher.set_qos(publisher_qos).unwrap();
///
/// // DataWriter is created but not enabled - no discovery announcement yet
/// let writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, DataWriterQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // Perform setup operations before enabling...
///
/// // Manually enable when ready to communicate
/// writer.enable().unwrap();
/// ```
#[derive(DdsType, Copy, Eq, Deserialize, Serialize)]
#[dds_type(crate_path = "crate", no_default)]
#[serde(default)]
pub struct EntityFactoryQosPolicy {
    /// Whether created entities are automatically enabled.
    pub autoenable_created_entities: bool,
}

impl Default for EntityFactoryQosPolicy {
    fn default() -> Self {
        Self { autoenable_created_entities: true }
    }
}

impl ConstDefault for EntityFactoryQosPolicy {
    const DEFAULT: Self = Self { autoenable_created_entities: true };
}

impl QosPolicy for EntityFactoryQosPolicy {
    fn name(&self) -> &str {
        ENTITYFACTORY_QOS_POLICY_NAME
    }
}

/// Controls which entities can communicate by grouping them into logical partitions.
///
/// Entities only communicate if they share at least one matching partition name.
/// Partition names support wildcard patterns using POSIX fnmatch syntax (e.g., "sensor/*").
///
/// By default, entities belong to a single partition with an empty string name ("").
///
/// # Default
/// Empty vector (belongs to default "" partition).
///
/// # Example
/// ```no_run
/// use int2dds::{
///     core::time::Duration,
///     infrastructure::{
///         qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos, PUBLISHER_QOS_DEFAULT},
///     subscription::qos::SubscriberQos,
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// // Publisher in partitions A, B, C
/// let mut publisher_qos = PUBLISHER_QOS_DEFAULT;
/// publisher_qos.partition.name.push("partition_A".to_string());
/// publisher_qos.partition.name.push("partition_B".to_string());
/// publisher_qos.partition.name.push("partition_C".to_string());
///
/// let publisher = participant
///     .create_publisher(publisher_qos, None, StatusMask::default())
///     .unwrap();
///
/// let writer_qos = DataWriterQos {
///     reliability: ReliabilityQosPolicy {
///         kind: ReliabilityQosPolicyKind::Reliable,
///         max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
///     .unwrap();
///
/// // Subscriber in partition A only - will match with publisher
/// let mut subscriber_qos = SubscriberQos::default();
/// subscriber_qos.partition.name.push("partition_A".to_string());
///
/// let _subscriber = participant
///     .create_subscriber(subscriber_qos, None, StatusMask::default())
///     .unwrap();
///
/// // Subscriber using wildcard pattern
/// let mut subscriber_qos_wildcard = SubscriberQos::default();
/// subscriber_qos_wildcard.partition.name.push("partition_*".to_string());
///
/// let _subscriber_wildcard = participant
///     .create_subscriber(subscriber_qos_wildcard, None, StatusMask::default())
///     .unwrap();
/// ```
///
/// # Pattern Matching
/// - Concrete names (e.g., "sensors/temperature") match exactly
/// - Regular expressions (e.g., "sensors/*") match against concrete names
/// - Two entities match if they share at least one common partition
#[derive(DdsType, ConstDefault, Eq)]
#[dds_type(crate_path = "crate")]
pub struct PartitionQosPolicy {
    /// List of partition names. Can be concrete names or wildcard patterns.
    pub name: Vec<String>,
}
impl QosPolicy for PartitionQosPolicy {
    fn name(&self) -> &str {
        PARTITION_QOS_POLICY_NAME
    }
}

/// Specifies the reliability level for data delivery.
///
/// - `BestEffort`: No delivery guarantee, lower latency, suitable for periodic data
/// - `Reliable`: Guaranteed delivery with acknowledgments and retransmission
#[derive(DdsType, PartialEq, Copy, Eq, PartialOrd, Ord)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub enum ReliabilityQosPolicyKind {
    /// Best-effort delivery - samples may be lost but latency is minimized.
    BestEffort = 1,
    /// Reliable delivery - all samples are guaranteed to be delivered.
    Reliable = 2,
}

impl ReliabilityQosPolicyKind {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::BestEffort),
            2 => Some(Self::Reliable),
            _ => None,
        }
    }
}

/// Controls whether data delivery is guaranteed (Reliable) or best-effort.
///
/// This QoS policy is RxO (requested/offered) and immutable after entity creation.
/// - DataWriter with BestEffort can only communicate with BestEffort DataReaders
/// - DataWriter with Reliable can communicate with both Reliable and BestEffort DataReaders
///
/// # Default
/// For DataWriter: `BestEffort` with 100ms max_blocking_time
/// For DataReader: `BestEffort`
///
/// # Example
/// ```no_run
/// use int2dds::{
///     core::time::Duration,
///     infrastructure::{
///         qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// let writer_qos = DataWriterQos {
///     reliability: ReliabilityQosPolicy {
///         kind: ReliabilityQosPolicyKind::Reliable,
///         max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 }, // 100ms
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, Copy, Eq)]
#[dds_type(crate_path = "crate", no_default)]
pub struct ReliabilityQosPolicy {
    /// The reliability level.
    pub kind: ReliabilityQosPolicyKind,
    /// Maximum time to block on write when resources are unavailable.
    pub max_blocking_time: Duration,
}

impl QosPolicy for ReliabilityQosPolicy {
    fn name(&self) -> &str {
        RELIABILITY_QOS_POLICY_NAME
    }
}

/// Specifies how DataWriters assert they are still alive.
///
/// - `Automatic`: Liveliness is asserted automatically by any DDS activity
/// - `ManualByParticipant`: Must call `assert_liveliness()` on DomainParticipant
/// - `ManualByTopic`: Must call `assert_liveliness()` on DataWriter
#[derive(DdsType, PartialEq, Default, Copy, Eq, PartialOrd, Ord)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub enum LivelinessQosPolicyKind {
    /// Liveliness is asserted automatically by the middleware.
    #[default]
    Automatic,
    /// Liveliness must be asserted by calling `assert_liveliness()` on DomainParticipant.
    ManualByParticipant,
    /// Liveliness must be asserted by calling `assert_liveliness()` on DataWriter.
    ManualByTopic,
}

impl ConstDefault for LivelinessQosPolicyKind {
    const DEFAULT: Self = LivelinessQosPolicyKind::Automatic;
}

impl LivelinessQosPolicyKind {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Automatic),
            1 => Some(Self::ManualByParticipant),
            2 => Some(Self::ManualByTopic),
            _ => None,
        }
    }
}

/// Specifies how DataWriters assert they are still alive.
///
/// This QoS policy is RxO (requested/offered) and immutable after entity creation.
/// If a DataWriter fails to assert liveliness within `lease_duration`, matched
/// DataReaders will be notified via `on_liveliness_changed` callback, and the
/// DataWriter will receive `on_liveliness_lost` callback.
///
/// # Default
/// - `kind`: Automatic
/// - `lease_duration`: Duration::INFINITE
///
/// # Example
/// ```no_run
/// use int2dds::{
///     core::time::Duration,
///     infrastructure::{
///         qos_policy::{LivelinessQosPolicy, LivelinessQosPolicyKind},
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
///     subscription::qos::{DataReaderQos, SubscriberQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// // Automatic liveliness with 2-second lease
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// let writer_qos = DataWriterQos {
///     liveliness: LivelinessQosPolicy {
///         kind: LivelinessQosPolicyKind::Automatic,
///         lease_duration: Duration::from_seconds(2),
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
///     .unwrap();
///
/// // DataReader requests liveliness notification within 3 seconds
/// let subscriber = participant
///     .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// let reader_qos = DataReaderQos {
///     liveliness: LivelinessQosPolicy {
///         kind: LivelinessQosPolicyKind::Automatic,
///         lease_duration: Duration::from_seconds(3), // More lenient than writer
///     },
///     ..Default::default()
/// };
///
/// let _reader = subscriber
///     .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, Copy, Eq)]
#[dds_type(crate_path = "crate", no_default)]
pub struct LivelinessQosPolicy {
    /// The liveliness assertion mechanism.
    pub kind: LivelinessQosPolicyKind,
    /// Maximum time between liveliness assertions.
    pub lease_duration: Duration,
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

impl ConstDefault for LivelinessQosPolicy {
    const DEFAULT: Self = Self {
        kind: LivelinessQosPolicyKind::DEFAULT,
        lease_duration: Duration { sec: Duration::INFINITE_SEC, nanosec: Duration::INFINITE_NSEC },
    };
}

impl QosPolicy for LivelinessQosPolicy {
    fn name(&self) -> &str {
        LIVELINESS_QOS_POLICY_NAME
    }
}

/// Specifies the durability level for historical data.
///
/// - `Volatile`: No historical data sent to late joiners (default)
/// - `TransientLocal`: Historical data kept in DataWriter memory
/// - `Transient`: Historical data kept in external service (Unsupported)
/// - `Persistent`: Historical data persisted to storage (Unsupported)
///
/// This QoS policy is RxO (requested/offered) and immutable after entity creation.
#[derive(DdsType, PartialEq, Default, Copy, Eq, PartialOrd, Ord)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub enum DurabilityQosPolicyKind {
    /// No historical data sent to late-joining DataReaders.
    #[default]
    Volatile,
    /// Historical data is kept in DataWriter memory for late joiners.
    TransientLocal,
    /// Historical data is stored in external durability service. (Unsupported)
    Transient,
    /// Historical data is persisted to non-volatile storage. (Unsupported)
    Persistent,
}

impl ConstDefault for DurabilityQosPolicyKind {
    const DEFAULT: Self = DurabilityQosPolicyKind::Volatile;
}

impl DurabilityQosPolicyKind {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Volatile),
            1 => Some(Self::TransientLocal),
            2 => Some(Self::Transient),
            3 => Some(Self::Persistent),
            _ => None,
        }
    }
}

/// Controls whether late-joining DataReaders receive historical data.
///
/// This QoS policy is RxO (requested/offered) and immutable after entity creation.
///
/// # Default
/// `Volatile` - No historical data sent to late joiners.
///
/// # Example
/// ```no_run
/// use int2dds::{
///     core::time::Duration,
///     infrastructure::{
///         qos_policy::{
///             DurabilityQosPolicy, DurabilityQosPolicyKind,
///             HistoryQosPolicy, HistoryQosPolicyKind,
///             ReliabilityQosPolicy, ReliabilityQosPolicyKind,
///         },
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // TransientLocal with Reliable and KeepAll for late joiners
/// let writer_qos = DataWriterQos {
///     reliability: ReliabilityQosPolicy {
///         kind: ReliabilityQosPolicyKind::Reliable,
///         max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
///     },
///     durability: DurabilityQosPolicy {
///         kind: DurabilityQosPolicyKind::TransientLocal,
///     },
///     history: HistoryQosPolicy {
///         kind: HistoryQosPolicyKind::KeepAll,
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
///     .unwrap();
/// ```
///
/// # Note
/// Only `Volatile` and `TransientLocal` are currently supported.
/// `Transient` and `Persistent` require an external durability service.
#[derive(DdsType, ConstDefault, Copy, Eq)]
#[dds_type(crate_path = "crate")]
pub struct DurabilityQosPolicy {
    /// The durability level.
    pub kind: DurabilityQosPolicyKind,
}

impl QosPolicy for DurabilityQosPolicy {
    fn name(&self) -> &str {
        DURABILITY_QOS_POLICY_NAME
    }
}

/// Limits memory resources for samples and instances.
///
/// This QoS policy is immutable after entity creation.
/// Use `LENGTH_UNLIMITED` (-1) for unlimited resources.
///
/// # Default
/// All limits are `LENGTH_UNLIMITED`.
///
/// # Example
/// ```no_run
/// use int2dds::{
///     infrastructure::{
///         qos_policy::{HistoryQosPolicy, HistoryQosPolicyKind, ResourceLimitsQosPolicy},
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
///     subscription::qos::{DataReaderQos, SubscriberQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // Limit DataWriter to 100 total samples across 10 instances
/// let writer_qos = DataWriterQos {
///     history: HistoryQosPolicy {
///         kind: HistoryQosPolicyKind::KeepAll,
///     },
///     resource_limits: ResourceLimitsQosPolicy {
///         max_samples: 100,
///         max_instances: 10,
///         max_samples_per_instance: 10,
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
///     .unwrap();
///
/// // Limit DataReader resources
/// let subscriber = participant
///     .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// let reader_qos = DataReaderQos {
///     resource_limits: ResourceLimitsQosPolicy {
///         max_samples: 500,
///         max_instances: 50,
///         max_samples_per_instance: 20,
///     },
///     ..Default::default()
/// };
///
/// let _reader = subscriber
///     .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, Copy, Eq, Serialize, Deserialize)]
#[dds_type(crate_path = "crate", no_default)]
#[serde(default)]
pub struct ResourceLimitsQosPolicy {
    /// Maximum total number of samples that can be stored.
    #[serde(deserialize_with = "deserialize_i32_or_unlimited")]
    #[serde(serialize_with = "serialize_i32_or_unlimited")]
    pub max_samples: i32,
    /// Maximum number of instances.
    #[serde(deserialize_with = "deserialize_i32_or_unlimited")]
    #[serde(serialize_with = "serialize_i32_or_unlimited")]
    pub max_instances: i32,
    /// Maximum number of samples per instance.
    #[serde(deserialize_with = "deserialize_i32_or_unlimited")]
    #[serde(serialize_with = "serialize_i32_or_unlimited")]
    pub max_samples_per_instance: i32,
}

impl Default for ResourceLimitsQosPolicy {
    fn default() -> Self {
        Self {
            max_instances: LENGTH_UNLIMITED,
            max_samples: LENGTH_UNLIMITED,
            max_samples_per_instance: LENGTH_UNLIMITED,
        }
    }
}

impl ConstDefault for ResourceLimitsQosPolicy {
    const DEFAULT: Self = Self {
        max_instances: LENGTH_UNLIMITED,
        max_samples: LENGTH_UNLIMITED,
        max_samples_per_instance: LENGTH_UNLIMITED,
    };
}

impl QosPolicy for ResourceLimitsQosPolicy {
    fn name(&self) -> &str {
        RESOURCELIMITS_QOS_POLICY_NAME
    }
}

/// Configures parameters for Transient/Persistent durability service.
///
/// This QoS policy is required when using `Transient` or `Persistent` durability.
/// It configures the external durability service's storage parameters.
///
/// **Note**: This QoS policy is currently unsupported.
///
/// # Default
/// - `service_cleanup_delay`: Duration::ZERO
/// - `history_kind`: KeepLast(1)
/// - All limits: LENGTH_UNLIMITED
#[derive(DdsType, Copy, Eq)]
#[dds_type(crate_path = "crate", no_default)]
pub struct DurabilityServiceQosPolicy {
    /// Delay before cleaning up stale data.
    pub service_cleanup_delay: Duration,
    /// History policy for the durability service.
    pub history_kind: HistoryQosPolicyKind,
    /// Maximum total samples in service storage.
    pub max_samples: i32,
    /// Maximum instances in service storage.
    pub max_instances: i32,
    /// Maximum samples per instance in service storage.
    pub max_samples_per_instance: i32,
}

impl Default for DurabilityServiceQosPolicy {
    fn default() -> Self {
        Self {
            service_cleanup_delay: Duration::default(),
            history_kind: HistoryQosPolicyKind::KeepLast(1),
            max_instances: LENGTH_UNLIMITED,
            max_samples: LENGTH_UNLIMITED,
            max_samples_per_instance: LENGTH_UNLIMITED,
        }
    }
}

impl ConstDefault for DurabilityServiceQosPolicy {
    const DEFAULT: Self = Self {
        service_cleanup_delay: Duration::DEFAULT,
        history_kind: HistoryQosPolicyKind::KeepLast(1),
        max_instances: LENGTH_UNLIMITED,
        max_samples: LENGTH_UNLIMITED,
        max_samples_per_instance: LENGTH_UNLIMITED,
    };
}

impl QosPolicy for DurabilityServiceQosPolicy {
    fn name(&self) -> &str {
        DURABILITYSERVICE_QOS_POLICY_NAME
    }
}

/// Specifies how samples are ordered when received from multiple DataWriters.
///
/// - `ByReceptionTimestamp`: Order by the time the sample was received (default)
/// - `BySourceTimestamp`: Order by the timestamp set by the DataWriter
#[derive(DdsType, PartialEq, Default, Copy, Eq, PartialOrd, Ord)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub enum DestinationOrderQosPolicyKind {
    /// Samples are ordered by the time they were received.
    #[default]
    ByReceptionTimestamp,
    /// Samples are ordered by the source timestamp set by the DataWriter.
    BySourceTimestamp,
}

impl ConstDefault for DestinationOrderQosPolicyKind {
    const DEFAULT: Self = DestinationOrderQosPolicyKind::ByReceptionTimestamp;
}

impl DestinationOrderQosPolicyKind {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::ByReceptionTimestamp),
            1 => Some(Self::BySourceTimestamp),
            _ => None,
        }
    }
}

/// Controls how samples are ordered when received from multiple DataWriters.
///
/// This QoS policy is RxO (requested/offered) and immutable after entity creation.
///
/// # Default
/// `ByReceptionTimestamp`
///
/// # Example
/// ```no_run
/// use int2dds::{
///     infrastructure::{
///         qos_policy::{DestinationOrderQosPolicy, DestinationOrderQosPolicyKind},
///         status::StatusMask,
///     },
///     subscription::qos::{DataReaderQos, SubscriberQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let subscriber = participant
///     .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // Order samples by source timestamp (requires synchronized clocks)
/// let reader_qos = DataReaderQos {
///     destination_order: DestinationOrderQosPolicy {
///         kind: DestinationOrderQosPolicyKind::BySourceTimestamp,
///     },
///     ..Default::default()
/// };
///
/// let _reader = subscriber
///     .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, ConstDefault, Copy, Eq)]
#[dds_type(crate_path = "crate")]
pub struct DestinationOrderQosPolicy {
    /// The ordering strategy for samples.
    pub kind: DestinationOrderQosPolicyKind,
}

impl QosPolicy for DestinationOrderQosPolicy {
    fn name(&self) -> &str {
        DESTINATIONORDER_QOS_POLICY_NAME
    }
}

/// Data representation identifiers for DDS-XTypes.
///
/// Specifies the encoding format used for data serialization.
///
/// - `XcdrDataRepresentation`: XCDR1 encoding (default, legacy)
/// - `XmlDataRepresentation`: XML encoding (Unsupported)
/// - `Xcdr2DataRepresentation`: XCDR2 encoding (recommended for new applications)
#[derive(DdsType, PartialEq, Default, Copy, Eq)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub enum DataRepresentationId {
    /// XCDR1 data representation (legacy).
    #[default]
    XcdrDataRepresentation = 0,
    /// XML data representation. (Unsupported)
    XmlDataRepresentation = 1,
    /// XCDR2 data representation (recommended).
    Xcdr2DataRepresentation = 2,
}

impl ConstDefault for DataRepresentationId {
    const DEFAULT: Self = DataRepresentationId::XcdrDataRepresentation;
}

impl DataRepresentationId {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::XcdrDataRepresentation),
            1 => Some(Self::XmlDataRepresentation),
            2 => Some(Self::Xcdr2DataRepresentation),
            _ => None,
        }
    }
}

/// Specifies the data encoding format for serialization.
///
/// This QoS policy defines which data representations are supported by the entity.
/// DataWriters and DataReaders must have at least one common representation to match.
///
/// # Default
/// Empty vector (uses XCDR1 by default).
///
/// # Example
/// ```no_run
/// use int2dds::{
///     infrastructure::{
///         qos_policy::{DataRepresentationId, DataRepresentationQosPolicy},
///         status::StatusMask,
///     },
///     publication::qos::{DataWriterQos, PublisherQos},
///     subscription::qos::{DataReaderQos, SubscriberQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let publisher = participant
///     .create_publisher(PublisherQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // Use XCDR2 encoding (recommended for new applications)
/// let writer_qos = DataWriterQos {
///     data_representation: DataRepresentationQosPolicy {
///         value: vec![DataRepresentationId::Xcdr2DataRepresentation],
///     },
///     ..Default::default()
/// };
///
/// let _writer = publisher
///     .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
///     .unwrap();
///
/// // DataReader must support the same representation
/// let subscriber = participant
///     .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// let reader_qos = DataReaderQos {
///     data_representation: DataRepresentationQosPolicy {
///         value: vec![DataRepresentationId::Xcdr2DataRepresentation],
///     },
///     ..Default::default()
/// };
///
/// let _reader = subscriber
///     .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, Eq, ConstDefault)]
#[dds_type(crate_path = "crate")]
pub struct DataRepresentationQosPolicy {
    /// List of supported data representations.
    pub value: Vec<DataRepresentationId>,
}

impl QosPolicy for DataRepresentationQosPolicy {
    fn name(&self) -> &str {
        DATAREPRESENTATION_QOS_POLICY_NAME
    }
}

/// Specifies the type consistency enforcement level for DDS-XTypes.
///
/// - `DisallowTypeCoercion`: Strict type matching required (default)
/// - `AllowTypeCoercion`: Allow compatible type coercion during deserialization
#[derive(DdsType, PartialEq, Default, Copy, Eq)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub enum TypeConsistencyKind {
    /// Strict type matching - types must be identical.
    #[default]
    DisallowTypeCoercion = 0,
    /// Allow type coercion for compatible types (e.g., adding optional fields).
    AllowTypeCoercion = 1,
}

impl ConstDefault for TypeConsistencyKind {
    const DEFAULT: Self = TypeConsistencyKind::DisallowTypeCoercion;
}

impl TypeConsistencyKind {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::DisallowTypeCoercion),
            1 => Some(Self::AllowTypeCoercion),
            _ => None,
        }
    }
}

/// Controls type consistency enforcement for DDS-XTypes.
///
/// This QoS policy determines how strictly types are matched between DataWriters
/// and DataReaders, and how the middleware handles type evolution.
///
/// **Note**: This QoS policy is defined for compatibility with DDS-XTypes specification.
/// The compatibility checking logic may be extended in future versions.
///
/// # Default
/// - `kind`: DisallowTypeCoercion
/// - All ignore flags: false
/// - `prevent_type_widening`: false
/// - `force_type_validation`: false
///
/// # Example
/// ```no_run
/// use int2dds::{
///     infrastructure::{
///         qos_policy::{TypeConsistencyEnforcementQosPolicy, TypeConsistencyKind},
///         status::StatusMask,
///     },
///     subscription::qos::{DataReaderQos, SubscriberQos},
/// #     domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
/// #     topic::{qos::TopicQos, type_support::DdsType},
/// };
/// #
/// #
/// # #[derive(DdsType)]
/// # #[dds_type(crate_path = "int2dds")]
/// # struct HelloWorldType { index: u32, message: String }
/// #
/// # let factory = DomainParticipantFactory::get_instance();
/// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
/// # let topic = participant.create_topic::<HelloWorldType>("topic", "HelloWorld", TopicQos::default(), None, StatusMask::default()).unwrap();
///
/// let subscriber = participant
///     .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
///     .unwrap();
///
/// // Allow type coercion for forward compatibility
/// let reader_qos = DataReaderQos {
///     type_consistency_enforcement: TypeConsistencyEnforcementQosPolicy {
///         kind: TypeConsistencyKind::AllowTypeCoercion,
///         ignore_sequence_bounds: true,
///         ignore_string_bounds: true,
///         ignore_member_names: false,
///         prevent_type_widening: false,
///         force_type_validation: false,
///     },
///     ..Default::default()
/// };
///
/// let _reader = subscriber
///     .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
///     .unwrap();
/// ```
#[derive(DdsType, Copy, Eq)]
#[dds_type(crate_path = "crate", no_default)]
pub struct TypeConsistencyEnforcementQosPolicy {
    /// The type consistency enforcement level.
    pub kind: TypeConsistencyKind,
    /// Ignore differences in sequence bounds when matching types.
    pub ignore_sequence_bounds: bool,
    /// Ignore differences in string bounds when matching types.
    pub ignore_string_bounds: bool,
    /// Ignore member names when matching types (use hash-based matching).
    pub ignore_member_names: bool,
    /// Prevent type widening (adding new members to received types).
    pub prevent_type_widening: bool,
    /// Force TypeObject validation even if hash matches.
    pub force_type_validation: bool,
}

impl Default for TypeConsistencyEnforcementQosPolicy {
    fn default() -> Self {
        Self {
            kind: TypeConsistencyKind::DisallowTypeCoercion,
            ignore_sequence_bounds: false,
            ignore_string_bounds: false,
            ignore_member_names: false,
            prevent_type_widening: false,
            force_type_validation: false,
        }
    }
}

impl ConstDefault for TypeConsistencyEnforcementQosPolicy {
    const DEFAULT: Self = Self {
        kind: TypeConsistencyKind::DEFAULT,
        ignore_sequence_bounds: false,
        ignore_string_bounds: false,
        ignore_member_names: false,
        prevent_type_widening: false,
        force_type_validation: false,
    };
}

impl QosPolicy for TypeConsistencyEnforcementQosPolicy {
    fn name(&self) -> &str {
        TYPECONSISTENCYENFORCEMENT_QOS_POLICY_NAME
    }
}

/// Extension to ReliabilityQosPolicy for int2DDS-specific reliability options.
/// This policy provides additional control over reliable communication behavior.
///
/// # Default
/// - `disable_piggyback_heartbeat: false` - Piggybacked heartbeats are enabled by default.
/// - `heartbeat_period: 2 seconds` - Period for sending periodic heartbeat messages.
/// - `initial_heartbeat_delay: 10ms` - Delay before sending initial heartbeat after reader discovery.
/// - `push_mode: true` - (Unsupported) Writer pushes data to readers.
/// - `nack_suppression_duration: 0` - (Unsupported) Duration to suppress NACKs.
/// - `nack_response_delay: 200ms` - (Unsupported) Delay before responding to a NACK.
#[derive(DdsType, Copy, Eq)]
#[dds_type(crate_path = "crate", no_default)]
pub struct ReliabilityExtensionQosPolicy {
    /// When `true`, heartbeat messages will not be piggybacked with DATA messages.
    /// Instead, heartbeats will only be sent via the periodic heartbeat timer.
    /// This can reduce network congestion but may increase latency for acknowledgments.
    pub disable_piggyback_heartbeat: bool,

    /// Period for sending periodic heartbeat messages.
    /// Default: 2 seconds
    pub heartbeat_period: Duration,

    /// Delay before sending initial heartbeat after reader discovery.
    /// Default: 10ms
    pub initial_heartbeat_delay: Duration,

    /// (Unsupported) When `true`, writer pushes data to readers.
    /// When `false`, reader pulls data (not implemented).
    /// Default: true
    pub push_mode: bool,

    /// (Unsupported) Duration to suppress NACKs from the same reader.
    /// Default: 0 (no suppression)
    pub nack_suppression_duration: Duration,

    /// (Unsupported) Delay before responding to a NACK.
    /// Default: 200ms
    pub nack_response_delay: Duration,
}

impl Default for ReliabilityExtensionQosPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl ConstDefault for ReliabilityExtensionQosPolicy {
    const DEFAULT: Self = Self {
        disable_piggyback_heartbeat: false,
        heartbeat_period: Duration { sec: 2, nanosec: 0 },
        initial_heartbeat_delay: Duration { sec: 0, nanosec: 10_000_000 },
        push_mode: true,
        nack_suppression_duration: Duration { sec: 0, nanosec: 0 },
        nack_response_delay: Duration { sec: 0, nanosec: 200_000_000 },
    };
}

impl QosPolicy for ReliabilityExtensionQosPolicy {
    fn name(&self) -> &str {
        RELIABILITY_EXTENSION_QOS_POLICY_NAME
    }
}
