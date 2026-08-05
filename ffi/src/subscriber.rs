//! # Subscriber and DataReader
//!
//! Functions for creating Subscribers and reading serialized data.
//!
//! ## Overview
//!
//! Subscribers group DataReaders and manage their QoS settings. DataReaders
//! receive data samples from matching DataWriters on the same Topic.
//!
//! ## Reading Data
//!
//! C users receive raw CDR bytes via `int2dds_datareader_take_serialized` / `int2dds_datareader_read_serialized`
//! and deserialize them with IDL-generated code.

use std::ffi::CStr;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    infrastructure::status::StatusMask,
    subscription::{
        data_reader::BoundedSerialized,
        data_reader_listener::DataReaderListener,
        sample_info::{InstanceStateKind, SampleInfo, SampleStateKind, ViewStateKind},
    },
};

use crate::data::Int2DdsData;

pub struct Int2DdsSerializedLoan {
    data: Bytes,
}

// Sample state masks
pub const INT2DDS_SAMPLE_STATE_READ: u32 = 0x0001;
pub const INT2DDS_SAMPLE_STATE_NOT_READ: u32 = 0x0002;
pub const INT2DDS_SAMPLE_STATE_ANY: u32 = 0xFFFF;

// View state masks
pub const INT2DDS_VIEW_STATE_NEW: u32 = 0x0001;
pub const INT2DDS_VIEW_STATE_NOT_NEW: u32 = 0x0002;
pub const INT2DDS_VIEW_STATE_ANY: u32 = 0xFFFF;

// Instance state masks
pub const INT2DDS_INSTANCE_STATE_ALIVE: u32 = 0x0001;
pub const INT2DDS_INSTANCE_STATE_NOT_ALIVE_DISPOSED: u32 = 0x0002;
pub const INT2DDS_INSTANCE_STATE_NOT_ALIVE_NO_WRITERS: u32 = 0x0004;
pub const INT2DDS_INSTANCE_STATE_ANY: u32 = 0xFFFF;

use super::{
    error::*,
    listener::{FfiDataReaderListener, Int2DdsDataReaderListener},
    qos::{Int2DdsDataReaderQos, Int2DdsSubscriberQos},
    status::{
        Int2DdsLivelinessChangedStatus, Int2DdsRequestedDeadlineMissedStatus,
        Int2DdsRequestedIncompatibleQosStatus, Int2DdsRequestedIncompatibleTypeStatus,
        Int2DdsSampleLostStatus, Int2DdsSampleRejectedStatus, Int2DdsSubscriptionMatchedStatus,
    },
    types::*,
};

/// Create a Subscriber
///
/// # Safety
/// - `participant` must be a valid participant
/// - `qos` can be null for the default QoS (engages the core resolution chain:
///   registered default → configured default profile → spec default)
/// - `subscriber_out` must be a valid pointer to a null pointer
/// - The returned subscriber must be freed with `int2dds_delete_subscriber`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_subscriber(
    participant: *const Int2DdsParticipant,
    qos: *const Int2DdsSubscriberQos,
    subscriber_out: *mut *mut Int2DdsSubscriber,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(subscriber_out);

    let participant_ref = &*participant;

    // NULL qos → default sentinel (engages profile fallback). Non-NULL → use as-is.
    let subscriber_qos = if qos.is_null() {
        int2dds::infrastructure::qos_kind::QosKind::Default
    } else {
        int2dds::infrastructure::qos_kind::QosKind::Specific((*qos).inner.clone())
    };

    let subscriber = ffi_try!(participant_ref.inner.create_subscriber(
        subscriber_qos,
        None,
        StatusMask::default()
    ));

    let subscriber_arc = Arc::new(subscriber);
    let subscriber_handle = Box::new(Int2DdsSubscriber { inner: subscriber_arc });
    *subscriber_out = Box::into_raw(subscriber_handle);

    INT2DDS_RET_OK
}

/// Create a Subscriber using a QoS profile path
///
/// # Safety
/// - `participant` must be a valid participant
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `subscriber_out` must be a valid pointer to a null pointer
/// - The returned subscriber must be freed with `int2dds_delete_subscriber`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_subscriber_with_profile(
    participant: *const Int2DdsParticipant,
    qos_path: *const std::os::raw::c_char,
    subscriber_out: *mut *mut Int2DdsSubscriber,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(qos_path);
    check_null!(subscriber_out);

    let participant_ref = &*participant;

    let qos_path_str = match CStr::from_ptr(qos_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let subscriber = ffi_try!(participant_ref.inner.create_subscriber_with_profile(
        qos_path_str,
        None,
        StatusMask::default()
    ));

    let subscriber_arc = Arc::new(subscriber);
    let subscriber_handle = Box::new(Int2DdsSubscriber { inner: subscriber_arc });
    *subscriber_out = Box::into_raw(subscriber_handle);

    INT2DDS_RET_OK
}

/// Set QoS on a Subscriber
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `qos` must be a valid subscriber QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscriber_set_qos(
    subscriber: *const Int2DdsSubscriber,
    qos: *const Int2DdsSubscriberQos,
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(qos);

    let subscriber_ref = &*subscriber;
    let qos_ref = &*qos;

    ffi_try!(subscriber_ref.inner.set_qos(qos_ref.inner.clone()));

    INT2DDS_RET_OK
}

