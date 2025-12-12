//! # Conditions
//!
//! GuardCondition functions for use with WaitSets.
//!
//! ## GuardCondition
//!
//! A manually triggered condition that can be used to wake up a WaitSet
//! from another thread. Useful for implementing shutdown signals or
//! custom synchronization.

use std::sync::Arc;

use int2dds::infrastructure::guard_condition::GuardCondition;

use super::{error::*, types::*};

/// Create a new GuardCondition
///
/// # Safety
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_guard_condition_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_guard_condition_new(
    condition_out: *mut *mut Int2DdsGuardCondition,
) -> Int2DdsRet {
    check_null!(condition_out);

    let guard_condition = GuardCondition::new();
    let condition_arc = Arc::new(guard_condition);

    let condition_handle = Box::new(Int2DdsGuardCondition { inner: condition_arc });

    *condition_out = Box::into_raw(condition_handle);

    INT2DDS_RET_OK
}

/// Set the trigger value of a GuardCondition
///
/// # Safety
/// - `condition` must be a valid guard condition
/// - `value` is the new trigger value (true to trigger, false to reset)
#[no_mangle]
pub unsafe extern "C" fn int2dds_guard_condition_set_trigger_value(
    condition: *const Int2DdsGuardCondition,
    value: bool,
) -> Int2DdsRet {
    check_null!(condition);

    let condition_ref = &*condition;

    match condition_ref.inner.set_trigger_value(value) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the trigger value of a GuardCondition
///
/// # Safety
/// - `condition` must be a valid guard condition
/// - `value_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_guard_condition_get_trigger_value(
    condition: *const Int2DdsGuardCondition,
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

/// Delete a GuardCondition
///
/// # Safety
/// - `condition` must be a valid guard condition
/// - `condition` must not be used after this call
/// - The condition should be detached from any WaitSets first
#[no_mangle]
pub unsafe extern "C" fn int2dds_guard_condition_delete(
    condition: *mut Int2DdsGuardCondition,
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
    use std::ptr;

    #[test]
    fn test_guard_condition() {
        unsafe {
            let mut condition: *mut Int2DdsGuardCondition = ptr::null_mut();

            // Create guard condition
            let ret = int2dds_guard_condition_new(&mut condition as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!condition.is_null());

            // Get initial trigger value
            let mut value: bool = true;
            let ret = int2dds_guard_condition_get_trigger_value(condition, &mut value);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!value); // Should be false initially

            // Delete condition
            let ret = int2dds_guard_condition_delete(condition);
            assert_eq!(ret, INT2DDS_RET_OK);
        }
    }
}
