//! # StatusCondition
//!
//! Functions for managing StatusConditions on DDS entities.
//!
//! ## Overview
//!
//! StatusCondition is a Condition that triggers based on status changes
//! of a DDS entity (DataReader, DataWriter, etc.). It can be attached
//! to a WaitSet to wait for specific status changes.
//!
//! ## Status Masks
//!
//! The enabled statuses determine which status changes trigger the condition.
//! Use the `INT2DDS_STATUS_*` constants to construct status masks.
use int2dds::infrastructure::status::StatusMask;

use super::{error::*, types::*};

// Status mask constants matching int2dds Rust library (status.rs StatusKind)
pub const INT2DDS_STATUS_INCONSISTENT_TOPIC: u32 = 1 << 0;
pub const INT2DDS_STATUS_OFFERED_DEADLINE_MISSED: u32 = 1 << 1;
pub const INT2DDS_STATUS_REQUESTED_DEADLINE_MISSED: u32 = 1 << 2;
pub const INT2DDS_STATUS_OFFERED_INCOMPATIBLE_TYPE: u32 = 1 << 3;
pub const INT2DDS_STATUS_REQUESTED_INCOMPATIBLE_TYPE: u32 = 1 << 4;
pub const INT2DDS_STATUS_OFFERED_INCOMPATIBLE_QOS: u32 = 1 << 5;
pub const INT2DDS_STATUS_REQUESTED_INCOMPATIBLE_QOS: u32 = 1 << 6;
pub const INT2DDS_STATUS_SAMPLE_LOST: u32 = 1 << 7;
pub const INT2DDS_STATUS_SAMPLE_REJECTED: u32 = 1 << 8;
pub const INT2DDS_STATUS_DATA_ON_READERS: u32 = 1 << 9;
pub const INT2DDS_STATUS_DATA_AVAILABLE: u32 = 1 << 10;
pub const INT2DDS_STATUS_LIVELINESS_LOST: u32 = 1 << 11;
pub const INT2DDS_STATUS_LIVELINESS_CHANGED: u32 = 1 << 12;
pub const INT2DDS_STATUS_PUBLICATION_MATCHED: u32 = 1 << 13;
pub const INT2DDS_STATUS_SUBSCRIPTION_MATCHED: u32 = 1 << 14;

/// Get the StatusCondition from a DataReader
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_statuscondition_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_statuscondition(
    reader: *const Int2DdsDataReader,
    condition_out: *mut *mut Int2DdsStatusCondition,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(condition_out);

    let reader_ref = &*reader;
    let status_condition = match reader_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    // Clone before .into() to preserve concrete type for set/get_enabled_statuses
    let kind = StatusConditionKind::Reader(status_condition.clone());
    let condition_handle =
        Box::new(Int2DdsStatusCondition { inner: status_condition.into(), kind });
    *condition_out = Box::into_raw(condition_handle);

    INT2DDS_RET_OK
}

/// Get the StatusCondition from a DataWriter
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_statuscondition_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_statuscondition(
    writer: *const Int2DdsDataWriter,
    condition_out: *mut *mut Int2DdsStatusCondition,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(condition_out);

    let writer_ref = &*writer;

    let status_condition = match writer_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    // Clone before .into() to preserve concrete type for set/get_enabled_statuses
    let kind = StatusConditionKind::Writer(status_condition.clone());
    let condition_handle =
        Box::new(Int2DdsStatusCondition { inner: status_condition.into(), kind });
    *condition_out = Box::into_raw(condition_handle);

    INT2DDS_RET_OK
}

/// Get the StatusCondition from a DomainParticipant
///
/// # Safety
/// - `participant` must be a valid participant
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_statuscondition_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_statuscondition(
    participant: *const Int2DdsParticipant,
    condition_out: *mut *mut Int2DdsStatusCondition,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(condition_out);

    let participant_ref = &*participant;
    let status_condition = match participant_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    let kind = StatusConditionKind::Participant(status_condition.clone());
    let condition_handle =
        Box::new(Int2DdsStatusCondition { inner: status_condition.into(), kind });
    *condition_out = Box::into_raw(condition_handle);

    INT2DDS_RET_OK
}