/// Get QoS from a Subscriber
///
/// The returned handle must be freed with `int2dds_subscriber_qos_destroy`.
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `qos_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscriber_get_qos(
    subscriber: *const Int2DdsSubscriber,
    qos_out: *mut *mut Int2DdsSubscriberQos,
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(qos_out);

    let subscriber_ref = &*subscriber;
    let qos = ffi_try!(subscriber_ref.inner.get_qos());
    let boxed = Box::new(Int2DdsSubscriberQos { inner: qos });
    *qos_out = Box::into_raw(boxed);

    INT2DDS_RET_OK
}

/// Get the 16-byte instance handle of a Subscriber.
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `handle_out` must point to a 16-byte buffer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscriber_get_instance_handle(
    subscriber: *const Int2DdsSubscriber,
    handle_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(handle_out);

    let subscriber_ref = &*subscriber;
    let handle = ffi_try!(subscriber_ref.inner.get_instance_handle());
    *handle_out = *handle.value();

    INT2DDS_RET_OK
}

/// Delete a Subscriber
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `subscriber` must not be used after this call
/// - All DataReaders created by this subscriber must be deleted first
#[no_mangle]
pub unsafe extern "C" fn int2dds_delete_subscriber(
    subscriber: *mut Int2DdsSubscriber,
) -> Int2DdsRet {
    if subscriber.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let subscriber_box = Box::from_raw(subscriber);
    if Arc::strong_count(&subscriber_box.inner) != 1 {
        let _ = Box::into_raw(subscriber_box);
        return INT2DDS_RET_PRECONDITION_NOT_MET;
    }

    let subscriber_obj = (*subscriber_box.inner).clone();

    let participant = match subscriber_obj.get_participant() {
        Ok(p) => p,
        Err(e) => {
            let _ = Box::into_raw(subscriber_box);
            return dds_error_to_code(&e);
        }
    };

    // On failure the subscriber is not deleted; restore the caller's handle
    // (into_raw) instead of leaving it freed. The Box drops (frees) only on success.
    match participant.delete_subscriber(subscriber_obj) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => {
            let _ = Box::into_raw(subscriber_box);
            dds_error_to_code(&e)
        }
    }
}

/// Attach FFI listener callbacks to a freshly created reader handle, then re-notify
/// for events that may have fired between entity creation and listener registration
/// (the SEDP matching race).
unsafe fn attach_reader_listener(
    reader_handle: &Arc<Int2DdsDataReader>,
    listener: *const Int2DdsDataReaderListener,
    mask: u32,
) -> Int2DdsRet {
    if listener.is_null() {
        return INT2DDS_RET_OK;
    }

    let weak = Arc::downgrade(reader_handle);
    let listener_arc = Arc::new(FfiDataReaderListener::new(*listener, weak));

    let listener_clone = listener_arc.clone()
        as Arc<
            dyn int2dds::subscription::data_reader_listener::DataReaderListener<Foo = Int2DdsData>,
        >;
    ffi_try!(reader_handle
        .inner
        .set_listener(Some(listener_clone), StatusMask::from_bits_truncate(mask)));

    *reader_handle.listener.write().unwrap() = Some(listener_arc.clone());

    if mask & crate::status_condition::INT2DDS_STATUS_SUBSCRIPTION_MATCHED != 0 {
        if let Ok(status) = reader_handle.inner.get_subscription_matched_status() {
            if status.current_count() > 0 {
                listener_arc.on_subscription_matched(&reader_handle.inner, &status);
            }
        }
    }
    if mask & crate::status_condition::INT2DDS_STATUS_REQUESTED_INCOMPATIBLE_QOS != 0 {
        if let Ok(status) = reader_handle.inner.get_requested_incompatible_qos_status() {
            if status.total_count() > 0 {
                listener_arc.on_requested_incompatible_qos(&reader_handle.inner, &status);
            }
        }
    }

    INT2DDS_RET_OK
}

/// Create a DataReader
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `topic` must be a valid topic
/// - `qos` can be null for default QoS
/// - `listener` can be null for no listener; `mask` specifies which status
///   changes trigger callbacks (pass 0 with a null listener)
/// - `reader_out` must be a valid pointer to a null pointer
/// - The returned reader must be freed with `int2dds_delete_datareader`
/// - Listener callbacks must be thread-safe and remain valid until reader is deleted
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader(
    subscriber: *const Int2DdsSubscriber,
    topic: *const Int2DdsTopic,
    qos: *const Int2DdsDataReaderQos,
    listener: *const Int2DdsDataReaderListener,
    mask: u32,
    reader_out: *mut *mut Int2DdsDataReader,
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(topic);
    check_null!(reader_out);

    let subscriber_ref = &*subscriber;
    let topic_ref = &*topic;

    // NULL qos → default sentinel (engages profile fallback). Non-NULL → use as-is.
    let reader_qos = if qos.is_null() {
        int2dds::infrastructure::qos_kind::QosKind::Default
    } else {
        int2dds::infrastructure::qos_kind::QosKind::Specific((*qos).inner.clone())
    };

    let reader = ffi_try!(subscriber_ref.inner.create_datareader::<Int2DdsData>(
        &*topic_ref.inner,
        reader_qos,
        None,
        StatusMask::from_bits_truncate(mask)
    ));

    let reader_handle = Arc::new(Int2DdsDataReader { inner: reader, listener: RwLock::new(None) });

    let ret = attach_reader_listener(&reader_handle, listener, mask);
    if ret != INT2DDS_RET_OK {
        return ret;
    }

    *reader_out = Arc::into_raw(reader_handle) as *mut Int2DdsDataReader;

    INT2DDS_RET_OK
}

/// Create a DataReader using a QoS profile path
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `topic` must be a valid topic
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `listener` can be null for no listener; `mask` specifies which status
///   changes trigger callbacks (pass 0 with a null listener)
/// - `reader_out` must be a valid pointer to a null pointer
/// - The returned reader must be freed with `int2dds_delete_datareader`
/// - Listener callbacks must be thread-safe and remain valid until reader is deleted
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader_with_profile(
    subscriber: *const Int2DdsSubscriber,
    topic: *const Int2DdsTopic,
    qos_path: *const std::os::raw::c_char,
    listener: *const Int2DdsDataReaderListener,
    mask: u32,
    reader_out: *mut *mut Int2DdsDataReader,
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(topic);
    check_null!(qos_path);
    check_null!(reader_out);

    let subscriber_ref = &*subscriber;
    let topic_ref = &*topic;

    let qos_path_str = match CStr::from_ptr(qos_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let reader = ffi_try!(subscriber_ref.inner.create_datareader_with_profile::<Int2DdsData>(
        &*topic_ref.inner,
        qos_path_str,
        None,
        StatusMask::from_bits_truncate(mask)
    ));

    let reader_handle = Arc::new(Int2DdsDataReader { inner: reader, listener: RwLock::new(None) });

    let ret = attach_reader_listener(&reader_handle, listener, mask);
    if ret != INT2DDS_RET_OK {
        return ret;
    }

    *reader_out = Arc::into_raw(reader_handle) as *mut Int2DdsDataReader;

    INT2DDS_RET_OK
}

/// Create a DataReader using a ContentFilteredTopic
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `cft` must be a valid ContentFilteredTopic
/// - `qos` can be null for default QoS
/// - `listener` can be null for no listener; `mask` specifies which status
///   changes trigger callbacks (pass 0 with a null listener)
/// - `reader_out` must be a valid pointer to a null pointer
/// - The returned reader must be freed with `int2dds_delete_datareader`
/// - Listener callbacks must be thread-safe and remain valid until reader is deleted
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader_cft(
    subscriber: *const Int2DdsSubscriber,
    cft: *const Int2DdsContentFilteredTopic,
    qos: *const Int2DdsDataReaderQos,
    listener: *const Int2DdsDataReaderListener,
    mask: u32,
    reader_out: *mut *mut Int2DdsDataReader,
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(cft);
    check_null!(reader_out);

    let subscriber_ref = &*subscriber;
    let cft_ref = &*cft;

    // NULL qos → default sentinel (engages profile fallback). Non-NULL → use as-is.
    let reader_qos = if qos.is_null() {
        int2dds::infrastructure::qos_kind::QosKind::Default
    } else {
        int2dds::infrastructure::qos_kind::QosKind::Specific((*qos).inner.clone())
    };

    let reader = ffi_try!(subscriber_ref.inner.create_datareader::<Int2DdsData>(
        &cft_ref.inner,
        reader_qos,
        None,
        StatusMask::from_bits_truncate(mask)
    ));

    let reader_handle = Arc::new(Int2DdsDataReader { inner: reader, listener: RwLock::new(None) });

    let ret = attach_reader_listener(&reader_handle, listener, mask);
    if ret != INT2DDS_RET_OK {
        return ret;
    }

    *reader_out = Arc::into_raw(reader_handle) as *mut Int2DdsDataReader;

    INT2DDS_RET_OK
}

/// Set or update the listener for a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `listener` can be null to remove the listener
/// - `mask` specifies which status changes trigger callbacks
/// - Listener callbacks must be thread-safe
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_set_listener(
    reader: *mut Int2DdsDataReader,
    listener: *const Int2DdsDataReaderListener,
    mask: u32,
) -> Int2DdsRet {
    check_null!(reader);

    // No early return is allowed between from_raw and into_raw below, or the
    // caller's strong reference would be dropped and later delete double-frees.
    let reader_arc = Arc::from_raw(reader as *const Int2DdsDataReader);

    let listener_arc = if !listener.is_null() {
        let weak = Arc::downgrade(&reader_arc);
        Some(Arc::new(FfiDataReaderListener::new(*listener, weak)))
    } else {
        None
    };

    let result = reader_arc.inner.set_listener(
        listener_arc.clone().map(|l| {
            l as Arc<
                dyn int2dds::subscription::data_reader_listener::DataReaderListener<
                    Foo = Int2DdsData,
                >,
            >
        }),
        StatusMask::from_bits_truncate(mask),
    );

    *reader_arc.listener.write().unwrap() = listener_arc;

    let _ = Arc::into_raw(reader_arc);

    match result {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the current listener from a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `listener_out` must be a valid pointer to Int2DdsDataReaderListener
/// - Returns a copy of the listener callbacks and user context
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_listener(
    reader: *const Int2DdsDataReader,
    listener_out: *mut Int2DdsDataReaderListener,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(listener_out);

    let reader_ref = &*reader;

    // Return the stored listener callbacks
    if let Some(listener_arc) = reader_ref.listener.read().unwrap().as_ref() {
        // Copy the callbacks struct
        *listener_out = listener_arc.callbacks;
        INT2DDS_RET_OK
    } else {
        // No listener set - return zeroed callbacks
        *listener_out = std::mem::zeroed();
        INT2DDS_RET_OK
    }
}

/// Set QoS on a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `qos` must be a valid datareader QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_set_qos(
    reader: *const Int2DdsDataReader,
    qos: *const Int2DdsDataReaderQos,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(qos);

    let reader_ref = &*reader;
    let qos_ref = &*qos;

    ffi_try!(reader_ref.inner.set_qos(qos_ref.inner.clone()));

    INT2DDS_RET_OK
}

/// Get QoS from a DataReader
///
/// The returned handle must be freed with `int2dds_datareader_qos_destroy`.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `qos_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_qos(
    reader: *const Int2DdsDataReader,
    qos_out: *mut *mut Int2DdsDataReaderQos,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(qos_out);

    let reader_ref = &*reader;
    let qos = ffi_try!(reader_ref.inner.get_qos());
    let boxed = Box::new(Int2DdsDataReaderQos { inner: qos });
    *qos_out = Box::into_raw(boxed);

    INT2DDS_RET_OK
}

