//! FFI Listener Support for Callback-based Event Notification
//!
//! This module provides callback-based listener functionality for the FFI layer,
//! bridging Rust DDS listeners to C function pointers.
//!
//! ## Thread Safety
//! All callbacks are invoked from DDS background threads and must be thread-safe.
//! Multiple callbacks may execute concurrently on different threads.
//!
//! ## Memory Management
//! Listener objects are managed via Arc for thread-safe sharing between FFI and
//! Rust DDS. User context pointers are passed through unchanged - C applications
//! are responsible for their lifetime management.

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::data::Int2DdsData;
use int2dds::infrastructure::status::{
    LivelinessChangedStatus, LivelinessLostStatus, OfferedDeadlineMissedStatus,
    OfferedIncompatibleQosStatus, PublicationMatchedStatus, RequestedDeadlineMissedStatus,
    RequestedIncompatibleQosStatus, SampleLostStatus, SampleRejectedStatus,
    SubscriptionMatchedStatus,
};
use int2dds::publication::data_writer::DataWriter;
use int2dds::publication::data_writer_listener::DataWriterListener;
use int2dds::subscription::data_reader::DataReader;
use int2dds::subscription::data_reader_listener::DataReaderListener;

use crate::status::{
    Int2DdsLivelinessChangedStatus, Int2DdsLivelinessLostStatus,
    Int2DdsOfferedDeadlineMissedStatus, Int2DdsOfferedIncompatibleQosStatus,
    Int2DdsPublicationMatchedStatus, Int2DdsRequestedDeadlineMissedStatus,
    Int2DdsRequestedIncompatibleQosStatus, Int2DdsSampleLostStatus, Int2DdsSampleRejectedStatus,
    Int2DdsSubscriptionMatchedStatus,
};
use crate::types::{Int2DdsDataReader, Int2DdsDataWriter};

// ============================================================================
// C Function Pointer Types
// ============================================================================

/// User context passed to all callbacks
pub type Int2DdsUserContext = *mut c_void;

// DataReader callback types
pub type Int2DdsOnDataAvailableCallback =
    unsafe extern "C" fn(reader: *mut Int2DdsDataReader, user_context: Int2DdsUserContext);

pub type Int2DdsOnSubscriptionMatchedCallback = unsafe extern "C" fn(
    reader: *mut Int2DdsDataReader,
    status: *const Int2DdsSubscriptionMatchedStatus,
    user_context: Int2DdsUserContext,
);

pub type Int2DdsOnSampleRejectedCallback = unsafe extern "C" fn(
    reader: *mut Int2DdsDataReader,
    status: *const Int2DdsSampleRejectedStatus,
    user_context: Int2DdsUserContext,
);

pub type Int2DdsOnLivelinessChangedCallback = unsafe extern "C" fn(
    reader: *mut Int2DdsDataReader,
    status: *const Int2DdsLivelinessChangedStatus,
    user_context: Int2DdsUserContext,
);

pub type Int2DdsOnRequestedDeadlineMissedCallback = unsafe extern "C" fn(
    reader: *mut Int2DdsDataReader,
    status: *const Int2DdsRequestedDeadlineMissedStatus,
    user_context: Int2DdsUserContext,
);

pub type Int2DdsOnRequestedIncompatibleQosCallback = unsafe extern "C" fn(
    reader: *mut Int2DdsDataReader,
    status: *const Int2DdsRequestedIncompatibleQosStatus,
    user_context: Int2DdsUserContext,
);

pub type Int2DdsOnSampleLostCallback = unsafe extern "C" fn(
    reader: *mut Int2DdsDataReader,
    status: *const Int2DdsSampleLostStatus,
    user_context: Int2DdsUserContext,
);

// DataWriter callback types
pub type Int2DdsOnPublicationMatchedCallback = unsafe extern "C" fn(
    writer: *mut Int2DdsDataWriter,
    status: *const Int2DdsPublicationMatchedStatus,
    user_context: Int2DdsUserContext,
);

