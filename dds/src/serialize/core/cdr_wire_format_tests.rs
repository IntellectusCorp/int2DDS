//! Cross-language CDR wire-format reference vectors (#385).
//!
//! The Rust core serializers are the source of truth for the wire format. This
//! test pins their output as hex constants; the same constants are embedded in
//! `csharp/tests/Int2Dds.Tests/CdrWireFormatTests.cs` and
//! `python/tests/test_cdr_wire_format.py` so a writer/reader pair in any
//! binding cannot drift together without a wire-format test failing.
//!
//! To regenerate after an intentional format change:
//!   cargo test -p int2dds --lib cdr_wire_format -- --ignored --nocapture

use crate::serialize::cdr::{
    CdrSerializer, ExtensibilityKind, PrimitiveSerialize, SequenceSerialize, StringSerialize,
    Xcdr2Serializer,
};
use crate::serialize::core::BufferManager;

const SHAPES: &[&str] = &[
    "bool_true",
    "u8",
    "i8",
    "u16",
    "i16",
    "u32",
    "i32",
    "u64",
    "i64",
    "f32",
    "f64",
    "string",
    "string_empty",
    "wstring",
    "wstring_empty",
    "bytes",
    "seq_u32",
    "mixed",
];

const FORMATS: &[&str] = &["xcdr1_le", "xcdr1_be", "xcdr2_le"];

const VECTORS: &[(&str, &str, &str)] = &[
    ("bool_true", "xcdr1_le", "0001000001"),
    ("bool_true", "xcdr1_be", "0000000001"),
    ("bool_true", "xcdr2_le", "0007000001"),
    ("u8", "xcdr1_le", "00010000ab"),
    ("u8", "xcdr1_be", "00000000ab"),
    ("u8", "xcdr2_le", "00070000ab"),
    ("i8", "xcdr1_le", "00010000fb"),
    ("i8", "xcdr1_be", "00000000fb"),
    ("i8", "xcdr2_le", "00070000fb"),
    ("u16", "xcdr1_le", "00010000efbe"),
    ("u16", "xcdr1_be", "00000000beef"),
    ("u16", "xcdr2_le", "00070000efbe"),
    ("i16", "xcdr1_le", "00010000c7cf"),
    ("i16", "xcdr1_be", "00000000cfc7"),
    ("i16", "xcdr2_le", "00070000c7cf"),
    ("u32", "xcdr1_le", "00010000efbeadde"),
    ("u32", "xcdr1_be", "00000000deadbeef"),
    ("u32", "xcdr2_le", "00070000efbeadde"),
    ("i32", "xcdr1_le", "00010000eb32a4f8"),
    ("i32", "xcdr1_be", "00000000f8a432eb"),
    ("i32", "xcdr2_le", "00070000eb32a4f8"),
    ("u64", "xcdr1_le", "000100008877665544332211"),
    ("u64", "xcdr1_be", "000000001122334455667788"),
    ("u64", "xcdr2_le", "000700008877665544332211"),
    ("i64", "xcdr1_le", "00010000eb7e16820befddee"),
    ("i64", "xcdr1_be", "00000000eeddef0b82167eeb"),
    ("i64", "xcdr2_le", "00070000eb7e16820befddee"),
    ("f32", "xcdr1_le", "000100000000c03f"),
    ("f32", "xcdr1_be", "000000003fc00000"),
    ("f32", "xcdr2_le", "000700000000c03f"),
    ("f64", "xcdr1_le", "0001000000000000000006c0"),
    ("f64", "xcdr1_be", "00000000c006000000000000"),
    ("f64", "xcdr2_le", "0007000000000000000006c0"),
    ("string", "xcdr1_le", "000100000600000068656c6c6f00"),
    ("string", "xcdr1_be", "000000000000000668656c6c6f00"),
    ("string", "xcdr2_le", "000700000600000068656c6c6f00"),
    ("string_empty", "xcdr1_le", "000100000100000000"),
    ("string_empty", "xcdr1_be", "000000000000000100"),
    ("string_empty", "xcdr2_le", "000700000100000000"),
    ("wstring", "xcdr1_le", "0001000003000000770003265cd5"),
    ("wstring", "xcdr1_be", "000000000000000300772603d55c"),
    ("wstring", "xcdr2_le", "0007000003000000770003265cd5"),
    ("wstring_empty", "xcdr1_le", "0001000000000000"),
    ("wstring_empty", "xcdr1_be", "0000000000000000"),
    ("wstring_empty", "xcdr2_le", "0007000000000000"),
    ("bytes", "xcdr1_le", "00010000050000000102030405"),
    ("bytes", "xcdr1_be", "00000000000000050102030405"),
    ("bytes", "xcdr2_le", "00070000050000000102030405"),
    ("seq_u32", "xcdr1_le", "000100000300000001000000ffffff7fffffffff"),
    ("seq_u32", "xcdr1_be", "0000000000000003000000017fffffffffffffff"),
    ("seq_u32", "xcdr2_le", "000700000300000001000000ffffff7fffffffff"),
    (
        "mixed",
        "xcdr1_le",
        "0001000042000000000000000807060504030201feff00000d0c0b0a030000006f6b0000000000000000f03f",
    ),
    (
        "mixed",
        "xcdr1_be",
        "0000000042000000000000000102030405060708fffe00000a0b0c0d000000036f6b00003ff0000000000000",
    ),
    (
        "mixed",
        "xcdr2_le",
        "00070000420000000807060504030201feff00000d0c0b0a030000006f6b0000000000000000f03f",
    ),
];