/// Get the 16-byte RTPS GUID of a DataReader.
///
/// Writes the reader's endpoint GUID (the same value advertised over SEDP
/// discovery as `endpoint_guid`) into `guid_out`. Read-only.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `guid_out` must be a valid pointer to a 16-byte buffer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_guid(
    reader: *const Int2DdsDataReader,
    guid_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(guid_out);

    let reader_ref = &*reader;
    *guid_out = reader_ref.inner.guid().to_bytes();

    INT2DDS_RET_OK
}

unsafe fn reader_handle_from_c(handle_ptr: *const [u8; 16]) -> InstanceHandle {
    if handle_ptr.is_null() {
        return InstanceHandle::NIL;
    }
    let value = *handle_ptr;
    if value == [0u8; 16] {
        InstanceHandle::NIL
    } else {
        InstanceHandle::new(value)
    }
}

/// Look up an instance handle from raw serialized key bytes.
///
/// Mirrors `int2dds_datawriter_lookup_instance` for the read side. Matches on the
/// serialized key bytes stored per instance; writes `InstanceHandle::NIL` (all zeros)
/// to `handle_out` when the instance is not known to this reader.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `key` must point to at least `key_len` readable bytes
/// - `handle_out` must be a valid pointer to a 16-byte buffer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_lookup_instance(
    reader: *const Int2DdsDataReader,
    key: *const u8,
    key_len: usize,
    handle_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(key);
    check_null!(handle_out);

    let reader_ref = &*reader;
    let key_bytes = std::slice::from_raw_parts(key, key_len);

    let handle = ffi_try!(reader_ref.inner.lookup_instance_serialized(key_bytes));
    *handle_out = *handle.value();

    INT2DDS_RET_OK
}

