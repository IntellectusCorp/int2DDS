//! FFI verification for the full DynamicValue surface: nested struct, enum,
//! bitmask, bitset, sequence-of-struct, primitive sequence, map, union and wide
//! string — built and read back entirely through the C-ABI value API.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::thread::sleep;
use std::time::Duration;

use int2dds_ffi::context::*;
use int2dds_ffi::dynamic::*;
use int2dds_ffi::dynamic_value::*;
use int2dds_ffi::error::*;
use int2dds_ffi::participant::*;
use int2dds_ffi::publisher::*;
use int2dds_ffi::subscriber::*;
use int2dds_ffi::xml::*;

const XML: &str = r#"
<types>
  <module name="t">
    <enum name="Mode">
      <enumerator name="OFF" value="0"/>
      <enumerator name="ON" value="1"/>
    </enum>
    <bitmask name="Caps" bit_bound="8">
      <bit_value name="A" position="0"/>
      <bit_value name="B" position="1"/>
    </bitmask>
    <bitset name="Health">
      <bitfield name="battery" bit_bound="7" type="uint8"/>
      <bitfield name="flags" bit_bound="2" type="uint8"/>
    </bitset>
    <struct name="Vec2">
      <member name="x" type="float32"/>
      <member name="y" type="float32"/>
    </struct>
    <union name="Cmd">
      <discriminator type="int32"/>
      <case><caseDiscriminator value="0"/><member name="speed" type="float32"/></case>
      <case><caseDiscriminator value="1"/><member name="stop" type="boolean"/></case>
    </union>
    <struct name="All" extensibility="mutable">
      <member name="id" type="uint32" key="true"/>
      <member name="mode" type="nonBasic" nonBasicTypeName="t::Mode"/>
      <member name="caps" type="nonBasic" nonBasicTypeName="t::Caps"/>
      <member name="health" type="nonBasic" nonBasicTypeName="t::Health"/>
      <member name="home" type="nonBasic" nonBasicTypeName="t::Vec2"/>
      <member name="points" type="nonBasic" nonBasicTypeName="t::Vec2" sequenceMaxLength="-1"/>
      <member name="samples" type="int32" sequenceMaxLength="-1"/>
      <member name="cmd" type="nonBasic" nonBasicTypeName="t::Cmd"/>
      <member name="label" type="wstring"/>
      <member name="scores" type="int32" key_type="string" mapMaxLength="8"/>
    </struct>
  </module>
</types>
"#;

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

// Each helper returns an owned value handle (asserting the constructor succeeded).
unsafe fn v_i32(value: i32) -> *mut Int2DdsDynamicValue {
    let mut out = ptr::null_mut();
    assert_eq!(int2dds_dynamic_value_i32(value, &mut out), INT2DDS_RET_OK);
    out
}
unsafe fn v_f32(value: f32) -> *mut Int2DdsDynamicValue {
    let mut out = ptr::null_mut();
    assert_eq!(int2dds_dynamic_value_f32(value, &mut out), INT2DDS_RET_OK);
    out
}
unsafe fn v_string(value: &str) -> *mut Int2DdsDynamicValue {
    let mut out = ptr::null_mut();
    assert_eq!(int2dds_dynamic_value_string(cstr(value).as_ptr(), &mut out), INT2DDS_RET_OK);
    out
}
unsafe fn set_value(data: *mut Int2DdsDynamicData, field: &str, value: *mut Int2DdsDynamicValue) {
    assert_eq!(int2dds_dynamic_data_set_value(data, cstr(field).as_ptr(), value), INT2DDS_RET_OK);
}
unsafe fn get_value(data: *const Int2DdsDynamicData, path: &str) -> *mut Int2DdsDynamicValue {
    let mut out = ptr::null_mut();
    assert_eq!(int2dds_dynamic_data_get_value(data, cstr(path).as_ptr(), &mut out), INT2DDS_RET_OK);
    out
}
// Build a Vec2 struct value from (x, y), consuming nothing the caller must track.
unsafe fn vec2_value(
    support: *const Int2DdsDynamicTypeSupport,
    x: f32,
    y: f32,
) -> *mut Int2DdsDynamicValue {
    let mut data = ptr::null_mut();
    assert_eq!(int2dds_dynamic_data_create(support, &mut data), INT2DDS_RET_OK);
    assert_eq!(int2dds_dynamic_data_set_f32(data, cstr("x").as_ptr(), x), INT2DDS_RET_OK);
    assert_eq!(int2dds_dynamic_data_set_f32(data, cstr("y").as_ptr(), y), INT2DDS_RET_OK);
    let mut value = ptr::null_mut();
    assert_eq!(int2dds_dynamic_value_struct(data, &mut value), INT2DDS_RET_OK);
    int2dds_dynamic_data_destroy(data);
    value
}

