// These test the pure byte-level logic that the JNI wrappers delegate to.
// Full JNI round-trips are covered by the Java smoke test in Task 11.
use int2dds_java::strings::{bytes_to_cstring, clamp_copy};

#[test]
fn utf8_bytes_become_a_nul_terminated_cstring() {
    let c = bytes_to_cstring("hello_world_topic".as_bytes()).unwrap();
    assert_eq!(c.to_bytes(), b"hello_world_topic");
    assert_eq!(c.as_bytes_with_nul().last(), Some(&0u8));
}

#[test]
fn non_ascii_survives_as_standard_utf8() {
    // JNI's NewStringUTF would mangle this; our byte[] path must not.
    let s = "센서/온도🌡";
    let c = bytes_to_cstring(s.as_bytes()).unwrap();
    assert_eq!(c.to_bytes(), s.as_bytes());
}

#[test]
fn interior_nul_is_rejected_not_truncated() {
    assert!(bytes_to_cstring(b"bad\0name").is_err());
}

#[test]
fn clamp_copy_truncates_and_reports_full_length() {
    let mut dst = [0u8; 4];
    let n = clamp_copy(&mut dst, b"abcdefgh");
    assert_eq!(n, 8, "returns the untruncated length");
    assert_eq!(&dst, b"abcd");
}

#[test]
fn clamp_copy_handles_exact_fit_and_empty() {
    let mut dst = [0u8; 3];
    assert_eq!(clamp_copy(&mut dst, b"abc"), 3);
    assert_eq!(&dst, b"abc");
    let mut empty: [u8; 0] = [];
    assert_eq!(clamp_copy(&mut empty, b"abc"), 3);
}
