//! # Publisher and DataWriter
//!
//! Functions for creating Publishers and writing data.
//!
//! ## Overview
//!
//! Publishers group DataWriters and manage their QoS settings. DataWriters
//! send data samples to matching DataReaders on the same Topic.
//!
//! ## Data Format
//!
//! Since DataWriter is generic in Rust, the FFI uses raw byte buffers.
//! Data must be pre-serialized in CDR format on the C side before writing.
//!
//! ## Instance Management
//!
//! For keyed topics, the FFI provides:
//! - `int2dds_register_instance` - Pre-register an instance for better performance
//! - `int2dds_unregister_instance` - Unregister when done with an instance
//! - `int2dds_dispose` - Indicate instance is no longer valid

use std::sync::Arc;

use int2dds::{
    common::instance_handle::InstanceHandle, infrastructure::status::StatusMask,
    publication::qos::PublisherQos, topic::RawData,
};

use super::{error::*, qos::Int2DdsDataWriterQos, types::*};

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

    let publisher_box = Box::from_raw(publisher);
    let publisher_obj = (*publisher_box.inner).clone();

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

    // Create DataWriter<RawData>
    let writer = ffi_try!(publisher_ref.inner.create_datawriter::<RawData>(
        &topic_ref.inner,
        writer_qos,
        None,
        StatusMask::default()
    ));

    let writer_handle = Box::new(Int2DdsDataWriter { inner: writer });

    *writer_out = Box::into_raw(writer_handle);

    INT2DDS_RET_OK
}

/// Write data to a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `data` must point to valid serialized data
/// - `data_size` must be the correct size of the data
///
/// Note: The data must be pre-serialized using CDR format.
#[no_mangle]
pub unsafe extern "C" fn int2dds_write(
    writer: *const Int2DdsDataWriter,
    data: *const u8,
    data_size: usize,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(data);

    let writer_ref = &*writer;

    // Create RawData from input bytes
    let raw_data = RawData::new(std::slice::from_raw_parts(data, data_size).to_vec());

    // Write with NIL handle (instance identification from data)
    ffi_try!(writer_ref.inner.write(&raw_data, InstanceHandle::NIL));

    INT2DDS_RET_OK
}

/// Write data to a DataWriter with key
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `data` must point to valid serialized data
/// - `data_size` must be the correct size of the data
/// - `key` must point to valid serialized key data
/// - `key_size` must be the correct size of the key
///
/// Note: The data and key must be pre-serialized using CDR format.
#[no_mangle]
pub unsafe extern "C" fn int2dds_write_with_key(
    writer: *const Int2DdsDataWriter,
    data: *const u8,
    data_size: usize,
    key: *const u8,
    key_size: usize,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(data);

    let writer_ref = &*writer;

    // Create RawData with key
    let data_vec = std::slice::from_raw_parts(data, data_size).to_vec();
    let raw_data = if !key.is_null() && key_size > 0 {
        let key_vec = std::slice::from_raw_parts(key, key_size).to_vec();
        RawData::with_key(data_vec, key_vec)
    } else {
        RawData::new(data_vec)
    };

    // Write with NIL handle
    ffi_try!(writer_ref.inner.write(&raw_data, InstanceHandle::NIL));

    INT2DDS_RET_OK
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

/// Register an instance for subsequent write operations
///
/// This operation informs the service that the application intends to modify
/// a particular instance, allowing pre-configuration for improved performance.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to valid serialized key data
/// - `key_size` must be the correct size of the key
/// - `handle_out` must be a valid pointer to 16-byte array for the instance handle
///
/// # Returns
/// Returns the instance handle (16 bytes) that can be used in subsequent write/dispose operations
#[no_mangle]
pub unsafe extern "C" fn int2dds_register_instance(
    writer: *const Int2DdsDataWriter,
    key: *const u8,
    key_size: usize,
    handle_out: *mut [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(handle_out);

    let writer_ref = &*writer;

    // Create RawData with key for registration (using key_only to avoid empty data allocation)
    let raw_data = if !key.is_null() && key_size > 0 {
        let key_vec = std::slice::from_raw_parts(key, key_size).to_vec();
        RawData::key_only(key_vec)
    } else {
        RawData::empty()
    };

    match writer_ref.inner.register_instance(&raw_data) {
        Ok(handle) => {
            *handle_out = *handle.value();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Unregister a previously registered instance
///
/// This operation reverses register_instance, indicating the application
/// no longer intends to modify the instance.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to valid serialized key data
/// - `key_size` must be the correct size of the key
/// - `handle` is a pointer to 16-byte instance handle (use all zeros for NIL)
#[no_mangle]
pub unsafe extern "C" fn int2dds_unregister_instance(
    writer: *const Int2DdsDataWriter,
    key: *const u8,
    key_size: usize,
    handle: *const [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);

    let writer_ref = &*writer;

    // Create RawData with key for identification (using key_only to avoid empty data allocation)
    let raw_data = if !key.is_null() && key_size > 0 {
        let key_vec = std::slice::from_raw_parts(key, key_size).to_vec();
        RawData::key_only(key_vec)
    } else {
        RawData::empty()
    };

    let instance_handle =
        if handle.is_null() { InstanceHandle::NIL } else { InstanceHandle::new(*handle) };

    match writer_ref.inner.unregister_instance(&raw_data, instance_handle) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Dispose an instance, indicating it is no longer valid
///
/// This operation requests the middleware to delete the data instance.
/// DataReaders will be notified of the disposal through instance state changes.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `key` must point to valid serialized key data
/// - `key_size` must be the correct size of the key
/// - `handle` is a pointer to 16-byte instance handle (use all zeros for NIL)
#[no_mangle]
pub unsafe extern "C" fn int2dds_dispose(
    writer: *const Int2DdsDataWriter,
    key: *const u8,
    key_size: usize,
    handle: *const [u8; 16],
) -> Int2DdsRet {
    check_null!(writer);

    let writer_ref = &*writer;

    // Create RawData with key for identification (using key_only to avoid empty data allocation)
    let raw_data = if !key.is_null() && key_size > 0 {
        let key_vec = std::slice::from_raw_parts(key, key_size).to_vec();
        RawData::key_only(key_vec)
    } else {
        RawData::empty()
    };

    let instance_handle =
        if handle.is_null() { InstanceHandle::NIL } else { InstanceHandle::new(*handle) };

    match writer_ref.inner.dispose(&raw_data, instance_handle) {
        Ok(()) => INT2DDS_RET_OK,
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
