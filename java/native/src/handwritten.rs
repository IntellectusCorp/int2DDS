//! The one FFI function the generator cannot emit.
//!
//! `int2dds_participant_qos_get_properties_with_prefix` takes a C callback and
//! a `user_data` pointer. We supply a trampoline that accumulates the pairs
//! into a `Vec`, then hand the whole set back to Java as `byte[][]`.

use jni::objects::{JByteArray, JClass, JObject, JObjectArray};
use jni::JNIEnv;
use std::ffi::{c_char, c_void, CStr};

/// Flatten name/value pairs into alternating UTF-8 entries.
///
/// The layout is positional, so an empty value still occupies its slot —
/// dropping it would shift every following name into a value position.
pub fn flatten_pairs(pairs: &[(String, String)]) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(pairs.len() * 2);
    for (n, v) in pairs {
        out.push(n.as_bytes().to_vec());
        out.push(v.as_bytes().to_vec());
    }
    out
}

/// Trampoline handed to the FFI. `user_data` points at a `Vec<(String, String)>`.
///
/// The FFI documents `name` and `value` as borrowed for the call only, so both
/// are copied here rather than retained.
///
/// # Safety
/// `name` and `value` must be valid NUL-terminated C strings; `user_data` must
/// point at a live `Vec<(String, String)>`.
unsafe extern "C" fn collect(
    name: *const c_char,
    value: *const c_char,
    user_data: *mut c_void,
) -> i32 {
    if name.is_null() || value.is_null() || user_data.is_null() {
        return 0;
    }
    let acc = &mut *(user_data as *mut Vec<(String, String)>);
    let n = CStr::from_ptr(name).to_string_lossy().into_owned();
    let v = CStr::from_ptr(value).to_string_lossy().into_owned();
    acc.push((n, v));
    0
}

/// Returns alternating name/value UTF-8 entries, or an empty array on error.
///
/// A null `prefix` reaches the FFI as a null pointer, which it rejects, so the
/// result is the empty array rather than every property.
///
/// # Safety
/// Invoked by the JVM under JNI conventions.
#[no_mangle]
pub extern "system" fn Java_com_intellectus_int2dds_internal_ffi_FfiHandwritten_participantQosPropertiesWithPrefix<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    qos: jni::sys::jlong,
    prefix: JByteArray<'local>,
) -> JObjectArray<'local> {
    let byte_class = env.find_class("[B").expect("[B class must resolve");
    let empty =
        env.new_object_array(0, &byte_class, JObject::null()).expect("allocate empty array");

    let prefix_bytes = crate::generated_support::take_bytes(&mut env, &prefix);
    let mut acc: Vec<(String, String)> = Vec::new();

    let ret = unsafe {
        int2dds_ffi::qos::int2dds_participant_qos_get_properties_with_prefix(
            qos as usize as *const _,
            crate::generated_support::ptr_or_null(&prefix_bytes) as *const c_char,
            Some(collect),
            &mut acc as *mut _ as *mut c_void,
        )
    };
    if ret != 0 {
        return empty;
    }

    let flat = flatten_pairs(&acc);
    let arr = match env.new_object_array(flat.len() as i32, &byte_class, JObject::null()) {
        Ok(a) => a,
        Err(_) => return empty,
    };
    for (i, entry) in flat.iter().enumerate() {
        let Ok(jb) = env.byte_array_from_slice(entry) else {
            return empty;
        };
        if env.set_object_array_element(&arr, i as i32, jb).is_err() {
            return empty;
        }
    }
    arr
}
