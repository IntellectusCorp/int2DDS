//! # Discovery API
//!
//! FFI functions for discovering remote DDS participants, publications,
//! and subscriptions. Provides handle enumeration, builtin topic data
//! retrieval, and field getters for opaque builtin topic data types.

use crate::error::*;
use crate::subscriber::{
    INT2DDS_INSTANCE_STATE_ALIVE, INT2DDS_INSTANCE_STATE_NOT_ALIVE_DISPOSED,
    INT2DDS_INSTANCE_STATE_NOT_ALIVE_NO_WRITERS,
};
use crate::types::{Int2DdsDataReader, Int2DdsDataWriter, Int2DdsParticipant};
use std::collections::hash_map::Entry;
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
use int2dds::core::types::LENGTH_UNLIMITED;
use int2dds::infrastructure::qos_policy::{
    DurabilityQosPolicyKind, LivelinessQosPolicyKind, ReliabilityQosPolicyKind,
};
use int2dds::infrastructure::status::StatusMask;
use int2dds::infrastructure::wait_set::WaitSet;
use int2dds::subscription::sample_info::{
    InstanceStateKind, SampleInfo, SampleStateKind, ViewStateKind,
};
use int2dds::EndpointDiscoveryEvent;

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

/// One instance of a snapshot. `data` is absent for a dispose, which travels as a key with no
/// payload, so the handle is the only identity a departure can be acted on by.
pub(crate) struct SnapshotEntry<T> {
    pub(crate) data: Option<T>,
    pub(crate) instance_state: u32,
    pub(crate) instance_handle: [u8; 16],
}

pub struct Int2DdsPublicationBuiltinTopicDataSeq {
    pub(crate) items: Vec<SnapshotEntry<PublicationBuiltinTopicData>>,
}

pub struct Int2DdsSubscriptionBuiltinTopicDataSeq {
    pub(crate) items: Vec<SnapshotEntry<SubscriptionBuiltinTopicData>>,
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

/// Translate an `INT2DDS_INSTANCE_STATE_*` mask into the states a read selects on.
/// An empty mask would select nothing, so it falls back to alive.
fn instance_states_from_mask(mask: u32) -> Vec<InstanceStateKind> {
    let mut states = Vec::new();
    if mask & INT2DDS_INSTANCE_STATE_ALIVE != 0 {
        states.push(InstanceStateKind::ALIVE_INSTANCE_STATE);
    }
    if mask & INT2DDS_INSTANCE_STATE_NOT_ALIVE_DISPOSED != 0 {
        states.push(InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);
    }
    if mask & INT2DDS_INSTANCE_STATE_NOT_ALIVE_NO_WRITERS != 0 {
        states.push(InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE);
    }
    if states.is_empty() {
        states.push(InstanceStateKind::ALIVE_INSTANCE_STATE);
    }
    states
}

/// Fold one sample into the snapshot, keyed by instance. The instance state belongs to the
/// instance so the later sample decides it, while the payload is kept from whichever sample had one.
fn merge_snapshot_sample<T>(
    by_handle: &mut HashMap<[u8; 16], SnapshotEntry<T>>,
    info: &SampleInfo,
    data: Option<T>,
) {
    let instance_handle = *info.instance_handle.value();
    let instance_state = u32::from(info.instance_state);
    match by_handle.entry(instance_handle) {
        Entry::Occupied(mut occupied) => {
            let entry = occupied.get_mut();
            entry.instance_state = instance_state;
            if data.is_some() {
                entry.data = data;
            }
        }
        Entry::Vacant(vacant) => {
            vacant.insert(SnapshotEntry { data, instance_state, instance_handle });
        }
    }
}

fn collect_publication_snapshot(
    participant: &Int2DdsParticipant,
    timeout_ms: i32,
    instance_states: &[InstanceStateKind],
) -> Result<Vec<SnapshotEntry<PublicationBuiltinTopicData>>, Int2DdsRet> {
    let builtin_subscriber =
        participant.inner.get_builtin_subscriber().map_err(|e| dds_error_to_code(&e))?;
    let publication_reader = builtin_subscriber
        .lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication")
        .map_err(|e| dds_error_to_code(&e))?;

    let condition =
        publication_reader.get_statuscondition().map_err(|e| dds_error_to_code(&e))?.clone();
    let _ = condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE);
    let wait_set = WaitSet::new();
    let _ = wait_set.attach_condition(condition);

