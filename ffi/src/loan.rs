//! DataReader loans (DDS v1.4 2.2.2.5.3.8 rule 3, 2.2.2.5.3.20).

use int2dds::dcps::{
    core::error::{DdsError, DdsResult},
    subscription::loaned_samples::{LoanedSample, LoanedSamples},
};

use crate::{
    data::Int2DdsData,
    error::*,
    subscriber::{reader_handle_from_c, state_kinds_from_masks},
    types::{Int2DdsDataReader, Int2DdsReadCondition, Int2DdsSampleInfo, ReadConditionKind},
};

/// Samples loaned from a DataReader's cache. Released only by `int2dds_datareader_return_loan`.
pub struct Int2DdsLoanedSamples {
    // None only while `int2dds_datareader_return_loan` holds the loan.
    inner: Option<LoanedSamples<Int2DdsData>>,
}

impl Int2DdsLoanedSamples {
    fn samples(&self) -> &[LoanedSample<Int2DdsData>] {
        self.inner.as_deref().unwrap_or(&[])
    }
}

unsafe fn finish(
    result: DdsResult<LoanedSamples<Int2DdsData>>,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    *loan_out = std::ptr::null_mut();
    match result {
        Ok(loan) => {
            *loan_out = Box::into_raw(Box::new(Int2DdsLoanedSamples { inner: Some(loan) }));
            INT2DDS_RET_OK
        }
        Err(DdsError::NoData) => INT2DDS_RET_NO_DATA,
        Err(e) => dds_error_to_code(&e),
    }
}

unsafe fn read_or_take(
    reader: *const Int2DdsDataReader,
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
    take: bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(loan_out);
    let (ss, vs, is) =
        state_kinds_from_masks(sample_state_mask, view_state_mask, instance_state_mask);
    let reader = &(*reader).inner;
    let result = if take {
        reader.take_loaned(max_samples, &ss, &vs, &is)
    } else {
        reader.read_loaned(max_samples, &ss, &vs, &is)
    };
    finish(result, loan_out)
}

unsafe fn read_or_take_w_condition(
    reader: *const Int2DdsDataReader,
    condition: *const Int2DdsReadCondition,
    max_samples: i32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
    take: bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(condition);
    check_null!(loan_out);
    let reader = &(*reader).inner;
    let result = match (&(*condition).kind, take) {
        (ReadConditionKind::Read(rc), true) => {
            reader.take_w_condition_loaned(max_samples, rc.clone())
        }
        (ReadConditionKind::Read(rc), false) => {
            reader.read_w_condition_loaned(max_samples, rc.clone())
        }
        (ReadConditionKind::Query(qc), true) => {
            reader.take_w_condition_loaned(max_samples, qc.clone())
        }
        (ReadConditionKind::Query(qc), false) => {
            reader.read_w_condition_loaned(max_samples, qc.clone())
        }
    };
    finish(result, loan_out)
}

#[derive(Clone, Copy)]
enum Scope {
    Instance,
    NextInstance,
}

#[allow(clippy::too_many_arguments)]
unsafe fn read_or_take_instance(
    reader: *const Int2DdsDataReader,
    handle: *const [u8; 16],
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
    scope: Scope,
    take: bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(loan_out);
    let handle = reader_handle_from_c(handle);
    let (ss, vs, is) =
        state_kinds_from_masks(sample_state_mask, view_state_mask, instance_state_mask);
    let reader = &(*reader).inner;
    let result = match (scope, take) {
        (Scope::Instance, false) => reader.read_instance_loaned(max_samples, handle, &ss, &vs, &is),
        (Scope::Instance, true) => reader.take_instance_loaned(max_samples, handle, &ss, &vs, &is),
        (Scope::NextInstance, false) => {
            reader.read_next_instance_loaned(max_samples, handle, &ss, &vs, &is)
        }
        (Scope::NextInstance, true) => {
            reader.take_next_instance_loaned(max_samples, handle, &ss, &vs, &is)
        }
    };
    finish(result, loan_out)
}