/// Get the StatusCondition from a Publisher
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_statuscondition_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_get_statuscondition(
    publisher: *const Int2DdsPublisher,
    condition_out: *mut *mut Int2DdsStatusCondition,
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(condition_out);

    let publisher_ref = &*publisher;
    let status_condition = match publisher_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    let kind = StatusConditionKind::Publisher(status_condition.clone());
    let condition_handle =
        Box::new(Int2DdsStatusCondition { inner: status_condition.into(), kind });
    *condition_out = Box::into_raw(condition_handle);

    INT2DDS_RET_OK
}

/// Get the StatusCondition from a Subscriber
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_statuscondition_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscriber_get_statuscondition(
    subscriber: *const Int2DdsSubscriber,
    condition_out: *mut *mut Int2DdsStatusCondition,
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(condition_out);

    let subscriber_ref = &*subscriber;
    let status_condition = match subscriber_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    let kind = StatusConditionKind::Subscriber(status_condition.clone());
    let condition_handle =
        Box::new(Int2DdsStatusCondition { inner: status_condition.into(), kind });
    *condition_out = Box::into_raw(condition_handle);

    INT2DDS_RET_OK
}

/// Get the StatusCondition from a Topic
///
/// # Safety
/// - `topic` must be a valid topic
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_statuscondition_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_get_statuscondition(
    topic: *const Int2DdsTopic,
    condition_out: *mut *mut Int2DdsStatusCondition,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(condition_out);

    let topic_ref = &*topic;
    let status_condition = match topic_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    let kind = StatusConditionKind::Topic(status_condition.clone());
    let condition_handle =
        Box::new(Int2DdsStatusCondition { inner: status_condition.into(), kind });
    *condition_out = Box::into_raw(condition_handle);

    INT2DDS_RET_OK
}

