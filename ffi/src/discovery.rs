//! # Discovery API
//!
//! FFI functions for discovering remote DDS participants, publications,
//! and subscriptions. Provides handle enumeration, builtin topic data
//! retrieval, and field getters for opaque builtin topic data types.

use crate::error::*;
use crate::types::{Int2DdsDataReader, Int2DdsDataWriter, Int2DdsParticipant};
use std::collections::HashMap;
use std::time::{Duration as StdDuration, Instant};

use int2dds::common::{
    builtin::topic::{
        participant_builtin_topic_data::ParticipantBuiltinTopicData,
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    },
    instance_handle::InstanceHandle,
};
use int2dds::core::time::Duration;
use int2dds::infrastructure::qos_policy::{DurabilityQosPolicyKind, ReliabilityQosPolicyKind};
use int2dds::infrastructure::status::StatusMask;
use int2dds::infrastructure::wait_set::WaitSet;
use int2dds::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};

// ============================================================================
// Opaque builtin topic data types
// ============================================================================

pub struct Int2DdsParticipantBuiltinTopicData {
    pub(crate) inner: ParticipantBuiltinTopicData,
}

pub struct Int2DdsPublicationBuiltinTopicData {
    pub(crate) inner: PublicationBuiltinTopicData,
}

pub struct Int2DdsSubscriptionBuiltinTopicData {
    pub(crate) inner: SubscriptionBuiltinTopicData,
}

pub struct Int2DdsPublicationBuiltinTopicDataSeq {
    pub(crate) items: Vec<PublicationBuiltinTopicData>,
}

pub struct Int2DdsSubscriptionBuiltinTopicDataSeq {
    pub(crate) items: Vec<SubscriptionBuiltinTopicData>,
}

unsafe impl Send for Int2DdsParticipantBuiltinTopicData {}
unsafe impl Sync for Int2DdsParticipantBuiltinTopicData {}
unsafe impl Send for Int2DdsPublicationBuiltinTopicData {}
unsafe impl Sync for Int2DdsPublicationBuiltinTopicData {}
unsafe impl Send for Int2DdsSubscriptionBuiltinTopicData {}
unsafe impl Sync for Int2DdsSubscriptionBuiltinTopicData {}
unsafe impl Send for Int2DdsPublicationBuiltinTopicDataSeq {}
unsafe impl Sync for Int2DdsPublicationBuiltinTopicDataSeq {}
unsafe impl Send for Int2DdsSubscriptionBuiltinTopicDataSeq {}
unsafe impl Sync for Int2DdsSubscriptionBuiltinTopicDataSeq {}

fn collect_publication_snapshot(
    participant: &Int2DdsParticipant,
    timeout_ms: i32,
) -> Result<Vec<PublicationBuiltinTopicData>, Int2DdsRet> {
    let builtin_subscriber =
        participant.inner.get_builtin_subscriber().map_err(|e| dds_error_to_code(&e))?;
    let publication_reader = builtin_subscriber
        .lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication")
        .map_err(|e| dds_error_to_code(&e))?;

    let mut condition =
        publication_reader.get_statuscondition().map_err(|e| dds_error_to_code(&e))?.clone();
    let _ = condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE);
    let wait_set = WaitSet::new();
    let _ = wait_set.attach_condition(condition);

    let deadline = if timeout_ms < 0 {
        None
    } else {
        Some(Instant::now() + StdDuration::from_millis(timeout_ms as u64))
    };

    let mut by_guid: HashMap<_, PublicationBuiltinTopicData> = HashMap::new();
    loop {
        let _ = publication_reader.get_status_changes();
        if let Ok(samples) = publication_reader.read(
            1000,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        ) {
            for sample in samples.iter() {
                if let Ok(data) = sample.data() {
                    by_guid.insert(data.endpoint_guid(), data);
                }
            }
        }

        match deadline {
            None => {
                if !by_guid.is_empty() {
                    break;
                }
                let _ = wait_set.wait(Duration { sec: 0, nanosec: 200_000_000 });
            }
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                let remaining = deadline.saturating_duration_since(now);
                let wait_ms = remaining.as_millis().min(200) as i32;
                let _ = wait_set.wait(Duration {
                    sec: wait_ms / 1000,
                    nanosec: ((wait_ms % 1000) as u32) * 1_000_000,
                });
            }
        }
    }

    Ok(by_guid.into_values().collect())
}

