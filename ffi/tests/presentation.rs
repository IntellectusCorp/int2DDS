// C-ABI GROUP presentation: coherent changes, access blocks, ordered get_datareaders.

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

const ANY: u32 = 0;

// CDR_LE encapsulation + `{ id, value }` as two little-endian int32 fields.
fn sample(id: u8, value: u8) -> [u8; 12] {
    [0x00, 0x01, 0x00, 0x00, id, 0, 0, 0, value, 0, 0, 0]
}

unsafe fn create_keyed_topic(
    participant: *mut Int2DdsParticipant,
    topic_name: &str,
) -> *mut Int2DdsTopic {
    let (name, type_name) =
        (CString::new(topic_name).unwrap(), CString::new("PresentationSample").unwrap());
    let (id, value) = (CString::new("id").unwrap(), CString::new("value").unwrap());

    let mut type_info = ptr::null_mut();
    assert_eq!(int2dds_type_info_create(type_name.as_ptr(), 0, &mut type_info), INT2DDS_RET_OK);
    assert_eq!(
        int2dds_type_info_add_field(
            type_info,
            id.as_ptr(),
            INT2DDS_FIELD_INT32,
            INT2DDS_MEMBER_KEY
        ),
        INT2DDS_RET_OK
    );
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
            ptr::null(),
            &mut topic
        ),
        INT2DDS_RET_OK
    );
    int2dds_type_info_destroy(type_info);

    topic
}

unsafe fn wait_for_writer_match(writer: *const Int2DdsDataWriter) {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        let mut count = 0usize;
        assert_eq!(
            int2dds_datawriter_get_matched_subscriptions(writer, ptr::null_mut(), 0, &mut count),
            INT2DDS_RET_OK
        );

        if count > 0 {
            return;
        }

        assert!(Instant::now() < deadline, "writer not matched");
        std::thread::sleep(Duration::from_millis(20));
    }
}

unsafe fn get_datareaders(subscriber: *const Int2DdsSubscriber) -> Vec<[u8; 16]> {
    let mut count = 0usize;
    assert_eq!(
        int2dds_subscriber_get_datareaders(
            subscriber,
            ANY,
            ANY,
            ANY,
            ptr::null_mut(),
            0,
            &mut count
        ),
        INT2DDS_RET_OK
    );

    let mut handles = vec![[0u8; 16]; count];
    let mut written_count = 0usize;
    assert_eq!(
        int2dds_subscriber_get_datareaders(
            subscriber,
            ANY,
            ANY,
            ANY,
            handles.as_mut_ptr(),
            handles.len(),
            &mut written_count
        ),
        INT2DDS_RET_OK
    );

    handles.truncate(written_count);
    handles
}

unsafe fn get_datareaders_in_access_block(subscriber: *const Int2DdsSubscriber) -> Vec<[u8; 16]> {
    assert_eq!(int2dds_subscriber_begin_access(subscriber), INT2DDS_RET_OK);

    let handles = get_datareaders(subscriber);

    assert_eq!(int2dds_subscriber_end_access(subscriber), INT2DDS_RET_OK);

    handles
}

// Takes one sample and returns its `value` field, or the return code on failure.
unsafe fn take_value(reader: *const Int2DdsDataReader) -> Result<u8, Int2DdsRet> {
    let mut buffer = [0u8; 64];
    let mut size = 0usize;
    let mut is_valid_data = false;

    let ret = int2dds_datareader_take_serialized(
        reader,
        buffer.as_mut_ptr(),
        buffer.len(),
        &mut size,
        &mut is_valid_data,
    );

    if ret != INT2DDS_RET_OK {
        return Err(ret);
    }

    assert!(is_valid_data);
    Ok(buffer[8])
}

