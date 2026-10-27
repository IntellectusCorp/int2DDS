//! Product version of this library, so a binding can refuse to run against a
//! native build from a different release. Struct layouts and signatures are
//! hard-coded on the binding side; a mismatched library would otherwise read
//! and write the wrong memory instead of failing.

use std::os::raw::c_char;

/// Copy the library's product version (UTF-8, NUL-terminated, e.g. `"0.1.3"`)
/// into `buf`. Returns the version's full byte length, excluding the NUL.
///
/// Same buffer contract as `int2dds_last_error_message`: `buf` null or
/// `buf_len <= 0` is query mode and writes nothing; a too-small buffer is
/// truncated and still returns the full length.
///
/// # Safety
/// `buf` must be null or point to at least `buf_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn int2dds_version(buf: *mut c_char, buf_len: i32) -> i32 {
    crate::last_error::write_c_string(env!("CARGO_PKG_VERSION"), buf, buf_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_workspace_version() {
        let mut buf = [0u8; 64];
        let n = unsafe { int2dds_version(buf.as_mut_ptr() as *mut c_char, buf.len() as i32) };
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(n as usize, version.len());
        assert_eq!(&buf[..version.len()], version.as_bytes());
        assert_eq!(buf[version.len()], 0, "must be NUL-terminated");
    }

    #[test]
    fn query_mode_writes_nothing_and_reports_the_length() {
        let n = unsafe { int2dds_version(std::ptr::null_mut(), 0) };
        assert_eq!(n as usize, env!("CARGO_PKG_VERSION").len());
    }
}
