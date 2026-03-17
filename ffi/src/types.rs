//! # Opaque Pointer Types
//!
//! FFI-safe handle types that wrap Rust DDS structures.
//!
//! ## Overview
//!
//! All types in this module are opaque pointers for C interoperability.
//! Memory is managed through Arc reference counting on the Rust side,
//! with explicit create/delete functions exposed to C.
//!
//! ## Thread Safety
//!
//! All handle types implement Send and Sync, making them safe to use
//! across threads in both Rust and C code.

use std::sync::Arc;

use int2dds::{
    domain::domain_participant::DomainParticipant,
    infrastructure::{condition::Condition, guard_condition::GuardCondition, wait_set::WaitSet},
    publication::{data_writer::DataWriter, publisher::Publisher},
    subscription::{data_reader::DataReader, sample_info::SampleInfo, subscriber::Subscriber},
    topic::topic::Topic,
};

use crate::data::Int2DdsData;
use crate::listener::{FfiDataReaderListener, FfiDataWriterListener};

/// Opaque handle to a DomainParticipantFactory
pub struct Int2DdsParticipantFactory {
    // Currently just a placeholder for future expansion
    pub(crate) _initialized: bool,
}

/// Opaque handle to a DomainParticipant
pub struct Int2DdsParticipant {
    pub(crate) inner: Arc<DomainParticipant>,
}

/// Opaque handle to a Publisher
pub struct Int2DdsPublisher {
    pub(crate) inner: Arc<Publisher>,
}

/// Opaque handle to a Subscriber
pub struct Int2DdsSubscriber {
    pub(crate) inner: Arc<Subscriber>,
}

/// Opaque handle to a DataWriter
pub struct Int2DdsDataWriter {
    pub(crate) inner: DataWriter<Int2DdsData>,
    pub(crate) listener: Option<Arc<FfiDataWriterListener>>,
}

/// Opaque handle to a DataReader
pub struct Int2DdsDataReader {
    pub(crate) inner: DataReader<Int2DdsData>,
    pub(crate) listener: Option<Arc<FfiDataReaderListener>>,
}

/// Opaque handle to a Topic
pub struct Int2DdsTopic {
    pub(crate) inner: Arc<Topic>,
    pub(crate) type_name: String,
}

/// Opaque handle to a WaitSet
pub struct Int2DdsWaitSet {
    pub(crate) inner: Arc<WaitSet>,
}

/// Opaque handle to a GuardCondition
pub struct Int2DdsGuardCondition {
    pub(crate) inner: Arc<GuardCondition>,
}

/// Opaque handle to a StatusCondition
/// StatusCondition is wrapped as a trait object to handle the generic QoS type.
pub struct Int2DdsStatusCondition {
    pub(crate) inner: Arc<dyn Condition + Send + Sync>,
}

/// Generic condition handle for use with WaitSet
pub struct Int2DdsCondition {
    pub(crate) inner: Arc<dyn Condition + Send + Sync>,
}

/// Sequence of conditions returned from WaitSet::wait
pub struct Int2DdsConditionSeq {
    pub(crate) conditions: Vec<Arc<dyn Condition + Send + Sync>>,
}

// ============================================================================
// SampleInfo FFI type
// ============================================================================

/// FFI-safe SampleInfo returned to C callers
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Int2DdsSampleInfo {
    pub source_timestamp_sec: i32,
    pub source_timestamp_nanosec: u32,
    pub sample_state: u32,
    pub view_state: u32,
    pub instance_state: u32,
    pub instance_handle: [u8; 16],
    pub publication_handle: [u8; 16],
    pub disposed_generation_count: i32,
    pub no_writers_generation_count: i32,
    pub sample_rank: i32,
    pub generation_rank: i32,
    pub absolute_generation_rank: i32,
    pub valid_data: bool,
}

impl From<&SampleInfo> for Int2DdsSampleInfo {
    fn from(info: &SampleInfo) -> Self {
        Self {
            source_timestamp_sec: info.source_timestamp.sec,
            source_timestamp_nanosec: info.source_timestamp.nanosec,
            sample_state: info.sample_state.bits(),
            view_state: info.view_state.bits(),
            instance_state: info.instance_state.bits(),
            instance_handle: *info.instance_handle.value(),
            publication_handle: *info.publication_handle.value(),
            disposed_generation_count: info.disposed_generation_count,
            no_writers_generation_count: info.no_writers_generation_count,
            sample_rank: info.sample_rank,
            generation_rank: info.generation_rank,
            absolute_generation_rank: info.absolute_generation_rank,
            valid_data: info.valid_data,
        }
    }
}

// ============================================================================
// Sample Sequence for batch read/take
// ============================================================================

/// Opaque sequence of (serialized data, SampleInfo) pairs for batch read/take
pub struct Int2DdsSampleSeq {
    pub(crate) samples: Vec<(Arc<[u8]>, SampleInfo)>,
}

// Safety: All types use Arc which is thread-safe
unsafe impl Send for Int2DdsParticipantFactory {}
unsafe impl Sync for Int2DdsParticipantFactory {}
unsafe impl Send for Int2DdsParticipant {}
unsafe impl Sync for Int2DdsParticipant {}
unsafe impl Send for Int2DdsPublisher {}
unsafe impl Sync for Int2DdsPublisher {}
unsafe impl Send for Int2DdsSubscriber {}
unsafe impl Sync for Int2DdsSubscriber {}
unsafe impl Send for Int2DdsDataWriter {}
unsafe impl Sync for Int2DdsDataWriter {}
unsafe impl Send for Int2DdsDataReader {}
unsafe impl Sync for Int2DdsDataReader {}
unsafe impl Send for Int2DdsTopic {}
unsafe impl Sync for Int2DdsTopic {}
unsafe impl Send for Int2DdsWaitSet {}
unsafe impl Sync for Int2DdsWaitSet {}
unsafe impl Send for Int2DdsGuardCondition {}
unsafe impl Sync for Int2DdsGuardCondition {}
unsafe impl Send for Int2DdsStatusCondition {}
unsafe impl Sync for Int2DdsStatusCondition {}
unsafe impl Send for Int2DdsCondition {}
unsafe impl Sync for Int2DdsCondition {}
unsafe impl Send for Int2DdsConditionSeq {}
unsafe impl Sync for Int2DdsConditionSeq {}
unsafe impl Send for Int2DdsSampleSeq {}
unsafe impl Sync for Int2DdsSampleSeq {}