    let deadline = if timeout_ms < 0 {
        None
    } else {
        Some(Instant::now() + StdDuration::from_millis(timeout_ms as u64))
    };

    let mut by_handle: HashMap<[u8; 16], SnapshotEntry<PublicationBuiltinTopicData>> =
        HashMap::new();
    loop {
        let _ = publication_reader.get_status_changes();
        if let Ok(samples) = publication_reader.read(
            LENGTH_UNLIMITED,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            instance_states,
        ) {
            for sample in samples.iter() {
                let info = sample.sample_info();
                merge_snapshot_sample(&mut by_handle, &info, sample.data().ok());
            }
        }

        match deadline {
            None => {
                if !by_handle.is_empty() {
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

    Ok(by_handle.into_values().collect())
}

fn collect_subscription_snapshot(
    participant: &Int2DdsParticipant,
    timeout_ms: i32,
    instance_states: &[InstanceStateKind],
) -> Result<Vec<SnapshotEntry<SubscriptionBuiltinTopicData>>, Int2DdsRet> {
    let builtin_subscriber =
        participant.inner.get_builtin_subscriber().map_err(|e| dds_error_to_code(&e))?;
    let subscription_reader = builtin_subscriber
        .lookup_datareader::<SubscriptionBuiltinTopicData>("DCPSSubscription")
        .map_err(|e| dds_error_to_code(&e))?;

    let condition =
        subscription_reader.get_statuscondition().map_err(|e| dds_error_to_code(&e))?.clone();
    let _ = condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE);
    let wait_set = WaitSet::new();
    let _ = wait_set.attach_condition(condition);

    let deadline = if timeout_ms < 0 {
        None
    } else {
        Some(Instant::now() + StdDuration::from_millis(timeout_ms as u64))
    };

    let mut by_handle: HashMap<[u8; 16], SnapshotEntry<SubscriptionBuiltinTopicData>> =
        HashMap::new();
    loop {
        let _ = subscription_reader.get_status_changes();
        if let Ok(samples) = subscription_reader.read(
            LENGTH_UNLIMITED,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            instance_states,
        ) {
            for sample in samples.iter() {
                let info = sample.sample_info();
                merge_snapshot_sample(&mut by_handle, &info, sample.data().ok());
            }
        }

        match deadline {
            None => {
                if !by_handle.is_empty() {
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

    Ok(by_handle.into_values().collect())
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
/// A NULL `buf` or zero `capacity` is a size query and returns OK with `size_out` set.
/// If `capacity` is nonzero but too small, returns `INT2DDS_RET_BUFFER_TOO_SMALL`
/// without copying (never emits a partially-truncated, invalid-UTF-8 string).
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
    if capacity < needed {
        return INT2DDS_RET_BUFFER_TOO_SMALL;
    }
    std::ptr::copy_nonoverlapping(s.as_ptr(), buf, s.len());
    *buf.add(s.len()) = 0; // null terminator
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
///
/// # Safety
/// - `participant` must be a valid participant
/// - `handles_out` may be null to query only the count; otherwise it must point to
///   `capacity` 16-byte handles
/// - `count_out` must be a valid pointer
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
        for (i, handle) in handles.iter().take(copy_count).enumerate() {
            *handles_out.add(i) = *handle.value();
        }
    }

    INT2DDS_RET_OK
}

/// Get matched subscription instance handles for a DataWriter.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `handles_out` may be null to query only the count; otherwise it must point to
///   `capacity` 16-byte handles
/// - `count_out` must be a valid pointer
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
        for (i, handle) in handles.iter().take(copy_count).enumerate() {
            *handles_out.add(i) = *handle.value();
        }
    }

    INT2DDS_RET_OK
}

/// Get matched publication instance handles for a DataReader.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `handles_out` may be null to query only the count; otherwise it must point to
///   `capacity` 16-byte handles
/// - `count_out` must be a valid pointer
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
        for (i, handle) in handles.iter().take(copy_count).enumerate() {
            *handles_out.add(i) = *handle.value();
        }
    }

    INT2DDS_RET_OK
}

/// Collect a snapshot of discovered publications via the builtin DCPSPublication reader.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `seq_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_take_discovered_publications_snapshot(
    participant: *const Int2DdsParticipant,
    timeout_ms: i32,
    seq_out: *mut *mut Int2DdsPublicationBuiltinTopicDataSeq,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(seq_out);

    int2dds_participant_take_discovered_publications_snapshot_filtered(
        participant,
        timeout_ms,
        INT2DDS_INSTANCE_STATE_ALIVE,
        seq_out,
    )
}

/// Collect a snapshot of discovered publications restricted to the given instance states.
/// `instance_state_mask` takes `INT2DDS_INSTANCE_STATE_*` values combined with a bitwise or.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `seq_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_take_discovered_publications_snapshot_filtered(
    participant: *const Int2DdsParticipant,
    timeout_ms: i32,
    instance_state_mask: u32,
    seq_out: *mut *mut Int2DdsPublicationBuiltinTopicDataSeq,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(seq_out);