fn collect_subscription_snapshot(
    participant: &Int2DdsParticipant,
    timeout_ms: i32,
) -> Result<Vec<SubscriptionBuiltinTopicData>, Int2DdsRet> {
    let builtin_subscriber =
        participant.inner.get_builtin_subscriber().map_err(|e| dds_error_to_code(&e))?;
    let subscription_reader = builtin_subscriber
        .lookup_datareader::<SubscriptionBuiltinTopicData>("DCPSSubscription")
        .map_err(|e| dds_error_to_code(&e))?;

    let mut condition =
        subscription_reader.get_statuscondition().map_err(|e| dds_error_to_code(&e))?.clone();
    let _ = condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE);
    let wait_set = WaitSet::new();
    let _ = wait_set.attach_condition(condition);

    let deadline = if timeout_ms < 0 {
        None
    } else {
        Some(Instant::now() + StdDuration::from_millis(timeout_ms as u64))
    };

    let mut by_guid: HashMap<_, SubscriptionBuiltinTopicData> = HashMap::new();
    loop {
        let _ = subscription_reader.get_status_changes();
        if let Ok(samples) = subscription_reader.read(
            1000,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        ) {
            for sample in samples.iter() {
                if let Ok(data) = sample.data() {
                    by_guid.insert(data.endpoint_guid(), data);
                }
            }
        }

        match deadline {
            None => {
                if !by_guid.is_empty() {
                    break;
                }
                let _ = wait_set.wait(Duration { sec: 0, nanosec: 200_000_000 });
            }
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                let remaining = deadline.saturating_duration_since(now);
                let wait_ms = remaining.as_millis().min(200) as i32;
                let _ = wait_set.wait(Duration {
                    sec: wait_ms / 1000,
                    nanosec: ((wait_ms % 1000) as u32) * 1_000_000,
                });
            }
        }
    }

    Ok(by_guid.into_values().collect())
}

// ============================================================================
// Helper: convert BuiltinTopicKey ([i32; 3]) to [u8; 12] in big-endian
// ============================================================================

fn builtin_topic_key_to_bytes(value: &[i32; 3]) -> [u8; 12] {
    let mut out = [0u8; 12];
    out[0..4].copy_from_slice(&value[0].to_be_bytes());
    out[4..8].copy_from_slice(&value[1].to_be_bytes());
    out[8..12].copy_from_slice(&value[2].to_be_bytes());
    out
}

/// Copy a Rust string into a caller-provided C buffer as null-terminated UTF-8.
/// Returns the required size (including null terminator) in `size_out`.
/// If `capacity` is too small, copies up to `capacity - 1` bytes plus null.
unsafe fn copy_string_to_c(
    s: &str,
    buf: *mut u8,
    capacity: usize,
    size_out: *mut usize,
) -> Int2DdsRet {
    let needed = s.len() + 1; // including null terminator
    if !size_out.is_null() {
        *size_out = needed;
    }
    if buf.is_null() || capacity == 0 {
        return INT2DDS_RET_OK;
    }
    let copy_len = std::cmp::min(s.len(), capacity - 1);
    std::ptr::copy_nonoverlapping(s.as_ptr(), buf, copy_len);
    *buf.add(copy_len) = 0; // null terminator
    INT2DDS_RET_OK
}

// ============================================================================
// Handle list functions
// ============================================================================

/// Get discovered participant instance handles.
///
/// Writes up to `capacity` handles into `handles_out`.
/// `count_out` receives the total number of discovered participants
/// (may be greater than `capacity`).
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_discovered_participants(
    participant: *const Int2DdsParticipant,
    handles_out: *mut [u8; 16],
    capacity: usize,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(count_out);

    let participant_ref = &*participant;
    let handles = ffi_try!(participant_ref.inner.get_discovered_participants());

    *count_out = handles.len();
    if !handles_out.is_null() {
        let copy_count = std::cmp::min(handles.len(), capacity);
        for i in 0..copy_count {
            *handles_out.add(i) = *handles[i].value();
        }
    }

    INT2DDS_RET_OK
}

