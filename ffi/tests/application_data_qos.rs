// C-ABI application data QoS: values set on local entities appear in the matched endpoints' discovery data.

use std::ffi::CString;
use std::ptr;
use std::time::{Duration, Instant};

use int2dds_ffi::context::*;
use int2dds_ffi::discovery::*;
use int2dds_ffi::error::*;
use int2dds_ffi::participant::*;
use int2dds_ffi::publisher::*;
use int2dds_ffi::qos::*;
use int2dds_ffi::subscriber::*;
use int2dds_ffi::topic::*;
use int2dds_ffi::type_info::*;
use int2dds_ffi::types::*;

const TOPIC_DATA: &[u8] = b"topic-data";
const PUBLISHER_GROUP_DATA: &[u8] = b"publisher-group-data";
const SUBSCRIBER_GROUP_DATA: &[u8] = b"subscriber-group-data";

unsafe fn create_topic(
    participant: *mut Int2DdsParticipant,
    topic_qos: *const Int2DdsTopicQos,
) -> *mut Int2DdsTopic {
    let (name, type_name) =
        (CString::new("ApplicationDataQos").unwrap(), CString::new("ApplicationData").unwrap());
    let value = CString::new("value").unwrap();

    let mut type_info = ptr::null_mut();
    assert_eq!(int2dds_type_info_create(type_name.as_ptr(), 0, &mut type_info), INT2DDS_RET_OK);
    assert_eq!(
        int2dds_type_info_add_field(type_info, value.as_ptr(), INT2DDS_FIELD_INT32, 0),
        INT2DDS_RET_OK
    );

    let mut topic = ptr::null_mut();
    assert_eq!(
        int2dds_create_topic_with_type_info(
            participant,
            name.as_ptr(),
            type_info,
            topic_qos,
            &mut topic
        ),
        INT2DDS_RET_OK
    );
    int2dds_type_info_destroy(type_info);

    topic
}

unsafe fn wait_for_matched_publication(reader: *const Int2DdsDataReader) -> [u8; 16] {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        let mut handle = [0u8; 16];
        let mut count = 0usize;
        assert_eq!(
            int2dds_datareader_get_matched_publications(reader, &mut handle, 1, &mut count),
            INT2DDS_RET_OK
        );

        if count > 0 {
            return handle;
        }

        assert!(Instant::now() < deadline, "reader not matched");
        std::thread::sleep(Duration::from_millis(20));
    }
}

unsafe fn wait_for_matched_subscription(writer: *const Int2DdsDataWriter) -> [u8; 16] {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        let mut handle = [0u8; 16];
        let mut count = 0usize;
        assert_eq!(
            int2dds_datawriter_get_matched_subscriptions(writer, &mut handle, 1, &mut count),
            INT2DDS_RET_OK
        );

        if count > 0 {
            return handle;
        }

        assert!(Instant::now() < deadline, "writer not matched");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn application_data_qos_reaches_matched_endpoints() {
    unsafe {
        let mut factory = ptr::null_mut();
        assert_eq!(int2dds_domain_participant_factory_get_instance(&mut factory), INT2DDS_RET_OK);

        let mut participant = ptr::null_mut();
        assert_eq!(
            int2dds_create_participant(factory, 163, ptr::null(), &mut participant),
            INT2DDS_RET_OK
        );

        let mut topic_qos = ptr::null_mut();
        assert_eq!(int2dds_topic_qos_create_default(&mut topic_qos), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_topic_qos_set_topic_data(topic_qos, TOPIC_DATA.as_ptr(), TOPIC_DATA.len()),
            INT2DDS_RET_OK
        );
        let topic = create_topic(participant, topic_qos);
        assert_eq!(int2dds_topic_qos_destroy(topic_qos), INT2DDS_RET_OK);

        let mut publisher_qos = ptr::null_mut();
        assert_eq!(int2dds_publisher_qos_create_default(&mut publisher_qos), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_publisher_qos_set_group_data(
                publisher_qos,
                PUBLISHER_GROUP_DATA.as_ptr(),
                PUBLISHER_GROUP_DATA.len()
            ),
            INT2DDS_RET_OK
        );
        let mut publisher = ptr::null_mut();
        assert_eq!(
            int2dds_create_publisher(participant, publisher_qos, &mut publisher),
            INT2DDS_RET_OK
        );
        assert_eq!(int2dds_publisher_qos_destroy(publisher_qos), INT2DDS_RET_OK);

        let mut subscriber_qos = ptr::null_mut();
        assert_eq!(int2dds_subscriber_qos_create_default(&mut subscriber_qos), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_subscriber_qos_set_group_data(
                subscriber_qos,
                SUBSCRIBER_GROUP_DATA.as_ptr(),
                SUBSCRIBER_GROUP_DATA.len()
            ),
            INT2DDS_RET_OK
        );
        let mut subscriber = ptr::null_mut();
        assert_eq!(
            int2dds_create_subscriber(participant, subscriber_qos, &mut subscriber),
            INT2DDS_RET_OK
        );
        assert_eq!(int2dds_subscriber_qos_destroy(subscriber_qos), INT2DDS_RET_OK);

        let mut writer = ptr::null_mut();
        assert_eq!(
            int2dds_create_datawriter(publisher, topic, ptr::null(), ptr::null(), 0, &mut writer),
            INT2DDS_RET_OK
        );
        let mut reader = ptr::null_mut();
        assert_eq!(
            int2dds_create_datareader(subscriber, topic, ptr::null(), ptr::null(), 0, &mut reader),
            INT2DDS_RET_OK
        );

        let publication_handle = wait_for_matched_publication(reader);
        let subscription_handle = wait_for_matched_subscription(writer);

        let mut publication_data = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_get_matched_publication_data(
                reader,
                &publication_handle,
                &mut publication_data
            ),
            INT2DDS_RET_OK
        );
        let mut subscription_data = ptr::null_mut();
        assert_eq!(
            int2dds_datawriter_get_matched_subscription_data(
                writer,
                &subscription_handle,
                &mut subscription_data
            ),
            INT2DDS_RET_OK
        );

        let mut buf = [0u8; 32];
        let mut size = 0usize;
        assert_eq!(
            int2dds_publication_builtin_topic_data_get_topic_data(
                publication_data,
                buf.as_mut_ptr(),
                buf.len(),
                &mut size
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(&buf[..size], TOPIC_DATA);

        assert_eq!(
            int2dds_subscription_builtin_topic_data_get_topic_data(
                subscription_data,
                buf.as_mut_ptr(),
                buf.len(),
                &mut size
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(&buf[..size], TOPIC_DATA);

        assert_eq!(
            int2dds_publication_builtin_topic_data_get_group_data(
                publication_data,
                buf.as_mut_ptr(),
                buf.len(),
                &mut size
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(&buf[..size], PUBLISHER_GROUP_DATA);

        assert_eq!(
            int2dds_subscription_builtin_topic_data_get_group_data(
                subscription_data,
                buf.as_mut_ptr(),
                buf.len(),
                &mut size
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(&buf[..size], SUBSCRIBER_GROUP_DATA);

        assert_eq!(
            int2dds_publication_builtin_topic_data_destroy(publication_data),
            INT2DDS_RET_OK
        );
        assert_eq!(
            int2dds_subscription_builtin_topic_data_destroy(subscription_data),
            INT2DDS_RET_OK
        );

        assert_eq!(int2dds_participant_delete_contained_entities(participant), INT2DDS_RET_OK);
        assert_eq!(int2dds_delete_participant(participant), INT2DDS_RET_OK);
    }
}
