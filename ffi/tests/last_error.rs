//! Tests that DdsError messages reach the last-error channel across the FFI.

use std::ffi::CString;
use std::os::raw::c_char;
use std::ptr;

use int2dds_ffi::context::*;
use int2dds_ffi::error::*;
use int2dds_ffi::last_error::int2dds_last_error_message;
use int2dds_ffi::participant::*;
use int2dds_ffi::subscriber::*;

fn last_error_string() -> String {
    let len = unsafe { int2dds_last_error_message(ptr::null_mut(), 0) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u8; (len + 1) as usize];
    let n = unsafe { int2dds_last_error_message(buf.as_mut_ptr() as *mut c_char, len + 1) };
    String::from_utf8_lossy(&buf[..n.max(0) as usize]).into_owned()
}

/// Error(String) failure: the reason reaches the last-error channel.
#[test]
fn error_message_propagates_through_ffi() {
    unsafe {
        let mut factory = ptr::null_mut();
        assert_eq!(int2dds_domain_participant_factory_get_instance(&mut factory), INT2DDS_RET_OK);
        let mut participant = ptr::null_mut();
        assert_eq!(
            int2dds_create_participant(factory, ptr::null(), 71, &mut participant),
            INT2DDS_RET_OK
        );

        // Missing profile → DdsError::Error("QoS profile not found: ...")
        let bogus = CString::new("NoSuchLib::NoSuchProfile").unwrap();
        let mut subscriber = ptr::null_mut();
        let ret =
            int2dds_create_subscriber_with_profile(participant, bogus.as_ptr(), &mut subscriber);

        assert_eq!(ret, INT2DDS_RET_ERROR);
        let msg = last_error_string();
        assert!(
            msg.contains("QoS profile not found"),
            "expected profile-not-found message, got: {msg:?}"
        );

        int2dds_delete_participant(participant);
    }
}

/// The NULL argument path stores an annotated "null pointer" message.
#[test]
fn null_pointer_sets_annotated_message() {
    let ret = unsafe {
        int2dds_create_subscriber_with_profile(ptr::null(), ptr::null(), ptr::null_mut())
    };
    assert_eq!(ret, INT2DDS_RET_NULL_POINTER);
    let msg = last_error_string();
    assert!(msg.contains("null pointer"), "got: {msg:?}");
}