    let participant_ref = &*participant;
    let states = instance_states_from_mask(instance_state_mask);
    let items = match collect_publication_snapshot(participant_ref, timeout_ms, &states) {
        Ok(items) => items,
        Err(ret) => return ret,
    };

    *seq_out = Box::into_raw(Box::new(Int2DdsPublicationBuiltinTopicDataSeq { items }));
    INT2DDS_RET_OK
}

/// Instance state of the entry at `index`, as an `INT2DDS_INSTANCE_STATE_*` value.
///
/// # Safety
/// - `seq` must be a valid publication data sequence
/// - `instance_state_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_seq_get_instance_state(
    seq: *const Int2DdsPublicationBuiltinTopicDataSeq,
    index: usize,
    instance_state_out: *mut u32,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(instance_state_out);
    let items = &(*seq).items;
    match items.get(index) {
        Some(entry) => {
            *instance_state_out = entry.instance_state;
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_PRECONDITION_NOT_MET,
    }
}

/// Instance handle of the entry at `index`, which is the endpoint GUID.
/// Present even for an entry with no announcement to read a GUID out of.
///
/// # Safety
/// - `seq` must be a valid publication data sequence
/// - `handle_out` must point to a 16-byte buffer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_seq_get_instance_handle(
    seq: *const Int2DdsPublicationBuiltinTopicDataSeq,
    index: usize,
    handle_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(handle_out);
    let items = &(*seq).items;
    match items.get(index) {
        Some(entry) => {
            *handle_out = entry.instance_handle;
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_PRECONDITION_NOT_MET,
    }
}

/// # Safety
/// - `seq` must be a valid publication data sequence
/// - `count_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_seq_length(
    seq: *const Int2DdsPublicationBuiltinTopicDataSeq,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(count_out);
    *count_out = (*seq).items.len();
    INT2DDS_RET_OK
}

/// # Safety
/// - `seq` must be a valid publication data sequence
/// - `data_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_seq_get(
    seq: *const Int2DdsPublicationBuiltinTopicDataSeq,
    index: usize,
    data_out: *mut *mut Int2DdsPublicationBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(data_out);
    // Absent for a key-only entry such as a dispose; read the instance handle for those.
    let items = &(*seq).items;
    let data = match items.get(index).and_then(|entry| entry.data.clone()) {
        Some(data) => data,
        None => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };
    *data_out = Box::into_raw(Box::new(Int2DdsPublicationBuiltinTopicData { inner: data }));
    INT2DDS_RET_OK
}