/// Get the raw serialized key bytes for an instance handle.
///
/// Mirrors `int2dds_datawriter_get_key_value` for the read side. Round-trips with
/// `int2dds_datareader_lookup_instance`.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `handle` must be a valid pointer to a 16-byte instance handle
/// - `key_buf` must point to at least `key_capacity` writable bytes
/// - `key_size_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_key_value(
    reader: *const Int2DdsDataReader,
    handle: *const [u8; 16],
    key_buf: *mut u8,
    key_capacity: usize,
    key_size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(handle);
    check_null!(key_buf);
    check_null!(key_size_out);

    let reader_ref = &*reader;
    let instance_handle = reader_handle_from_c(handle);

    let key_data = ffi_try!(reader_ref.inner.get_key_value_serialized(instance_handle));
    *key_size_out = key_data.len();

    if key_data.len() > key_capacity {
        return INT2DDS_RET_BUFFER_TOO_SMALL;
    }

    std::ptr::copy_nonoverlapping(key_data.as_ptr(), key_buf, key_data.len());

    INT2DDS_RET_OK
}

/// Check whether a DataReader currently has any cached samples.
///
/// This is a level-triggered readiness check over the local reader cache.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `has_data_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_has_data(
    reader: *const Int2DdsDataReader,
    has_data_out: *mut bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(has_data_out);

    let reader_ref = &*reader;

    match reader_ref.inner.has_cached_data() {
        Ok(has_data) => {
            *has_data_out = has_data;
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Delete a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `reader` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_delete_datareader(reader: *mut Int2DdsDataReader) -> Int2DdsRet {
    if reader.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    // Reclaim the caller's strong reference; the reader is freed only once this
    // Arc and any callback that upgraded its Weak are dropped.
    let reader_arc = Arc::from_raw(reader as *const Int2DdsDataReader);
    let reader_obj = reader_arc.inner.clone();

    let _ = reader_obj.set_listener(None, StatusMask::default());

    let subscriber = match reader_obj.get_subscriber() {
        Ok(s) => s,
        Err(e) => {
            let _ = Arc::into_raw(reader_arc);
            return dds_error_to_code(&e);
        }
    };

    // On failure the reader is not deleted; hand the caller's strong reference
    // back (into_raw) so the handle stays valid instead of being freed.
    match subscriber.delete_datareader(reader_obj) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => {
            let _ = Arc::into_raw(reader_arc);
            dds_error_to_code(&e)
        }
    }
}

/// Get subscription matched status for a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_subscription_matched_status(
    reader: *const Int2DdsDataReader,
    status_out: *mut Int2DdsSubscriptionMatchedStatus,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(status_out);

    let reader_ref = &*reader;

    match reader_ref.inner.get_subscription_matched_status() {
        Ok(status) => {
            *status_out = Int2DdsSubscriptionMatchedStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get liveliness changed status for a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_liveliness_changed_status(
    reader: *const Int2DdsDataReader,
    status_out: *mut Int2DdsLivelinessChangedStatus,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(status_out);

    let reader_ref = &*reader;

    match reader_ref.inner.get_liveliness_changed_status() {
        Ok(status) => {
            *status_out = Int2DdsLivelinessChangedStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get sample rejected status for a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_sample_rejected_status(
    reader: *const Int2DdsDataReader,
    status_out: *mut Int2DdsSampleRejectedStatus,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(status_out);

    let reader_ref = &*reader;

    match reader_ref.inner.get_sample_rejected_status() {
        Ok(status) => {
            *status_out = Int2DdsSampleRejectedStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get sample lost status for a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_sample_lost_status(
    reader: *const Int2DdsDataReader,
    status_out: *mut Int2DdsSampleLostStatus,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(status_out);

    let reader_ref = &*reader;

    match reader_ref.inner.get_sample_lost_status() {
        Ok(status) => {
            *status_out = Int2DdsSampleLostStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get requested deadline missed status for a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_requested_deadline_missed_status(
    reader: *const Int2DdsDataReader,
    status_out: *mut Int2DdsRequestedDeadlineMissedStatus,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(status_out);

    let reader_ref = &*reader;

    match reader_ref.inner.get_requested_deadline_missed_status() {
        Ok(status) => {
            *status_out = Int2DdsRequestedDeadlineMissedStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get requested incompatible QoS status for a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_requested_incompatible_qos_status(
    reader: *const Int2DdsDataReader,
    status_out: *mut Int2DdsRequestedIncompatibleQosStatus,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(status_out);

    let reader_ref = &*reader;

    match reader_ref.inner.get_requested_incompatible_qos_status() {
        Ok(status) => {
            *status_out = Int2DdsRequestedIncompatibleQosStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get requested incompatible type status for a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_requested_incompatible_type_status(
    reader: *const Int2DdsDataReader,
    status_out: *mut Int2DdsRequestedIncompatibleTypeStatus,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(status_out);

    let reader_ref = &*reader;

    match reader_ref.inner.get_requested_incompatible_type_status() {
        Ok(status) => {
            *status_out = Int2DdsRequestedIncompatibleTypeStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Delete all entities contained by a subscriber
///
/// This operation deletes all DataReader objects contained by this Subscriber.
/// It also recursively calls delete_contained_entities on each DataReader.
///
/// # Safety
/// - `subscriber` must be a valid subscriber
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscriber_delete_contained_entities(
    subscriber: *const Int2DdsSubscriber,
) -> Int2DdsRet {
    check_null!(subscriber);

    let subscriber_ref = &*subscriber;

    match subscriber_ref.inner.delete_contained_entities() {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

// ============================================================================
// Raw Serialized Data Read/Take Functions
// ============================================================================

unsafe fn emit_bounded_serialized(
    outcome: BoundedSerialized,
    buffer: *mut u8,
    actual_size_out: *mut usize,
) -> (Int2DdsRet, Option<SampleInfo>) {
    match outcome {
        BoundedSerialized::TooSmall { required } => {
            *actual_size_out = required;
            (INT2DDS_RET_BUFFER_TOO_SMALL, None)
        }
        BoundedSerialized::Fit(data, info) => {
            *actual_size_out = data.len();
            if info.valid_data {
                std::ptr::copy_nonoverlapping(data.as_ptr(), buffer, data.len());
            }
            (INT2DDS_RET_OK, Some(info))
        }
    }
}

/// Take pre-serialized data from a DataReader, bypassing TypeSupport deserialization.
///
/// Copies the raw CDR bytes (including encapsulation header) into the caller's buffer.
/// The sample is removed from the cache only when it fits the caller's buffer.
///
/// # Parameters
/// - `reader`: A valid datareader
/// - `buffer`: Pointer to the caller's byte buffer for receiving serialized data
/// - `buffer_capacity`: Size of the buffer in bytes
/// - `actual_size_out`: Receives the actual number of bytes written
/// - `valid_data_out`: Set to true if this is a valid data sample (not dispose/unregister)
///
/// # Returns
/// - INT2DDS_RET_OK on success
/// - INT2DDS_RET_NO_DATA if no samples available
/// - INT2DDS_RET_BUFFER_TOO_SMALL if the buffer is too small; the sample is preserved and
///   actual_size_out contains the required size, so a retry with a larger buffer succeeds
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `buffer` must point to at least `buffer_capacity` writable bytes
/// - `actual_size_out` and `valid_data_out` must be valid pointers
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_serialized(
    reader: *const Int2DdsDataReader,
    buffer: *mut u8,
    buffer_capacity: usize,
    actual_size_out: *mut usize,
    valid_data_out: *mut bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(buffer);
    check_null!(actual_size_out);
    check_null!(valid_data_out);

    let reader_ref = &*reader;

    let outcome = match reader_ref.inner.take_next_serialized_bounded(buffer_capacity) {
        Ok(outcome) => outcome,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *valid_data_out = false;
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (ret, info) = emit_bounded_serialized(outcome, buffer, actual_size_out);
    *valid_data_out = info.map_or(true, |i| i.valid_data);
    ret
}

/// Take pre-serialized data and loan the returned byte slice to the caller.
///
/// The returned `data_out` pointer remains valid until `loan_out` is passed to
/// `int2dds_datareader_return_serialized_loan`. This avoids copying the payload into a
/// caller-owned buffer for consumers that immediately deserialize the bytes.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `data_out`, `actual_size_out`, `valid_data_out`, and `loan_out` must be valid pointers
/// - if `*loan_out` is non-null, the caller must return it exactly once
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_serialized_loaned(
    reader: *const Int2DdsDataReader,
    data_out: *mut *const u8,
    actual_size_out: *mut usize,
    valid_data_out: *mut bool,
    loan_out: *mut *mut Int2DdsSerializedLoan,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(data_out);
    check_null!(actual_size_out);
    check_null!(valid_data_out);
    check_null!(loan_out);

    *data_out = std::ptr::null();
    *actual_size_out = 0;
    *valid_data_out = false;
    *loan_out = std::ptr::null_mut();

    let reader_ref = &*reader;

    let (serialized_data, sample_info) = match reader_ref.inner.take_next_serialized_bytes() {
        Ok(result) => result,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    *valid_data_out = sample_info.valid_data;
    *actual_size_out = serialized_data.len();

    if !sample_info.valid_data {
        return INT2DDS_RET_OK;
    }

    let loan = Box::new(Int2DdsSerializedLoan { data: serialized_data });
    *data_out = loan.data.as_ptr();
    *loan_out = Box::into_raw(loan);

    INT2DDS_RET_OK
}

/// Return a serialized data loan produced by `int2dds_datareader_take_serialized_loaned`.
///
/// # Safety
/// - `loan` must be null or a pointer returned by `int2dds_datareader_take_serialized_loaned`
/// - `loan` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_return_serialized_loan(
    loan: *mut Int2DdsSerializedLoan,
) -> Int2DdsRet {
    if !loan.is_null() {
        drop(Box::from_raw(loan));
    }
    INT2DDS_RET_OK
}

/// Read pre-serialized data from a DataReader without removing from cache.
///
/// Same as `int2dds_datareader_take_serialized` but the sample remains in the cache.
///
/// # Safety
/// - Same as `int2dds_datareader_take_serialized`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_serialized(
    reader: *const Int2DdsDataReader,
    buffer: *mut u8,
    buffer_capacity: usize,
    actual_size_out: *mut usize,
    valid_data_out: *mut bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(buffer);
    check_null!(actual_size_out);
    check_null!(valid_data_out);

    let reader_ref = &*reader;

    let outcome = match reader_ref.inner.read_next_serialized_bounded(buffer_capacity) {
        Ok(outcome) => outcome,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *valid_data_out = false;
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (ret, info) = emit_bounded_serialized(outcome, buffer, actual_size_out);
    *valid_data_out = info.map_or(true, |i| i.valid_data);
    ret
}

// ============================================================================
// Read/Take with SampleInfo (Feature 1)
// ============================================================================

/// Take pre-serialized data with full SampleInfo
///
/// # Safety
/// - Same as `int2dds_datareader_take_serialized`, plus `info_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_serialized_w_info(
    reader: *const Int2DdsDataReader,
    buffer: *mut u8,
    buffer_capacity: usize,
    actual_size_out: *mut usize,
    info_out: *mut Int2DdsSampleInfo,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(buffer);
    check_null!(actual_size_out);
    check_null!(info_out);

    let reader_ref = &*reader;

    let outcome = match reader_ref.inner.take_next_serialized_bounded(buffer_capacity) {
        Ok(outcome) => outcome,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (ret, info) = emit_bounded_serialized(outcome, buffer, actual_size_out);
    if let Some(info) = info {
        *info_out = Int2DdsSampleInfo::from(&info);
    }
    ret
}

/// Read pre-serialized data with full SampleInfo (sample remains in cache)
///
/// # Safety
/// - Same as `int2dds_datareader_read_serialized`, plus `info_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_serialized_w_info(
    reader: *const Int2DdsDataReader,
    buffer: *mut u8,
    buffer_capacity: usize,
    actual_size_out: *mut usize,
    info_out: *mut Int2DdsSampleInfo,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(buffer);
    check_null!(actual_size_out);
    check_null!(info_out);

    let reader_ref = &*reader;

    let outcome = match reader_ref.inner.read_next_serialized_bounded(buffer_capacity) {
        Ok(outcome) => outcome,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (ret, info) = emit_bounded_serialized(outcome, buffer, actual_size_out);
    if let Some(info) = info {
        *info_out = Int2DdsSampleInfo::from(&info);
    }
    ret
}

// ============================================================================
// Batch Read/Take (Feature 3)
// ============================================================================

/// Take multiple serialized samples as a batch
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `seq_out` must be a valid pointer to a null pointer
/// - The returned sequence must be freed with `int2dds_sample_seq_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_serialized_batch(
    reader: *const Int2DdsDataReader,
    max_samples: i32,
    seq_out: *mut *mut Int2DdsSampleSeq,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(seq_out);

    let reader_ref = &*reader;

    let samples = match reader_ref.inner.take_serialized(
        max_samples,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    ) {
        Ok(r) => r,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples: Vec::new() }));
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    if samples.is_empty() {
        *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples: Vec::new() }));
        return INT2DDS_RET_NO_DATA;
    }

    *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples }));
    INT2DDS_RET_OK
}

/// Read multiple serialized samples as a batch (samples remain in cache)
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `seq_out` must be a valid pointer to a null pointer
/// - The returned sequence must be freed with `int2dds_sample_seq_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_serialized_batch(
    reader: *const Int2DdsDataReader,
    max_samples: i32,
    seq_out: *mut *mut Int2DdsSampleSeq,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(seq_out);

    let reader_ref = &*reader;

    let samples = match reader_ref.inner.read_serialized(
        max_samples,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    ) {
        Ok(r) => r,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples: Vec::new() }));
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    if samples.is_empty() {
        *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples: Vec::new() }));
        return INT2DDS_RET_NO_DATA;
    }

    *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples }));
    INT2DDS_RET_OK
}

/// Take pre-serialized samples belonging to a single instance, as a batch.
///
/// `handle` is a 16-byte instance handle (e.g. from `int2dds_datareader_lookup_instance`
/// or a prior sample's info). A nil handle returns `INT2DDS_RET_BAD_PARAMETER`; an
/// unknown handle returns no samples. The mask arguments are bitmasks of the
/// SampleState/ViewState/InstanceState kinds; a mask of 0 selects "any".
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `handle` must point to a 16-byte instance handle
/// - `seq_out` must be a valid pointer to a null pointer
/// - The returned sequence must be freed with `int2dds_sample_seq_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_instance_serialized_batch(
    reader: *const Int2DdsDataReader,
    handle: *const [u8; 16],
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    seq_out: *mut *mut Int2DdsSampleSeq,
) -> Int2DdsRet {
    read_or_take_instance_serialized_batch(
        reader,
        handle,
        max_samples,
        sample_state_mask,
        view_state_mask,
        instance_state_mask,
        seq_out,
        true,
    )
}

/// Read pre-serialized samples belonging to a single instance, as a batch
/// (samples remain in the cache).
///
/// # Safety
/// - Same as `int2dds_datareader_take_instance_serialized_batch`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_instance_serialized_batch(
    reader: *const Int2DdsDataReader,
    handle: *const [u8; 16],
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    seq_out: *mut *mut Int2DdsSampleSeq,
) -> Int2DdsRet {
    read_or_take_instance_serialized_batch(
        reader,
        handle,
        max_samples,
        sample_state_mask,
        view_state_mask,
        instance_state_mask,
        seq_out,
        false,
    )
}

/// A mask of 0 selects ANY, mirroring the `extensibility = -1` default sentinel.
fn state_kinds_from_masks(
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
) -> ([SampleStateKind; 1], [ViewStateKind; 1], [InstanceStateKind; 1]) {
    (
        [if sample_state_mask == 0 {
            SampleStateKind::ANY_SAMPLE_STATE
        } else {
            SampleStateKind::from_bits_truncate(sample_state_mask)
        }],
        [if view_state_mask == 0 {
            ViewStateKind::ANY_VIEW_STATE
        } else {
            ViewStateKind::from_bits_truncate(view_state_mask)
        }],
        [if instance_state_mask == 0 {
            InstanceStateKind::ANY_INSTANCE_STATE
        } else {
            InstanceStateKind::from_bits_truncate(instance_state_mask)
        }],
    )
}

#[allow(clippy::too_many_arguments)]
unsafe fn read_or_take_instance_serialized_batch(
    reader: *const Int2DdsDataReader,
    handle: *const [u8; 16],
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    seq_out: *mut *mut Int2DdsSampleSeq,
    take: bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(handle);
    check_null!(seq_out);

    let reader_ref = &*reader;
    let instance_handle = InstanceHandle::new(*handle);
    let (ss, vs, is) =
        state_kinds_from_masks(sample_state_mask, view_state_mask, instance_state_mask);

    let result = if take {
        reader_ref.inner.take_instance_serialized(max_samples, instance_handle, &ss, &vs, &is)
    } else {
        reader_ref.inner.read_instance_serialized(max_samples, instance_handle, &ss, &vs, &is)
    };

    match result {
        Ok(samples) => {
            let empty = samples.is_empty();
            *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples }));
            if empty {
                INT2DDS_RET_NO_DATA
            } else {
                INT2DDS_RET_OK
            }
        }
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples: Vec::new() }));
            INT2DDS_RET_NO_DATA
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the number of samples in a sequence
#[no_mangle]
pub unsafe extern "C" fn int2dds_sample_seq_length(seq: *const Int2DdsSampleSeq) -> usize {
    if seq.is_null() {
        return 0;
    }
    (*seq).samples.len()
}

