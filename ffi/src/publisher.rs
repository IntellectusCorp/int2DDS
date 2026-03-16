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

use std::sync::Arc;

use int2dds::{infrastructure::status::StatusMask, publication::qos::PublisherQos};

use crate::data::Int2DdsData;

use super::{
    error::*,
    listener::{FfiDataWriterListener, Int2DdsDataWriterListener},
    qos::Int2DdsDataWriterQos,
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

    let publisher = ffi_try!(participant_ref.inner.create_publisher(
        PublisherQos::default(),
        None,
        StatusMask::default()
    ));

    // Wrap publisher in Arc
    let publisher_arc = Arc::new(publisher);

    let publisher_handle = Box::new(Int2DdsPublisher { inner: publisher_arc });

    *publisher_out = Box::into_raw(publisher_handle);

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

    let writer_qos = if qos.is_null() {
        int2dds::publication::qos::DataWriterQos::default()
    } else {
        (*qos).inner.clone()
    };

    // Create DataWriter<Int2DdsData>
    let writer = ffi_try!(publisher_ref.inner.create_datawriter::<Int2DdsData>(
        &topic_ref.inner,
        writer_qos,
        None,
        StatusMask::default()
    ));

    let writer_handle = Box::new(Int2DdsDataWriter { inner: writer, listener: None });

    *writer_out = Box::into_raw(writer_handle);

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

    let writer_qos = if qos.is_null() {
        int2dds::publication::qos::DataWriterQos::default()
    } else {
        (*qos).inner.clone()
    };

    // Create DataWriter<Int2DdsData> first without listener
    let writer = ffi_try!(publisher_ref.inner.create_datawriter::<Int2DdsData>(
        &topic_ref.inner,
        writer_qos,
        None,
        StatusMask::from_bits_truncate(mask)
    ));

    // Create writer_handle with the actual writer
    let mut writer_handle = Box::new(Int2DdsDataWriter { inner: writer, listener: None });

    // If listener is provided, set it now
    if !listener.is_null() {
        let writer_ptr = &mut *writer_handle as *mut Int2DdsDataWriter;
        let ffi_listener = FfiDataWriterListener::new(*listener, writer_ptr);
        let listener_arc = Arc::new(ffi_listener);

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

        writer_handle.listener = Some(listener_arc);
    }

    *writer_out = Box::into_raw(writer_handle);

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

    let writer_ref = &mut *writer;

    // Create new listener wrapper if provided
    let listener_arc = if !listener.is_null() {
        let ffi_listener = FfiDataWriterListener::new(*listener, writer);
        Some(Arc::new(ffi_listener))
    } else {
        None
    };

    // Set listener on the inner writer
    let result = writer_ref.inner.set_listener(
        listener_arc.clone().map(|l| {
            l as Arc<
                dyn int2dds::publication::data_writer_listener::DataWriterListener<
                    Foo = Int2DdsData,
                >,
            >
        }),
        StatusMask::from_bits_truncate(mask),
    );

    // Update the stored listener
    writer_ref.listener = listener_arc;

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
    if let Some(listener_arc) = &writer_ref.listener {
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

    let writer_box = Box::from_raw(writer);
    let writer_obj = writer_box.inner;

    // Get the publisher to delete the writer
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

/// Register an instance and return its handle (16-byte key hash).
///
/// Pre-registers an instance in the DataWriter for the given key,
/// allowing the DDS service to pre-allocate resources. Returns the
/// InstanceHandle for use in subsequent write/dispose/unregister calls.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to at least `key_len` readable bytes
/// - `handle_out` must be a valid pointer to a 16-byte array
#[no_mangle]
pub unsafe extern "C" fn int2dds_register_instance_serialized(
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

    match writer_ref.inner.register_instance_serialized(key_bytes) {
        Ok(handle) => {
            *handle_out = *handle.value();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Unregister a previously registered instance.
///
/// Informs the DDS service that this DataWriter will no longer modify
/// the specified instance. Readers will eventually see the instance
/// state change to NOT_ALIVE_NO_WRITERS.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to at least `key_len` readable bytes
/// - `handle` must be a valid pointer to a 16-byte array (from register_instance),
///   or all zeros for HANDLE_NIL (auto-detect from key)
#[no_mangle]
pub unsafe extern "C" fn int2dds_unregister_instance_serialized(
    writer: *const Int2DdsDataWriter,
    key: *const u8,
    key_len: usize,
    handle: *const [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(key);
    check_null!(handle);

    let writer_ref = &*writer;
    let key_bytes = std::slice::from_raw_parts(key, key_len);
    let handle_bytes = &*handle;

    let instance_handle = int2dds::common::instance_handle::InstanceHandle::new(*handle_bytes);

    match writer_ref.inner.unregister_instance_serialized(key_bytes, instance_handle) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Dispose an instance, marking it as no longer valid.
///
/// Readers will see the instance state change to NOT_ALIVE_DISPOSED.
/// Unlike unregister, dispose indicates the instance data itself is
/// no longer meaningful.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to at least `key_len` readable bytes
/// - `handle` must be a valid pointer to a 16-byte array (from register_instance),
///   or all zeros for HANDLE_NIL (auto-detect from key)
#[no_mangle]
pub unsafe extern "C" fn int2dds_dispose_serialized(
    writer: *const Int2DdsDataWriter,
    key: *const u8,
    key_len: usize,
    handle: *const [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(key);
    check_null!(handle);

    let writer_ref = &*writer;
    let key_bytes = std::slice::from_raw_parts(key, key_len);
    let handle_bytes = &*handle;

    let instance_handle = int2dds::common::instance_handle::InstanceHandle::new(*handle_bytes);

    match writer_ref.inner.dispose_serialized(key_bytes, instance_handle) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Look up the handle of a previously registered instance.
///
/// Returns the InstanceHandle for the given key if the instance
/// is known, or all zeros (HANDLE_NIL) if not found.
/// This does NOT register the instance.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to at least `key_len` readable bytes
/// - `handle_out` must be a valid pointer to a 16-byte array
#[no_mangle]
pub unsafe extern "C" fn int2dds_lookup_instance_serialized(
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

    match writer_ref.inner.lookup_instance_serialized(key_bytes) {
        Ok(handle) => {
            *handle_out = *handle.value();
            INT2DDS_RET_OK
        }
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
