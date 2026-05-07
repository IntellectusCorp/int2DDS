//! FFI-compatible status structures for DDS entity state changes
//!
//! This module provides C-compatible status structures that mirror the
//! Rust DDS status types. These structures use #[repr(C)] to ensure
//! proper memory layout for FFI boundary crossing.

use int2dds::infrastructure::qos_policy::QosPolicyId;
use int2dds::infrastructure::status::{
    LivelinessChangedStatus, LivelinessLostStatus, OfferedDeadlineMissedStatus,
    OfferedIncompatibleQosStatus, PublicationMatchedStatus, RequestedDeadlineMissedStatus,
    RequestedIncompatibleQosStatus, SampleLostStatus, SampleRejectedStatus,
    SampleRejectedStatusKind, SubscriptionMatchedStatus,
};

// ============================================================================
// QoS Policy ID
// ============================================================================

/// C-compatible QoS policy ID enum
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Int2DdsQosPolicyId {
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
    Property = 25,
}

impl From<QosPolicyId> for Int2DdsQosPolicyId {
    fn from(policy_id: QosPolicyId) -> Self {
        match policy_id {
            QosPolicyId::Invalid => Self::Invalid,
            QosPolicyId::UserData => Self::UserData,
            QosPolicyId::Durability => Self::Durability,
            QosPolicyId::Presentation => Self::Presentation,
            QosPolicyId::Deadline => Self::Deadline,
            QosPolicyId::LatencyBudget => Self::LatencyBudget,
            QosPolicyId::Ownership => Self::Ownership,
            QosPolicyId::OwnershipStrength => Self::OwnershipStrength,
            QosPolicyId::Liveliness => Self::Liveliness,
            QosPolicyId::TimeBasedFilter => Self::TimeBasedFilter,
            QosPolicyId::Partition => Self::Partition,
            QosPolicyId::Reliability => Self::Reliability,
            QosPolicyId::DestinationOrder => Self::DestinationOrder,
            QosPolicyId::History => Self::History,
            QosPolicyId::ResourceLimits => Self::ResourceLimits,
            QosPolicyId::EntityFactory => Self::EntityFactory,
            QosPolicyId::WriterDataLifecycle => Self::WriterDataLifecycle,
            QosPolicyId::ReaderDataLifecycle => Self::ReaderDataLifecycle,
            QosPolicyId::TopicData => Self::TopicData,
            QosPolicyId::GroupData => Self::GroupData,
            QosPolicyId::TransportPriority => Self::TransportPriority,
            QosPolicyId::Lifespan => Self::Lifespan,
            QosPolicyId::DurabilityService => Self::DurabilityService,
            QosPolicyId::DataRepresentation => Self::DataRepresentation,
            QosPolicyId::TypeConsistencyEnforcement => Self::TypeConsistencyEnforcement,
            QosPolicyId::Property => Self::Property,
        }
    }
}

// ============================================================================
// Publication Matched Status
// ============================================================================

/// C-compatible publication matched status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsPublicationMatchedStatus {
    /// Total cumulative count of DataReaders that matched
    pub total_count: i32,
    /// Change in total_count since last access
    pub total_count_change: i32,
    /// Current number of matched DataReaders
    pub current_count: i32,
    /// Change in current_count since last access
    pub current_count_change: i32,
    /// Handle of the last matched DataReader
    pub last_subscription_handle: [u8; 16],
}

impl From<&PublicationMatchedStatus> for Int2DdsPublicationMatchedStatus {
    fn from(status: &PublicationMatchedStatus) -> Self {
        Self {
            total_count: status.total_count(),
            total_count_change: status.total_count_change(),
            current_count: status.current_count(),
            current_count_change: status.current_count_change(),
            last_subscription_handle: *status.last_subscription_handle().value(),
        }
    }
}

// ============================================================================
// Subscription Matched Status
// ============================================================================