/// Get serialized data at a given index in the sequence
///
/// # Safety
/// - `seq` must be a valid sample sequence
/// - `index` must be less than the sequence length
/// - `buffer` must point to at least `buffer_capacity` writable bytes
/// - `actual_size_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_sample_seq_get_data(
    seq: *const Int2DdsSampleSeq,
    index: usize,
    buffer: *mut u8,
    buffer_capacity: usize,
    actual_size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(buffer);
    check_null!(actual_size_out);

    let seq_ref = &*seq;
    if index >= seq_ref.samples.len() {
        return INT2DDS_RET_INVALID_ARGUMENT;
    }

    let (data, _info) = &seq_ref.samples[index];
    *actual_size_out = data.len();

    if data.len() > buffer_capacity {
        return INT2DDS_RET_BUFFER_TOO_SMALL;
    }

    std::ptr::copy_nonoverlapping(data.as_ptr(), buffer, data.len());
    INT2DDS_RET_OK
}

/// Get SampleInfo at a given index in the sequence
///
/// # Safety
/// - `seq` must be a valid sample sequence
/// - `index` must be less than the sequence length
/// - `info_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_sample_seq_get_info(
    seq: *const Int2DdsSampleSeq,
    index: usize,
    info_out: *mut Int2DdsSampleInfo,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(info_out);

    let seq_ref = &*seq;
    if index >= seq_ref.samples.len() {
        return INT2DDS_RET_INVALID_ARGUMENT;
    }

    let (_data, info) = &seq_ref.samples[index];
    *info_out = Int2DdsSampleInfo::from(info);
    INT2DDS_RET_OK
}

