//! # WaitSet
//!
//! Functions for waiting on multiple conditions simultaneously.
//!
//! ## Overview
//!
//! A WaitSet allows applications to wait for multiple conditions to be triggered.
//! Conditions can include:
//! - GuardConditions (manual triggers)
//! - StatusConditions from DataReaders (data available)
//! - StatusConditions from DataWriters (publication matched)
//!
//! ## Usage Pattern
//!
//! 1. Create a WaitSet with `int2dds_waitset_new`
//! 2. Get StatusConditions from entities with `int2dds_datareader_get_statuscondition`
//! 3. Attach conditions with `int2dds_waitset_attach_condition`
//! 4. Wait for conditions with `int2dds_waitset_wait`
//! 5. Clean up with `int2dds_waitset_delete`

use std::sync::Arc;

use int2dds::{core::time::Duration, infrastructure::wait_set::WaitSet};

use super::{error::*, types::*};

/// Create a new WaitSet
///
/// # Safety
/// - `waitset_out` must be a valid pointer to a null pointer
/// - The returned waitset must be freed with `int2dds_waitset_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_new(waitset_out: *mut *mut Int2DdsWaitSet) -> Int2DdsRet {
    check_null!(waitset_out);

    let waitset = WaitSet::new();
    let waitset_arc = Arc::new(waitset);

    let waitset_handle = Box::new(Int2DdsWaitSet { inner: waitset_arc });

    *waitset_out = Box::into_raw(waitset_handle);

    INT2DDS_RET_OK
}

