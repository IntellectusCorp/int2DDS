//! # Discovery API
//!
//! FFI functions for discovering remote DDS participants, publications,
//! and subscriptions. Provides handle enumeration, builtin topic data
//! retrieval, and field getters for opaque builtin topic data types.

use crate::error::*;
use crate::types::{Int2DdsDataReader, Int2DdsDataWriter, Int2DdsParticipant};

use int2dds::common::{
    builtin::topic::{
        participant_builtin_topic_data::ParticipantBuiltinTopicData,
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    },
    instance_handle::InstanceHandle,
};

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

unsafe impl Send for Int2DdsParticipantBuiltinTopicData {}
unsafe impl Sync for Int2DdsParticipantBuiltinTopicData {}
unsafe impl Send for Int2DdsPublicationBuiltinTopicData {}
unsafe impl Sync for Int2DdsPublicationBuiltinTopicData {}
unsafe impl Send for Int2DdsSubscriptionBuiltinTopicData {}
unsafe impl Sync for Int2DdsSubscriptionBuiltinTopicData {}

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

/// Free a SubscriptionBuiltinTopicData obtained from discovery.
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscription_builtin_topic_data_destroy(
    data: *mut Int2DdsSubscriptionBuiltinTopicData,
) -> Int2DdsRet {
    check_null!(data);
    drop(Box::from_raw(data));
    INT2DDS_RET_OK
}