fn write_shape<S: PrimitiveSerialize + StringSerialize + SequenceSerialize>(
    s: &mut S,
    shape: &str,
) {
    match shape {
        "bool_true" => s.serialize_bool(true).unwrap(),
        "u8" => s.serialize_u8(0xAB).unwrap(),
        "i8" => s.serialize_i8(-5).unwrap(),
        "u16" => s.serialize_u16(0xBEEF).unwrap(),
        "i16" => s.serialize_i16(-12345).unwrap(),
        "u32" => s.serialize_u32(0xDEAD_BEEF).unwrap(),
        "i32" => s.serialize_i32(-123_456_789).unwrap(),
        "u64" => s.serialize_u64(0x1122_3344_5566_7788).unwrap(),
        "i64" => s.serialize_i64(-1_234_567_890_123_456_789).unwrap(),
        "f32" => s.serialize_f32(1.5).unwrap(),
        "f64" => s.serialize_f64(-2.75).unwrap(),
        "string" => s.serialize_string("hello").unwrap(),
        "string_empty" => s.serialize_string("").unwrap(),
        "wstring" => s.serialize_wstring16("w\u{2603}\u{d55c}").unwrap(),
        "wstring_empty" => s.serialize_wstring16("").unwrap(),
        "bytes" => s.serialize_byte_sequence(&[1, 2, 3, 4, 5]).unwrap(),
        "seq_u32" => s.serialize_u32_sequence(&[1, 0x7FFF_FFFF, 0xFFFF_FFFF]).unwrap(),
        "mixed" => {
            s.serialize_u8(0x42).unwrap();
            s.serialize_u64(0x0102_0304_0506_0708).unwrap();
            s.serialize_i16(-2).unwrap();
            s.serialize_u32(0x0A0B_0C0D).unwrap();
            s.serialize_string("ok").unwrap();
            s.serialize_f64(1.0).unwrap();
        }
        other => panic!("unknown shape {other}"),
    }
}

fn build(shape: &str, format: &str) -> Vec<u8> {
    match format {
        "xcdr1_le" | "xcdr1_be" => {
            let mut s = CdrSerializer::new(format == "xcdr1_le");
            s.write_encapsulation_header().unwrap();
            write_shape(&mut s, shape);
            s.into_bytes()
        }
        "xcdr2_le" => {
            let mut s = Xcdr2Serializer::new(true, ExtensibilityKind::Final);
            s.write_encapsulation_header().unwrap();
            write_shape(&mut s, shape);
            s.into_bytes()
        }
        other => panic!("unknown format {other}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn wire_format_vectors_match_core_serializers() {
    assert_eq!(VECTORS.len(), SHAPES.len() * FORMATS.len());
    for (shape, format, expected) in VECTORS {
        assert_eq!(&hex(&build(shape, format)), expected, "shape={shape} format={format}");
    }
}

#[test]
#[ignore]
fn dump_wire_format_vectors() {
    for shape in SHAPES {
        for format in FORMATS {
            println!("    (\"{shape}\", \"{format}\", \"{}\"),", hex(&build(shape, format)));
        }
    }
}
