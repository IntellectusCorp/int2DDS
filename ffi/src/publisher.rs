//! # Publisher and DataWriter
//!
//! Functions for creating Publishers and writing serialized data.
//!
//! ## Overview
//!
//! Publishers group DataWriters and manage their QoS settings. DataWriters
//! send data samples to matching DataReaders on the same Topic.
//!
//! ## Data Format
//!
//! C users serialize data with IDL-generated code and pass CDR bytes
//! directly via `int2dds_write_serialized`.

use std::ffi::CStr;
use std::sync::{Arc, RwLock};

use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::{Duration, Time},
    infrastructure::status::StatusMask,
    publication::{data_writer::SerializedWriteLoan, data_writer_listener::DataWriterListener},
};

use crate::data::Int2DdsData;

pub struct Int2DdsSerializedWriteLoan {
    inner: Option<SerializedWriteLoan>,
}

use super::{
    error::*,
    listener::{FfiDataWriterListener, Int2DdsDataWriterListener},
    qos::{Int2DdsDataWriterQos, Int2DdsPublisherQos},
    status::{
        Int2DdsLivelinessLostStatus, Int2DdsOfferedDeadlineMissedStatus,
        Int2DdsOfferedIncompatibleQosStatus, Int2DdsOfferedIncompatibleTypeStatus,
    },
    types::*,
};

/// Create a Publisher
///
/// # Safety
/// - `participant` must be a valid participant
/// - `qos` can be null for default QoS
/// - `publisher_out` must be a valid pointer to a null pointer
/// - The returned publisher must be freed with `int2dds_delete_publisher`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_publisher(
    participant: *const Int2DdsParticipant,
    publisher_out: *mut *mut Int2DdsPublisher,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(publisher_out);

    let participant_ref = &*participant;

    // Pass the default sentinel so the core resolution chain (registered
    // default → configured default profile → spec default) is engaged.
    let publisher = ffi_try!(participant_ref.inner.create_publisher(
        int2dds::publication::qos::PUBLISHER_QOS_DEFAULT,
        None,
        StatusMask::default()
    ));

    // Wrap publisher in Arc
    let publisher_arc = Arc::new(publisher);

    let publisher_handle = Box::new(Int2DdsPublisher { inner: publisher_arc });

    *publisher_out = Box::into_raw(publisher_handle);

    INT2DDS_RET_OK
}

/// Create a Publisher with QoS
///
/// # Safety
/// - `participant` must be a valid participant
/// - `qos` must be a valid publisher QoS handle
/// - `publisher_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_publisher_with_qos(
    participant: *const Int2DdsParticipant,
    qos: *const Int2DdsPublisherQos,
    publisher_out: *mut *mut Int2DdsPublisher,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(qos);
    check_null!(publisher_out);

    let participant_ref = &*participant;
    let qos_ref = &*qos;

    let publisher = ffi_try!(participant_ref.inner.create_publisher(
        qos_ref.inner.clone(),
        None,
        StatusMask::default()
    ));

    let publisher_arc = Arc::new(publisher);
    let publisher_handle = Box::new(Int2DdsPublisher { inner: publisher_arc });
    *publisher_out = Box::into_raw(publisher_handle);

    INT2DDS_RET_OK
}

/// Create a Publisher using a QoS profile path
///
/// # Safety
/// - `participant` must be a valid participant
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `publisher_out` must be a valid pointer to a null pointer
/// - The returned publisher must be freed with `int2dds_delete_publisher`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_publisher_with_profile(
    participant: *const Int2DdsParticipant,
    qos_path: *const std::os::raw::c_char,
    publisher_out: *mut *mut Int2DdsPublisher,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(qos_path);
    check_null!(publisher_out);

    let participant_ref = &*participant;

    let qos_path_str = match CStr::from_ptr(qos_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let publisher = ffi_try!(participant_ref.inner.create_publisher_with_profile(
        qos_path_str,
        None,
        StatusMask::default()
    ));

    let publisher_arc = Arc::new(publisher);
    let publisher_handle = Box::new(Int2DdsPublisher { inner: publisher_arc });
    *publisher_out = Box::into_raw(publisher_handle);

    INT2DDS_RET_OK
}

