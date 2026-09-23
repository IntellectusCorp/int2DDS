// Pointer discipline for the buffers the generated forwarders hand to the FFI.
// The JNI-touching halves of these helpers need a live JVM and are exercised by
// the Java smoke test in Task 11; what is testable here is the part that
// decides whether the FFI sees a pointer or a null.
use int2dds_java::generated_support::{fixed_ptr, fixed_ptr_mut, ptr_or_null};

#[test]
fn absent_byte_array_becomes_a_null_pointer_not_an_empty_string() {
    // A null `const char*` means "absent" in this FFI — an empty string does
    // not. Collapsing the two would silently change, for example, which QoS
    // profile a participant is created with.
    let absent: Option<Vec<u8>> = None;
    assert!(ptr_or_null(&absent).is_null());

    let present = Some(b"profile\0".to_vec());
    assert!(!ptr_or_null(&present).is_null());
}

#[test]
fn present_buffer_keeps_its_bytes_addressable() {
    let buf = Some(b"topic\0".to_vec());
    let p = ptr_or_null(&buf);
    let seen = unsafe { std::slice::from_raw_parts(p, 6) };
    assert_eq!(seen, b"topic\0");
}

#[test]
fn absent_fixed_array_becomes_a_null_pointer() {
    let absent: Option<[u8; 16]> = None;
    assert!(fixed_ptr(&absent).is_null());

    let present = Some([7u8; 16]);
    assert!(!fixed_ptr(&present).is_null());
}

#[test]
fn fixed_out_buffer_is_addressable_and_writable() {
    let mut out = [0u8; 16];
    let p = fixed_ptr_mut(&mut out);
    unsafe { (*p)[3] = 42 };
    assert_eq!(out[3], 42);
}
