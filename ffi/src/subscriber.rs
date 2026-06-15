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
//! C users receive raw CDR bytes via `int2dds_take_serialized` / `int2dds_read_serialized`
//! and deserialize them with IDL-generated code.

use std::ffi::CStr;
use std::sync::Arc;

use bytes::Bytes;
use int2dds::{
    core::time::Duration,
    infrastructure::status::StatusMask,
    subscription::{
        data_reader_listener::DataReaderListener,
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
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
        Int2DdsRequestedIncompatibleQosStatus, Int2DdsSampleLostStatus,
        Int2DdsSampleRejectedStatus,
    },
    types::*,
};

/// Create a Subscriber
///
/// # Safety
/// - `participant` must be a valid participant
/// - `qos` can be null for default QoS
/// - `subscriber_out` must be a valid pointer to a null pointer
/// - The returned subscriber must be freed with `int2dds_delete_subscriber`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_subscriber(
    participant: *const Int2DdsParticipant,
    subscriber_out: *mut *mut Int2DdsSubscriber,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(subscriber_out);

    let participant_ref = &*participant;

    // Pass the default sentinel so the core resolution chain (registered
    // default → configured default profile → spec default) is engaged.
    let subscriber = ffi_try!(participant_ref.inner.create_subscriber(
        int2dds::subscription::qos::SUBSCRIBER_QOS_DEFAULT,
        None,
        StatusMask::default()
    ));

    // Wrap subscriber in Arc
    let subscriber_arc = Arc::new(subscriber);

    let subscriber_handle = Box::new(Int2DdsSubscriber { inner: subscriber_arc });

    *subscriber_out = Box::into_raw(subscriber_handle);

    INT2DDS_RET_OK
}

/// Create a Subscriber with QoS
///
/// # Safety
/// - `participant` must be a valid participant
/// - `qos` must be a valid subscriber QoS handle
/// - `subscriber_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_subscriber_with_qos(
    participant: *const Int2DdsParticipant,
    qos: *const Int2DdsSubscriberQos,
    subscriber_out: *mut *mut Int2DdsSubscriber,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(qos);
    check_null!(subscriber_out);

    let participant_ref = &*participant;
    let qos_ref = &*qos;

    let subscriber = ffi_try!(participant_ref.inner.create_subscriber(
        qos_ref.inner.clone(),
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

    let subscriber_ref = &*subscriber;
    if Arc::strong_count(&subscriber_ref.inner) != 1 {
        return INT2DDS_RET_PRECONDITION_NOT_MET;
    }

    // Destructure Box to move Arc out
    let Int2DdsSubscriber { inner: subscriber_arc } = *Box::from_raw(subscriber);

    // Try to unwrap Arc without cloning (succeeds if this is the only reference)
    let subscriber_obj = match Arc::try_unwrap(subscriber_arc) {
        Ok(s) => s,
        Err(_arc) => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };

    // Get the participant to delete the subscriber
    let participant = match subscriber_obj.get_participant() {
        Ok(p) => p,
        Err(e) => return dds_error_to_code(&e),
    };

    match participant.delete_subscriber(subscriber_obj) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Create a DataReader
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `topic` must be a valid topic
/// - `qos` can be null for default QoS
/// - `reader_out` must be a valid pointer to a null pointer
/// - The returned reader must be freed with `int2dds_delete_datareader`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader(
    subscriber: *const Int2DdsSubscriber,
    topic: *const Int2DdsTopic,
    qos: *const Int2DdsDataReaderQos,
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

    // Create DataReader<Int2DdsData>
    let reader = ffi_try!(subscriber_ref.inner.create_datareader::<Int2DdsData>(
        &*topic_ref.inner,
        reader_qos,
        None,
        StatusMask::default()
    ));

    let reader_handle = Box::new(Int2DdsDataReader { inner: reader, listener: None });

    *reader_out = Box::into_raw(reader_handle);

    INT2DDS_RET_OK
}

/// Create a DataReader with listener callbacks
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `topic` must be a valid topic
/// - `qos` can be null for default QoS
/// - `listener` can be null for no listener
/// - `mask` specifies which status changes trigger callbacks
/// - `reader_out` must be a valid pointer to a null pointer
/// - The returned reader must be freed with `int2dds_delete_datareader`
/// - Listener callbacks must be thread-safe and remain valid until reader is deleted
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader_with_listener(
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

    // Create DataReader<Int2DdsData> first without listener
    let reader = ffi_try!(subscriber_ref.inner.create_datareader::<Int2DdsData>(
        &*topic_ref.inner,
        reader_qos,
        None,
        StatusMask::from_bits_truncate(mask)
    ));

    // Create reader_handle with the actual reader
    let mut reader_handle = Box::new(Int2DdsDataReader { inner: reader, listener: None });

    // If listener is provided, set it now
    if !listener.is_null() {
        let reader_ptr = &mut *reader_handle as *mut Int2DdsDataReader;
        let ffi_listener = FfiDataReaderListener::new(*listener, reader_ptr);
        let listener_arc = Arc::new(ffi_listener);

        // Set the listener on the reader
        let listener_clone = listener_arc.clone()
            as Arc<
                dyn int2dds::subscription::data_reader_listener::DataReaderListener<
                    Foo = Int2DdsData,
                >,
            >;
        ffi_try!(reader_handle
            .inner
            .set_listener(Some(listener_clone), StatusMask::from_bits_truncate(mask)));

        reader_handle.listener = Some(listener_arc.clone());

        // Check if matching already occurred before the listener was set.
        // This handles the race condition where SEDP matching completes between
        // create_datareader() and set_listener().
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
    }

    *reader_out = Box::into_raw(reader_handle);

    INT2DDS_RET_OK
}

/// Create a DataReader using a QoS profile path
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `topic` must be a valid topic
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `reader_out` must be a valid pointer to a null pointer
/// - The returned reader must be freed with `int2dds_delete_datareader`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader_with_profile(
    subscriber: *const Int2DdsSubscriber,
    topic: *const Int2DdsTopic,
    qos_path: *const std::os::raw::c_char,
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
        StatusMask::default()
    ));

    let reader_handle = Box::new(Int2DdsDataReader { inner: reader, listener: None });

    *reader_out = Box::into_raw(reader_handle);

    INT2DDS_RET_OK
}