/// Get matched subscription instance handles for a DataWriter.
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_matched_subscriptions(
    writer: *const Int2DdsDataWriter,
    handles_out: *mut [u8; 16],
    capacity: usize,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(count_out);

    let writer_ref = &*writer;
    let handles = ffi_try!(writer_ref.inner.get_matched_subscriptions());

    *count_out = handles.len();
    if !handles_out.is_null() {
        let copy_count = std::cmp::min(handles.len(), capacity);
        for i in 0..copy_count {
            *handles_out.add(i) = *handles[i].value();
        }
    }

    INT2DDS_RET_OK
}

/// Get matched publication instance handles for a DataReader.
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_matched_publications(
    reader: *const Int2DdsDataReader,
    handles_out: *mut [u8; 16],
    capacity: usize,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(count_out);

    let reader_ref = &*reader;
    let handles = ffi_try!(reader_ref.inner.get_matched_publications());

    *count_out = handles.len();
    if !handles_out.is_null() {
        let copy_count = std::cmp::min(handles.len(), capacity);
        for i in 0..copy_count {
            *handles_out.add(i) = *handles[i].value();
        }
    }

    INT2DDS_RET_OK
}

/// Get the number of discovered publications currently known to a participant.
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_discovered_publication_count(
    participant: *const Int2DdsParticipant,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(count_out);

    let participant_ref = &*participant;
    let publications = ffi_try!(participant_ref.inner.get_discovered_publications());
    *count_out = publications.len();
    INT2DDS_RET_OK
}

/// Get discovered publication data by stable snapshot index.
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_discovered_publication_data_by_index(
    participant: *const Int2DdsParticipant,
    index: usize,
    data_out: *mut *mut Int2DdsPublicationBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(data_out);

    let participant_ref = &*participant;
    let publications = ffi_try!(participant_ref.inner.get_discovered_publications());
    let data = match publications.get(index) {
        Some(value) => value.clone(),
        None => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };

    *data_out = Box::into_raw(Box::new(Int2DdsPublicationBuiltinTopicData { inner: data }));
    INT2DDS_RET_OK
}

/// Get the number of discovered subscriptions currently known to a participant.
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_discovered_subscription_count(
    participant: *const Int2DdsParticipant,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(count_out);

    let participant_ref = &*participant;
    let subscriptions = ffi_try!(participant_ref.inner.get_discovered_subscriptions());
    *count_out = subscriptions.len();
    INT2DDS_RET_OK
}

/// Get discovered subscription data by stable snapshot index.
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_discovered_subscription_data_by_index(
    participant: *const Int2DdsParticipant,
    index: usize,
    data_out: *mut *mut Int2DdsSubscriptionBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(data_out);

    let participant_ref = &*participant;
    let subscriptions = ffi_try!(participant_ref.inner.get_discovered_subscriptions());
    let data = match subscriptions.get(index) {
        Some(value) => value.clone(),
        None => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };

    *data_out = Box::into_raw(Box::new(Int2DdsSubscriptionBuiltinTopicData { inner: data }));
    INT2DDS_RET_OK
}

