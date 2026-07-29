//! Environment-variable–driven configuration helpers.
//!
//! These wrappers expose `int2dds::common::env::*` setters through the C ABI so
//! C/C#/Python clients can configure transport-layer fallbacks (e.g. multicast
//! TTL) without authoring a JSON profile or building a `PropertyQosPolicy`.
//!
//! All setters mutate the *current process* environment and must be called
//! before the first `DomainParticipant` is created in order to take effect.

use std::ffi::CStr;
use std::os::raw::c_char;

use super::error::*;

/// Sets the IPv4 multicast TTL fallback via the `INT2DDS_MULTICAST_TTL`
/// environment variable.
///
/// Used by `TransportConfig` only when a `DomainParticipantQos` does not carry an
/// explicit `int2dds.transport.UDPv4.multicast_ttl` property entry, so explicit
/// QoS settings always win.
///
/// # Safety
/// Call before the first `DomainParticipant` is created. Mutating process
/// environment from threads other than the main one is undefined behavior on
/// some platforms.
#[no_mangle]
pub unsafe extern "C" fn int2dds_env_set_multicast_ttl(ttl: u8) -> Int2DdsRet {
    int2dds::common::env::set_multicast_ttl(ttl);
    INT2DDS_RET_OK
}

/// Reads the current `INT2DDS_MULTICAST_TTL` env override.
///
/// On success writes the parsed TTL into `*ttl_out` and sets `*has_value_out`
/// to `true`. When the variable is unset, empty, or invalid (non-`u8`),
/// `*has_value_out` is set to `false` and `*ttl_out` is left untouched.
///
/// # Safety
/// `ttl_out` and `has_value_out` must be valid, writable pointers.
#[no_mangle]
pub unsafe extern "C" fn int2dds_env_get_multicast_ttl(
    ttl_out: *mut u8,
    has_value_out: *mut bool,
) -> Int2DdsRet {
    check_null!(ttl_out);
    check_null!(has_value_out);

    match int2dds::common::env::get_multicast_ttl_override() {
        Some(ttl) => {
            *ttl_out = ttl;
            *has_value_out = true;
        }
        None => {
            *has_value_out = false;
        }
    }
    INT2DDS_RET_OK
}

/// Sets the QoS profile file path(s) to auto-load, via the `DDS_QOS_PROFILE`
/// environment variable. The `DomainParticipantFactory` singleton auto-loads
/// these when it first initializes (on the first participant creation); the
/// `*_QOS_DEFAULT` resolution then draws QoS from the selected default profile.
///
/// Multiple paths may be joined with `,` (also `;` on Windows / `:` on Unix).
///
/// # Safety
/// - `path` must be a valid, null-terminated UTF-8 C string.
/// - Call before the first `DomainParticipant` is created so the factory
///   singleton picks it up when it initializes.
#[no_mangle]
pub unsafe extern "C" fn int2dds_env_set_qos_profile(path: *const c_char) -> Int2DdsRet {
    check_null!(path);
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    int2dds::common::env::set_qos_profile(path_str);
    INT2DDS_RET_OK
}

/// Selects the default QoS profile (`"Library::Profile"`) via the
/// `DDS_DEFAULT_QOS_PROFILE` environment variable. The `*_QOS_DEFAULT`
/// resolution reads this at entity-creation time, so `NULL`-QoS creators
/// (participant/publisher/writer with default QoS) draw from this profile.
///
/// # Safety
/// - `profile` must be a valid, null-terminated UTF-8 C string
///   (e.g. `"HelloWorldDataFrag::Reliable"`).
#[no_mangle]
pub unsafe extern "C" fn int2dds_env_set_default_qos_profile(profile: *const c_char) -> Int2DdsRet {
    check_null!(profile);
    let profile_str = match CStr::from_ptr(profile).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    int2dds::common::env::set_default_qos_profile(profile_str);
    INT2DDS_RET_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear() {
        unsafe { std::env::remove_var("INT2DDS_MULTICAST_TTL") };
    }

    #[test]
    fn set_then_get_round_trips() {
        unsafe {
            clear();
            let mut ttl: u8 = 0;
            let mut has: bool = true;
            assert_eq!(int2dds_env_get_multicast_ttl(&mut ttl, &mut has), INT2DDS_RET_OK);
            assert!(!has, "unset variable reports has_value=false");

            assert_eq!(int2dds_env_set_multicast_ttl(48), INT2DDS_RET_OK);

            let mut ttl: u8 = 0;
            let mut has: bool = false;
            assert_eq!(int2dds_env_get_multicast_ttl(&mut ttl, &mut has), INT2DDS_RET_OK);
            assert!(has);
            assert_eq!(ttl, 48);
            clear();
        }
    }

    #[test]
    fn rejects_null_outputs() {
        unsafe {
            assert_eq!(
                int2dds_env_get_multicast_ttl(std::ptr::null_mut(), &mut false),
                INT2DDS_RET_NULL_POINTER
            );
            assert_eq!(
                int2dds_env_get_multicast_ttl(&mut 0u8, std::ptr::null_mut()),
                INT2DDS_RET_NULL_POINTER
            );
        }
    }
}
