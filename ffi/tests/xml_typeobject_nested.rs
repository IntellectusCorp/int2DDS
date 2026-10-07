//! Read-path fix: `int2dds_xml_type_registry_get_type_object` must carry the
//! nested-type dependency closure so a `TypeObject` obtained from an XML
//! registry can decode nested struct members, not just top-level primitives.
//!
//! Exercises the fix entirely through the C ABI: load an XML type with a
//! struct member referencing another struct, fetch its TypeObject via
//! `int2dds_xml_type_registry_get_type_object`, decode a CDR sample built for
//! that layout via `int2dds_dynamic_data_from_sample`, and read the nested
//! member back both by dotted path and via `int2dds_dynamic_data_get_member`.
//!
//! A control case rebuilds the same TypeObject *without* its dependency
//! closure (the pre-fix shape, via the still-public `from_type_object`) and
//! asserts the nested member does NOT resolve — proving the closure is what
//! makes the fix work.

use std::ptr;

use int2dds::dcps::topic::type_support::DdsType;
use int2dds::serialize::cdr::{ExtensibilityKind as CdrExtKind, XcdrSerialize, XcdrSerializer};
use int2dds::serialize::core::BufferManager;
use int2dds::xtypes::TypeObject;

use int2dds_ffi::context::*;
use int2dds_ffi::dynamic::*;
use int2dds_ffi::error::*;
use int2dds_ffi::participant::*;
use int2dds_ffi::xml::*;

const XML: &str = r#"
<types>
  <module name="t">
    <struct name="Inner" extensibility="final">
      <member name="x" type="int32"/>
      <member name="y" type="int32"/>
    </struct>
    <struct name="Outer" extensibility="final">
      <member name="inner" type="nonBasic" nonBasicTypeName="t::Inner"/>
    </struct>
  </module>
</types>
"#;

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct Inner {
    x: i32,
    y: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct Outer {
    inner: Inner,
}

fn cstr(s: &str) -> std::ffi::CString {
    std::ffi::CString::new(s).unwrap()
}

fn serialize_with_header<T: XcdrSerialize>(value: &T, ext: CdrExtKind) -> Vec<u8> {
    let mut ser = XcdrSerializer::new(true, ext);
    ser.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut ser).unwrap();
    ser.into_bytes()
}

#[test]
fn xml_get_type_object_resolves_nested_struct_member() {
    unsafe {
        let mut factory = ptr::null_mut();
        assert_eq!(int2dds_domain_participant_factory_get_instance(&mut factory), INT2DDS_RET_OK);
        let mut participant = ptr::null_mut();
        assert_eq!(
            int2dds_create_participant(factory, 55, ptr::null(), &mut participant),
            INT2DDS_RET_OK
        );

        let mut registry = ptr::null_mut();
        assert_eq!(int2dds_xml_type_registry_create(&mut registry), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_xml_type_registry_load_str(registry, cstr(XML).as_ptr()),
            INT2DDS_RET_OK
        );

        let mut to = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_get_type_object(registry, cstr("t::Outer").as_ptr(), &mut to),
            INT2DDS_RET_OK
        );

        let sample = Outer { inner: Inner { x: 11, y: -22 } };
        let bytes = serialize_with_header(&sample, CdrExtKind::Final);

        // ---- Fixed path: TypeObject carries the nested-dependency closure ----
        let mut data = ptr::null_mut();
        assert_eq!(
            int2dds_dynamic_data_from_sample(
                participant,
                bytes.as_ptr(),
                bytes.len(),
                to,
                &mut data
            ),
            INT2DDS_RET_OK
        );

        // Dotted path.
        let mut x = 0i32;
        assert_eq!(
            int2dds_dynamic_data_get_i32(data, cstr("inner.x").as_ptr(), &mut x),
            INT2DDS_RET_OK
        );
        assert_eq!(x, 11);

        // get_member + nested get.
        let mut inner = ptr::null_mut();
        assert_eq!(
            int2dds_dynamic_data_get_member(data, cstr("inner").as_ptr(), &mut inner),
            INT2DDS_RET_OK
        );
        let mut y = 0i32;
        assert_eq!(int2dds_dynamic_data_get_i32(inner, cstr("y").as_ptr(), &mut y), INT2DDS_RET_OK);
        assert_eq!(y, -22);

        int2dds_dynamic_data_destroy(inner);
        int2dds_dynamic_data_destroy(data);
        int2dds_type_object_destroy(to);

        // ---- Control: the same TypeObject without its dependency closure (the
        // pre-fix shape) must NOT resolve the nested member. ----
        let mut core_registry = int2dds::config::xml::XmlTypeRegistry::new();
        core_registry.load_str(XML).unwrap();
        let complete = core_registry.get_type_object("t::Outer").unwrap().clone();
        let to_nodeps =
            Box::into_raw(Box::new(int2dds_ffi::dynamic::Int2DdsTypeObject::from_type_object(
                TypeObject::Complete(complete),
            )));

        let mut data_nodeps = ptr::null_mut();
        let ret = int2dds_dynamic_data_from_sample(
            participant,
            bytes.as_ptr(),
            bytes.len(),
            to_nodeps,
            &mut data_nodeps,
        );
        assert_ne!(
            ret, INT2DDS_RET_OK,
            "without the dependency closure the nested struct member must NOT resolve (control)"
        );

        int2dds_type_object_destroy(to_nodeps);
        int2dds_xml_type_registry_destroy(registry);
        int2dds_delete_participant(participant);
    }
}
