//! # XML Type Registry
//!
//! C-callable surface over [`XmlTypeRegistry`], letting C/host languages load
//! types defined in XML at runtime and obtain a dynamic type support for
//! pub/sub via the `int2dds_*_dynamic` functions in [`crate::dynamic`].

use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Arc;

use int2dds::config::xml::XmlTypeRegistry;
use int2dds::xtypes::TypeObject;

use crate::dynamic::{copy_str_to_c, Int2DdsDynamicTypeSupport, Int2DdsTypeObject};
use crate::error::*;

/// Opaque handle wrapping an [`XmlTypeRegistry`].
pub struct Int2DdsXmlTypeRegistry {
    inner: XmlTypeRegistry,
}
unsafe impl Send for Int2DdsXmlTypeRegistry {}
unsafe impl Sync for Int2DdsXmlTypeRegistry {}

/// Create an empty XML type registry. Destroy with
/// `int2dds_xml_type_registry_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_xml_type_registry_create(
    out: *mut *mut Int2DdsXmlTypeRegistry,
) -> Int2DdsRet {
    check_null!(out);
    *out = Box::into_raw(Box::new(Int2DdsXmlTypeRegistry { inner: XmlTypeRegistry::new() }));
    INT2DDS_RET_OK
}

/// Create an XML type registry and load `path` into it in one step.
#[no_mangle]
pub unsafe extern "C" fn int2dds_xml_type_registry_from_file(
    path: *const c_char,
    out: *mut *mut Int2DdsXmlTypeRegistry,
) -> Int2DdsRet {
    check_null!(path);
    check_null!(out);
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let registry = ffi_try!(XmlTypeRegistry::from_file(path_str));
    *out = Box::into_raw(Box::new(Int2DdsXmlTypeRegistry { inner: registry }));
    INT2DDS_RET_OK
}

/// Load additional types from an XML file into an existing registry.
#[no_mangle]
pub unsafe extern "C" fn int2dds_xml_type_registry_load_file(
    registry: *mut Int2DdsXmlTypeRegistry,
    path: *const c_char,
) -> Int2DdsRet {
    check_null!(registry);
    check_null!(path);
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    ffi_try!((*registry).inner.load_file(path_str));
    INT2DDS_RET_OK
}

/// Load additional types from an in-memory XML string into an existing registry.
#[no_mangle]
pub unsafe extern "C" fn int2dds_xml_type_registry_load_str(
    registry: *mut Int2DdsXmlTypeRegistry,
    xml: *const c_char,
) -> Int2DdsRet {
    check_null!(registry);
    check_null!(xml);
    let xml_str = match CStr::from_ptr(xml).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    ffi_try!((*registry).inner.load_str(xml_str));
    INT2DDS_RET_OK
}

/// Look up a loaded type by name and build a dynamic type support (with its full
/// dependency closure). Destroy the result with
/// `int2dds_dynamic_type_support_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_xml_type_registry_get_type_support(
    registry: *const Int2DdsXmlTypeRegistry,
    name: *const c_char,
    out: *mut *mut Int2DdsDynamicTypeSupport,
) -> Int2DdsRet {
    check_null!(registry);
    check_null!(name);
    check_null!(out);
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    // Report an unknown name the same way `..._get_type_object` does, rather
    // than as a generic error, so both lookups surface not-found identically.
    if (*registry).inner.get_type_object(name_str).is_none() {
        return INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND;
    }
    let support = ffi_try!((*registry).inner.get(name_str));
    *out = Box::into_raw(Box::new(Int2DdsDynamicTypeSupport { inner: Arc::new(support) }));
    INT2DDS_RET_OK
}

/// Look up a loaded type by name and return its TypeObject, carrying its full
/// nested-dependency closure so struct/array/sequence members decode correctly.
/// Destroy the result with `int2dds_type_object_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_xml_type_registry_get_type_object(
    registry: *const Int2DdsXmlTypeRegistry,
    name: *const c_char,
    out: *mut *mut Int2DdsTypeObject,
) -> Int2DdsRet {
    check_null!(registry);
    check_null!(name);
    check_null!(out);
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let (complete, deps) = match (*registry).inner.get_type_object_with_deps(name_str) {
        Some(v) => v,
        None => return INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    };
    let handle =
        Int2DdsTypeObject::from_type_object_with_deps(TypeObject::Complete(complete), deps);
    *out = Box::into_raw(Box::new(handle));
    INT2DDS_RET_OK
}

/// Number of types loaded in the registry.
#[no_mangle]
pub unsafe extern "C" fn int2dds_xml_type_registry_type_count(
    registry: *const Int2DdsXmlTypeRegistry,
    out: *mut usize,
) -> Int2DdsRet {
    check_null!(registry);
    check_null!(out);
    *out = (*registry).inner.type_names().len();
    INT2DDS_RET_OK
}

/// Copy the fully-qualified name of the type at `index` into `buf`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_xml_type_registry_type_name(
    registry: *const Int2DdsXmlTypeRegistry,
    index: usize,
    buf: *mut c_char,
    buf_len: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(registry);
    check_null!(buf);
    check_null!(out_len);
    let names = (*registry).inner.type_names();
    let name = match names.get(index) {
        Some(n) => n,
        None => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    copy_str_to_c(name, buf, buf_len, out_len)
}

/// Destroy an XML type registry handle. Safe to call with null.
#[no_mangle]
pub unsafe extern "C" fn int2dds_xml_type_registry_destroy(registry: *mut Int2DdsXmlTypeRegistry) {
    if !registry.is_null() {
        drop(Box::from_raw(registry));
    }
}
