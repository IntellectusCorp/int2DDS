//! Direct `ByteBuffer` address extraction.
//!
//! Generated forwarders take payload pointers as `jlong` addresses rather than
//! unwrapping a `JByteBuffer` on every call. Java calls this once per buffer
//! and reuses the address, keeping the hot path free of JNI object handling.

use jni::objects::{JByteBuffer, JClass, JObject};
use jni::sys::jlong;
use jni::JNIEnv;

/// Returns the native address of a direct `ByteBuffer`, or 0 if the buffer is
/// not direct. Java-side callers must reject 0.
///
/// # Safety
/// Invoked by the JVM under JNI conventions.
#[no_mangle]
pub extern "system" fn Java_com_intellectus_int2dds_internal_ffi_Ffi_directBufferAddress(
    env: JNIEnv,
    _class: JClass,
    buf: JByteBuffer,
) -> jlong {
    match env.get_direct_buffer_address(&buf) {
        Ok(ptr) => ptr as jlong,
        Err(_) => 0,
    }
}

/// Wraps a raw native address as a direct `ByteBuffer` of `cap` bytes, the
/// reverse of `directBufferAddress`. Used to hand the caller a writable view
/// over a DDS-owned buffer returned by `int2dds_datawriter_prepare_serialized_write`
/// (a raw address `long`, not a `JByteBuffer` -- there is nothing to unwrap on
/// that path, only an address to wrap). Bound to `FfiHandwritten`, not `Ffi`:
/// this helper is hand-written, not generated. Returns a null `ByteBuffer` on
/// failure; Java-side callers must reject null.
///
/// # Safety
/// `addr` must be a valid pointer to at least `cap` writable bytes that stays
/// valid at least as long as the returned buffer is used -- guaranteed here by
/// the loan the DDS core holds until commit/abort, not by this function.
/// Invoked by the JVM under JNI conventions.
#[no_mangle]
pub extern "system" fn Java_com_intellectus_int2dds_internal_ffi_FfiHandwritten_addressToDirectByteBuffer<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    addr: jlong,
    cap: jlong,
) -> JByteBuffer<'local> {
    unsafe {
        match env.new_direct_byte_buffer(addr as usize as *mut u8, cap as usize) {
            Ok(buf) => buf,
            Err(_) => JByteBuffer::from(JObject::null()),
        }
    }
}