/// Create a DataReader with listener callbacks using a QoS profile path
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `topic` must be a valid topic
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `listener` can be null for no listener
/// - `mask` specifies which status changes trigger callbacks
/// - `reader_out` must be a valid pointer to a null pointer
/// - The returned reader must be freed with `int2dds_delete_datareader`
/// - Listener callbacks must be thread-safe and remain valid until reader is deleted
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader_with_profile_and_listener(
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

    // Create DataReader<Int2DdsData> first without listener
    let reader = ffi_try!(subscriber_ref.inner.create_datareader_with_profile::<Int2DdsData>(
        &*topic_ref.inner,
        qos_path_str,
        None,
        StatusMask::from_bits_truncate(mask)
    ));

    // Create reader_handle with the actual reader
    let mut reader_handle = Box::new(Int2DdsDataReader { inner: reader, listener: None });

    // If listener is provided, set it now
    if !listener.is_null() {
        let reader_ptr = &mut *reader_handle as *mut Int2DdsDataReader;
        let ffi_listener = FfiDataReaderListener::new(*listener, reader_ptr);
        let listener_arc = Arc::new(ffi_listener);

        // Set the listener on the reader
        let listener_clone = listener_arc.clone()
            as Arc<
                dyn int2dds::subscription::data_reader_listener::DataReaderListener<
                    Foo = Int2DdsData,
                >,
            >;
        ffi_try!(reader_handle
            .inner
            .set_listener(Some(listener_clone), StatusMask::from_bits_truncate(mask)));

        reader_handle.listener = Some(listener_arc.clone());

        // Check if matching already occurred before the listener was set.
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
    }

    *reader_out = Box::into_raw(reader_handle);

    INT2DDS_RET_OK
}

