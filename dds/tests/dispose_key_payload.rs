//! Regression coverage for dispose serializedKey decoding.
//!
//! A dispose/unregister sample carries the instance key as a K-flag
//! SerializedPayload (CDR with a 4-byte encapsulation header), not as the
//! headerless KeyHash-format bytes used for instance-handle computation.
//! `deserialize_key_payload` must read the encapsulation header (so endianness
//! comes from the wire) instead of assuming headerless big-endian.

use std::any::Any;

use int2dds::topic::type_support::{DdsType, TypeSupport};

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct KeyedShape {
    #[dds(key)]
    color: String,
    x: i32,
}

#[test]
fn deserialize_key_payload_roundtrip_big_endian() {
    let ts = KeyedShape::get_type_support();
    let shape = KeyedShape { color: "BLUE".to_string(), x: 7 };

    // serialize_key is headerless big-endian; wrap it as a wire serializedKey.
    let key = ts.serialize_key(&shape as &dyn Any).unwrap();
    let mut payload = vec![0x00, 0x00, 0x00, 0x00]; // CDR_BE encapsulation header
    payload.extend_from_slice(&key);

    let decoded = ts.deserialize_key_payload(&payload).unwrap();
    let decoded = decoded.downcast_ref::<KeyedShape>().unwrap();
    assert_eq!(decoded.color, "BLUE");
}

#[test]
fn deserialize_key_payload_little_endian_wire() {
    // Exact bytes of a Cyclone/OpenDDS dispose serializedKey for color "BLUE":
    // CDR_LE header, little-endian string length 5, then "BLUE\0".
    let ts = KeyedShape::get_type_support();
    let payload: &[u8] = &[
        0x00, 0x01, 0x00, 0x03, // CDR_LE encapsulation header
        0x05, 0x00, 0x00, 0x00, // string length 5 (little-endian)
        0x42, 0x4c, 0x55, 0x45, 0x00, // "BLUE\0"
    ];

    let decoded = ts.deserialize_key_payload(payload).unwrap();
    let decoded = decoded.downcast_ref::<KeyedShape>().unwrap();
    assert_eq!(decoded.color, "BLUE");
}