/// Collect a snapshot of discovered publications via the builtin DCPSPublication reader.
#[no_mangle]
pub unsafe extern "C" fn int2dds_take_discovered_publications_snapshot(
    participant: *const Int2DdsParticipant,
    timeout_ms: i32,
    seq_out: *mut *mut Int2DdsPublicationBuiltinTopicDataSeq,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(seq_out);

    let participant_ref = &*participant;
    let items = match collect_publication_snapshot(participant_ref, timeout_ms) {
        Ok(items) => items,
        Err(ret) => return ret,
    };

    *seq_out = Box::into_raw(Box::new(Int2DdsPublicationBuiltinTopicDataSeq { items }));
    INT2DDS_RET_OK
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_seq_len(
    seq: *const Int2DdsPublicationBuiltinTopicDataSeq,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(count_out);
    *count_out = (*seq).items.len();
    INT2DDS_RET_OK
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_seq_get(
    seq: *const Int2DdsPublicationBuiltinTopicDataSeq,
    index: usize,
    data_out: *mut *mut Int2DdsPublicationBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(data_out);
    let data = match (&(*seq).items).get(index) {
        Some(item) => item.clone(),
        None => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };
    *data_out = Box::into_raw(Box::new(Int2DdsPublicationBuiltinTopicData { inner: data }));
    INT2DDS_RET_OK
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_seq_destroy(
    seq: *mut Int2DdsPublicationBuiltinTopicDataSeq,
) -> Int2DdsRet {
    check_null!(seq);
    drop(Box::from_raw(seq));
    INT2DDS_RET_OK
}

/// Collect a snapshot of discovered subscriptions via the builtin DCPSSubscription reader.
#[no_mangle]
pub unsafe extern "C" fn int2dds_take_discovered_subscriptions_snapshot(
    participant: *const Int2DdsParticipant,
    timeout_ms: i32,
    seq_out: *mut *mut Int2DdsSubscriptionBuiltinTopicDataSeq,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(seq_out);

    let participant_ref = &*participant;
    let items = match collect_subscription_snapshot(participant_ref, timeout_ms) {
        Ok(items) => items,
        Err(ret) => return ret,
    };

    *seq_out = Box::into_raw(Box::new(Int2DdsSubscriptionBuiltinTopicDataSeq { items }));
    INT2DDS_RET_OK
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_seq_len(
    seq: *const Int2DdsSubscriptionBuiltinTopicDataSeq,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(count_out);
    *count_out = (*seq).items.len();
    INT2DDS_RET_OK
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_seq_get(
    seq: *const Int2DdsSubscriptionBuiltinTopicDataSeq,
    index: usize,
    data_out: *mut *mut Int2DdsSubscriptionBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(data_out);
    let data = match (&(*seq).items).get(index) {
        Some(item) => item.clone(),
        None => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };
    *data_out = Box::into_raw(Box::new(Int2DdsSubscriptionBuiltinTopicData { inner: data }));
    INT2DDS_RET_OK
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_seq_destroy(
    seq: *mut Int2DdsSubscriptionBuiltinTopicDataSeq,
) -> Int2DdsRet {
    check_null!(seq);
    drop(Box::from_raw(seq));
    INT2DDS_RET_OK
}

// ============================================================================
// BuiltinTopicData retrieval functions
// ============================================================================

/// Get discovered participant data for a given handle.
/// On success, `*data_out` receives a heap-allocated opaque pointer.
/// The caller must free it with `int2dds_participant_builtin_topic_data_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_discovered_participant_data(
    participant: *const Int2DdsParticipant,
    handle: *const [u8; 16],
    data_out: *mut *mut Int2DdsParticipantBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(handle);
    check_null!(data_out);

    let participant_ref = &*participant;
    let instance_handle = InstanceHandle::new(*handle);
    let data = ffi_try!(participant_ref.inner.get_discovered_participant_data(instance_handle));

    *data_out = Box::into_raw(Box::new(Int2DdsParticipantBuiltinTopicData { inner: data }));
    INT2DDS_RET_OK
}

/// Get matched subscription data for a given handle.
/// On success, `*data_out` receives a heap-allocated opaque pointer.
/// The caller must free it with `int2dds_subscription_builtin_topic_data_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_matched_subscription_data(
    writer: *const Int2DdsDataWriter,
    handle: *const [u8; 16],
    data_out: *mut *mut Int2DdsSubscriptionBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(handle);
    check_null!(data_out);

    let writer_ref = &*writer;
    let instance_handle = InstanceHandle::new(*handle);
    let data = ffi_try!(writer_ref.inner.get_matched_subscription_data(instance_handle));

    *data_out = Box::into_raw(Box::new(Int2DdsSubscriptionBuiltinTopicData { inner: data }));
    INT2DDS_RET_OK
}

/// Get matched publication data for a given handle.
/// On success, `*data_out` receives a heap-allocated opaque pointer.
/// The caller must free it with `int2dds_publication_builtin_topic_data_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_matched_publication_data(
    reader: *const Int2DdsDataReader,
    handle: *const [u8; 16],
    data_out: *mut *mut Int2DdsPublicationBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(handle);
    check_null!(data_out);

    let reader_ref = &*reader;
    let instance_handle = InstanceHandle::new(*handle);
    let data = ffi_try!(reader_ref.inner.get_matched_publication_data(instance_handle));

    *data_out = Box::into_raw(Box::new(Int2DdsPublicationBuiltinTopicData { inner: data }));
    INT2DDS_RET_OK
}

// ============================================================================
// ParticipantBuiltinTopicData getters + destroy
// ============================================================================

/// Get the key from a ParticipantBuiltinTopicData.
/// `key_out` must point to a 12-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_builtin_topic_data_get_key(
    data: *const Int2DdsParticipantBuiltinTopicData,
    key_out: *mut [u8; 12],
) -> Int2DdsRet {
    check_null!(data);
    check_null!(key_out);

    let data_ref = &*data;
    *key_out = builtin_topic_key_to_bytes(&data_ref.inner.key().value);
    INT2DDS_RET_OK
}

/// Get the user_data from a ParticipantBuiltinTopicData.
/// Copies up to `capacity` bytes into `buf`. `size_out` receives the actual size.
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_builtin_topic_data_get_user_data(
    data: *const Int2DdsParticipantBuiltinTopicData,
    buf: *mut u8,
    capacity: usize,
    size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(size_out);

    let data_ref = &*data;
    let user_data = &data_ref.inner.user_data().value;

    *size_out = user_data.len();
    if !buf.is_null() && capacity > 0 {
        let copy_len = std::cmp::min(user_data.len(), capacity);
        std::ptr::copy_nonoverlapping(user_data.as_ptr(), buf, copy_len);
    }

    INT2DDS_RET_OK
}

/// Free a ParticipantBuiltinTopicData obtained from discovery.
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_builtin_topic_data_destroy(
    data: *mut Int2DdsParticipantBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(data);
    drop(Box::from_raw(data));
    INT2DDS_RET_OK
}

// ============================================================================
// PublicationBuiltinTopicData getters + destroy
// ============================================================================

/// Get the key from a PublicationBuiltinTopicData.
/// `key_out` must point to a 12-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_key(
    data: *const Int2DdsPublicationBuiltinTopicData,
    key_out: *mut [u8; 12],
) -> Int2DdsRet {
    check_null!(data);
    check_null!(key_out);

    let data_ref = &*data;
    *key_out = builtin_topic_key_to_bytes(&data_ref.inner.key().value);
    INT2DDS_RET_OK
}

/// Get the endpoint GUID from a PublicationBuiltinTopicData.
/// `guid_out` must point to a 16-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_endpoint_guid(
    data: *const Int2DdsPublicationBuiltinTopicData,
    guid_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(data);
    check_null!(guid_out);

    let data_ref = &*data;
    *guid_out = data_ref.inner.endpoint_guid().to_bytes();
    INT2DDS_RET_OK
}