unsafe fn read_or_take_next_instance_w_condition(
    reader: *const Int2DdsDataReader,
    previous_handle: *const [u8; 16],
    condition: *const Int2DdsReadCondition,
    max_samples: i32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
    take: bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(condition);
    check_null!(loan_out);
    let previous = reader_handle_from_c(previous_handle);
    let reader = &(*reader).inner;
    let result = match (&(*condition).kind, take) {
        (ReadConditionKind::Read(rc), true) => {
            reader.take_next_instance_w_condition_loaned(max_samples, previous, rc.clone())
        }
        (ReadConditionKind::Read(rc), false) => {
            reader.read_next_instance_w_condition_loaned(max_samples, previous, rc.clone())
        }
        (ReadConditionKind::Query(qc), true) => {
            reader.take_next_instance_w_condition_loaned(max_samples, previous, qc.clone())
        }
        (ReadConditionKind::Query(qc), false) => {
            reader.read_next_instance_w_condition_loaned(max_samples, previous, qc.clone())
        }
    };
    finish(result, loan_out)
}

/// Loan samples from the cache without removing them. `NO_DATA` with a null loan when
/// nothing matches. A state mask of 0 selects ANY.
///
/// # Safety
/// - `reader` must be a valid datareader, `loan_out` a valid pointer
/// - A non-null `*loan_out` must be released with `int2dds_datareader_return_loan`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_loaned(
    reader: *const Int2DdsDataReader,
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take(
        reader,
        max_samples,
        sample_state_mask,
        view_state_mask,
        instance_state_mask,
        loan_out,
        false,
    )
}

/// `int2dds_datareader_read_loaned` that removes the samples from the cache.
///
/// # Safety
/// - Same as `int2dds_datareader_read_loaned`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_loaned(
    reader: *const Int2DdsDataReader,
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take(
        reader,
        max_samples,
        sample_state_mask,
        view_state_mask,
        instance_state_mask,
        loan_out,
        true,
    )
}

/// Loan samples matching a Read/QueryCondition without removing them.
///
/// # Safety
/// - `reader` must be a valid datareader, `condition` a condition created from it,
///   `loan_out` a valid pointer
/// - A non-null `*loan_out` must be released with `int2dds_datareader_return_loan`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_w_condition_loaned(
    reader: *const Int2DdsDataReader,
    condition: *const Int2DdsReadCondition,
    max_samples: i32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take_w_condition(reader, condition, max_samples, loan_out, false)
}

/// `int2dds_datareader_read_w_condition_loaned` that removes the samples from the cache.
///
/// # Safety
/// - Same as `int2dds_datareader_read_w_condition_loaned`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_w_condition_loaned(
    reader: *const Int2DdsDataReader,
    condition: *const Int2DdsReadCondition,
    max_samples: i32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take_w_condition(reader, condition, max_samples, loan_out, true)
}

/// `int2dds_datareader_read_loaned` limited to the instance `handle`. A NULL or all-zero
/// (NIL) handle is `INVALID_ARGUMENT`.
///
/// # Safety
/// - `reader` must be a valid datareader, `loan_out` a valid pointer
/// - `handle` must be null or point to a 16-byte instance handle
/// - A non-null `*loan_out` must be released with `int2dds_datareader_return_loan`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_instance_loaned(
    reader: *const Int2DdsDataReader,
    handle: *const [u8; 16],
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take_instance(
        reader,
        handle,
        max_samples,
        sample_state_mask,
        view_state_mask,
        instance_state_mask,
        loan_out,
        Scope::Instance,
        false,
    )
}

/// `int2dds_datareader_read_instance_loaned` that removes the samples from the cache.
///
/// # Safety
/// - Same as `int2dds_datareader_read_instance_loaned`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_instance_loaned(
    reader: *const Int2DdsDataReader,
    handle: *const [u8; 16],
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take_instance(
        reader,
        handle,
        max_samples,
        sample_state_mask,
        view_state_mask,
        instance_state_mask,
        loan_out,
        Scope::Instance,
        true,
    )
}

/// `int2dds_datareader_read_instance_loaned` for the next instance after `previous_handle`
/// that has matching samples. A NULL or NIL handle starts from the first instance.
///
/// # Safety
/// - Same as `int2dds_datareader_read_instance_loaned`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_next_instance_loaned(
    reader: *const Int2DdsDataReader,
    previous_handle: *const [u8; 16],
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take_instance(
        reader,
        previous_handle,
        max_samples,
        sample_state_mask,
        view_state_mask,
        instance_state_mask,
        loan_out,
        Scope::NextInstance,
        false,
    )
}