#[test]
fn xml_dynamic_complex_round_trip() {
    unsafe {
        let mut factory = ptr::null_mut();
        assert_eq!(int2dds_domain_participant_factory_get_instance(&mut factory), INT2DDS_RET_OK);
        let mut participant = ptr::null_mut();
        assert_eq!(
            int2dds_create_participant(factory, ptr::null(), 52, &mut participant),
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
                cstr("t::All").as_ptr(),
                &mut support
            ),
            INT2DDS_RET_OK
        );
        let mut vec2_support = ptr::null_mut();
        assert_eq!(
            int2dds_xml_type_registry_get_type_support(
                registry,
                cstr("t::Vec2").as_ptr(),
                &mut vec2_support
            ),
            INT2DDS_RET_OK
        );

        let mut topic = ptr::null_mut();
        assert_eq!(
            int2dds_create_topic_dynamic(
                participant,
                cstr("AllTopic").as_ptr(),
                support,
                ptr::null(),
                &mut topic
            ),
            INT2DDS_RET_OK
        );
        let mut publisher = ptr::null_mut();
        assert_eq!(int2dds_create_publisher(participant, &mut publisher), INT2DDS_RET_OK);
        let mut subscriber = ptr::null_mut();
        assert_eq!(int2dds_create_subscriber(participant, &mut subscriber), INT2DDS_RET_OK);
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

        // ---- Build the sample exercising every kind ----
        let mut data = ptr::null_mut();
        assert_eq!(int2dds_dynamic_data_create(support, &mut data), INT2DDS_RET_OK);

        assert_eq!(int2dds_dynamic_data_set_u32(data, cstr("id").as_ptr(), 7), INT2DDS_RET_OK);

        // enum
        let mut mode = ptr::null_mut();
        assert_eq!(int2dds_dynamic_value_enum(cstr("ON").as_ptr(), 1, &mut mode), INT2DDS_RET_OK);
        set_value(data, "mode", mode);

        // bitmask / bitset
        let mut caps = ptr::null_mut();
        assert_eq!(int2dds_dynamic_value_bitmask(0b11, &mut caps), INT2DDS_RET_OK);
        set_value(data, "caps", caps);
        let mut health = ptr::null_mut();
        assert_eq!(int2dds_dynamic_value_bitset((2u64 << 7) | 80, &mut health), INT2DDS_RET_OK);
        set_value(data, "health", health);

        // nested struct
        set_value(data, "home", vec2_value(vec2_support, 1.5, 2.5));

        // sequence of struct
        let mut points = ptr::null_mut();
        assert_eq!(int2dds_dynamic_value_sequence(&mut points), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_dynamic_value_push(points, vec2_value(vec2_support, -3.0, 4.0)),
            INT2DDS_RET_OK
        );
        assert_eq!(
            int2dds_dynamic_value_push(points, vec2_value(vec2_support, 5.0, -6.0)),
            INT2DDS_RET_OK
        );
        set_value(data, "points", points);

        // primitive sequence
        let mut samples = ptr::null_mut();
        assert_eq!(int2dds_dynamic_value_sequence(&mut samples), INT2DDS_RET_OK);
        assert_eq!(int2dds_dynamic_value_push(samples, v_i32(100)), INT2DDS_RET_OK);
        assert_eq!(int2dds_dynamic_value_push(samples, v_i32(-200)), INT2DDS_RET_OK);
        set_value(data, "samples", samples);

        // union (discriminator 0 -> speed)
        let mut cmd = ptr::null_mut();
        assert_eq!(int2dds_dynamic_value_union(v_i32(0), v_f32(2.5), &mut cmd), INT2DDS_RET_OK);
        set_value(data, "cmd", cmd);

        // wide string
        let mut label = ptr::null_mut();
        assert_eq!(
            int2dds_dynamic_value_wstring(cstr("로봇").as_ptr(), &mut label),
            INT2DDS_RET_OK
        );
        set_value(data, "label", label);

        // map<string,int32>
        let mut scores = ptr::null_mut();
        assert_eq!(int2dds_dynamic_value_map(&mut scores), INT2DDS_RET_OK);
        assert_eq!(
            int2dds_dynamic_value_map_insert(scores, v_string("alpha"), v_i32(10)),
            INT2DDS_RET_OK
        );
        assert_eq!(
            int2dds_dynamic_value_map_insert(scores, v_string("beta"), v_i32(20)),
            INT2DDS_RET_OK
        );
        set_value(data, "scores", scores);

        assert_eq!(int2dds_dynamic_writer_write(writer, data), INT2DDS_RET_OK);

        // ---- Take and verify ----
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

        // id
        let mut id = 0u32;
        assert_eq!(
            int2dds_dynamic_data_get_u32(received, cstr("id").as_ptr(), &mut id),
            INT2DDS_RET_OK
        );
        assert_eq!(id, 7);

        // enum (name restored on deserialize)
        let mode_v = get_value(received, "mode");
        let mut buf = [0 as c_char; 32];
        let mut out_len = 0usize;
        let mut mode_num = 0i32;
        assert_eq!(
            int2dds_dynamic_value_as_enum(
                mode_v,
                buf.as_mut_ptr(),
                buf.len(),
                &mut out_len,
                &mut mode_num
            ),
            INT2DDS_RET_OK
        );
        assert_eq!(mode_num, 1);
        assert_eq!(CStr::from_ptr(buf.as_ptr()).to_str().unwrap(), "ON");
        int2dds_dynamic_value_destroy(mode_v);

        // bitmask / bitset
        let caps_v = get_value(received, "caps");
        let mut caps_bits = 0u64;
        assert_eq!(int2dds_dynamic_value_as_bitmask(caps_v, &mut caps_bits), INT2DDS_RET_OK);
        assert_eq!(caps_bits, 0b11);
        int2dds_dynamic_value_destroy(caps_v);

        let health_v = get_value(received, "health");
        let mut health_bits = 0u64;
        assert_eq!(int2dds_dynamic_value_as_bitset(health_v, &mut health_bits), INT2DDS_RET_OK);
        assert_eq!(health_bits, (2u64 << 7) | 80);
        int2dds_dynamic_value_destroy(health_v);

        // nested struct via path getter
        let mut home_x = 0f32;
        assert_eq!(
            int2dds_dynamic_data_get_f32(received, cstr("home.x").as_ptr(), &mut home_x),
            INT2DDS_RET_OK
        );
        assert_eq!(home_x, 1.5);
        let mut home_y = 0f32;
        assert_eq!(
            int2dds_dynamic_data_get_f32(received, cstr("home.y").as_ptr(), &mut home_y),
            INT2DDS_RET_OK
        );
        assert_eq!(home_y, 2.5);

        // sequence of struct: length + element field
        let points_v = get_value(received, "points");
        let mut points_len = 0usize;
        assert_eq!(int2dds_dynamic_value_len(points_v, &mut points_len), INT2DDS_RET_OK);
        assert_eq!(points_len, 2);
        int2dds_dynamic_value_destroy(points_v);
        let mut p1x = 0f32;
        assert_eq!(
            int2dds_dynamic_data_get_f32(received, cstr("points[1].x").as_ptr(), &mut p1x),
            INT2DDS_RET_OK
        );
        assert_eq!(p1x, 5.0);

        // primitive sequence
        let mut s0 = 0i32;
        assert_eq!(
            int2dds_dynamic_data_get_i32(received, cstr("samples[0]").as_ptr(), &mut s0),
            INT2DDS_RET_OK
        );
        assert_eq!(s0, 100);
        let mut s1 = 0i32;
        assert_eq!(
            int2dds_dynamic_data_get_i32(received, cstr("samples[1]").as_ptr(), &mut s1),
            INT2DDS_RET_OK
        );
        assert_eq!(s1, -200);

        // union
        let cmd_v = get_value(received, "cmd");
        let mut disc = ptr::null_mut();
        assert_eq!(int2dds_dynamic_value_union_discriminator(cmd_v, &mut disc), INT2DDS_RET_OK);
        let mut disc_num = 0i32;
        assert_eq!(int2dds_dynamic_value_as_i32(disc, &mut disc_num), INT2DDS_RET_OK);
        assert_eq!(disc_num, 0);
        int2dds_dynamic_value_destroy(disc);
        let mut branch = ptr::null_mut();
        assert_eq!(int2dds_dynamic_value_union_value(cmd_v, &mut branch), INT2DDS_RET_OK);
        let mut speed = 0f32;
        assert_eq!(int2dds_dynamic_value_as_f32(branch, &mut speed), INT2DDS_RET_OK);
        assert_eq!(speed, 2.5);
        int2dds_dynamic_value_destroy(branch);
        int2dds_dynamic_value_destroy(cmd_v);

        // wide string
        let label_v = get_value(received, "label");
        let mut lbuf = [0 as c_char; 64];
        let mut llen = 0usize;
        assert_eq!(
            int2dds_dynamic_value_as_string(label_v, lbuf.as_mut_ptr(), lbuf.len(), &mut llen),
            INT2DDS_RET_OK
        );
        assert_eq!(CStr::from_ptr(lbuf.as_ptr()).to_str().unwrap(), "로봇");
        int2dds_dynamic_value_destroy(label_v);

        // map
        let scores_v = get_value(received, "scores");
        let mut scores_len = 0usize;
        assert_eq!(int2dds_dynamic_value_len(scores_v, &mut scores_len), INT2DDS_RET_OK);
        assert_eq!(scores_len, 2);
        let key0 = {
            let mut k = ptr::null_mut();
            assert_eq!(int2dds_dynamic_value_map_key(scores_v, 0, &mut k), INT2DDS_RET_OK);
            k
        };
        let mut kbuf = [0 as c_char; 32];
        let mut klen = 0usize;
        assert_eq!(
            int2dds_dynamic_value_as_string(key0, kbuf.as_mut_ptr(), kbuf.len(), &mut klen),
            INT2DDS_RET_OK
        );
        assert_eq!(CStr::from_ptr(kbuf.as_ptr()).to_str().unwrap(), "alpha");
        int2dds_dynamic_value_destroy(key0);
        let val0 = {
            let mut v = ptr::null_mut();
            assert_eq!(int2dds_dynamic_value_map_value(scores_v, 0, &mut v), INT2DDS_RET_OK);
            v
        };
        let mut val0_num = 0i32;
        assert_eq!(int2dds_dynamic_value_as_i32(val0, &mut val0_num), INT2DDS_RET_OK);
        assert_eq!(val0_num, 10);
        int2dds_dynamic_value_destroy(val0);
        int2dds_dynamic_value_destroy(scores_v);

        // ---- Cleanup ----
        int2dds_dynamic_data_destroy(received);
        int2dds_dynamic_data_destroy(data);
        int2dds_dynamic_reader_destroy(reader);
        int2dds_dynamic_writer_destroy(writer);
        int2dds_dynamic_type_support_destroy(vec2_support);
        int2dds_dynamic_type_support_destroy(support);
        int2dds_xml_type_registry_destroy(registry);
        int2dds_delete_participant(participant);
    }
}