/// C-compatible subscription matched status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsSubscriptionMatchedStatus {
    /// Total cumulative count of DataWriters that matched
    pub total_count: i32,
    /// Change in total_count since last access
    pub total_count_change: i32,
    /// Current number of matched DataWriters
    pub current_count: i32,
    /// Change in current_count since last access
    pub current_count_change: i32,
    /// Handle of the last matched DataWriter
    pub last_publication_handle: [u8; 16],
}

impl From<&SubscriptionMatchedStatus> for Int2DdsSubscriptionMatchedStatus {
    fn from(status: &SubscriptionMatchedStatus) -> Self {
        Self {
            total_count: status.total_count(),
            total_count_change: status.total_count_change(),
            current_count: status.current_count(),
            current_count_change: status.current_count_change(),
            last_publication_handle: *status.last_publication_handle().value(),
        }
    }
}

// ============================================================================
// Sample Rejected Status
// ============================================================================

/// C-compatible sample rejected status kind
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Int2DdsSampleRejectedStatusKind {
    NotRejected = 0,
    RejectedByInstancesLimit = 1,
    RejectedBySamplesLimit = 2,
    RejectedBySamplesPerInstanceLimit = 3,
}

impl From<SampleRejectedStatusKind> for Int2DdsSampleRejectedStatusKind {
    fn from(kind: SampleRejectedStatusKind) -> Self {
        match kind {
            SampleRejectedStatusKind::NotRejected => Self::NotRejected,
            SampleRejectedStatusKind::RejectedByInstancesLimit => Self::RejectedByInstancesLimit,
            SampleRejectedStatusKind::RejectedBySamplesLimit => Self::RejectedBySamplesLimit,
            SampleRejectedStatusKind::RejectedBySamplesPerInstanceLimit => {
                Self::RejectedBySamplesPerInstanceLimit
            }
        }
    }
}

/// C-compatible sample rejected status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsSampleRejectedStatus {
    /// Total cumulative count of samples rejected
    pub total_count: i32,
    /// Change in total_count since last access
    pub total_count_change: i32,
    /// Reason for last sample rejection
    pub last_reason: Int2DdsSampleRejectedStatusKind,
    /// Handle of the instance for the last rejected sample
    pub last_instance_handle: [u8; 16],
}

impl From<&SampleRejectedStatus> for Int2DdsSampleRejectedStatus {
    fn from(status: &SampleRejectedStatus) -> Self {
        Self {
            total_count: status.total_count(),
            total_count_change: status.total_count_change(),
            last_reason: status.last_reason().into(),
            last_instance_handle: *status.last_instance_handle().value(),
        }
    }
}

// ============================================================================
// Liveliness Changed Status
// ============================================================================

/// C-compatible liveliness changed status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsLivelinessChangedStatus {
    /// Current count of alive DataWriters
    pub alive_count: i32,
    /// Current count of not-alive DataWriters
    pub not_alive_count: i32,
    /// Change in alive_count since last access
    pub alive_count_change: i32,
    /// Change in not_alive_count since last access
    pub not_alive_count_change: i32,
    /// Handle of the last DataWriter whose liveliness changed
    pub last_publication_handle: [u8; 16],
}

impl From<&LivelinessChangedStatus> for Int2DdsLivelinessChangedStatus {
    fn from(status: &LivelinessChangedStatus) -> Self {
        Self {
            alive_count: status.alive_count(),
            not_alive_count: status.not_alive_count(),
            alive_count_change: status.alive_count_change(),
            not_alive_count_change: status.not_alive_count_change(),
            last_publication_handle: *status.last_publication_handle().value(),
        }
    }
}

// ============================================================================
// Requested Deadline Missed Status
// ============================================================================

/// C-compatible requested deadline missed status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsRequestedDeadlineMissedStatus {
    /// Total cumulative count of missed deadlines
    pub total_count: i32,
    /// Change in total_count since last access
    pub total_count_change: i32,
    /// Handle of the last instance for which deadline was missed
    pub last_instance_handle: [u8; 16],
}

impl From<&RequestedDeadlineMissedStatus> for Int2DdsRequestedDeadlineMissedStatus {
    fn from(status: &RequestedDeadlineMissedStatus) -> Self {
        Self {
            total_count: status.total_count(),
            total_count_change: status.total_count_change(),
            last_instance_handle: *status.last_instance_handle().value(),
        }
    }
}