/// `int2dds_datareader_read_next_instance_loaned` that removes the samples from the cache.
///
/// # Safety
/// - Same as `int2dds_datareader_read_instance_loaned`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_next_instance_loaned(
    reader: *const Int2DdsDataReader,
    previous_handle: *const [u8; 16],
    max_samples: i32,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take_instance(
        reader,
        previous_handle,
        max_samples,
        sample_state_mask,
        view_state_mask,
        instance_state_mask,
        loan_out,
        Scope::NextInstance,
        true,
    )
}

/// `int2dds_datareader_read_next_instance_loaned` with a Read/QueryCondition instead of
/// state masks.
///
/// # Safety
/// - `reader` must be a valid datareader, `condition` a condition created from it,
///   `loan_out` a valid pointer
/// - `previous_handle` must be null or point to a 16-byte instance handle
/// - A non-null `*loan_out` must be released with `int2dds_datareader_return_loan`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_next_instance_w_condition_loaned(
    reader: *const Int2DdsDataReader,
    previous_handle: *const [u8; 16],
    condition: *const Int2DdsReadCondition,
    max_samples: i32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take_next_instance_w_condition(
        reader,
        previous_handle,
        condition,
        max_samples,
        loan_out,
        false,
    )
}

/// `int2dds_datareader_read_next_instance_w_condition_loaned` that removes the samples from
/// the cache.
///
/// # Safety
/// - Same as `int2dds_datareader_read_next_instance_w_condition_loaned`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_next_instance_w_condition_loaned(
    reader: *const Int2DdsDataReader,
    previous_handle: *const [u8; 16],
    condition: *const Int2DdsReadCondition,
    max_samples: i32,
    loan_out: *mut *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    read_or_take_next_instance_w_condition(
        reader,
        previous_handle,
        condition,
        max_samples,
        loan_out,
        true,
    )
}

/// Number of elements in a loan; 0 for null.
///
/// # Safety
/// - `loan` must be null or a loan not yet returned
#[no_mangle]
pub unsafe extern "C" fn int2dds_loaned_samples_length(loan: *const Int2DdsLoanedSamples) -> usize {
    if loan.is_null() {
        return 0;
    }
    (*loan).samples().len()
}

/// SampleInfo of element `index`.
///
/// # Safety
/// - `loan` must be a loan not yet returned, `info_out` a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_loaned_samples_get_info(
    loan: *const Int2DdsLoanedSamples,
    index: usize,
    info_out: *mut Int2DdsSampleInfo,
) -> Int2DdsRet {
    check_null!(loan);
    check_null!(info_out);
    match (*loan).samples().get(index) {
        Some(sample) => {
            *info_out = Int2DdsSampleInfo::from(sample.sample_info());
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_INVALID_ARGUMENT,
    }
}

/// Lends the serialized bytes of element `index`; valid until the loan is returned.
/// `NO_DATA` with a null pointer for an element without data.
///
/// # Safety
/// - `loan` must be a loan not yet returned; `data_out` and `size_out` valid pointers
#[no_mangle]
pub unsafe extern "C" fn int2dds_loaned_samples_get_data(
    loan: *const Int2DdsLoanedSamples,
    index: usize,
    data_out: *mut *const u8,
    size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(loan);
    check_null!(data_out);
    check_null!(size_out);
    *data_out = std::ptr::null();
    *size_out = 0;
    let Some(sample) = (*loan).samples().get(index) else {
        return INT2DDS_RET_INVALID_ARGUMENT;
    };
    match sample.serialized_data() {
        Some(bytes) => {
            *data_out = bytes.as_ptr();
            *size_out = bytes.len();
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Returns `loan` to `reader` and frees it. A loan from another reader is left valid
/// and owned by the caller (`PRECONDITION_NOT_MET`). A null loan is a no-op.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `loan` must be null or a loan not yet returned; it must not be used after `OK`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_return_loan(
    reader: *const Int2DdsDataReader,
    loan: *mut Int2DdsLoanedSamples,
) -> Int2DdsRet {
    check_null!(reader);
    if loan.is_null() {
        return INT2DDS_RET_OK;
    }
    let slot = &mut *loan;
    let Some(inner) = slot.inner.take() else {
        return INT2DDS_RET_OK;
    };
    match (*reader).inner.return_loan(inner) {
        Ok(()) => {
            drop(Box::from_raw(loan));
            INT2DDS_RET_OK
        }
        Err((e, inner)) => {
            slot.inner = Some(inner);
            dds_error_to_code(&e)
        }
    }
}
