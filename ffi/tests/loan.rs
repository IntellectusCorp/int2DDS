//! C-ABI loan scenario: lend, refuse a foreign return, block deletion, return, delete.

use std::ffi::CString;
use std::ptr;
use std::time::{Duration, Instant};

use int2dds_ffi::context::*;
use int2dds_ffi::error::*;
use int2dds_ffi::loan::*;
use int2dds_ffi::participant::*;
use int2dds_ffi::publisher::*;
use int2dds_ffi::read_condition::*;
use int2dds_ffi::subscriber::*;
use int2dds_ffi::topic::*;
use int2dds_ffi::type_info::*;
use int2dds_ffi::types::*;

const ANY: u32 = 0xFFFF;
/// CDR_LE encapsulation + `LoanSample { id: 1, value: 7 }`.
const SAMPLE: [u8; 12] = [0x00, 0x01, 0x00, 0x00, 1, 0, 0, 0, 7, 0, 0, 0];

/// One writer and `reader_count` readers on a topic keyed by `id`; `SAMPLE` written and cached.
unsafe fn setup(
    domain: i32,
    topic_name: &str,
    reader_count: usize,
) -> (*mut Int2DdsParticipant, Vec<*mut Int2DdsDataReader>) {
    let mut factory = ptr::null_mut();
    assert_eq!(int2dds_domain_participant_factory_get_instance(&mut factory), INT2DDS_RET_OK);
    let mut participant = ptr::null_mut();
    assert_eq!(
        int2dds_create_participant(factory, domain, ptr::null(), &mut participant),
        INT2DDS_RET_OK
    );
    let (name, type_name) =
        (CString::new(topic_name).unwrap(), CString::new("LoanSample").unwrap());
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
    let mut publisher = ptr::null_mut();
    assert_eq!(int2dds_create_publisher(participant, ptr::null(), &mut publisher), INT2DDS_RET_OK);
    let mut writer = ptr::null_mut();
    assert_eq!(
        int2dds_create_datawriter(publisher, topic, ptr::null(), ptr::null(), 0, &mut writer),
        INT2DDS_RET_OK
    );
    let mut subscriber = ptr::null_mut();
    assert_eq!(
        int2dds_create_subscriber(participant, ptr::null(), &mut subscriber),
        INT2DDS_RET_OK
    );
    let readers: Vec<_> = (0..reader_count)
        .map(|_| {
            let mut reader = ptr::null_mut();
            assert_eq!(
                int2dds_create_datareader(
                    subscriber,
                    topic,
                    ptr::null(),
                    ptr::null(),
                    0,
                    &mut reader
                ),
                INT2DDS_RET_OK
            );
            reader
        })
        .collect();

    // Rewrite until every reader has the sample: writes before matching are lost.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert_eq!(
            int2dds_datawriter_write_serialized(writer, SAMPLE.as_ptr(), SAMPLE.len()),
            INT2DDS_RET_OK
        );
        std::thread::sleep(Duration::from_millis(20));
        if readers.iter().all(|&r| has_sample(r)) {
            break;
        }
        assert!(Instant::now() < deadline, "sample not delivered");
    }
    (participant, readers)
}

unsafe fn has_sample(reader: *const Int2DdsDataReader) -> bool {
    let mut loan = ptr::null_mut();
    let found =
        int2dds_datareader_read_loaned(reader, 1, ANY, ANY, ANY, &mut loan) == INT2DDS_RET_OK;
    assert_eq!(int2dds_datareader_return_loan(reader, loan), INT2DDS_RET_OK);
    found
}

unsafe fn teardown(participant: *mut Int2DdsParticipant) {
    assert_eq!(int2dds_participant_delete_contained_entities(participant), INT2DDS_RET_OK);
    assert_eq!(int2dds_delete_participant(participant), INT2DDS_RET_OK);
}