pub type Int2DdsOnOfferedDeadlineMissedCallback = unsafe extern "C" fn(
    writer: *mut Int2DdsDataWriter,
    status: *const Int2DdsOfferedDeadlineMissedStatus,
    user_context: Int2DdsUserContext,
);

pub type Int2DdsOnOfferedIncompatibleQosCallback = unsafe extern "C" fn(
    writer: *mut Int2DdsDataWriter,
    status: *const Int2DdsOfferedIncompatibleQosStatus,
    user_context: Int2DdsUserContext,
);

pub type Int2DdsOnLivelinessLostCallback = unsafe extern "C" fn(
    writer: *mut Int2DdsDataWriter,
    status: *const Int2DdsLivelinessLostStatus,
    user_context: Int2DdsUserContext,
);

// ============================================================================
// C-Compatible Listener Structs
// ============================================================================

/// C-compatible DataReader listener callbacks
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Int2DdsDataReaderListener {
    pub on_data_available: Int2DdsOnDataAvailableCallback,
    pub on_subscription_matched: Int2DdsOnSubscriptionMatchedCallback,
    pub on_sample_rejected: Int2DdsOnSampleRejectedCallback,
    pub on_liveliness_changed: Int2DdsOnLivelinessChangedCallback,
    pub on_requested_deadline_missed: Int2DdsOnRequestedDeadlineMissedCallback,
    pub on_requested_incompatible_qos: Int2DdsOnRequestedIncompatibleQosCallback,
    pub on_sample_lost: Int2DdsOnSampleLostCallback,
    pub user_context: Int2DdsUserContext,
}

/// C-compatible DataWriter listener callbacks
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Int2DdsDataWriterListener {
    pub on_publication_matched: Int2DdsOnPublicationMatchedCallback,
    pub on_offered_deadline_missed: Int2DdsOnOfferedDeadlineMissedCallback,
    pub on_offered_incompatible_qos: Int2DdsOnOfferedIncompatibleQosCallback,
    pub on_liveliness_lost: Int2DdsOnLivelinessLostCallback,
    pub user_context: Int2DdsUserContext,
}

// Safety: These types only contain function pointers and raw pointers
unsafe impl Send for Int2DdsDataReaderListener {}
unsafe impl Sync for Int2DdsDataReaderListener {}
unsafe impl Send for Int2DdsDataWriterListener {}
unsafe impl Sync for Int2DdsDataWriterListener {}

// ============================================================================
// Rust Listener Wrappers
// ============================================================================

/// Rust wrapper implementing DataReaderListener trait
/// Bridges Rust listener callbacks to C function pointers
pub struct FfiDataReaderListener {
    pub(crate) callbacks: Int2DdsDataReaderListener,
    pub(crate) reader_handle: *mut Int2DdsDataReader,
}

impl FfiDataReaderListener {
    /// Create new FFI DataReader listener
    pub fn new(
        callbacks: Int2DdsDataReaderListener,
        reader_handle: *mut Int2DdsDataReader,
    ) -> Self {
        Self { callbacks, reader_handle }
    }
}

// Safety: Function pointers and raw pointers are Send/Sync if used correctly
unsafe impl Send for FfiDataReaderListener {}
unsafe impl Sync for FfiDataReaderListener {}

impl DataReaderListener for FfiDataReaderListener {
    type Foo = Int2DdsData;

