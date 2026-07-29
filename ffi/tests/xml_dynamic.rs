//! End-to-end FFI verification: load an XML-defined type, then publish and
//! subscribe a DynamicData sample entirely through the C-ABI surface.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::thread::sleep;
use std::time::Duration;

use int2dds_ffi::context::*;
use int2dds_ffi::dynamic::*;
use int2dds_ffi::error::*;
use int2dds_ffi::participant::*;
use int2dds_ffi::publisher::*;
use int2dds_ffi::subscriber::*;
use int2dds_ffi::xml::*;

const XML: &str = r#"
<types>
  <struct name="Telemetry">
    <member name="id" type="uint32" key="true"/>
    <member name="temperature" type="float32"/>
    <member name="active" type="boolean"/>
    <member name="label" type="string"/>
    <member name="count" type="int64"/>
  </struct>
</types>
"#;

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn xml_dynamic_pub_sub_round_trip() {
    unsafe {
        // Factory + participant
        let mut factory = ptr::null_mut();
        assert_eq!(int2dds_domain_participant_factory_get_instance(&mut factory), INT2DDS_RET_OK);
        let mut participant = ptr::null_mut();
        assert_eq!(
            int2dds_create_participant(factory, 51, ptr::null(), &mut participant),
            INT2DDS_RET_OK
        );

        // Load the XML type and obtain a dynamic type support
        let mut registry = ptr::null_mut();
        assert_eq!(int2dds_xml_type_registry_create(&mut registry), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_xml_type_registry_load_str(registry, cstr(XML).as_ptr()),
            INT2DDS_RET_OK
        );

        let mut count = 0usize;
        assert_eq!(int2dds_xml_type_registry_type_count(registry, &mut count), INT2DDS_RET_OK);
        assert_eq!(count, 1);

        let mut support = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_get_type_support(
                registry,
                cstr("Telemetry").as_ptr(),
                &mut support
            ),
            INT2DDS_RET_OK
        );

        // Topic + endpoints
        let mut topic = ptr::null_mut();
        assert_eq!(
            int2dds_create_topic_dynamic(
                participant,
                cstr("TelemetryTopic").as_ptr(),
                support,
                ptr::null(),
                &mut topic
            ),
            INT2DDS_RET_OK
        );

        let mut publisher = ptr::null_mut();
        assert_eq!(
            int2dds_create_publisher(participant, ptr::null(), &mut publisher),
            INT2DDS_RET_OK
        );
        let mut subscriber = ptr::null_mut();
        assert_eq!(
            int2dds_create_subscriber(participant, ptr::null(), &mut subscriber),
            INT2DDS_RET_OK
        );

        let mut writer = ptr::null_mut();
        assert_eq!(
            int2dds_create_datawriter_dynamic(publisher, topic, support, ptr::null(), &mut writer),
            INT2DDS_RET_OK
        );
        let mut reader = ptr::null_mut();
        assert_eq!(
            int2dds_create_datareader_dynamic(subscriber, topic, support, ptr::null(), &mut reader),
            INT2DDS_RET_OK
        );

        // Wait for the writer to match the reader
        let mut matched = false;
        for _ in 0..200 {
            let mut c = 0i32;
            assert_eq!(
                int2dds_dynamic_writer_publication_matched_count(writer, &mut c),
                INT2DDS_RET_OK
            );
            if c > 0 {
                matched = true;
                break;
            }
            sleep(Duration::from_millis(50));
        }
        assert!(matched, "writer never matched the reader");

        // Build and publish a sample
        let mut data = ptr::null_mut();
        assert_eq!(int2dds_dynamic_data_create(support, &mut data), INT2DDS_RET_OK);
        assert_eq!(int2dds_dynamic_data_set_u32(data, cstr("id").as_ptr(), 7), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_dynamic_data_set_f32(data, cstr("temperature").as_ptr(), 23.5),
            INT2DDS_RET_OK
        );
        assert_eq!(
            int2dds_dynamic_data_set_bool(data, cstr("active").as_ptr(), true),
            INT2DDS_RET_OK
        );
        assert_eq!(
            int2dds_dynamic_data_set_string(
                data,
                cstr("label").as_ptr(),
                cstr("sensor-A").as_ptr()
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(
            int2dds_dynamic_data_set_i64(data, cstr("count").as_ptr(), -100),
            INT2DDS_RET_OK
        );

        assert_eq!(int2dds_dynamic_writer_write(writer, data), INT2DDS_RET_OK);

        // Take it back on the reader
        let mut received = ptr::null_mut();
        let mut got = false;
        for _ in 0..200 {
            let ret = int2dds_dynamic_reader_take(reader, &mut received, ptr::null_mut());
            if ret == INT2DDS_RET_OK {
                got = true;
                break;
            }
            assert_eq!(ret, INT2DDS_RET_NO_DATA);
            sleep(Duration::from_millis(50));
        }
        assert!(got, "no sample received");

        // Verify every field round-tripped
        let mut id = 0u32;
        assert_eq!(
            int2dds_dynamic_data_get_u32(received, cstr("id").as_ptr(), &mut id),
            INT2DDS_RET_OK
        );
        assert_eq!(id, 7);

        let mut temperature = 0f32;
        assert_eq!(
            int2dds_dynamic_data_get_f32(received, cstr("temperature").as_ptr(), &mut temperature),
            INT2DDS_RET_OK
        );
        assert_eq!(temperature, 23.5);

        let mut active = false;
        assert_eq!(
            int2dds_dynamic_data_get_bool(received, cstr("active").as_ptr(), &mut active),
            INT2DDS_RET_OK
        );
        assert!(active);

        let mut buf = [0 as c_char; 64];
        let mut out_len = 0usize;
        assert_eq!(
            int2dds_dynamic_data_get_string(
                received,
                cstr("label").as_ptr(),
                buf.as_mut_ptr(),
                buf.len(),
                &mut out_len
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(CStr::from_ptr(buf.as_ptr()).to_str().unwrap(), "sensor-A");

        let mut c = 0i64;
        assert_eq!(
            int2dds_dynamic_data_get_i64(received, cstr("count").as_ptr(), &mut c),
            INT2DDS_RET_OK
        );
        assert_eq!(c, -100);

        // Cleanup
        int2dds_dynamic_data_destroy(received);
        int2dds_dynamic_data_destroy(data);
        int2dds_dynamic_reader_destroy(reader);
        int2dds_dynamic_writer_destroy(writer);
        int2dds_dynamic_type_support_destroy(support);
        int2dds_xml_type_registry_destroy(registry);
        int2dds_delete_participant(participant);
    }
}