/// Delete a sample sequence and free its memory
///
/// # Safety
/// - `seq` must be a valid sample sequence or null
/// - `seq` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_sample_seq_delete(seq: *mut Int2DdsSampleSeq) -> Int2DdsRet {
    if seq.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }
    let _ = Box::from_raw(seq);
    INT2DDS_RET_OK
}

// ============================================================================
// Read/Take with state condition filter (Feature 7)
// ============================================================================

/// Read a single serialized sample with state condition filter.
/// A state mask of 0 selects "any".
///
/// # Safety
/// - Same as `int2dds_datareader_read_serialized_w_info`, plus state masks
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_serialized_w_states(
    reader: *const Int2DdsDataReader,
    buffer: *mut u8,
    buffer_capacity: usize,
    actual_size_out: *mut usize,
    info_out: *mut Int2DdsSampleInfo,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(buffer);
    check_null!(actual_size_out);
    check_null!(info_out);

    let reader_ref = &*reader;
    let (ss, vs, is) =
        state_kinds_from_masks(sample_state_mask, view_state_mask, instance_state_mask);

    let outcome = match reader_ref.inner.read_serialized_bounded(&ss, &vs, &is, buffer_capacity) {
        Ok(outcome) => outcome,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (ret, info) = emit_bounded_serialized(outcome, buffer, actual_size_out);
    if let Some(info) = info {
        *info_out = Int2DdsSampleInfo::from(&info);
    }
    ret
}

/// Take a single serialized sample with state condition filter.
/// A state mask of 0 selects "any".
///
/// # Safety
/// - Same as `int2dds_datareader_take_serialized_w_info`, plus state masks
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_serialized_w_states(
    reader: *const Int2DdsDataReader,
    buffer: *mut u8,
    buffer_capacity: usize,
    actual_size_out: *mut usize,
    info_out: *mut Int2DdsSampleInfo,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(buffer);
    check_null!(actual_size_out);
    check_null!(info_out);

    let reader_ref = &*reader;
    let (ss, vs, is) =
        state_kinds_from_masks(sample_state_mask, view_state_mask, instance_state_mask);

    let outcome = match reader_ref.inner.take_serialized_bounded(&ss, &vs, &is, buffer_capacity) {
        Ok(outcome) => outcome,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (ret, info) = emit_bounded_serialized(outcome, buffer, actual_size_out);
    if let Some(info) = info {
        *info_out = Int2DdsSampleInfo::from(&info);
    }
    ret
}

/// Take batch with state condition filter. A state mask of 0 selects "any".
///
/// # Safety
/// - Same as `int2dds_datareader_take_serialized_batch`, plus state masks
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_serialized_batch_w_states(
    reader: *const Int2DdsDataReader,
    max_samples: i32,
    seq_out: *mut *mut Int2DdsSampleSeq,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(seq_out);

    let reader_ref = &*reader;
    let (ss, vs, is) =
        state_kinds_from_masks(sample_state_mask, view_state_mask, instance_state_mask);

    let samples = match reader_ref.inner.take_serialized(max_samples, &ss, &vs, &is) {
        Ok(r) => r,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples: Vec::new() }));
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    if samples.is_empty() {
        *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples: Vec::new() }));
        return INT2DDS_RET_NO_DATA;
    }

    *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples }));
    INT2DDS_RET_OK
}