/// Create a DataReader using a ContentFilteredTopic
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `cft` must be a valid ContentFilteredTopic
/// - `qos` can be null for default QoS
/// - `reader_out` must be a valid pointer to a null pointer
/// - The returned reader must be freed with `int2dds_delete_datareader`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader_cft(
    subscriber: *const Int2DdsSubscriber,
    cft: *const Int2DdsContentFilteredTopic,
    qos: *const Int2DdsDataReaderQos,
    reader_out: *mut *mut Int2DdsDataReader,
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(cft);
    check_null!(reader_out);

    let subscriber_ref = &*subscriber;
    let cft_ref = &*cft;

    let reader_qos = if qos.is_null() {
        int2dds::subscription::qos::DataReaderQos::default()
    } else {
        (*qos).inner.clone()
    };

    let reader = ffi_try!(subscriber_ref.inner.create_datareader::<Int2DdsData>(
        &cft_ref.inner,
        reader_qos,
        None,
        StatusMask::default()
    ));

    let reader_handle = Box::new(Int2DdsDataReader { inner: reader, listener: None });
    *reader_out = Box::into_raw(reader_handle);

    INT2DDS_RET_OK
}

/// Create a DataReader using a ContentFilteredTopic with listener callbacks
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `cft` must be a valid ContentFilteredTopic
/// - `qos` can be null for default QoS
/// - `listener` can be null for no listener
/// - `mask` specifies which status changes trigger callbacks
/// - `reader_out` must be a valid pointer to a null pointer
/// - The returned reader must be freed with `int2dds_delete_datareader`
/// - Listener callbacks must be thread-safe and remain valid until reader is deleted
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader_cft_with_listener(
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

    let reader_qos = if qos.is_null() {
        int2dds::subscription::qos::DataReaderQos::default()
    } else {
        (*qos).inner.clone()
    };

    let reader = ffi_try!(subscriber_ref.inner.create_datareader::<Int2DdsData>(
        &cft_ref.inner,
        reader_qos,
        None,
        StatusMask::from_bits_truncate(mask)
    ));

    let mut reader_handle = Box::new(Int2DdsDataReader { inner: reader, listener: None });

    if !listener.is_null() {
        let reader_ptr = &mut *reader_handle as *mut Int2DdsDataReader;
        let ffi_listener = FfiDataReaderListener::new(*listener, reader_ptr);
        let listener_arc = Arc::new(ffi_listener);

        let listener_clone = listener_arc.clone()
            as Arc<
                dyn int2dds::subscription::data_reader_listener::DataReaderListener<
                    Foo = Int2DdsData,
                >,
            >;
        ffi_try!(reader_handle
            .inner
            .set_listener(Some(listener_clone), StatusMask::from_bits_truncate(mask)));

        reader_handle.listener = Some(listener_arc.clone());

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
    }

    *reader_out = Box::into_raw(reader_handle);

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

    let reader_ref = &mut *reader;

    // Create new listener wrapper if provided
    let listener_arc = if !listener.is_null() {
        let ffi_listener = FfiDataReaderListener::new(*listener, reader);
        Some(Arc::new(ffi_listener))
    } else {
        None
    };

    // Set listener on the inner reader
    let result = reader_ref.inner.set_listener(
        listener_arc.clone().map(|l| {
            l as Arc<
                dyn int2dds::subscription::data_reader_listener::DataReaderListener<
                    Foo = Int2DdsData,
                >,
            >
        }),
        StatusMask::from_bits_truncate(mask),
    );

    // Update the stored listener
    reader_ref.listener = listener_arc;

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
    if let Some(listener_arc) = &reader_ref.listener {
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

    let reader_box = Box::from_raw(reader);
    let reader_obj = reader_box.inner;

    // Get the subscriber to delete the reader
    let subscriber = match reader_obj.get_subscriber() {
        Ok(s) => s,
        Err(e) => return dds_error_to_code(&e),
    };

    match subscriber.delete_datareader(reader_obj) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get subscription matched status
///
/// Returns the number of matched writers for this DataReader.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `total_count_out` must be a valid pointer
/// - `current_count_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_get_subscription_matched_status(
    reader: *const Int2DdsDataReader,
    total_count_out: *mut i32,
    current_count_out: *mut i32,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(total_count_out);
    check_null!(current_count_out);

    let reader_ref = &*reader;

    match reader_ref.inner.get_subscription_matched_status() {
        Ok(status) => {
            *total_count_out = status.total_count();
            *current_count_out = status.current_count();
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

/// Take pre-serialized data from a DataReader, bypassing TypeSupport deserialization.
///
/// Copies the raw CDR bytes (including encapsulation header) into the caller's buffer.
/// The sample is removed from the cache.
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
/// - INT2DDS_RET_ERROR if buffer is too small (actual_size_out will contain the required size)
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `buffer` must point to at least `buffer_capacity` writable bytes
/// - `actual_size_out` and `valid_data_out` must be valid pointers
#[no_mangle]
pub unsafe extern "C" fn int2dds_take_serialized(
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

    let (serialized_data, sample_info) = match reader_ref.inner.take_next_serialized_bytes() {
        Ok(result) => result,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *valid_data_out = false;
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    *valid_data_out = sample_info.valid_data;
    *actual_size_out = serialized_data.len();

    if !sample_info.valid_data {
        return INT2DDS_RET_OK;
    }

    if serialized_data.len() > buffer_capacity {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(serialized_data.as_ptr(), buffer, serialized_data.len());

    INT2DDS_RET_OK
}

/// Take pre-serialized data and loan the returned byte slice to the caller.
///
/// The returned `data_out` pointer remains valid until `loan_out` is passed to
/// `int2dds_return_serialized_loan`. This avoids copying the payload into a
/// caller-owned buffer for consumers that immediately deserialize the bytes.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `data_out`, `actual_size_out`, `valid_data_out`, and `loan_out` must be valid pointers
/// - if `*loan_out` is non-null, the caller must return it exactly once
#[no_mangle]
pub unsafe extern "C" fn int2dds_take_serialized_loaned(
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

/// Return a serialized data loan produced by `int2dds_take_serialized_loaned`.
///
/// # Safety
/// - `loan` must be null or a pointer returned by `int2dds_take_serialized_loaned`
/// - `loan` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_return_serialized_loan(
    loan: *mut Int2DdsSerializedLoan,
) -> Int2DdsRet {
    if !loan.is_null() {
        drop(Box::from_raw(loan));
    }
    INT2DDS_RET_OK
}

/// Read pre-serialized data from a DataReader without removing from cache.
///
/// Same as `int2dds_take_serialized` but the sample remains in the cache.
///
/// # Safety
/// - Same as `int2dds_take_serialized`
#[no_mangle]
pub unsafe extern "C" fn int2dds_read_serialized(
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

    // Use read_serialized with NOT_READ state for "next" semantics
    let results = match reader_ref.inner.read_serialized(
        1,
        &[int2dds::subscription::sample_info::SampleStateKind::NOT_READ_SAMPLE_STATE],
        &[int2dds::subscription::sample_info::ViewStateKind::ANY_VIEW_STATE],
        &[int2dds::subscription::sample_info::InstanceStateKind::ANY_INSTANCE_STATE],
    ) {
        Ok(r) => r,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *valid_data_out = false;
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (serialized_data, sample_info) = match results.into_iter().next() {
        Some(item) => item,
        None => {
            *valid_data_out = false;
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
    };

    *valid_data_out = sample_info.valid_data;
    *actual_size_out = serialized_data.len();

    if !sample_info.valid_data {
        return INT2DDS_RET_OK;
    }

    if serialized_data.len() > buffer_capacity {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(serialized_data.as_ptr(), buffer, serialized_data.len());

    INT2DDS_RET_OK
}

// ============================================================================
// Read/Take with SampleInfo (Feature 1)
// ============================================================================

/// Take pre-serialized data with full SampleInfo
///
/// # Safety
/// - Same as `int2dds_take_serialized`, plus `info_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_take_serialized_w_info(
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

    let (serialized_data, sample_info) = match reader_ref.inner.take_next_serialized() {
        Ok(result) => result,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    *info_out = Int2DdsSampleInfo::from(&sample_info);
    *actual_size_out = serialized_data.len();

    if !sample_info.valid_data {
        return INT2DDS_RET_OK;
    }

    if serialized_data.len() > buffer_capacity {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(serialized_data.as_ptr(), buffer, serialized_data.len());

    INT2DDS_RET_OK
}

/// Read pre-serialized data with full SampleInfo (sample remains in cache)
///
/// # Safety
/// - Same as `int2dds_read_serialized`, plus `info_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_read_serialized_w_info(
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

    let results = match reader_ref.inner.read_serialized(
        1,
        &[SampleStateKind::NOT_READ_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    ) {
        Ok(r) => r,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (serialized_data, sample_info) = match results.into_iter().next() {
        Some(item) => item,
        None => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
    };

    *info_out = Int2DdsSampleInfo::from(&sample_info);
    *actual_size_out = serialized_data.len();

    if !sample_info.valid_data {
        return INT2DDS_RET_OK;
    }

    if serialized_data.len() > buffer_capacity {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(serialized_data.as_ptr(), buffer, serialized_data.len());

    INT2DDS_RET_OK
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
pub unsafe extern "C" fn int2dds_take_serialized_batch(
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
pub unsafe extern "C" fn int2dds_read_serialized_batch(
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
        return INT2DDS_RET_ERROR;
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

/// Read a single serialized sample with state condition filter
///
/// # Safety
/// - Same as `int2dds_read_serialized_w_info`, plus state masks
#[no_mangle]
pub unsafe extern "C" fn int2dds_read_serialized_w_condition(
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

    let results = match reader_ref.inner.read_serialized(
        1,
        &[SampleStateKind::from_bits_truncate(sample_state_mask)],
        &[ViewStateKind::from_bits_truncate(view_state_mask)],
        &[InstanceStateKind::from_bits_truncate(instance_state_mask)],
    ) {
        Ok(r) => r,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (serialized_data, sample_info) = match results.into_iter().next() {
        Some(item) => item,
        None => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
    };

    *info_out = Int2DdsSampleInfo::from(&sample_info);
    *actual_size_out = serialized_data.len();

    if !sample_info.valid_data {
        return INT2DDS_RET_OK;
    }

    if serialized_data.len() > buffer_capacity {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(serialized_data.as_ptr(), buffer, serialized_data.len());
    INT2DDS_RET_OK
}

/// Take a single serialized sample with state condition filter
///
/// # Safety
/// - Same as `int2dds_take_serialized_w_info`, plus state masks
#[no_mangle]
pub unsafe extern "C" fn int2dds_take_serialized_w_condition(
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

    let results = match reader_ref.inner.take_serialized(
        1,
        &[SampleStateKind::from_bits_truncate(sample_state_mask)],
        &[ViewStateKind::from_bits_truncate(view_state_mask)],
        &[InstanceStateKind::from_bits_truncate(instance_state_mask)],
    ) {
        Ok(r) => r,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let (serialized_data, sample_info) = match results.into_iter().next() {
        Some(item) => item,
        None => {
            *actual_size_out = 0;
            return INT2DDS_RET_NO_DATA;
        }
    };

    *info_out = Int2DdsSampleInfo::from(&sample_info);
    *actual_size_out = serialized_data.len();

    if !sample_info.valid_data {
        return INT2DDS_RET_OK;
    }

    if serialized_data.len() > buffer_capacity {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(serialized_data.as_ptr(), buffer, serialized_data.len());
    INT2DDS_RET_OK
}

/// Take batch with state condition filter
///
/// # Safety
/// - Same as `int2dds_take_serialized_batch`, plus state masks
#[no_mangle]
pub unsafe extern "C" fn int2dds_take_serialized_batch_w_condition(
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

    let samples = match reader_ref.inner.take_serialized(
        max_samples,
        &[SampleStateKind::from_bits_truncate(sample_state_mask)],
        &[ViewStateKind::from_bits_truncate(view_state_mask)],
        &[InstanceStateKind::from_bits_truncate(instance_state_mask)],
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

/// Read batch with state condition filter
///
/// # Safety
/// - Same as `int2dds_read_serialized_batch`, plus state masks
#[no_mangle]
pub unsafe extern "C" fn int2dds_read_serialized_batch_w_condition(
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

    let samples = match reader_ref.inner.read_serialized(
        max_samples,
        &[SampleStateKind::from_bits_truncate(sample_state_mask)],
        &[ViewStateKind::from_bits_truncate(view_state_mask)],
        &[InstanceStateKind::from_bits_truncate(instance_state_mask)],
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
                ptr::null(),
                0,
                &mut participant as *mut _,
            );

            // Create subscriber
            let ret = int2dds_create_subscriber(participant, &mut subscriber as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!subscriber.is_null());

            // Cleanup
            int2dds_delete_subscriber(subscriber);
            participant::int2dds_delete_participant(participant);
            context::int2dds_domain_participant_factory_finalize(factory);
        }
    }
}
