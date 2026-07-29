//! Direct `ByteBuffer` address extraction.
//!
//! Generated forwarders take payload pointers as `jlong` addresses rather than
//! unwrapping a `JByteBuffer` on every call. Java calls this once per buffer
//! and reuses the address, keeping the hot path free of JNI object handling.

use jni::objects::{JByteBuffer, JClass};
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