/// Read batch with state condition filter. A state mask of 0 selects "any".
///
/// # Safety
/// - Same as `int2dds_datareader_read_serialized_batch`, plus state masks
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_serialized_batch_w_states(
    reader: *const Int2DdsDataReader,
    max_samples: i32,
    seq_out: *mut *mut Int2DdsSampleSeq,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(seq_out);

    let reader_ref = &*reader;
    let (ss, vs, is) =
        state_kinds_from_masks(sample_state_mask, view_state_mask, instance_state_mask);

    let samples = match reader_ref.inner.read_serialized(max_samples, &ss, &vs, &is) {
        Ok(r) => r,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples: Vec::new() }));
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    if samples.is_empty() {
        *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples: Vec::new() }));
        return INT2DDS_RET_NO_DATA;
    }

    *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples }));
    INT2DDS_RET_OK
}

/// Block until the DataReader has received all historical data from matched
/// TRANSIENT_LOCAL writers, or until the timeout expires.
///
/// # Safety
/// - `reader` must be a valid datareader
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_wait_for_historical_data(
    reader: *const Int2DdsDataReader,
    timeout_ms: i64,
) -> Int2DdsRet {
    check_null!(reader);

    let reader_ref = &*reader;
    let max_wait = Duration {
        sec: (timeout_ms / 1000) as i32,
        nanosec: ((timeout_ms % 1000) * 1_000_000) as u32,
    };

    match reader_ref.inner.wait_for_historical_data(max_wait) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{context, participant};
    use std::ptr;

    // Note: This test is disabled because it hangs during cleanup.
    // The DomainParticipant destruction involves background threads that
    // don't shutdown cleanly in test environment.
    #[test]
    #[ignore]
    fn test_subscriber_create_delete() {
        unsafe {
            let mut factory: *mut Int2DdsParticipantFactory = ptr::null_mut();
            let mut participant: *mut Int2DdsParticipant = ptr::null_mut();
            let mut subscriber: *mut Int2DdsSubscriber = ptr::null_mut();

            // Initialize
            context::int2dds_domain_participant_factory_get_instance(&mut factory as *mut _);
            participant::int2dds_create_participant(
                factory,
                0,
                ptr::null(),
                &mut participant as *mut _,
            );

            // Create subscriber
            let ret =
                int2dds_create_subscriber(participant, ptr::null(), &mut subscriber as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!subscriber.is_null());

            // Cleanup
            int2dds_delete_subscriber(subscriber);
            participant::int2dds_delete_participant(participant);
            context::int2dds_domain_participant_factory_finalize(factory);
        }
    }
}
