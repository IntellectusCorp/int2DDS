//! Regression test: destroying a dynamic writer/reader through the C-ABI
//! must unregister it from its parent publisher/subscriber, otherwise the
//! parent (and the participant) can never be deleted afterward.

use std::ffi::CString;
use std::ptr;

use int2dds_ffi::context::*;
use int2dds_ffi::dynamic::*;
use int2dds_ffi::error::*;
use int2dds_ffi::participant::*;
use int2dds_ffi::publisher::*;
use int2dds_ffi::subscriber::*;
use int2dds_ffi::topic::*;
use int2dds_ffi::xml::*;

const XML: &str = r#"
<types>
  <struct name="Telemetry">
    <member name="id" type="uint32" key="true"/>
    <member name="temperature" type="float32"/>
  </struct>
</types>
"#;

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn dynamic_writer_destroy_unregisters_from_publisher() {
    unsafe {
        let mut factory = ptr::null_mut();
        assert_eq!(int2dds_domain_participant_factory_get_instance(&mut factory), INT2DDS_RET_OK);
        let mut participant = ptr::null_mut();
        assert_eq!(
            int2dds_create_participant(factory, 53, ptr::null(), &mut participant),
            INT2DDS_RET_OK
        );

        let mut registry = ptr::null_mut();
        assert_eq!(int2dds_xml_type_registry_create(&mut registry), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_xml_type_registry_load_str(registry, cstr(XML).as_ptr()),
            INT2DDS_RET_OK
        );

        let mut support = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_get_type_support(
                registry,
                cstr("Telemetry").as_ptr(),
                &mut support
            ),
            INT2DDS_RET_OK
        );

        let mut topic = ptr::null_mut();
        assert_eq!(
            int2dds_create_topic_dynamic(
                participant,
                cstr("TelemetryTopicW").as_ptr(),
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

        let mut writer = ptr::null_mut();
        assert_eq!(
            int2dds_create_datawriter_dynamic(publisher, topic, support, ptr::null(), &mut writer),
            INT2DDS_RET_OK
        );

        // Destroying the dynamic writer must fully unregister it; otherwise
        // the publisher below still believes it owns a live writer.
        int2dds_dynamic_writer_destroy(writer);

        assert_eq!(
            int2dds_delete_publisher(publisher),
            INT2DDS_RET_OK,
            "publisher delete failed after dynamic writer destroy \
             (writer was not unregistered from its parent)"
        );

        assert_eq!(int2dds_delete_topic(topic), INT2DDS_RET_OK);
        int2dds_dynamic_type_support_destroy(support);
        int2dds_xml_type_registry_destroy(registry);

        assert_eq!(
            int2dds_delete_participant(participant),
            INT2DDS_RET_OK,
            "participant delete failed after publisher was deleted"
        );
    }
}

#[test]
fn dynamic_reader_destroy_unregisters_from_subscriber() {
    unsafe {
        let mut factory = ptr::null_mut();
        assert_eq!(int2dds_domain_participant_factory_get_instance(&mut factory), INT2DDS_RET_OK);
        let mut participant = ptr::null_mut();
        assert_eq!(
            int2dds_create_participant(factory, 54, ptr::null(), &mut participant),
            INT2DDS_RET_OK
        );

        let mut registry = ptr::null_mut();
        assert_eq!(int2dds_xml_type_registry_create(&mut registry), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_xml_type_registry_load_str(registry, cstr(XML).as_ptr()),
            INT2DDS_RET_OK
        );

        let mut support = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_get_type_support(
                registry,
                cstr("Telemetry").as_ptr(),
                &mut support
            ),
            INT2DDS_RET_OK
        );

        let mut topic = ptr::null_mut();
        assert_eq!(
            int2dds_create_topic_dynamic(
                participant,
                cstr("TelemetryTopicR").as_ptr(),
                support,
                ptr::null(),
                &mut topic
            ),
            INT2DDS_RET_OK
        );

        let mut subscriber = ptr::null_mut();
        assert_eq!(
            int2dds_create_subscriber(participant, ptr::null(), &mut subscriber),
            INT2DDS_RET_OK
        );

        let mut reader = ptr::null_mut();
        assert_eq!(
            int2dds_create_datareader_dynamic(subscriber, topic, support, ptr::null(), &mut reader),
            INT2DDS_RET_OK
        );

        // Destroying the dynamic reader must fully unregister it; otherwise
        // the subscriber below still believes it owns a live reader.
        int2dds_dynamic_reader_destroy(reader);

        assert_eq!(
            int2dds_delete_subscriber(subscriber),
            INT2DDS_RET_OK,
            "subscriber delete failed after dynamic reader destroy \
             (reader was not unregistered from its parent)"
        );

        assert_eq!(int2dds_delete_topic(topic), INT2DDS_RET_OK);
        int2dds_dynamic_type_support_destroy(support);
        int2dds_xml_type_registry_destroy(registry);

        assert_eq!(
            int2dds_delete_participant(participant),
            INT2DDS_RET_OK,
            "participant delete failed after subscriber was deleted"
        );
    }
}