// ============================================================================
// Requested Incompatible QoS Status
// ============================================================================

/// C-compatible QoS policy count
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsQosPolicyCount {
    /// QoS policy ID
    pub policy_id: Int2DdsQosPolicyId,
    /// Count of incompatibilities for this policy
    pub count: i32,
}

/// C-compatible requested incompatible QoS status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsRequestedIncompatibleQosStatus {
    /// Total cumulative count of incompatible QoS
    pub total_count: i32,
    /// Change in total_count since last access
    pub total_count_change: i32,
    /// ID of the last incompatible policy
    pub last_policy_id: Int2DdsQosPolicyId,
    /// Count of policies (always 0 for now, policies list not exposed)
    pub policies_count: u32,
}

impl From<&RequestedIncompatibleQosStatus> for Int2DdsRequestedIncompatibleQosStatus {
    fn from(status: &RequestedIncompatibleQosStatus) -> Self {
        Self {
            total_count: status.total_count(),
            total_count_change: status.total_count_change(),
            last_policy_id: status.last_policy_id().into(),
            policies_count: 0, // Policies list not exposed in Phase 1
        }
    }
}

// ============================================================================
// Sample Lost Status
// ============================================================================

/// C-compatible sample lost status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsSampleLostStatus {
    /// Total cumulative count of samples lost
    pub total_count: i32,
    /// Change in total_count since last access
    pub total_count_change: i32,
}

impl From<&SampleLostStatus> for Int2DdsSampleLostStatus {
    fn from(status: &SampleLostStatus) -> Self {
        Self { total_count: status.total_count(), total_count_change: status.total_count_change() }
    }
}

// ============================================================================
// Offered Deadline Missed Status
// ============================================================================

/// C-compatible offered deadline missed status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsOfferedDeadlineMissedStatus {
    /// Total cumulative count of missed deadlines
    pub total_count: i32,
    /// Change in total_count since last access
    pub total_count_change: i32,
    /// Handle of the last instance for which deadline was missed
    pub last_instance_handle: [u8; 16],
}

impl From<&OfferedDeadlineMissedStatus> for Int2DdsOfferedDeadlineMissedStatus {
    fn from(status: &OfferedDeadlineMissedStatus) -> Self {
        Self {
            total_count: status.total_count(),
            total_count_change: status.total_count_change(),
            last_instance_handle: *status.last_instance_handle().value(),
        }
    }
}

// ============================================================================
// Offered Incompatible QoS Status
// ============================================================================

/// C-compatible offered incompatible QoS status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsOfferedIncompatibleQosStatus {
    /// Total cumulative count of incompatible QoS
    pub total_count: i32,
    /// Change in total_count since last access
    pub total_count_change: i32,
    /// ID of the last incompatible policy
    pub last_policy_id: Int2DdsQosPolicyId,
    /// Count of policies (always 0 for now, policies list not exposed)
    pub policies_count: u32,
}

impl From<&OfferedIncompatibleQosStatus> for Int2DdsOfferedIncompatibleQosStatus {
    fn from(status: &OfferedIncompatibleQosStatus) -> Self {
        Self {
            total_count: status.total_count(),
            total_count_change: status.total_count_change(),
            last_policy_id: status.last_policy_id().into(),
            policies_count: 0, // Policies list not exposed in Phase 1
        }
    }
}

// ============================================================================
// Liveliness Lost Status
// ============================================================================

/// C-compatible liveliness lost status
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Int2DdsLivelinessLostStatus {
    /// Total cumulative count of times liveliness was lost
    pub total_count: i32,
    /// Change in total_count since last access
    pub total_count_change: i32,
}

impl From<&LivelinessLostStatus> for Int2DdsLivelinessLostStatus {
    fn from(status: &LivelinessLostStatus) -> Self {
        Self { total_count: status.total_count(), total_count_change: status.total_count_change() }
    }
}