/// # Safety
/// - `seq` must be a valid publication data sequence
/// - `seq` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_seq_delete(
    seq: *mut Int2DdsPublicationBuiltinTopicDataSeq,
) -> Int2DdsRet {
    check_null!(seq);
    drop(Box::from_raw(seq));
    INT2DDS_RET_OK
}

/// Collect a snapshot of discovered subscriptions via the builtin DCPSSubscription reader.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `seq_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_take_discovered_subscriptions_snapshot(
    participant: *const Int2DdsParticipant,
    timeout_ms: i32,
    seq_out: *mut *mut Int2DdsSubscriptionBuiltinTopicDataSeq,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(seq_out);

    int2dds_participant_take_discovered_subscriptions_snapshot_filtered(
        participant,
        timeout_ms,
        INT2DDS_INSTANCE_STATE_ALIVE,
        seq_out,
    )
}

/// Collect a snapshot of discovered subscriptions restricted to the given instance states.
/// See `int2dds_participant_take_discovered_publications_snapshot_filtered` for the mask.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `seq_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_take_discovered_subscriptions_snapshot_filtered(
    participant: *const Int2DdsParticipant,
    timeout_ms: i32,
    instance_state_mask: u32,
    seq_out: *mut *mut Int2DdsSubscriptionBuiltinTopicDataSeq,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(seq_out);

    let participant_ref = &*participant;
    let states = instance_states_from_mask(instance_state_mask);
    let items = match collect_subscription_snapshot(participant_ref, timeout_ms, &states) {
        Ok(items) => items,
        Err(ret) => return ret,
    };

    *seq_out = Box::into_raw(Box::new(Int2DdsSubscriptionBuiltinTopicDataSeq { items }));
    INT2DDS_RET_OK
}

/// Instance state of the entry at `index`, as an `INT2DDS_INSTANCE_STATE_*` value.
///
/// # Safety
/// - `seq` must be a valid subscription data sequence
/// - `instance_state_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_seq_get_instance_state(
    seq: *const Int2DdsSubscriptionBuiltinTopicDataSeq,
    index: usize,
    instance_state_out: *mut u32,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(instance_state_out);
    let items = &(*seq).items;
    match items.get(index) {
        Some(entry) => {
            *instance_state_out = entry.instance_state;
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_PRECONDITION_NOT_MET,
    }
}

/// Instance handle of the entry at `index`, which is the endpoint GUID.
/// See `int2dds_publication_builtin_topic_data_seq_get_instance_handle`.
///
/// # Safety
/// - `seq` must be a valid subscription data sequence
/// - `handle_out` must point to a 16-byte buffer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_seq_get_instance_handle(
    seq: *const Int2DdsSubscriptionBuiltinTopicDataSeq,
    index: usize,
    handle_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(handle_out);
    let items = &(*seq).items;
    match items.get(index) {
        Some(entry) => {
            *handle_out = entry.instance_handle;
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_PRECONDITION_NOT_MET,
    }
}

/// # Safety
/// - `seq` must be a valid subscription data sequence
/// - `count_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_seq_length(
    seq: *const Int2DdsSubscriptionBuiltinTopicDataSeq,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(count_out);
    *count_out = (*seq).items.len();
    INT2DDS_RET_OK
}

/// # Safety
/// - `seq` must be a valid subscription data sequence
/// - `data_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_seq_get(
    seq: *const Int2DdsSubscriptionBuiltinTopicDataSeq,
    index: usize,
    data_out: *mut *mut Int2DdsSubscriptionBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(data_out);
    // Absent for a key-only entry such as a dispose; read the instance handle for those.
    let items = &(*seq).items;
    let data = match items.get(index).and_then(|entry| entry.data.clone()) {
        Some(data) => data,
        None => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };
    *data_out = Box::into_raw(Box::new(Int2DdsSubscriptionBuiltinTopicData { inner: data }));
    INT2DDS_RET_OK
}