    fn on_data_available(&self, _reader: &DataReader<Int2DdsData>) {
        if self.callbacks.on_data_available as usize != 0 {
            let callback = self.callbacks.on_data_available;
            let reader_handle = self.reader_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(reader_handle, user_context);
            }));
        }
    }

    fn on_subscription_matched(
        &self,
        _reader: &DataReader<Int2DdsData>,
        status: &SubscriptionMatchedStatus,
    ) {
        if self.callbacks.on_subscription_matched as usize != 0 {
            let callback = self.callbacks.on_subscription_matched;
            let ffi_status: Int2DdsSubscriptionMatchedStatus = status.into();
            let reader_handle = self.reader_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(reader_handle, &ffi_status as *const _, user_context);
            }));
        }
    }

    fn on_sample_rejected(&self, _reader: &DataReader<Int2DdsData>, status: &SampleRejectedStatus) {
        if self.callbacks.on_sample_rejected as usize != 0 {
            let callback = self.callbacks.on_sample_rejected;
            let ffi_status: Int2DdsSampleRejectedStatus = status.into();
            let reader_handle = self.reader_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(reader_handle, &ffi_status as *const _, user_context);
            }));
        }
    }

    fn on_liveliness_changed(
        &self,
        _reader: &DataReader<Int2DdsData>,
        status: &LivelinessChangedStatus,
    ) {
        if self.callbacks.on_liveliness_changed as usize != 0 {
            let callback = self.callbacks.on_liveliness_changed;
            let ffi_status: Int2DdsLivelinessChangedStatus = status.into();
            let reader_handle = self.reader_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(reader_handle, &ffi_status as *const _, user_context);
            }));
        }
    }

    fn on_requested_deadline_missed(
        &self,
        _reader: &DataReader<Int2DdsData>,
        status: &RequestedDeadlineMissedStatus,
    ) {
        if self.callbacks.on_requested_deadline_missed as usize != 0 {
            let callback = self.callbacks.on_requested_deadline_missed;
            let ffi_status: Int2DdsRequestedDeadlineMissedStatus = status.into();
            let reader_handle = self.reader_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(reader_handle, &ffi_status as *const _, user_context);
            }));
        }
    }

    fn on_requested_incompatible_qos(
        &self,
        _reader: &DataReader<Int2DdsData>,
        status: &RequestedIncompatibleQosStatus,
    ) {
        if self.callbacks.on_requested_incompatible_qos as usize != 0 {
            let callback = self.callbacks.on_requested_incompatible_qos;
            let ffi_status: Int2DdsRequestedIncompatibleQosStatus = status.into();
            let reader_handle = self.reader_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(reader_handle, &ffi_status as *const _, user_context);
            }));
        }
    }

    fn on_sample_lost(&self, _reader: &DataReader<Int2DdsData>, status: &SampleLostStatus) {
        if self.callbacks.on_sample_lost as usize != 0 {
            let callback = self.callbacks.on_sample_lost;
            let ffi_status: Int2DdsSampleLostStatus = status.into();
            let reader_handle = self.reader_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(reader_handle, &ffi_status as *const _, user_context);
            }));
        }
    }
}

/// Rust wrapper implementing DataWriterListener trait
/// Bridges Rust listener callbacks to C function pointers
pub struct FfiDataWriterListener {
    pub(crate) callbacks: Int2DdsDataWriterListener,
    pub(crate) writer_handle: *mut Int2DdsDataWriter,
}

impl FfiDataWriterListener {
    /// Create new FFI DataWriter listener
    pub fn new(
        callbacks: Int2DdsDataWriterListener,
        writer_handle: *mut Int2DdsDataWriter,
    ) -> Self {
        Self { callbacks, writer_handle }
    }
}

// Safety: Function pointers and raw pointers are Send/Sync if used correctly
unsafe impl Send for FfiDataWriterListener {}
unsafe impl Sync for FfiDataWriterListener {}

impl DataWriterListener for FfiDataWriterListener {
    type Foo = Int2DdsData;