/// Wait for conditions to be triggered
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `timeout_ms` is the timeout in milliseconds, or -1 for infinite
///
/// Returns:
/// - INT2DDS_RET_OK if conditions were triggered
/// - INT2DDS_RET_TIMEOUT if the timeout expired
/// - INT2DDS_RET_ERROR for other errors
#[deprecated(
    note = "discards triggered conditions; use int2dds_waitset_wait_ex (ms) or int2dds_waitset_wait_ex_ns (ns)"
)]
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_wait(
    waitset: *const Int2DdsWaitSet,
    timeout_ms: i64,
) -> Int2DdsRet {
    check_null!(waitset);

    let waitset_ref = &*waitset;

    let duration = if timeout_ms < 0 {
        Duration::infinite()
    } else {
        Duration {
            sec: (timeout_ms / 1000) as i32,
            nanosec: ((timeout_ms % 1000) * 1_000_000) as u32,
        }
    };

    match waitset_ref.inner.wait(duration) {
        Ok(_conditions) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Wait for conditions to be triggered, with nanosecond timeout resolution.
///
/// Identical to `int2dds_waitset_wait` but the timeout is given in nanoseconds so
/// sub-millisecond waits are honored. Additive: the millisecond entry point is
/// unchanged.
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `timeout_ns` is the timeout in nanoseconds, or -1 for infinite
#[deprecated(
    note = "discards triggered conditions; use int2dds_waitset_wait_ex_ns (ns) or int2dds_waitset_wait_ex (ms)"
)]
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_wait_ns(
    waitset: *const Int2DdsWaitSet,
    timeout_ns: i64,
) -> Int2DdsRet {
    check_null!(waitset);

    let waitset_ref = &*waitset;

    let duration = if timeout_ns < 0 {
        Duration::infinite()
    } else {
        Duration {
            sec: (timeout_ns / 1_000_000_000) as i32,
            nanosec: (timeout_ns % 1_000_000_000) as u32,
        }
    };

    match waitset_ref.inner.wait(duration) {
        Ok(_conditions) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Wait for conditions to be triggered and return the triggered conditions
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `timeout_ms` is the timeout in milliseconds, or -1 for infinite
/// - `conditions_out` must be a valid pointer to a null pointer
/// - The returned condition sequence must be freed with `int2dds_condition_seq_delete`
///
/// Returns:
/// - INT2DDS_RET_OK if conditions were triggered
/// - INT2DDS_RET_TIMEOUT if the timeout expired
/// - INT2DDS_RET_ERROR for other errors
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_wait_ex(
    waitset: *const Int2DdsWaitSet,
    timeout_ms: i64,
    conditions_out: *mut *mut Int2DdsConditionSeq,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(conditions_out);

    let waitset_ref = &*waitset;

    let duration = if timeout_ms < 0 {
        Duration::infinite()
    } else {
        Duration {
            sec: (timeout_ms / 1000) as i32,
            nanosec: ((timeout_ms % 1000) * 1_000_000) as u32,
        }
    };

    match waitset_ref.inner.wait(duration) {
        Ok(conditions) => {
            let seq = Box::new(Int2DdsConditionSeq { conditions });
            *conditions_out = Box::into_raw(seq);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Wait for conditions to be triggered and return them, with nanosecond timeout resolution.
///
/// Combines `int2dds_waitset_wait_ex` (returns triggered conditions) with
/// nanosecond timeout precision, so no capability is lost when migrating off the
/// deprecated `int2dds_waitset_wait` / `int2dds_waitset_wait_ns` entry points.
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `timeout_ns` is the timeout in nanoseconds, or -1 for infinite
/// - `conditions_out` must be a valid pointer to a null pointer
/// - The returned condition sequence must be freed with `int2dds_condition_seq_delete`
///
/// Returns:
/// - INT2DDS_RET_OK if conditions were triggered
/// - INT2DDS_RET_TIMEOUT if the timeout expired
/// - INT2DDS_RET_ERROR for other errors
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_wait_ex_ns(
    waitset: *const Int2DdsWaitSet,
    timeout_ns: i64,
    conditions_out: *mut *mut Int2DdsConditionSeq,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(conditions_out);

    let waitset_ref = &*waitset;

    let duration = if timeout_ns < 0 {
        Duration::infinite()
    } else {
        Duration {
            sec: (timeout_ns / 1_000_000_000) as i32,
            nanosec: (timeout_ns % 1_000_000_000) as u32,
        }
    };

    match waitset_ref.inner.wait(duration) {
        Ok(conditions) => {
            let seq = Box::new(Int2DdsConditionSeq { conditions });
            *conditions_out = Box::into_raw(seq);
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the number of conditions in a condition sequence
///
/// # Safety
/// - `seq` must be a valid condition sequence
/// - `count_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_condition_seq_length(
    seq: *const Int2DdsConditionSeq,
    count_out: *mut usize,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(count_out);

    let seq_ref = &*seq;
    *count_out = seq_ref.conditions.len();

    INT2DDS_RET_OK
}

/// Get a condition from a condition sequence by index
///
/// # Safety
/// - `seq` must be a valid condition sequence
/// - `index` must be less than the sequence length
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_condition_delete`
///
/// Note: The returned condition is an owned handle (cloned from the sequence).
/// It remains valid even after the sequence is deleted.
#[no_mangle]
pub unsafe extern "C" fn int2dds_condition_seq_get(
    seq: *const Int2DdsConditionSeq,
    index: usize,
    condition_out: *mut *const Int2DdsCondition,
) -> Int2DdsRet {
    check_null!(seq);
    check_null!(condition_out);

    let seq_ref = &*seq;

    if index >= seq_ref.conditions.len() {
        return INT2DDS_RET_INVALID_ARGUMENT;
    }

    // Create a temporary condition handle
    // Note: This creates a new Arc clone, so the condition remains valid
    let condition = Box::new(Int2DdsCondition { inner: seq_ref.conditions[index].clone() });
    *condition_out = Box::into_raw(condition);

    INT2DDS_RET_OK
}

/// Delete a condition sequence
///
/// # Safety
/// - `seq` must be a valid condition sequence
/// - `seq` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_condition_seq_delete(seq: *mut Int2DdsConditionSeq) -> Int2DdsRet {
    if seq.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let _seq = Box::from_raw(seq);

    INT2DDS_RET_OK
}

/// Get the trigger value of a condition
///
/// # Safety
/// - `condition` must be a valid condition
/// - `triggered_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_condition_get_trigger_value(
    condition: *const Int2DdsCondition,
    triggered_out: *mut bool,
) -> Int2DdsRet {
    check_null!(condition);
    check_null!(triggered_out);

    let condition_ref = &*condition;
    match condition_ref.inner.get_trigger_value() {
        Ok(triggered) => {
            *triggered_out = triggered;
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Delete a condition handle
///
/// # Safety
/// - `condition` must be a valid condition obtained from `int2dds_condition_seq_get`
/// - `condition` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_condition_delete(condition: *mut Int2DdsCondition) -> Int2DdsRet {
    if condition.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let _condition = Box::from_raw(condition);

    INT2DDS_RET_OK
}

/// Attach a GuardCondition to the WaitSet
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `condition` must be a valid guard condition
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_attach_guard_condition(
    waitset: *const Int2DdsWaitSet,
    condition: *const Int2DdsGuardCondition,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(condition);

    let waitset_ref = &*waitset;
    let condition_ref = &*condition;

    let condition_arc: Arc<dyn int2dds::infrastructure::condition::Condition + Send + Sync> =
        condition_ref.inner.clone();

    match waitset_ref.inner.attach_condition(condition_arc) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Detach a GuardCondition from the WaitSet
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `condition` must be a valid guard condition that was previously attached
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_detach_guard_condition(
    waitset: *const Int2DdsWaitSet,
    condition: *const Int2DdsGuardCondition,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(condition);

    let waitset_ref = &*waitset;
    let condition_ref = &*condition;

    let condition_arc: Arc<dyn int2dds::infrastructure::condition::Condition + Send + Sync> =
        condition_ref.inner.clone();

    match waitset_ref.inner.detach_condition(condition_arc) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Attach a StatusCondition to the WaitSet
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `condition` must be a valid status condition
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_attach_condition(
    waitset: *const Int2DdsWaitSet,
    condition: *const Int2DdsStatusCondition,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(condition);

    let waitset_ref = &*waitset;
    let condition_ref = &*condition;

    match waitset_ref.inner.attach_condition(condition_ref.inner.clone()) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Detach a StatusCondition from the WaitSet
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `condition` must be a valid status condition that was previously attached
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_detach_condition(
    waitset: *const Int2DdsWaitSet,
    condition: *const Int2DdsStatusCondition,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(condition);

    let waitset_ref = &*waitset;
    let condition_ref = &*condition;

    match waitset_ref.inner.detach_condition(condition_ref.inner.clone()) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Attach a DataReader's status condition to the WaitSet
///
/// This allows waiting for data to arrive on a DataReader.
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `reader` must be a valid datareader
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_attach_datareader(
    waitset: *const Int2DdsWaitSet,
    reader: *const Int2DdsDataReader,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(reader);

    let waitset_ref = &*waitset;
    let reader_ref = &*reader;

    // Get status condition from reader
    let status_condition = match reader_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    match waitset_ref.inner.attach_condition(status_condition) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Detach a DataReader's status condition from the WaitSet
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `reader` must be a valid datareader that was previously attached
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_detach_datareader(
    waitset: *const Int2DdsWaitSet,
    reader: *const Int2DdsDataReader,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(reader);

    let waitset_ref = &*waitset;
    let reader_ref = &*reader;

    // Get status condition from reader
    let status_condition = match reader_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    match waitset_ref.inner.detach_condition(status_condition) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Attach a DataWriter's status condition to the WaitSet
///
/// This allows waiting for publication matched events on a DataWriter.
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `writer` must be a valid datawriter
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_attach_datawriter(
    waitset: *const Int2DdsWaitSet,
    writer: *const Int2DdsDataWriter,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(writer);

    let waitset_ref = &*waitset;
    let writer_ref = &*writer;

    // Get status condition from writer
    let status_condition = match writer_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    match waitset_ref.inner.attach_condition(status_condition) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Detach a DataWriter's status condition from the WaitSet
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `writer` must be a valid datawriter that was previously attached
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_detach_datawriter(
    waitset: *const Int2DdsWaitSet,
    writer: *const Int2DdsDataWriter,
) -> Int2DdsRet {
    check_null!(waitset);
    check_null!(writer);

    let waitset_ref = &*waitset;
    let writer_ref = &*writer;

    // Get status condition from writer
    let status_condition = match writer_ref.inner.get_statuscondition() {
        Ok(cond) => cond,
        Err(e) => return dds_error_to_code(&e),
    };

    match waitset_ref.inner.detach_condition(status_condition) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Delete a WaitSet
///
/// # Safety
/// - `waitset` must be a valid waitset
/// - `waitset` must not be used after this call
/// - All conditions should be detached first
#[no_mangle]
pub unsafe extern "C" fn int2dds_waitset_delete(waitset: *mut Int2DdsWaitSet) -> Int2DdsRet {
    if waitset.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let _waitset = Box::from_raw(waitset);

    INT2DDS_RET_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition;
    use std::ptr;

    #[test]
    fn test_waitset_basic() {
        unsafe {
            let mut waitset: *mut Int2DdsWaitSet = ptr::null_mut();

            // Create waitset
            let ret = int2dds_waitset_new(&mut waitset as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!waitset.is_null());

            // Delete waitset
            let ret = int2dds_waitset_delete(waitset);
            assert_eq!(ret, INT2DDS_RET_OK);
        }
    }

    #[test]
    fn test_waitset_with_guard_condition() {
        unsafe {
            let mut waitset: *mut Int2DdsWaitSet = ptr::null_mut();
            let mut guard_condition: *mut Int2DdsGuardCondition = ptr::null_mut();

            // Create waitset and condition
            int2dds_waitset_new(&mut waitset as *mut _);
            condition::int2dds_guard_condition_new(&mut guard_condition as *mut _);

            // Attach condition
            let ret = int2dds_waitset_attach_guard_condition(waitset, guard_condition);
            assert_eq!(ret, INT2DDS_RET_OK);

            // Detach condition
            let ret = int2dds_waitset_detach_guard_condition(waitset, guard_condition);
            assert_eq!(ret, INT2DDS_RET_OK);

            // Cleanup
            int2dds_waitset_delete(waitset);
            condition::int2dds_guard_condition_delete(guard_condition);
        }
    }

    #[test]
    fn test_waitset_timeout() {
        unsafe {
            let mut waitset: *mut Int2DdsWaitSet = ptr::null_mut();
            int2dds_waitset_new(&mut waitset as *mut _);

            // Wait with short timeout (should timeout)
            let timeout_ms = 100; // 100ms
            let ret = int2dds_waitset_wait(waitset, timeout_ms);
            assert_eq!(ret, INT2DDS_RET_TIMEOUT);

            int2dds_waitset_delete(waitset);
        }
    }
}