/// # Safety
/// - `seq` must be a valid subscription data sequence
/// - `seq` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_seq_delete(
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
///
/// # Safety
/// - `participant` must be a valid participant
/// - `handle` must point to a 16-byte buffer
/// - `data_out` must be a valid pointer to a null pointer
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
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `handle` must point to a 16-byte buffer
/// - `data_out` must be a valid pointer to a null pointer
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
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `handle` must point to a 16-byte buffer
/// - `data_out` must be a valid pointer to a null pointer
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
///
/// # Safety
/// - `data` must be a valid ParticipantBuiltinTopicData
/// - `key_out` must point to a 12-byte buffer
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
///
/// # Safety
/// - `data` must be a valid ParticipantBuiltinTopicData
/// - `buf` may be null to query only the required size; otherwise it must
///   point to at least `capacity` writable bytes
/// - `size_out` must be a valid pointer, or null
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
///
/// # Safety
/// - `data` must be a valid ParticipantBuiltinTopicData
/// - `data` must not be used after this call
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
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `key_out` must point to a 12-byte buffer
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
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `guid_out` must point to a 16-byte buffer
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
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `key_out` must point to a 12-byte buffer
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
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `buf` may be null to query only the required size; otherwise it must
///   point to at least `capacity` writable bytes
/// - `size_out` must be a valid pointer, or null
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
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `buf` may be null to query only the required size; otherwise it must
///   point to at least `capacity` writable bytes
/// - `size_out` must be a valid pointer, or null
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