#[test]
fn loan_scenario() {
    unsafe {
        let (participant, readers) = setup(161, "LoanScenario", 2);
        let (reader, other) = (readers[0], readers[1]);
        let mut rc = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_create_readcondition(reader, ANY, ANY, ANY, &mut rc),
            INT2DDS_RET_OK
        );

        // Condition loan lends the cached bytes.
        let mut loan = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_read_w_condition_loaned(reader, rc, 1, &mut loan),
            INT2DDS_RET_OK
        );
        let (mut a, mut b) = (ptr::null(), ptr::null());
        let mut size = 0usize;
        assert_eq!(int2dds_loaned_samples_get_data(loan, 0, &mut a, &mut size), INT2DDS_RET_OK);
        assert_eq!(int2dds_loaned_samples_get_data(loan, 0, &mut b, &mut size), INT2DDS_RET_OK);
        assert_eq!(a, b);
        assert_eq!(std::slice::from_raw_parts(a, size), &SAMPLE[..]);
        assert_eq!(
            int2dds_loaned_samples_get_data(loan, 1, &mut a, &mut size),
            INT2DDS_RET_INVALID_ARGUMENT
        );

        // Another reader refuses it and the loan stays usable; the owner cannot be deleted.
        assert_eq!(int2dds_datareader_return_loan(other, loan), INT2DDS_RET_PRECONDITION_NOT_MET);
        assert_eq!(int2dds_loaned_samples_length(loan), 1);
        assert_eq!(int2dds_readcondition_delete(rc), INT2DDS_RET_OK);
        assert_eq!(int2dds_delete_datareader(reader), INT2DDS_RET_PRECONDITION_NOT_MET);

        // Returned to its reader; a null loan is a no-op.
        let mut info: Int2DdsSampleInfo = std::mem::zeroed();
        assert_eq!(int2dds_loaned_samples_get_info(loan, 0, &mut info), INT2DDS_RET_OK);
        assert_eq!(int2dds_datareader_return_loan(reader, loan), INT2DDS_RET_OK);
        assert_eq!(int2dds_datareader_return_loan(reader, ptr::null_mut()), INT2DDS_RET_OK);

        // Instance variants lend the same sample: by its handle, or the first instance after NIL.
        let handle = info.instance_handle;
        let mut by_handle = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_read_instance_loaned(
                reader,
                &handle,
                -1,
                ANY,
                ANY,
                ANY,
                &mut by_handle
            ),
            INT2DDS_RET_OK
        );
        let mut next = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_read_next_instance_loaned(
                reader,
                ptr::null(),
                -1,
                ANY,
                ANY,
                ANY,
                &mut next
            ),
            INT2DDS_RET_OK
        );
        let mut rc = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_create_readcondition(reader, ANY, ANY, ANY, &mut rc),
            INT2DDS_RET_OK
        );
        let mut next_cond = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_read_next_instance_w_condition_loaned(
                reader,
                ptr::null(),
                rc,
                -1,
                &mut next_cond
            ),
            INT2DDS_RET_OK
        );
        for l in [by_handle, next, next_cond] {
            assert_eq!(int2dds_loaned_samples_length(l), 1);
            assert_eq!(int2dds_loaned_samples_get_data(l, 0, &mut a, &mut size), INT2DDS_RET_OK);
            assert_eq!(std::slice::from_raw_parts(a, size), &SAMPLE[..]);
            assert_eq!(int2dds_datareader_return_loan(reader, l), INT2DDS_RET_OK);
        }
        assert_eq!(int2dds_readcondition_delete(rc), INT2DDS_RET_OK);
        let mut nil = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_read_instance_loaned(
                reader,
                ptr::null(),
                -1,
                ANY,
                ANY,
                ANY,
                &mut nil
            ),
            INT2DDS_RET_INVALID_ARGUMENT
        );
        assert!(nil.is_null());

        // Take empties the cache; then nothing is loaned and the reader can be deleted.
        let mut taken = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_take_loaned(reader, -1, ANY, ANY, ANY, &mut taken),
            INT2DDS_RET_OK
        );
        assert_eq!(int2dds_datareader_return_loan(reader, taken), INT2DDS_RET_OK);
        let mut empty = ptr::null_mut();
        assert_eq!(
            int2dds_datareader_read_loaned(reader, -1, ANY, ANY, ANY, &mut empty),
            INT2DDS_RET_NO_DATA
        );
        assert!(empty.is_null());
        assert_eq!(int2dds_delete_datareader(reader), INT2DDS_RET_OK);
        teardown(participant);
    }
}
