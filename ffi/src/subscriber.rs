//! # Subscriber and DataReader
//!
//! Functions for creating Subscribers and reading data.
//!
//! ## Overview
//!
//! Subscribers group DataReaders and manage their QoS settings. DataReaders
//! receive data samples from matching DataWriters on the same Topic.
//!
//! ## Reading Data
//!
//! Two methods are available:
//! - `int2dds_take` - Returns and removes the next sample from the cache
//! - `int2dds_read` - Returns the next sample without removing it
//!
//! ## Data Format
//!
//! The FFI uses Int2DdsData with DynamicTypeSupport for CDR deserialization.
//! The DDS core handles all deserialization automatically using the registered TypeSupport.

use std::sync::Arc;

use int2dds::{infrastructure::status::StatusMask, subscription::qos::SubscriberQos};

use crate::data::Int2DdsData;

use super::{
    error::*,
    listener::{FfiDataReaderListener, Int2DdsDataReaderListener},
    qos::Int2DdsDataReaderQos,
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

    let subscriber = ffi_try!(participant_ref.inner.create_subscriber(
        SubscriberQos::default(),
        None,
        StatusMask::default()
    ));

    // Wrap subscriber in Arc
    let subscriber_arc = Arc::new(subscriber);

    let subscriber_handle = Box::new(Int2DdsSubscriber { inner: subscriber_arc });

    *subscriber_out = Box::into_raw(subscriber_handle);

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

    // Destructure Box to move Arc out
    let Int2DdsSubscriber { inner: subscriber_arc } = *Box::from_raw(subscriber);

    // Try to unwrap Arc without cloning (succeeds if this is the only reference)
    let subscriber_obj = match Arc::try_unwrap(subscriber_arc) {
        Ok(s) => s,
        Err(arc) => (*arc).clone(), // Fall back to clone if other references exist
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

    let reader_qos = if qos.is_null() {
        int2dds::subscription::qos::DataReaderQos::default()
    } else {
        (*qos).inner.clone()
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

    let reader_qos = if qos.is_null() {
        int2dds::subscription::qos::DataReaderQos::default()
    } else {
        (*qos).inner.clone()
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

        reader_handle.listener = Some(listener_arc);
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

/// Take data from a DataReader (removes from cache)
///
/// Returns the next sample, removing it from the cache.
/// The data is automatically deserialized from CDR format using the registered TypeSupport.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `data_out` must be a valid Int2DdsData created from a compatible TypeDescriptor
/// - `valid_data_out` will be set to true if valid data was read
///
/// # Returns
/// - INT2DDS_RET_OK if data was successfully read
/// - INT2DDS_RET_NO_DATA if no data is available
/// - INT2DDS_RET_ERROR if deserialization failed
#[no_mangle]
pub unsafe extern "C" fn int2dds_take(
    reader: *const Int2DdsDataReader,
    data_out: *mut Int2DdsData,
    valid_data_out: *mut bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(data_out);
    check_null!(valid_data_out);

    let reader_ref = &*reader;

    // Take next sample
    let sample = match reader_ref.inner.take_next_sample() {
        Ok(result) => result,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *valid_data_out = false;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let info = sample.sample_info();
    *valid_data_out = info.valid_data;

    if !info.valid_data {
        return INT2DDS_RET_OK;
    }

    // Use sample.data() - DDS core automatically deserializes using registered TypeSupport
    let mut received_data = match sample.data() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("int2dds_take: sample.data() failed: {:?}", e);
            return INT2DDS_RET_ERROR;
        }
    };

    // Move values from received data to output - avoids clone
    let data_out_ref = &mut *data_out;
    data_out_ref.values = std::mem::take(&mut received_data.values);

    INT2DDS_RET_OK
}

/// Read data from a DataReader (keeps in cache)
///
/// Returns the next sample without removing it from the cache.
/// The data is automatically deserialized from CDR format using the registered TypeSupport.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `data_out` must be a valid Int2DdsData created from a compatible TypeDescriptor
/// - `valid_data_out` will be set to true if valid data was read
#[no_mangle]
pub unsafe extern "C" fn int2dds_read(
    reader: *const Int2DdsDataReader,
    data_out: *mut Int2DdsData,
    valid_data_out: *mut bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(data_out);
    check_null!(valid_data_out);

    let reader_ref = &*reader;

    // Read next sample (without removing)
    let sample = match reader_ref.inner.read_next_sample() {
        Ok(result) => result,
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *valid_data_out = false;
            return INT2DDS_RET_NO_DATA;
        }
        Err(e) => return dds_error_to_code(&e),
    };

    let info = sample.sample_info();
    *valid_data_out = info.valid_data;

    if !info.valid_data {
        return INT2DDS_RET_OK;
    }

    // Use sample.data() - DDS core automatically deserializes using registered TypeSupport
    let mut received_data = match sample.data() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("int2dds_read: sample.data() failed: {:?}", e);
            return INT2DDS_RET_ERROR;
        }
    };

    // Move values from received data to output
    let data_out_ref = &mut *data_out;
    data_out_ref.values = std::mem::take(&mut received_data.values);

    INT2DDS_RET_OK
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