/// Take a clone of the TypeObject embedded in a PublicationBuiltinTopicData.
/// Caller owns the returned handle and must destroy it via `int2dds_type_object_destroy`.
/// Returns DYNAMIC_FIELD_NOT_FOUND if the publication did not carry a TypeObject.
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_take_type_object(
    data: *const Int2DdsPublicationBuiltinTopicData,
    out: *mut *mut crate::dynamic::Int2DdsTypeObject,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(out);
    let to = match (*data).inner.type_object() {
        Some(t) => t.clone(),
        None => return INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    };
    let h = Box::new(crate::dynamic::Int2DdsTypeObject { inner: to, deps: Vec::new() });
    *out = Box::into_raw(h);
    INT2DDS_RET_OK
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

/// Get the liveliness kind from a PublicationBuiltinTopicData.
/// `kind_out`: 0 = AUTOMATIC, 1 = MANUAL_BY_PARTICIPANT, 2 = MANUAL_BY_TOPIC.
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `kind_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_liveliness_kind(
    data: *const Int2DdsPublicationBuiltinTopicData,
    kind_out: *mut i32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(kind_out);
    let data_ref = &*data;
    *kind_out = match data_ref.inner.liveliness().kind {
        LivelinessQosPolicyKind::Automatic => 0,
        LivelinessQosPolicyKind::ManualByParticipant => 1,
        LivelinessQosPolicyKind::ManualByTopic => 2,
    };
    INT2DDS_RET_OK
}

/// Get the liveliness lease duration from a PublicationBuiltinTopicData.
/// An infinite duration is reported as (0x7fffffff, 0x7fffffff).
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `sec_out` and `nanosec_out` must be valid pointers
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_liveliness_lease_duration(
    data: *const Int2DdsPublicationBuiltinTopicData,
    sec_out: *mut i32,
    nanosec_out: *mut u32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(sec_out);
    check_null!(nanosec_out);
    let lease = (*data).inner.liveliness().lease_duration;
    *sec_out = lease.sec;
    *nanosec_out = lease.nanosec;
    INT2DDS_RET_OK
}

/// Get the deadline period from a PublicationBuiltinTopicData.
/// An infinite duration is reported as (0x7fffffff, 0x7fffffff).
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `sec_out` and `nanosec_out` must be valid pointers
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_deadline(
    data: *const Int2DdsPublicationBuiltinTopicData,
    sec_out: *mut i32,
    nanosec_out: *mut u32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(sec_out);
    check_null!(nanosec_out);
    let period = (*data).inner.deadline().period;
    *sec_out = period.sec;
    *nanosec_out = period.nanosec;
    INT2DDS_RET_OK
}

/// Get the lifespan duration from a PublicationBuiltinTopicData.
/// An infinite duration is reported as (0x7fffffff, 0x7fffffff).
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `sec_out` and `nanosec_out` must be valid pointers
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_lifespan(
    data: *const Int2DdsPublicationBuiltinTopicData,
    sec_out: *mut i32,
    nanosec_out: *mut u32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(sec_out);
    check_null!(nanosec_out);
    let duration = (*data).inner.lifespan().duration;
    *sec_out = duration.sec;
    *nanosec_out = duration.nanosec;
    INT2DDS_RET_OK
}

/// Get the user_data from a PublicationBuiltinTopicData.
/// Copies up to `capacity` bytes into `buf`. `size_out` receives the actual size.
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `buf` must be valid for `capacity` bytes, or null to query the size only
/// - `size_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_builtin_topic_data_get_user_data(
    data: *const Int2DdsPublicationBuiltinTopicData,
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

/// Free a PublicationBuiltinTopicData obtained from discovery.
///
/// # Safety
/// - `data` must be a valid PublicationBuiltinTopicData
/// - `data` must not be used after this call
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
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `key_out` must point to a 12-byte buffer
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
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `guid_out` must point to a 16-byte buffer
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
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `key_out` must point to a 12-byte buffer
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
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `buf` may be null to query only the required size; otherwise it must
///   point to at least `capacity` writable bytes
/// - `size_out` must be a valid pointer, or null
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
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `buf` may be null to query only the required size; otherwise it must
///   point to at least `capacity` writable bytes
/// - `size_out` must be a valid pointer, or null
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

/// Get the liveliness kind from a SubscriptionBuiltinTopicData.
/// `kind_out`: 0 = AUTOMATIC, 1 = MANUAL_BY_PARTICIPANT, 2 = MANUAL_BY_TOPIC.
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `kind_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_liveliness_kind(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    kind_out: *mut i32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(kind_out);
    let data_ref = &*data;
    *kind_out = match data_ref.inner.liveliness().kind {
        LivelinessQosPolicyKind::Automatic => 0,
        LivelinessQosPolicyKind::ManualByParticipant => 1,
        LivelinessQosPolicyKind::ManualByTopic => 2,
    };
    INT2DDS_RET_OK
}

/// Get the liveliness lease duration from a SubscriptionBuiltinTopicData.
/// An infinite duration is reported as (0x7fffffff, 0x7fffffff).
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `sec_out` and `nanosec_out` must be valid pointers
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_liveliness_lease_duration(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    sec_out: *mut i32,
    nanosec_out: *mut u32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(sec_out);
    check_null!(nanosec_out);
    let lease = (*data).inner.liveliness().lease_duration;
    *sec_out = lease.sec;
    *nanosec_out = lease.nanosec;
    INT2DDS_RET_OK
}

/// Get the deadline period from a SubscriptionBuiltinTopicData.
/// An infinite duration is reported as (0x7fffffff, 0x7fffffff).
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `sec_out` and `nanosec_out` must be valid pointers
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_deadline(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
    sec_out: *mut i32,
    nanosec_out: *mut u32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(sec_out);
    check_null!(nanosec_out);
    let period = (*data).inner.deadline().period;
    *sec_out = period.sec;
    *nanosec_out = period.nanosec;
    INT2DDS_RET_OK
}

/// Get the user_data from a SubscriptionBuiltinTopicData.
/// Copies up to `capacity` bytes into `buf`. `size_out` receives the actual size.
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `buf` must be valid for `capacity` bytes, or null to query the size only
/// - `size_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_get_user_data(
    data: *const Int2DdsSubscriptionBuiltinTopicData,
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

/// Free a SubscriptionBuiltinTopicData obtained from discovery.
///
/// # Safety
/// - `data` must be a valid SubscriptionBuiltinTopicData
/// - `data` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_destroy(
    data: *mut Int2DdsSubscriptionBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(data);
    drop(Box::from_raw(data));
    INT2DDS_RET_OK
}

// ============================================================================
// Endpoint (SEDP) discovery push callback
// ============================================================================

/// C callback invoked on every remote endpoint discovery/dispose.
/// `is_writer`: 1 = publication/writer, 0 = subscription/reader.
/// `is_alive`:  1 = alive (`pub_data` or `sub_data` valid), 0 = disposed (data null).
/// `guid` always points to the 16-byte endpoint GUID (valid only during the call).
/// All pointers are borrowed and must be copied out before returning.
pub type Int2DdsEndpointDiscoveryCallback = extern "C" fn(
    ctx: *mut core::ffi::c_void,
    is_writer: i32,
    is_alive: i32,
    pub_data: *const Int2DdsPublicationBuiltinTopicData,
    sub_data: *const Int2DdsSubscriptionBuiltinTopicData,
    guid: *const [u8; 16],
);

struct EndpointDiscoveryCtx(*mut core::ffi::c_void);
// The caller owns `ctx` and must keep it valid while the callback is registered.
unsafe impl Send for EndpointDiscoveryCtx {}
unsafe impl Sync for EndpointDiscoveryCtx {}

/// Register a callback for remote endpoint discovery events. `callback` must be
/// non-null; to disable, register a no-op callback (or destroy the participant)
/// before freeing `ctx`.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `callback` must be a valid function pointer
/// - `ctx` is passed back to `callback` unchanged and must stay valid for as long as
///   the callback can fire
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_set_endpoint_discovery_callback(
    participant: *const Int2DdsParticipant,
    callback: Int2DdsEndpointDiscoveryCallback,
    ctx: *mut core::ffi::c_void,
) -> Int2DdsRet {
    check_null!(participant);
    let participant_ref = &*participant;

    let ctx_wrap = EndpointDiscoveryCtx(ctx);
    let cb = callback;
    let closure: std::sync::Arc<dyn Fn(&EndpointDiscoveryEvent) + Send + Sync> =
        std::sync::Arc::new(move |event: &EndpointDiscoveryEvent| {
            // Capture the wrapper, not its raw pointer field, so the closure stays Send + Sync.
            let ctx_wrap = &ctx_wrap;
            let ctx = ctx_wrap.0;
            match event {
                EndpointDiscoveryEvent::WriterAlive(data) => {
                    let guid = data.endpoint_guid().to_bytes();
                    let wrapped = Int2DdsPublicationBuiltinTopicData { inner: data.clone() };
                    cb(
                        ctx,
                        1,
                        1,
                        &wrapped as *const Int2DdsPublicationBuiltinTopicData,
                        core::ptr::null(),
                        &guid as *const [u8; 16],
                    );
                }
                EndpointDiscoveryEvent::WriterDisposed(g) => {
                    let guid = g.to_bytes();
                    cb(ctx, 1, 0, core::ptr::null(), core::ptr::null(), &guid as *const [u8; 16]);
                }
                EndpointDiscoveryEvent::ReaderAlive(data) => {
                    let guid = data.endpoint_guid().to_bytes();
                    let wrapped = Int2DdsSubscriptionBuiltinTopicData { inner: data.clone() };
                    cb(
                        ctx,
                        0,
                        1,
                        core::ptr::null(),
                        &wrapped as *const Int2DdsSubscriptionBuiltinTopicData,
                        &guid as *const [u8; 16],
                    );
                }
                EndpointDiscoveryEvent::ReaderDisposed(g) => {
                    let guid = g.to_bytes();
                    cb(ctx, 0, 0, core::ptr::null(), core::ptr::null(), &guid as *const [u8; 16]);
                }
            }
        });

    ffi_try!(participant_ref.inner.set_endpoint_discovery_callback(closure));
    INT2DDS_RET_OK
}