/// Set QoS on a Publisher
///
/// Applies new QoS policies to an existing Publisher. Some policies can only
/// be changed before the entity is enabled; attempting to change immutable
/// policies on an enabled entity returns IMMUTABLE_POLICY.
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `qos` must be a valid publisher QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_set_qos(
    publisher: *const Int2DdsPublisher,
    qos: *const Int2DdsPublisherQos,
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(qos);

    let publisher_ref = &*publisher;
    let qos_ref = &*qos;

    ffi_try!(publisher_ref.inner.set_qos(qos_ref.inner.clone()));

    INT2DDS_RET_OK
}

/// Get QoS from a Publisher
///
/// Returns a new QoS handle containing the current QoS policies of the Publisher.
/// The returned handle must be freed with `int2dds_publisher_qos_destroy`.
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `qos_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_get_qos(
    publisher: *const Int2DdsPublisher,
    qos_out: *mut *mut Int2DdsPublisherQos,
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(qos_out);

    let publisher_ref = &*publisher;
    let qos = ffi_try!(publisher_ref.inner.get_qos());
    let boxed = Box::new(Int2DdsPublisherQos { inner: qos });
    *qos_out = Box::into_raw(boxed);

    INT2DDS_RET_OK
}

/// Get the 16-byte instance handle of a Publisher.
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `handle_out` must point to a 16-byte buffer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_get_instance_handle(
    publisher: *const Int2DdsPublisher,
    handle_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(handle_out);

    let publisher_ref = &*publisher;
    let handle = ffi_try!(publisher_ref.inner.get_instance_handle());
    *handle_out = *handle.value();

    INT2DDS_RET_OK
}

/// Set QoS on a DataWriter
///
/// Applies new QoS policies to an existing DataWriter. Some policies can only
/// be changed before the entity is enabled; attempting to change immutable
/// policies on an enabled entity returns IMMUTABLE_POLICY.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `qos` must be a valid datawriter QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_set_qos(
    writer: *const Int2DdsDataWriter,
    qos: *const Int2DdsDataWriterQos,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(qos);

    let writer_ref = &*writer;
    let qos_ref = &*qos;

    ffi_try!(writer_ref.inner.set_qos(qos_ref.inner.clone()));

    INT2DDS_RET_OK
}

/// Get QoS from a DataWriter
///
/// Returns a new QoS handle containing the current QoS policies of the DataWriter.
/// The returned handle must be freed with `int2dds_datawriter_qos_destroy`.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `qos_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_qos(
    writer: *const Int2DdsDataWriter,
    qos_out: *mut *mut Int2DdsDataWriterQos,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(qos_out);

    let writer_ref = &*writer;
    let qos = ffi_try!(writer_ref.inner.get_qos());
    let boxed = Box::new(Int2DdsDataWriterQos { inner: qos });
    *qos_out = Box::into_raw(boxed);

    INT2DDS_RET_OK
}

/// Get the 16-byte RTPS GUID of a DataWriter.
///
/// Writes the writer's endpoint GUID (the same value advertised over SEDP
/// discovery as `endpoint_guid`) into `guid_out`. Read-only.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `guid_out` must be a valid pointer to a 16-byte buffer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_guid(
    writer: *const Int2DdsDataWriter,
    guid_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(guid_out);

    let writer_ref = &*writer;
    *guid_out = writer_ref.inner.guid().to_bytes();

    INT2DDS_RET_OK
}