/// Get the current status change bitmask from a DataReader.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `mask_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_get_status_changes(
    reader: *const Int2DdsDataReader,
    mask_out: *mut u32,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(mask_out);

    let reader_ref = &*reader;

    match reader_ref.inner.get_status_changes() {
        Ok(mask) => {
            *mask_out = mask.bits();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the current status change bitmask from a DataWriter.
///
/// # Safety
/// - `writer` must be a valid datawriter
/// - `mask_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_get_status_changes(
    writer: *const Int2DdsDataWriter,
    mask_out: *mut u32,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(mask_out);

    let writer_ref = &*writer;

    match writer_ref.inner.get_status_changes() {
        Ok(mask) => {
            *mask_out = mask.bits();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the current status change bitmask from a DomainParticipant.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `mask_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_status_changes(
    participant: *const Int2DdsParticipant,
    mask_out: *mut u32,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(mask_out);

    match (*participant).inner.get_status_changes() {
        Ok(mask) => {
            *mask_out = mask.bits();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the current status change bitmask from a Publisher.
///
/// # Safety
/// - `publisher` must be a valid publisher
/// - `mask_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_get_status_changes(
    publisher: *const Int2DdsPublisher,
    mask_out: *mut u32,
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(mask_out);

    match (*publisher).inner.get_status_changes() {
        Ok(mask) => {
            *mask_out = mask.bits();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the current status change bitmask from a Subscriber.
///
/// # Safety
/// - `subscriber` must be a valid subscriber
/// - `mask_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscriber_get_status_changes(
    subscriber: *const Int2DdsSubscriber,
    mask_out: *mut u32,
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(mask_out);

    match (*subscriber).inner.get_status_changes() {
        Ok(mask) => {
            *mask_out = mask.bits();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the current status change bitmask from a Topic.
///
/// # Safety
/// - `topic` must be a valid topic
/// - `mask_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_get_status_changes(
    topic: *const Int2DdsTopic,
    mask_out: *mut u32,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(mask_out);

    match (*topic).inner.get_status_changes() {
        Ok(mask) => {
            *mask_out = mask.bits();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Set the enabled statuses for a StatusCondition
///
/// Only the statuses in the mask will trigger the condition.
///
/// # Safety
/// - `condition` must be a valid status condition
/// - `mask` is a bitmask of INT2DDS_STATUS_* constants
#[no_mangle]
pub unsafe extern "C" fn int2dds_statuscondition_set_enabled_statuses(
    condition: *const Int2DdsStatusCondition,
    mask: u32,
) -> Int2DdsRet {
    check_null!(condition);

    let condition_ref = &*condition;
    let status_mask = StatusMask::from_bits_truncate(mask);
    let result = match &condition_ref.kind {
        StatusConditionKind::Reader(sc) => sc.set_enabled_statuses(status_mask),
        StatusConditionKind::Writer(sc) => sc.set_enabled_statuses(status_mask),
        StatusConditionKind::Participant(sc) => sc.set_enabled_statuses(status_mask),
        StatusConditionKind::Publisher(sc) => sc.set_enabled_statuses(status_mask),
        StatusConditionKind::Subscriber(sc) => sc.set_enabled_statuses(status_mask),
        StatusConditionKind::Topic(sc) => sc.set_enabled_statuses(status_mask),
    };

    match result {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the enabled statuses for a StatusCondition
///
/// # Safety
/// - `condition` must be a valid status condition
/// - `mask_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_statuscondition_get_enabled_statuses(
    condition: *const Int2DdsStatusCondition,
    mask_out: *mut u32,
) -> Int2DdsRet {
    check_null!(condition);
    check_null!(mask_out);

    let condition_ref = &*condition;
    let result = match &condition_ref.kind {
        StatusConditionKind::Reader(sc) => sc.get_enabled_statuses(),
        StatusConditionKind::Writer(sc) => sc.get_enabled_statuses(),
        StatusConditionKind::Participant(sc) => sc.get_enabled_statuses(),
        StatusConditionKind::Publisher(sc) => sc.get_enabled_statuses(),
        StatusConditionKind::Subscriber(sc) => sc.get_enabled_statuses(),
        StatusConditionKind::Topic(sc) => sc.get_enabled_statuses(),
    };

    match result {
        Ok(mask) => {
            *mask_out = mask.bits();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the trigger value of a StatusCondition
///
/// Returns true if any of the enabled statuses have changed.
///
/// # Safety
/// - `condition` must be a valid status condition
/// - `value_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_statuscondition_get_trigger_value(
    condition: *const Int2DdsStatusCondition,
    value_out: *mut bool,
) -> Int2DdsRet {
    check_null!(condition);
    check_null!(value_out);

    let condition_ref = &*condition;

    match condition_ref.inner.get_trigger_value() {
        Ok(value) => {
            *value_out = value;
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Delete a StatusCondition
///
/// # Safety
/// - `condition` must be a valid status condition
/// - `condition` must not be used after this call
/// - The condition should be detached from any WaitSets first
#[no_mangle]
pub unsafe extern "C" fn int2dds_statuscondition_delete(
    condition: *mut Int2DdsStatusCondition,
) -> Int2DdsRet {
    if condition.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let _condition = Box::from_raw(condition);

    INT2DDS_RET_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_mask_constants() {
        // Verify status mask constants match int2dds Rust library
        assert_eq!(INT2DDS_STATUS_DATA_ON_READERS, 1 << 9);
        assert_eq!(INT2DDS_STATUS_DATA_AVAILABLE, 1 << 10);
        assert_eq!(INT2DDS_STATUS_SUBSCRIPTION_MATCHED, 1 << 14);
        assert_eq!(INT2DDS_STATUS_PUBLICATION_MATCHED, 1 << 13);
    }
}