#[test]
fn group_coherent_set_is_listed_in_write_order() {
    unsafe {
        let mut factory = ptr::null_mut();
        assert_eq!(int2dds_domain_participant_factory_get_instance(&mut factory), INT2DDS_RET_OK);

        let mut participant = ptr::null_mut();
        assert_eq!(
            int2dds_create_participant(factory, 162, ptr::null(), &mut participant),
            INT2DDS_RET_OK
        );

        let topic_a = create_keyed_topic(participant, "PresentationA");
        let topic_b = create_keyed_topic(participant, "PresentationB");

        let mut publisher_qos = ptr::null_mut();
        assert_eq!(int2dds_publisher_qos_create_default(&mut publisher_qos), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_publisher_qos_set_presentation(
                publisher_qos,
                INT2DDS_QOS_PRESENTATION_GROUP,
                true,
                true
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
            int2dds_subscriber_qos_set_presentation(
                subscriber_qos,
                INT2DDS_QOS_PRESENTATION_GROUP,
                true,
                true
            ),
            INT2DDS_RET_OK
        );
        let mut subscriber = ptr::null_mut();
        assert_eq!(
            int2dds_create_subscriber(participant, subscriber_qos, &mut subscriber),
            INT2DDS_RET_OK
        );
        assert_eq!(int2dds_subscriber_qos_destroy(subscriber_qos), INT2DDS_RET_OK);

        let mut writer_qos = ptr::null_mut();
        assert_eq!(int2dds_datawriter_qos_create_default(&mut writer_qos), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_datawriter_qos_set_reliability(
                writer_qos,
                INT2DDS_QOS_RELIABILITY_RELIABLE,
                100_000_000
            ),
            INT2DDS_RET_OK
        );
        let mut reader_qos = ptr::null_mut();
        assert_eq!(int2dds_datareader_qos_create_default(&mut reader_qos), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_datareader_qos_set_reliability(
                reader_qos,
                INT2DDS_QOS_RELIABILITY_RELIABLE,
                100_000_000
            ),
            INT2DDS_RET_OK
        );

        let (mut writer_a, mut writer_b) = (ptr::null_mut(), ptr::null_mut());
        assert_eq!(
            int2dds_create_datawriter(
                publisher,
                topic_a,
                writer_qos,
                ptr::null(),
                0,
                &mut writer_a
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(
            int2dds_create_datawriter(
                publisher,
                topic_b,
                writer_qos,
                ptr::null(),
                0,
                &mut writer_b
            ),
            INT2DDS_RET_OK
        );

        let (mut reader_a, mut reader_b) = (ptr::null_mut(), ptr::null_mut());
        assert_eq!(
            int2dds_create_datareader(
                subscriber,
                topic_a,
                reader_qos,
                ptr::null(),
                0,
                &mut reader_a
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(
            int2dds_create_datareader(
                subscriber,
                topic_b,
                reader_qos,
                ptr::null(),
                0,
                &mut reader_b
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(int2dds_datawriter_qos_destroy(writer_qos), INT2DDS_RET_OK);
        assert_eq!(int2dds_datareader_qos_destroy(reader_qos), INT2DDS_RET_OK);

        wait_for_writer_match(writer_a);
        wait_for_writer_match(writer_b);

        // Unbalanced end calls are rejected.
        assert_eq!(
            int2dds_publisher_end_coherent_changes(publisher),
            INT2DDS_RET_PRECONDITION_NOT_MET
        );
        assert_eq!(int2dds_subscriber_end_access(subscriber), INT2DDS_RET_PRECONDITION_NOT_MET);

        // GROUP access scope requires an access block for get_datareaders.
        let mut count = 0usize;
        assert_eq!(
            int2dds_subscriber_get_datareaders(
                subscriber,
                ANY,
                ANY,
                ANY,
                ptr::null_mut(),
                0,
                &mut count
            ),
            INT2DDS_RET_PRECONDITION_NOT_MET
        );

        assert_eq!(int2dds_publisher_begin_coherent_changes(publisher), INT2DDS_RET_OK);
        for (writer, data) in
            [(writer_a, sample(1, 10)), (writer_b, sample(1, 20)), (writer_a, sample(2, 30))]
        {
            assert_eq!(
                int2dds_datawriter_write_serialized(writer, data.as_ptr(), data.len()),
                INT2DDS_RET_OK
            );
        }
        assert_eq!(int2dds_publisher_end_coherent_changes(publisher), INT2DDS_RET_OK);

        let mut guid_a = [0u8; 16];
        let mut guid_b = [0u8; 16];
        assert_eq!(int2dds_datareader_get_guid(reader_a, &mut guid_a), INT2DDS_RET_OK);
        assert_eq!(int2dds_datareader_get_guid(reader_b, &mut guid_b), INT2DDS_RET_OK);

        let deadline = Instant::now() + Duration::from_secs(10);
        let handles = loop {
            let handles = get_datareaders_in_access_block(subscriber);

            if handles.len() == 3 {
                break handles;
            }

            assert!(Instant::now() < deadline, "coherent set not delivered: {handles:?}");
            std::thread::sleep(Duration::from_millis(20));
        };

        assert_eq!(handles, vec![guid_a, guid_b, guid_a]);

        // GROUP access scope rejects a take outside an access block.
        assert_eq!(take_value(reader_a), Err(INT2DDS_RET_PRECONDITION_NOT_MET));

        // Taking one sample per listed reader, in list order, yields the write order.
        assert_eq!(int2dds_subscriber_begin_access(subscriber), INT2DDS_RET_OK);

        let values: Vec<u8> = get_datareaders(subscriber)
            .iter()
            .map(|handle| {
                let reader = if *handle == guid_a { reader_a } else { reader_b };
                take_value(reader).unwrap()
            })
            .collect();

        assert_eq!(int2dds_subscriber_end_access(subscriber), INT2DDS_RET_OK);

        assert_eq!(values, vec![10, 20, 30]);

        // The matched publication carries the publisher's presentation QoS.
        let mut publication_handle = [0u8; 16];
        let mut publication_count = 0usize;
        assert_eq!(
            int2dds_datareader_get_matched_publications(
                reader_a,
                &mut publication_handle,
                1,
                &mut publication_count
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(publication_count, 1);

        let mut publication_data = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_get_matched_publication_data(
                reader_a,
                &publication_handle,
                &mut publication_data
            ),
            INT2DDS_RET_OK
        );

        let (mut access_scope, mut coherent_access, mut ordered_access) = (-1, false, false);
        assert_eq!(
            int2dds_publication_builtin_topic_data_get_presentation(
                publication_data,
                &mut access_scope,
                &mut coherent_access,
                &mut ordered_access
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(access_scope, INT2DDS_QOS_PRESENTATION_GROUP);
        assert!(coherent_access);
        assert!(ordered_access);
        assert_eq!(
            int2dds_publication_builtin_topic_data_destroy(publication_data),
            INT2DDS_RET_OK
        );

        assert_eq!(int2dds_participant_delete_contained_entities(participant), INT2DDS_RET_OK);
        assert_eq!(int2dds_delete_participant(participant), INT2DDS_RET_OK);
    }
}