/// Delete a Publisher
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `publisher` must not be used after this call
/// - All DataWriters created by this publisher must be deleted first
#[no_mangle]
pub unsafe extern "C" fn int2dds_delete_publisher(publisher: *mut Int2DdsPublisher) -> Int2DdsRet {
    if publisher.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let publisher_ref = &*publisher;
    if Arc::strong_count(&publisher_ref.inner) != 1 {
        return INT2DDS_RET_PRECONDITION_NOT_MET;
    }

    // Destructure Box to move Arc out
    let Int2DdsPublisher { inner: publisher_arc } = *Box::from_raw(publisher);

    // Try to unwrap Arc without cloning (succeeds if this is the only reference)
    let publisher_obj = match Arc::try_unwrap(publisher_arc) {
        Ok(p) => p,
        Err(_arc) => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };

    // Get the participant to delete the publisher
    let participant = match publisher_obj.get_participant() {
        Ok(p) => p,
        Err(e) => return dds_error_to_code(&e),
    };

    match participant.delete_publisher(publisher_obj) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Create a DataWriter
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `topic` must be a valid topic
/// - `qos` can be null for default QoS
/// - `writer_out` must be a valid pointer to a null pointer
/// - The returned writer must be freed with `int2dds_delete_datawriter`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datawriter(
    publisher: *const Int2DdsPublisher,
    topic: *const Int2DdsTopic,
    qos: *const Int2DdsDataWriterQos,
    writer_out: *mut *mut Int2DdsDataWriter,
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(topic);
    check_null!(writer_out);

    let publisher_ref = &*publisher;
    let topic_ref = &*topic;

    // NULL qos → default sentinel (engages profile fallback). Non-NULL → use as-is.
    let writer_qos = if qos.is_null() {
        int2dds::infrastructure::qos_kind::QosKind::Default
    } else {
        int2dds::infrastructure::qos_kind::QosKind::Specific((*qos).inner.clone())
    };

    // Create DataWriter<Int2DdsData>
    let writer = ffi_try!(publisher_ref.inner.create_datawriter::<Int2DdsData>(
        &topic_ref.inner,
        writer_qos,
        None,
        StatusMask::default()
    ));

    let writer_handle = Arc::new(Int2DdsDataWriter { inner: writer, listener: RwLock::new(None) });

    *writer_out = Arc::into_raw(writer_handle) as *mut Int2DdsDataWriter;

    INT2DDS_RET_OK
}

/// Create a DataWriter with listener callbacks
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `topic` must be a valid topic
/// - `qos` can be null for default QoS
/// - `listener` can be null for no listener
/// - `mask` specifies which status changes trigger callbacks
/// - `writer_out` must be a valid pointer to a null pointer
/// - The returned writer must be freed with `int2dds_delete_datawriter`
/// - Listener callbacks must be thread-safe and remain valid until writer is deleted
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datawriter_with_listener(
    publisher: *const Int2DdsPublisher,
    topic: *const Int2DdsTopic,
    qos: *const Int2DdsDataWriterQos,
    listener: *const Int2DdsDataWriterListener,
    mask: u32,
    writer_out: *mut *mut Int2DdsDataWriter,
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(topic);
    check_null!(writer_out);

    let publisher_ref = &*publisher;
    let topic_ref = &*topic;

    // NULL qos → default sentinel (engages profile fallback). Non-NULL → use as-is.
    let writer_qos = if qos.is_null() {
        int2dds::infrastructure::qos_kind::QosKind::Default
    } else {
        int2dds::infrastructure::qos_kind::QosKind::Specific((*qos).inner.clone())
    };

    // Create DataWriter<Int2DdsData> first without listener
    let writer = ffi_try!(publisher_ref.inner.create_datawriter::<Int2DdsData>(
        &topic_ref.inner,
        writer_qos,
        None,
        StatusMask::from_bits_truncate(mask)
    ));

    // Create writer_handle with the actual writer
    let writer_handle = Arc::new(Int2DdsDataWriter { inner: writer, listener: RwLock::new(None) });

    // If listener is provided, set it now
    if !listener.is_null() {
        let weak = Arc::downgrade(&writer_handle);
        let listener_arc = Arc::new(FfiDataWriterListener::new(*listener, weak));

        // Set the listener on the writer
        let listener_clone = listener_arc.clone()
            as Arc<
                dyn int2dds::publication::data_writer_listener::DataWriterListener<
                    Foo = Int2DdsData,
                >,
            >;
        ffi_try!(writer_handle
            .inner
            .set_listener(Some(listener_clone), StatusMask::from_bits_truncate(mask)));

        *writer_handle.listener.write().unwrap() = Some(listener_arc.clone());

        // Check if matching already occurred before the listener was set.
        // This handles the race condition where SEDP matching completes between
        // create_datawriter() and set_listener().
        if mask & crate::status_condition::INT2DDS_STATUS_PUBLICATION_MATCHED != 0 {
            if let Ok(status) = writer_handle.inner.get_publication_matched_status() {
                if status.current_count() > 0 {
                    listener_arc.on_publication_matched(&writer_handle.inner, &status);
                }
            }
        }
        if mask & crate::status_condition::INT2DDS_STATUS_OFFERED_INCOMPATIBLE_QOS != 0 {
            if let Ok(status) = writer_handle.inner.get_offered_incompatible_qos_status() {
                if status.total_count() > 0 {
                    listener_arc.on_offered_incompatible_qos(&writer_handle.inner, &status);
                }
            }
        }
    }

    *writer_out = Arc::into_raw(writer_handle) as *mut Int2DdsDataWriter;

    INT2DDS_RET_OK
}

