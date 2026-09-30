//! Verify that the sample XML files parse through the FFI XML registry and that
//! every type they declare resolves to a dynamic type support.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::ptr;

use int2dds_ffi::dynamic::*;
use int2dds_ffi::error::*;
use int2dds_ffi::xml::*;

fn xml_path(name: &str) -> CString {
    let p: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", name].iter().collect();
    assert!(p.exists(), "missing example XML: {}", p.display());
    CString::new(p.to_str().unwrap()).unwrap()
}

/// Load `file` through the FFI registry and assert every declared type resolves
/// to a dynamic type support. Returns the resolved type names.
unsafe fn load_and_resolve(file: &str) -> Vec<String> {
    let mut registry = ptr::null_mut();
    assert_eq!(
        int2dds_xml_type_registry_from_file(xml_path(file).as_ptr(), &mut registry),
        INT2DDS_RET_OK,
        "failed to load {file}"
    );

    let mut count = 0usize;
    assert_eq!(int2dds_xml_type_registry_type_count(registry, &mut count), INT2DDS_RET_OK);
    assert!(count > 0, "{file} declared no types");

    let mut names = Vec::new();
    for i in 0..count {
        let mut buf = [0 as c_char; 256];
        let mut out_len = 0usize;
        assert_eq!(
            int2dds_xml_type_registry_type_name(
                registry,
                i,
                buf.as_mut_ptr(),
                buf.len(),
                &mut out_len
            ),
            INT2DDS_RET_OK
        );
        let name = CStr::from_ptr(buf.as_ptr()).to_str().unwrap().to_string();

        // Every declared type must resolve to a full dynamic type support.
        let cname = CString::new(name.clone()).unwrap();
        let mut support = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_get_type_support(registry, cname.as_ptr(), &mut support),
            INT2DDS_RET_OK,
            "{file}: type '{name}' failed to resolve to a dynamic type support"
        );
        int2dds_dynamic_type_support_destroy(support);

        // And to a TypeObject for introspection.
        let mut type_obj = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_get_type_object(registry, cname.as_ptr(), &mut type_obj),
            INT2DDS_RET_OK
        );
        int2dds_type_object_destroy(type_obj);

        names.push(name);
    }

    int2dds_xml_type_registry_destroy(registry);
    names
}

#[test]
fn simple_sample_parses() {
    unsafe {
        let names = load_and_resolve("sensor_data.xml");
        assert!(names.iter().any(|n| n == "SensorData"));
    }
}

#[test]
fn unknown_type_name_is_not_found_from_both_lookups() {
    unsafe {
        let mut registry = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_from_file(
                xml_path("sensor_data.xml").as_ptr(),
                &mut registry
            ),
            INT2DDS_RET_OK
        );

        let bogus = CString::new("NoSuchType").unwrap();

        // Both lookups must report an unknown name the same way.
        let mut support = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_get_type_support(registry, bogus.as_ptr(), &mut support),
            INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND
        );
        assert!(support.is_null());

        let mut type_obj = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_get_type_object(registry, bogus.as_ptr(), &mut type_obj),
            INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND
        );
        assert!(type_obj.is_null());

        int2dds_xml_type_registry_destroy(registry);
    }
}