    fn on_publication_matched(
        &self,
        _writer: &DataWriter<Int2DdsData>,
        status: &PublicationMatchedStatus,
    ) {
        if self.callbacks.on_publication_matched as usize != 0 {
            let callback = self.callbacks.on_publication_matched;
            let ffi_status: Int2DdsPublicationMatchedStatus = status.into();
            let writer_handle = self.writer_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(writer_handle, &ffi_status as *const _, user_context);
            }));
        }
    }

    fn on_offered_deadline_missed(
        &self,
        _writer: &DataWriter<Int2DdsData>,
        status: &OfferedDeadlineMissedStatus,
    ) {
        if self.callbacks.on_offered_deadline_missed as usize != 0 {
            let callback = self.callbacks.on_offered_deadline_missed;
            let ffi_status: Int2DdsOfferedDeadlineMissedStatus = status.into();
            let writer_handle = self.writer_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(writer_handle, &ffi_status as *const _, user_context);
            }));
        }
    }

    fn on_offered_incompatible_qos(
        &self,
        _writer: &DataWriter<Int2DdsData>,
        status: &OfferedIncompatibleQosStatus,
    ) {
        if self.callbacks.on_offered_incompatible_qos as usize != 0 {
            let callback = self.callbacks.on_offered_incompatible_qos;
            let ffi_status: Int2DdsOfferedIncompatibleQosStatus = status.into();
            let writer_handle = self.writer_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(writer_handle, &ffi_status as *const _, user_context);
            }));
        }
    }

    fn on_liveliness_lost(&self, _writer: &DataWriter<Int2DdsData>, status: &LivelinessLostStatus) {
        if self.callbacks.on_liveliness_lost as usize != 0 {
            let callback = self.callbacks.on_liveliness_lost;
            let ffi_status: Int2DdsLivelinessLostStatus = status.into();
            let writer_handle = self.writer_handle;
            let user_context = self.callbacks.user_context;
            invoke_callback(AssertUnwindSafe(move || unsafe {
                callback(writer_handle, &ffi_status as *const _, user_context);
            }));
        }
    }
}

// ============================================================================
// Panic Handling Helper
// ============================================================================

/// Invoke a callback with panic catching
/// Prevents panics in C callbacks from unwinding into Rust DDS
fn invoke_callback<F>(f: F)
where
    F: FnOnce() + std::panic::UnwindSafe,
{
    if let Err(e) = catch_unwind(AssertUnwindSafe(f)) {
        eprintln!("FFI callback panicked: {:?}", e);
        // Log the panic but don't propagate - DDS must remain stable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

    static CALLBACK_INVOKED: AtomicBool = AtomicBool::new(false);
    static USER_CONTEXT_VALUE: AtomicI32 = AtomicI32::new(0);

    unsafe extern "C" fn test_on_data_available(
        _reader: *mut Int2DdsDataReader,
        user_context: Int2DdsUserContext,
    ) {
        CALLBACK_INVOKED.store(true, Ordering::SeqCst);
        if !user_context.is_null() {
            let ctx = user_context as *const i32;
            USER_CONTEXT_VALUE.store(*ctx, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_data_reader_listener_creation() {
        let listener = Int2DdsDataReaderListener {
            on_data_available: test_on_data_available,
            on_subscription_matched: unsafe { std::mem::transmute(0usize) },
            on_sample_rejected: unsafe { std::mem::transmute(0usize) },
            on_liveliness_changed: unsafe { std::mem::transmute(0usize) },
            on_requested_deadline_missed: unsafe { std::mem::transmute(0usize) },
            on_requested_incompatible_qos: unsafe { std::mem::transmute(0usize) },
            on_sample_lost: unsafe { std::mem::transmute(0usize) },
            user_context: std::ptr::null_mut(),
        };

        let reader_handle = std::ptr::null_mut();

        let ffi_listener = FfiDataReaderListener::new(listener, reader_handle);

        // Verify listener was created successfully
        assert!(ffi_listener.callbacks.on_data_available as usize != 0);
        assert!(ffi_listener.callbacks.on_subscription_matched as usize == 0);
    }

    #[test]
    fn test_panic_catching() {
        // Callback that panics
        unsafe extern "C" fn panicking_callback(
            _reader: *mut Int2DdsDataReader,
            _user_context: Int2DdsUserContext,
        ) {
            panic!("Test panic");
        }

        // Should not propagate panic
        invoke_callback(|| unsafe {
            panicking_callback(std::ptr::null_mut(), std::ptr::null_mut());
        });

        // If we got here, panic was caught successfully
        assert!(true);
    }
}