/// Create a DataWriter using a QoS profile path
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `topic` must be a valid topic
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `writer_out` must be a valid pointer to a null pointer
/// - The returned writer must be freed with `int2dds_delete_datawriter`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datawriter_with_profile(
    publisher: *const Int2DdsPublisher,
    topic: *const Int2DdsTopic,
    qos_path: *const std::os::raw::c_char,
    writer_out: *mut *mut Int2DdsDataWriter,
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(topic);
    check_null!(qos_path);
    check_null!(writer_out);

    let publisher_ref = &*publisher;
    let topic_ref = &*topic;

    let qos_path_str = match CStr::from_ptr(qos_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let writer = ffi_try!(publisher_ref.inner.create_datawriter_with_profile::<Int2DdsData>(
        &topic_ref.inner,
        qos_path_str,
        None,
        StatusMask::default()
    ));

    let writer_handle = Arc::new(Int2DdsDataWriter { inner: writer, listener: RwLock::new(None) });

    *writer_out = Arc::into_raw(writer_handle) as *mut Int2DdsDataWriter;

    INT2DDS_RET_OK
}

/// Create a DataWriter with listener callbacks using a QoS profile path
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `topic` must be a valid topic
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `listener` can be null for no listener
/// - `mask` specifies which status changes trigger callbacks
/// - `writer_out` must be a valid pointer to a null pointer
/// - The returned writer must be freed with `int2dds_delete_datawriter`
/// - Listener callbacks must be thread-safe and remain valid until writer is deleted
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datawriter_with_profile_and_listener(
    publisher: *const Int2DdsPublisher,
    topic: *const Int2DdsTopic,
    qos_path: *const std::os::raw::c_char,
    listener: *const Int2DdsDataWriterListener,
    mask: u32,
    writer_out: *mut *mut Int2DdsDataWriter,
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(topic);
    check_null!(qos_path);
    check_null!(writer_out);

    let publisher_ref = &*publisher;
    let topic_ref = &*topic;

    let qos_path_str = match CStr::from_ptr(qos_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    // Create DataWriter<Int2DdsData> first without listener
    let writer = ffi_try!(publisher_ref.inner.create_datawriter_with_profile::<Int2DdsData>(
        &topic_ref.inner,
        qos_path_str,
        None,
        StatusMask::from_bits_truncate(mask)
    ));

    // Create writer_handle with the actual writer
    let writer_handle = Arc::new(Int2DdsDataWriter { inner: writer, listener: RwLock::new(None) });

    // If listener is provided, set it now
    if !listener.is_null() {
        let weak = Arc::downgrade(&writer_handle);
        let listener_arc = Arc::new(FfiDataWriterListener::new(*listener, weak));

        // Set the listener on the writer
        let listener_clone = listener_arc.clone()
            as Arc<
                dyn int2dds::publication::data_writer_listener::DataWriterListener<
                    Foo = Int2DdsData,
                >,
            >;
        ffi_try!(writer_handle
            .inner
            .set_listener(Some(listener_clone), StatusMask::from_bits_truncate(mask)));

        *writer_handle.listener.write().unwrap() = Some(listener_arc.clone());

        // Check if matching already occurred before the listener was set.
        if mask & crate::status_condition::INT2DDS_STATUS_PUBLICATION_MATCHED != 0 {
            if let Ok(status) = writer_handle.inner.get_publication_matched_status() {
                if status.current_count() > 0 {
                    listener_arc.on_publication_matched(&writer_handle.inner, &status);
                }
            }
        }
        if mask & crate::status_condition::INT2DDS_STATUS_OFFERED_INCOMPATIBLE_QOS != 0 {
            if let Ok(status) = writer_handle.inner.get_offered_incompatible_qos_status() {
                if status.total_count() > 0 {
                    listener_arc.on_offered_incompatible_qos(&writer_handle.inner, &status);
                }
            }
        }
    }

    *writer_out = Arc::into_raw(writer_handle) as *mut Int2DdsDataWriter;

    INT2DDS_RET_OK
}

/// Set or update the listener for a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `listener` can be null to remove the listener
/// - `mask` specifies which status changes trigger callbacks
/// - Listener callbacks must be thread-safe
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_set_listener(
    writer: *mut Int2DdsDataWriter,
    listener: *const Int2DdsDataWriterListener,
    mask: u32,
) -> Int2DdsRet {
    check_null!(writer);

    // No early return is allowed between from_raw and into_raw below, or the
    // caller's strong reference would be dropped and later delete double-frees.
    let writer_arc = Arc::from_raw(writer as *const Int2DdsDataWriter);

    let listener_arc = if !listener.is_null() {
        let weak = Arc::downgrade(&writer_arc);
        Some(Arc::new(FfiDataWriterListener::new(*listener, weak)))
    } else {
        None
    };

    let result = writer_arc.inner.set_listener(
        listener_arc.clone().map(|l| {
            l as Arc<
                dyn int2dds::publication::data_writer_listener::DataWriterListener<
                    Foo = Int2DdsData,
                >,
            >
        }),
        StatusMask::from_bits_truncate(mask),
    );

    *writer_arc.listener.write().unwrap() = listener_arc;

    let _ = Arc::into_raw(writer_arc);

    match result {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the current listener from a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `listener_out` must be a valid pointer to Int2DdsDataWriterListener
/// - Returns a copy of the listener callbacks and user context
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_listener(
    writer: *const Int2DdsDataWriter,
    listener_out: *mut Int2DdsDataWriterListener,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(listener_out);

    let writer_ref = &*writer;

    // Return the stored listener callbacks
    if let Some(listener_arc) = writer_ref.listener.read().unwrap().as_ref() {
        // Copy the callbacks struct
        *listener_out = listener_arc.callbacks;
        INT2DDS_RET_OK
    } else {
        // No listener set - return zeroed callbacks
        *listener_out = std::mem::zeroed();
        INT2DDS_RET_OK
    }
}

/// Delete a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `writer` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_delete_datawriter(writer: *mut Int2DdsDataWriter) -> Int2DdsRet {
    if writer.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    // Reclaim the caller's strong reference; the writer is freed only once this
    // Arc and any callback that upgraded its Weak are dropped.
    let writer_arc = Arc::from_raw(writer as *const Int2DdsDataWriter);
    let writer_obj = writer_arc.inner.clone();

    let _ = writer_obj.set_listener(None, StatusMask::default());

    let publisher = match writer_obj.get_publisher() {
        Ok(p) => p,
        Err(e) => return dds_error_to_code(&e),
    };

    match publisher.delete_datawriter(writer_obj) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get publication matched status
///
/// Returns the number of matched readers for this DataWriter.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `total_count_out` must be a valid pointer
/// - `current_count_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_get_publication_matched_status(
    writer: *const Int2DdsDataWriter,
    total_count_out: *mut i32,
    current_count_out: *mut i32,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(total_count_out);
    check_null!(current_count_out);

    let writer_ref = &*writer;

    match writer_ref.inner.get_publication_matched_status() {
        Ok(status) => {
            *total_count_out = status.total_count();
            *current_count_out = status.current_count();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get liveliness lost status for a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_liveliness_lost_status(
    writer: *const Int2DdsDataWriter,
    status_out: *mut Int2DdsLivelinessLostStatus,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(status_out);

    let writer_ref = &*writer;

    match writer_ref.inner.get_liveliness_lost_status() {
        Ok(status) => {
            *status_out = Int2DdsLivelinessLostStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get offered deadline missed status for a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_offered_deadline_missed_status(
    writer: *const Int2DdsDataWriter,
    status_out: *mut Int2DdsOfferedDeadlineMissedStatus,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(status_out);

    let writer_ref = &*writer;

    match writer_ref.inner.get_offered_deadline_missed_status() {
        Ok(status) => {
            *status_out = Int2DdsOfferedDeadlineMissedStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get offered incompatible QoS status for a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_offered_incompatible_qos_status(
    writer: *const Int2DdsDataWriter,
    status_out: *mut Int2DdsOfferedIncompatibleQosStatus,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(status_out);

    let writer_ref = &*writer;

    match writer_ref.inner.get_offered_incompatible_qos_status() {
        Ok(status) => {
            *status_out = Int2DdsOfferedIncompatibleQosStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get offered incompatible type status for a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_offered_incompatible_type_status(
    writer: *const Int2DdsDataWriter,
    status_out: *mut Int2DdsOfferedIncompatibleTypeStatus,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(status_out);

    let writer_ref = &*writer;

    match writer_ref.inner.get_offered_incompatible_type_status() {
        Ok(status) => {
            *status_out = Int2DdsOfferedIncompatibleTypeStatus::from(&status);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Delete all entities contained by a publisher
///
/// This operation deletes all DataWriter objects contained by this Publisher.
///
/// # Safety
/// - `publisher` must be a valid publisher
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_delete_contained_entities(
    publisher: *const Int2DdsPublisher,
) -> Int2DdsRet {
    check_null!(publisher);

    let publisher_ref = &*publisher;

    match publisher_ref.inner.delete_contained_entities() {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

// ============================================================================
// Raw Serialized Data Write Functions
// ============================================================================

/// Write pre-serialized data to a DataWriter, bypassing TypeSupport serialization.
///
/// The caller is responsible for CDR-serializing the data (including the
/// encapsulation header) before calling this function.
///
/// # Parameters
/// - `writer`: A valid datawriter
/// - `data`: Pointer to the CDR-serialized byte buffer
/// - `data_len`: Length of the serialized data in bytes
/// - `key`: Pointer to the serialized key bytes (can be null if no key)
/// - `key_len`: Length of the key bytes
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `data` must point to at least `data_len` readable bytes
/// - If `key` is not null, it must point to at least `key_len` readable bytes
#[no_mangle]
pub unsafe extern "C" fn int2dds_write_serialized(
    writer: *const Int2DdsDataWriter,
    data: *const u8,
    data_len: usize,
    key: *const u8,
    key_len: usize,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(data);

    let writer_ref = &*writer;
    let serialized_data = std::slice::from_raw_parts(data, data_len);

    let serialized_key = if key.is_null() || key_len == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(key, key_len))
    };

    match writer_ref.inner.write_serialized(serialized_data, serialized_key) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_prepare_serialized_write(
    writer: *const Int2DdsDataWriter,
    capacity: usize,
    data_out: *mut *mut u8,
    capacity_out: *mut usize,
    loan_out: *mut *mut Int2DdsSerializedWriteLoan,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(data_out);
    check_null!(capacity_out);
    check_null!(loan_out);

    *data_out = std::ptr::null_mut();
    *capacity_out = 0;
    *loan_out = std::ptr::null_mut();

    let writer_ref = &*writer;
    let mut loan = match writer_ref.inner.prepare_serialized_write(capacity) {
        Ok(loan) => loan,
        Err(e) => return dds_error_to_code(&e),
    };

    *data_out = loan.as_mut_ptr();
    *capacity_out = loan.capacity();
    *loan_out = Box::into_raw(Box::new(Int2DdsSerializedWriteLoan { inner: Some(loan) }));

    INT2DDS_RET_OK
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_commit_serialized_write(
    writer: *const Int2DdsDataWriter,
    loan: *mut Int2DdsSerializedWriteLoan,
    actual_size: usize,
    key: *const u8,
    key_len: usize,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(loan);

    let writer_ref = &*writer;
    let mut loan_box = Box::from_raw(loan);
    let loan_inner = match loan_box.inner.take() {
        Some(loan_inner) => loan_inner,
        None => return INT2DDS_RET_ERROR,
    };

    let serialized_key = if key.is_null() || key_len == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(key, key_len))
    };

    match writer_ref.inner.commit_serialized_write(loan_inner, actual_size, serialized_key) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_abort_serialized_write(
    loan: *mut Int2DdsSerializedWriteLoan,
) -> Int2DdsRet {
    if !loan.is_null() {
        drop(Box::from_raw(loan));
    }
    INT2DDS_RET_OK
}

/// Write pre-serialized data with an explicit source timestamp.
///
/// Same as `int2dds_write_serialized`, but allows the caller to specify a
/// source timestamp instead of using the current time.
///
/// # Parameters
/// - `writer`: A valid datawriter
/// - `data`: Pointer to the CDR-serialized byte buffer
/// - `data_len`: Length of the serialized data in bytes
/// - `key`: Pointer to the serialized key bytes (can be null if no key)
/// - `key_len`: Length of the key bytes
/// - `timestamp_sec`: Seconds component of the source timestamp
/// - `timestamp_nanosec`: Nanoseconds component of the source timestamp
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `data` must point to at least `data_len` readable bytes
/// - If `key` is not null, it must point to at least `key_len` readable bytes
#[no_mangle]
pub unsafe extern "C" fn int2dds_write_serialized_w_timestamp(
    writer: *const Int2DdsDataWriter,
    data: *const u8,
    data_len: usize,
    key: *const u8,
    key_len: usize,
    timestamp_sec: i32,
    timestamp_nanosec: u32,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(data);

    let writer_ref = &*writer;
    let serialized_data = std::slice::from_raw_parts(data, data_len);

    let serialized_key = if key.is_null() || key_len == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(key, key_len))
    };

    let timestamp = Time { sec: timestamp_sec, nanosec: timestamp_nanosec };

    match writer_ref.inner.write_serialized_w_timestamp(serialized_data, serialized_key, timestamp)
    {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Block until all reliable DataReader entities have acknowledged all written data,
/// or until the timeout expires.
///
/// # Safety
/// - `writer` must be a valid datawriter
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_wait_for_acknowledgments(
    writer: *const Int2DdsDataWriter,
    timeout_ms: i64,
) -> Int2DdsRet {
    check_null!(writer);

    let writer_ref = &*writer;
    let max_wait = Duration {
        sec: (timeout_ms / 1000) as i32,
        nanosec: ((timeout_ms % 1000) * 1_000_000) as u32,
    };

    match writer_ref.inner.wait_for_acknowledgments(max_wait) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Block until all reliable DataWriter entities owned by this Publisher have been
/// acknowledged by all matched reliable DataReader entities, or until the timeout expires.
///
/// # Safety
/// - `publisher` must be a valid publisher
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_wait_for_acknowledgments(
    publisher: *const Int2DdsPublisher,
    timeout_ms: i64,
) -> Int2DdsRet {
    check_null!(publisher);

    let publisher_ref = &*publisher;
    let max_wait = Duration {
        sec: (timeout_ms / 1000) as i32,
        nanosec: ((timeout_ms % 1000) * 1_000_000) as u32,
    };

    match publisher_ref.inner.wait_for_acknowledgments(max_wait) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

// ============================================================================
// Instance Lifecycle (Feature 2)
// ============================================================================

/// Helper to convert a C [u8; 16] handle pointer to InstanceHandle
unsafe fn handle_from_c(handle_ptr: *const [u8; 16]) -> InstanceHandle {
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

/// Register an instance with serialized key bytes
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to at least `key_len` readable bytes
/// - `handle_out` must be a valid pointer to a 16-byte array
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_register_instance(
    writer: *const Int2DdsDataWriter,
    key: *const u8,
    key_len: usize,
    handle_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(key);
    check_null!(handle_out);

    let writer_ref = &*writer;
    let key_bytes = std::slice::from_raw_parts(key, key_len);

    let handle = ffi_try!(writer_ref.inner.register_instance_serialized(key_bytes));
    *handle_out = *handle.value();

    INT2DDS_RET_OK
}

/// Dispose an instance with serialized key bytes
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to at least `key_len` readable bytes
/// - `handle` must be a valid pointer to a 16-byte instance handle (or null for NIL)
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_dispose(
    writer: *const Int2DdsDataWriter,
    key: *const u8,
    key_len: usize,
    handle: *const [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(key);

    let writer_ref = &*writer;
    let key_bytes = std::slice::from_raw_parts(key, key_len);
    let instance_handle = handle_from_c(handle);

    ffi_try!(writer_ref.inner.dispose_serialized(key_bytes, instance_handle));

    INT2DDS_RET_OK
}

/// Unregister an instance with serialized key bytes
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to at least `key_len` readable bytes
/// - `handle` must be a valid pointer to a 16-byte instance handle (or null for NIL)
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_unregister_instance(
    writer: *const Int2DdsDataWriter,
    key: *const u8,
    key_len: usize,
    handle: *const [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(key);

    let writer_ref = &*writer;
    let key_bytes = std::slice::from_raw_parts(key, key_len);
    let instance_handle = handle_from_c(handle);

    ffi_try!(writer_ref.inner.unregister_instance_serialized(key_bytes, instance_handle));

    INT2DDS_RET_OK
}

/// Lookup an instance handle from serialized key bytes
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to at least `key_len` readable bytes
/// - `handle_out` must be a valid pointer to a 16-byte array
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_lookup_instance(
    writer: *const Int2DdsDataWriter,
    key: *const u8,
    key_len: usize,
    handle_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(key);
    check_null!(handle_out);

    let writer_ref = &*writer;
    let key_bytes = std::slice::from_raw_parts(key, key_len);

    let handle = ffi_try!(writer_ref.inner.lookup_instance_serialized(key_bytes));
    *handle_out = *handle.value();

    INT2DDS_RET_OK
}

/// Get key value for an instance handle
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `handle` must be a valid pointer to a 16-byte instance handle
/// - `key_buf` must point to at least `key_capacity` writable bytes
/// - `key_size_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_key_value(
    writer: *const Int2DdsDataWriter,
    handle: *const [u8; 16],
    key_buf: *mut u8,
    key_capacity: usize,
    key_size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(handle);
    check_null!(key_buf);
    check_null!(key_size_out);

    let writer_ref = &*writer;
    let instance_handle = handle_from_c(handle);

    let key_data = ffi_try!(writer_ref.inner.get_key_value_serialized(instance_handle));
    *key_size_out = key_data.len();

    if key_data.len() > key_capacity {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(key_data.as_ptr(), key_buf, key_data.len());

    INT2DDS_RET_OK
}

// ============================================================================
// Assert Liveliness (Feature 5)
// ============================================================================

/// Assert liveliness for a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_assert_liveliness(
    writer: *const Int2DdsDataWriter,
) -> Int2DdsRet {
    check_null!(writer);

    let writer_ref = &*writer;
    ffi_try!(writer_ref.inner.assert_liveliness());

    INT2DDS_RET_OK
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
    fn test_publisher_create_delete() {
        unsafe {
            let mut factory: *mut Int2DdsParticipantFactory = ptr::null_mut();
            let mut participant: *mut Int2DdsParticipant = ptr::null_mut();
            let mut publisher: *mut Int2DdsPublisher = ptr::null_mut();

            // Initialize
            context::int2dds_domain_participant_factory_get_instance(&mut factory as *mut _);
            participant::int2dds_create_participant(
                factory,
                ptr::null(),
                0,
                &mut participant as *mut _,
            );

            // Create publisher
            let ret = int2dds_create_publisher(participant, &mut publisher as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!publisher.is_null());

            // Cleanup
            int2dds_delete_publisher(publisher);
            participant::int2dds_delete_participant(participant);
            context::int2dds_domain_participant_factory_finalize(factory);
        }
    }
}