/// Get the participant key from a PublicationBuiltinTopicData.
/// `key_out` must point to a 12-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_participant_key(
    data: *const Int2DdsPublicationBuiltinTopicData,
    key_out: *mut [u8; 12],
) -> Int2DdsRet {
    check_null!(data);
    check_null!(key_out);

    let data_ref = &*data;
    *key_out = builtin_topic_key_to_bytes(&data_ref.inner.participant_key().value);
    INT2DDS_RET_OK
}

/// Get the topic name from a PublicationBuiltinTopicData.
/// Copies a null-terminated UTF-8 string into `buf`.
/// `size_out` receives the required size (including null terminator).
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_topic_name(
    data: *const Int2DdsPublicationBuiltinTopicData,
    buf: *mut u8,
    capacity: usize,
    size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(data);

    let data_ref = &*data;
    copy_string_to_c(&data_ref.inner.topic_name(), buf, capacity, size_out)
}

/// Get the type name from a PublicationBuiltinTopicData.
/// Copies a null-terminated UTF-8 string into `buf`.
/// `size_out` receives the required size (including null terminator).
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_type_name(
    data: *const Int2DdsPublicationBuiltinTopicData,
    buf: *mut u8,
    capacity: usize,
    size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(data);

    let data_ref = &*data;
    copy_string_to_c(&data_ref.inner.type_name(), buf, capacity, size_out)
}

/// Get the reliability kind from a PublicationBuiltinTopicData.
/// `kind_out`: 0 = BEST_EFFORT, 1 = RELIABLE (matches INT2DDS_QOS_RELIABILITY_*).
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `kind_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_reliability_kind(
    data: *const Int2DdsPublicationBuiltinTopicData,
    kind_out: *mut i32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(kind_out);
    let data_ref = &*data;
    *kind_out = match data_ref.inner.reliability().kind {
        ReliabilityQosPolicyKind::BestEffort => 0,
        ReliabilityQosPolicyKind::Reliable => 1,
    };
    INT2DDS_RET_OK
}

/// Get the durability kind from a PublicationBuiltinTopicData.
/// `kind_out`: 0 = VOLATILE, 1 = TRANSIENT_LOCAL, 2 = TRANSIENT, 3 = PERSISTENT.
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `kind_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_durability_kind(
    data: *const Int2DdsPublicationBuiltinTopicData,
    kind_out: *mut i32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(kind_out);
    let data_ref = &*data;
    *kind_out = match data_ref.inner.durability().kind {
        DurabilityQosPolicyKind::Volatile => 0,
        DurabilityQosPolicyKind::TransientLocal => 1,
        DurabilityQosPolicyKind::Transient => 2,
        DurabilityQosPolicyKind::Persistent => 3,
    };
    INT2DDS_RET_OK
}

