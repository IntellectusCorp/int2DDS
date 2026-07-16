//! Thread-local last-error message, so bindings can retrieve the
//! `DdsError::Error(String)` reason that the `i32` return code drops.

use std::cell::RefCell;
use std::os::raw::c_char;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Store the calling thread's last error message (Error variant only).
pub(crate) fn set_last_error(msg: &str) {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(msg.to_owned()));
}

/// Clear the calling thread's last error message (non-Error failures).
pub(crate) fn clear_last_error() {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = None);
}

/// Copy the calling thread's last error message (UTF-8, NUL-terminated) into `buf`.
/// Returns the full message byte length, excluding the NUL.
///
/// `buf` null or `buf_len <= 0`: query mode, writes nothing, returns the length.
/// Message longer than `buf_len - 1`: truncated at a UTF-8 boundary, still
/// returns the full (pre-truncation) length. No message: writes "" and returns 0.
///
/// # Safety
/// `buf` must be null or point to at least `buf_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn int2dds_last_error_message(buf: *mut c_char, buf_len: i32) -> i32 {
    LAST_ERROR.with(|slot| {
        let borrow = slot.borrow();
        let msg = borrow.as_deref().unwrap_or("");
        let full_len = msg.len();

        if buf.is_null() || buf_len <= 0 {
            return full_len as i32;
        }

        // Reserve one byte for the NUL terminator.
        let max_bytes = (buf_len as usize) - 1;
        let mut end = max_bytes.min(full_len);
        while end > 0 && !msg.is_char_boundary(end) {
            end -= 1;
        }
        std::ptr::copy_nonoverlapping(msg.as_ptr() as *const c_char, buf, end);
        *buf.add(end) = 0;
        full_len as i32
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(buf_len: i32) -> (i32, String) {
        let mut buf = vec![0u8; buf_len.max(0) as usize];
        let ptr =
            if buf.is_empty() { std::ptr::null_mut() } else { buf.as_mut_ptr() as *mut c_char };
        let n = unsafe { int2dds_last_error_message(ptr, buf_len) };
        let s = buf.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
        (n, s)
    }

    #[test]
    fn stores_and_returns_message() {
        clear_last_error();
        set_last_error("QoS profile not found: Foo::Bar");
        let (n, s) = read(64);
        assert_eq!(n, "QoS profile not found: Foo::Bar".len() as i32);
        assert_eq!(s, "QoS profile not found: Foo::Bar");
    }

    #[test]
    fn empty_when_cleared() {
        set_last_error("something");
        clear_last_error();
        let (n, s) = read(64);
        assert_eq!(n, 0);
        assert_eq!(s, "");
    }

    #[test]
    fn query_mode_returns_length_without_writing() {
        clear_last_error();
        set_last_error("hello");
        let n = unsafe { int2dds_last_error_message(std::ptr::null_mut(), 0) };
        assert_eq!(n, 5);
    }

    #[test]
    fn truncates_at_char_boundary() {
        clear_last_error();
        set_last_error("héllo"); // 'é' is 2 bytes, 6 bytes total
                                 // buf_len=3 fits 2 bytes, but 'é' straddles the boundary, so only "h".
        let (n, s) = read(3);
        assert_eq!(n, 6); // full length before truncation
        assert_eq!(s, "h");
    }

    #[test]
    fn isolated_per_thread() {
        clear_last_error();
        set_last_error("main-thread");
        let n =
            std::thread::spawn(|| unsafe { int2dds_last_error_message(std::ptr::null_mut(), 0) })
                .join()
                .unwrap();
        assert_eq!(n, 0); // no message on another thread
    }
}
