//! Status types and masks for DDS entity state changes.
//!
//! This module defines status structures that report changes in DDS entity states.
//! Each status type corresponds to a specific event such as data availability,
//! QoS policy violations, or entity discovery. Status objects are accessed through
//! entity methods like `get_*_status()` and can trigger listener callbacks.
//!
//! Status changes are tracked using `StatusMask` bitflags, allowing selective
//! monitoring of specific status types through listeners and status conditions.

use std::any::Any;

use super::qos_policy::QosPolicyId;
use crate::common::instance_handle::InstanceHandle;
use bitflags::bitflags;

#[derive(Clone, Copy, Debug)]
pub struct QosPolicyCount {
    pub policy_id: QosPolicyId,
    pub count: i32,
}

pub type QosPolicyCountSeq = Vec<QosPolicyCount>;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StatusKind: u32 {
        const INCONSISTENT_TOPIC = 0x0001 << 0;
        const OFFERED_DEADLINE_MISSED = 0x0001 << 1;
        const REQUESTED_DEADLINE_MISSED = 0x0001 << 2;
        const OFFERED_INCOMPATIBLE_QOS = 0x0001 << 5;
        const REQUESTED_INCOMPATIBLE_QOS = 0x0001 << 6;
        const SAMPLE_LOST = 0x0001 << 7;
        const SAMPLE_REJECTED = 0x0001 << 8;
        const DATA_ON_READERS = 0x0001 << 9;
        const DATA_AVAILABLE = 0x0001 << 10;
        const LIVELINESS_LOST = 0x0001 << 11;
        const LIVELINESS_CHANGED = 0x0001 << 12;
        const PUBLICATION_MATCHED = 0x0001 << 13;
        const SUBSCRIPTION_MATCHED = 0x0001 << 14;
    }
}
pub type StatusMask = StatusKind;
impl Default for StatusMask {
    fn default() -> Self {
        Self::ALL
    }
}
impl StatusMask {
    pub const NONE: Self = StatusMask::empty();
    pub const ALL: Self = StatusKind::all();
}

pub trait StatusInfo: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct InconsistentTopicStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
}
impl StatusInfo for InconsistentTopicStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl InconsistentTopicStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SampleLostStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
}
impl StatusInfo for SampleLostStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl SampleLostStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub enum SampleRejectedStatusKind {
    #[default]
    NotRejected,
    RejectedByInstancesLimit,
    RejectedBySamplesLimit,
    RejectedBySamplesPerInstanceLimit,
}
#[derive(Debug, Default, Clone, Copy)]
pub struct SampleRejectedStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
    pub(crate) last_reason: SampleRejectedStatusKind,
    pub(crate) last_instance_handle: InstanceHandle,
}
impl StatusInfo for SampleRejectedStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl SampleRejectedStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
    pub fn last_reason(&self) -> SampleRejectedStatusKind {
        self.last_reason
    }
    pub fn last_instance_handle(&self) -> InstanceHandle {
        self.last_instance_handle
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LivelinessLostStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
}
impl StatusInfo for LivelinessLostStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl LivelinessLostStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LivelinessChangedStatus {
    pub(crate) alive_count: i32,
    pub(crate) not_alive_count: i32,
    pub(crate) alive_count_change: i32,
    pub(crate) not_alive_count_change: i32,
    pub(crate) last_publication_handle: InstanceHandle,
}
impl StatusInfo for LivelinessChangedStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl LivelinessChangedStatus {
    pub fn alive_count(&self) -> i32 {
        self.alive_count
    }
    pub fn not_alive_count(&self) -> i32 {
        self.not_alive_count
    }
    pub fn alive_count_change(&self) -> i32 {
        self.alive_count_change
    }
    pub fn not_alive_count_change(&self) -> i32 {
        self.not_alive_count_change
    }
    pub fn last_publication_handle(&self) -> InstanceHandle {
        self.last_publication_handle
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OfferedDeadlineMissedStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
    pub(crate) last_instance_handle: InstanceHandle,
}
impl StatusInfo for OfferedDeadlineMissedStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl OfferedDeadlineMissedStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
    pub fn last_instance_handle(&self) -> InstanceHandle {
        self.last_instance_handle
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RequestedDeadlineMissedStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
    pub(crate) last_instance_handle: InstanceHandle,
}

impl StatusInfo for RequestedDeadlineMissedStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl RequestedDeadlineMissedStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
    pub fn last_instance_handle(&self) -> InstanceHandle {
        self.last_instance_handle
    }
}

#[derive(Default, Clone, Debug)]
pub struct OfferedIncompatibleQosStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
    pub(crate) last_policy_id: QosPolicyId,
    pub(crate) policies: QosPolicyCountSeq,
}
impl StatusInfo for OfferedIncompatibleQosStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl OfferedIncompatibleQosStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
    pub fn last_policy_id(&self) -> QosPolicyId {
        self.last_policy_id
    }
    pub fn policies(&self) -> QosPolicyCountSeq {
        self.policies.clone()
    }
}

#[derive(Default, Clone, Debug)]
pub struct RequestedIncompatibleQosStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
    pub(crate) last_policy_id: QosPolicyId,
    pub(crate) policies: QosPolicyCountSeq,
}
impl StatusInfo for RequestedIncompatibleQosStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl RequestedIncompatibleQosStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
    pub fn last_policy_id(&self) -> QosPolicyId {
        self.last_policy_id
    }
    pub fn policies(&self) -> QosPolicyCountSeq {
        self.policies.clone()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PublicationMatchedStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
    pub(crate) current_count: i32,
    pub(crate) current_count_change: i32,
    pub(crate) last_subscription_handle: InstanceHandle,
}
impl StatusInfo for PublicationMatchedStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl PublicationMatchedStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
    pub fn current_count(&self) -> i32 {
        self.current_count
    }
    pub fn current_count_change(&self) -> i32 {
        self.current_count_change
    }
    pub fn last_subscription_handle(&self) -> InstanceHandle {
        self.last_subscription_handle
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SubscriptionMatchedStatus {
    pub(crate) total_count: i32,
    pub(crate) total_count_change: i32,
    pub(crate) current_count: i32,
    pub(crate) current_count_change: i32,
    pub(crate) last_publication_handle: InstanceHandle,
}
impl StatusInfo for SubscriptionMatchedStatus {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl SubscriptionMatchedStatus {
    pub fn total_count(&self) -> i32 {
        self.total_count
    }
    pub fn total_count_change(&self) -> i32 {
        self.total_count_change
    }
    pub fn current_count(&self) -> i32 {
        self.current_count
    }
    pub fn current_count_change(&self) -> i32 {
        self.current_count_change
    }
    pub fn last_publication_handle(&self) -> InstanceHandle {
        self.last_publication_handle
    }
}