/// Free a PublicationBuiltinTopicData obtained from discovery.
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_destroy(
    data: *mut Int2DdsPublicationBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(data);
    drop(Box::from_raw(data));
    INT2DDS_RET_OK
}

// ============================================================================
// SubscriptionBuiltinTopicData getters + destroy
// ============================================================================

/// Get the key from a SubscriptionBuiltinTopicData.
/// `key_out` must point to a 12-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_key(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    key_out: *mut [u8; 12],
) -> Int2DdsRet {
    check_null!(data);
    check_null!(key_out);

    let data_ref = &*data;
    *key_out = builtin_topic_key_to_bytes(&data_ref.inner.key().value);
    INT2DDS_RET_OK
}

/// Get the endpoint GUID from a SubscriptionBuiltinTopicData.
/// `guid_out` must point to a 16-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_endpoint_guid(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    guid_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(data);
    check_null!(guid_out);

    let data_ref = &*data;
    *guid_out = data_ref.inner.endpoint_guid().to_bytes();
    INT2DDS_RET_OK
}

/// Get the participant key from a SubscriptionBuiltinTopicData.
/// `key_out` must point to a 12-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_participant_key(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    key_out: *mut [u8; 12],
) -> Int2DdsRet {
    check_null!(data);
    check_null!(key_out);

    let data_ref = &*data;
    *key_out = builtin_topic_key_to_bytes(&data_ref.inner.participant_key().value);
    INT2DDS_RET_OK
}

/// Get the topic name from a SubscriptionBuiltinTopicData.
/// Copies a null-terminated UTF-8 string into `buf`.
/// `size_out` receives the required size (including null terminator).
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_topic_name(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    buf: *mut u8,
    capacity: usize,
    size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(data);

    let data_ref = &*data;
    copy_string_to_c(&data_ref.inner.topic_name(), buf, capacity, size_out)
}

/// Get the type name from a SubscriptionBuiltinTopicData.
/// Copies a null-terminated UTF-8 string into `buf`.
/// `size_out` receives the required size (including null terminator).
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_type_name(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    buf: *mut u8,
    capacity: usize,
    size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(data);

    let data_ref = &*data;
    copy_string_to_c(&data_ref.inner.type_name(), buf, capacity, size_out)
}

/// Get the reliability kind from a SubscriptionBuiltinTopicData.
/// `kind_out`: 0 = BEST_EFFORT, 1 = RELIABLE (matches INT2DDS_QOS_RELIABILITY_*).
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `kind_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_reliability_kind(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    kind_out: *mut i32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(kind_out);
    let data_ref = &*data;
    *kind_out = match data_ref.inner.reliability().kind {
        ReliabilityQosPolicyKind::BestEffort => 0,
        ReliabilityQosPolicyKind::Reliable => 1,
    };
    INT2DDS_RET_OK
}

/// Get the durability kind from a SubscriptionBuiltinTopicData.
/// `kind_out`: 0 = VOLATILE, 1 = TRANSIENT_LOCAL, 2 = TRANSIENT, 3 = PERSISTENT.
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `kind_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_durability_kind(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    kind_out: *mut i32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(kind_out);
    let data_ref = &*data;
    *kind_out = match data_ref.inner.durability().kind {
        DurabilityQosPolicyKind::Volatile => 0,
        DurabilityQosPolicyKind::TransientLocal => 1,
        DurabilityQosPolicyKind::Transient => 2,
        DurabilityQosPolicyKind::Persistent => 3,
    };
    INT2DDS_RET_OK
}

/// Free a SubscriptionBuiltinTopicData obtained from discovery.
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_destroy(
    data: *mut Int2DdsSubscriptionBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(data);
    drop(Box::from_raw(data));
    INT2DDS_RET_OK
}
