//! XCDR2 wire vectors pinned without `#[derive(DdsType)]`.
//!
//! The XCDR2 tests in `cdr/tests/derive_wire/xcdr2.rs` all go through the
//! derive macro, so the code under test also produces the expectation; and the
//! kernel's own XCDR2 tests are round-trip or error-path only. Neither can catch a
//! framing change that is applied symmetrically to the writer and the reader —
//! which is exactly what a codec merge does. These fix the bytes instead.

use std::collections::BTreeMap;

use speedy::Endianness;

use crate::cdr::{
    CdrSerializer, ExtensibilityKind, LcHint, MemberHeader, PrimitiveSerialize, SequenceSerialize,
    Xcdr2Deserializer, Xcdr2Serializer, XcdrSerialize,
};
use crate::{BufferManager, SerializationError, WChar};

const LE: bool = true;
const BE: bool = false;

fn ser(little_endian: bool, extensibility: ExtensibilityKind) -> Xcdr2Serializer {
    let mut s = Xcdr2Serializer::new(little_endian, extensibility);
    s.write_encapsulation_header().unwrap();
    s
}

/// Everything after the 4-byte encapsulation header.
fn body(s: Xcdr2Serializer) -> Vec<u8> {
    s.into_bytes()[4..].to_vec()
}

fn xcdr_body<T: XcdrSerialize>(value: &T) -> Vec<u8> {
    let mut s = ser(LE, ExtensibilityKind::Final);
    value.serialize_xcdr(&mut s).unwrap();
    body(s)
}

// -- encapsulation header ----------------------------------------------------

#[test]
fn encapsulation_id_per_extensibility_and_endianness() {
    let cases = [
        (BE, ExtensibilityKind::Final, [0x00, 0x06]),
        (LE, ExtensibilityKind::Final, [0x00, 0x07]),
        (BE, ExtensibilityKind::Appendable, [0x00, 0x08]),
        (LE, ExtensibilityKind::Appendable, [0x00, 0x09]),
        (BE, ExtensibilityKind::Mutable, [0x00, 0x0A]),
        (LE, ExtensibilityKind::Mutable, [0x00, 0x0B]),
    ];
    for (little_endian, extensibility, id) in cases {
        let bytes = ser(little_endian, extensibility).into_bytes();
        assert_eq!(&bytes[..2], &id, "{extensibility:?} le={little_endian}");
        assert_eq!(&bytes[2..4], &[0x00, 0x00], "{extensibility:?} options");
    }
}

#[test]
fn type_hash_sets_the_options_bit_and_appends_fourteen_bytes() {
    let hash = [0xAAu8; 14];
    let mut s = Xcdr2Serializer::with_type_hash(LE, ExtensibilityKind::Final, hash);
    s.write_encapsulation_header().unwrap();
    let bytes = s.into_bytes();
    assert_eq!(&bytes[..4], &[0x00, 0x07, 0x00, 0x01]);
    assert_eq!(&bytes[4..], &hash);
}

// -- EMHEADER, `MemberHeader::write` path ------------------------------------

fn emh(member_id: u32, length: usize, must_understand: bool, e: Endianness) -> Vec<u8> {
    let mut buf = Vec::new();
    MemberHeader::with_must_understand(member_id, length, must_understand)
        .write(&mut buf, e)
        .unwrap();
    buf
}

/// This writer reaches LC=0..4 only; LC=5/6/7 belong to `Xcdr2Serializer::select_lc`.
#[test]
fn member_header_write_emits_lc0_through_lc4() {
    let e = Endianness::LittleEndian;
    assert_eq!(emh(5, 1, false, e), [0x05, 0x00, 0x00, 0x00]);
    assert_eq!(emh(5, 2, false, e), [0x05, 0x00, 0x00, 0x10]);
    assert_eq!(emh(5, 4, false, e), [0x05, 0x00, 0x00, 0x20]);
    assert_eq!(emh(5, 8, false, e), [0x05, 0x00, 0x00, 0x30]);
    assert_eq!(emh(5, 3, false, e), [0x05, 0x00, 0x00, 0x40, 0x03, 0x00, 0x00, 0x00]);
    assert_eq!(emh(5, 12, false, e), [0x05, 0x00, 0x00, 0x40, 0x0C, 0x00, 0x00, 0x00]);
}

#[test]
fn member_header_write_carries_must_understand_and_a_28_bit_id() {
    let e = Endianness::LittleEndian;
    assert_eq!(emh(5, 4, true, e), [0x05, 0x00, 0x00, 0xA0]);
    assert_eq!(emh(0x0FFF_FFFF, 4, false, e), [0xFF, 0xFF, 0xFF, 0x2F]);
    assert_eq!(emh(0x0FFF_FFFF, 4, true, e), [0xFF, 0xFF, 0xFF, 0xAF]);
    assert_eq!(emh(5, 4, false, Endianness::BigEndian), [0x20, 0x00, 0x00, 0x05]);
    assert_eq!(emh(5, 3, false, Endianness::BigEndian), [0x40, 0x00, 0x00, 0x05, 0, 0, 0, 3]);
    assert!(matches!(
        MemberHeader::new(0x1000_0000, 4).write(&mut Vec::new(), e),
        Err(SerializationError::InvalidMemberId(_))
    ));
}

// -- EMHEADER, read path -----------------------------------------------------

/// LC=5/6/7 consume 4 bytes, not 8: the NEXTINT word is the payload's own first
/// four bytes (DDS-XTypes 7.4.3.4.2), so the reader must not skip past it.
#[test]
fn member_header_read_decodes_every_length_code() {
    let e = Endianness::LittleEndian;
    let cases: [(&[u8], u32, usize); 8] = [
        (&[0x05, 0x00, 0x00, 0x00], 1, 4),
        (&[0x05, 0x00, 0x00, 0x10], 2, 4),
        (&[0x05, 0x00, 0x00, 0x20], 4, 4),
        (&[0x05, 0x00, 0x00, 0x30], 8, 4),
        (&[0x05, 0x00, 0x00, 0x40, 0x0A, 0x00, 0x00, 0x00], 10, 8),
        (&[0x05, 0x00, 0x00, 0x50, 0x0A, 0x00, 0x00, 0x00], 14, 4),
        (&[0x05, 0x00, 0x00, 0x60, 0x03, 0x00, 0x00, 0x00], 16, 4),
        (&[0x05, 0x00, 0x00, 0x70, 0x03, 0x00, 0x00, 0x00], 28, 4),
    ];
    for (wire, member_length, consumed) in cases {
        let (h, n) = MemberHeader::read(wire, 0, e).unwrap();
        assert_eq!((h.member_id, h.member_length, n), (5, member_length, consumed), "{wire:02X?}");
        assert!(!h.must_understand);
    }
}

#[test]
fn member_header_read_honours_the_flag_bit_and_the_available_bytes() {
    let e = Endianness::LittleEndian;
    let (h, _) = MemberHeader::read(&[0x05, 0x00, 0x00, 0xA0], 0, e).unwrap();
    assert!(h.must_understand);
    assert_eq!(h.member_length, 4, "the flag bit must not leak into the length code");

    assert!(MemberHeader::read(&[0x05, 0x00, 0x00], 0, e).is_err());
    assert!(MemberHeader::read(&[0x05, 0x00, 0x00, 0x40], 0, e).is_err(), "LC=4 needs NEXTINT");
    assert!(MemberHeader::read(&[0x00; 8], 4, e).is_ok());
}

#[test]
fn member_header_write_round_trips_through_read() {
    let e = Endianness::LittleEndian;
    for length in [1usize, 2, 4, 8, 3, 12, 65_540] {
        let wire = emh(9, length, true, e);
        let (h, n) = MemberHeader::read(&wire, 0, e).unwrap();
        assert_eq!((h.member_id, h.member_length as usize, h.must_understand), (9, length, true));
        assert_eq!(n, wire.len(), "consumed bytes must cover the whole header");
    }
}

// -- EMHEADER, `write_member_with_lc` path -----------------------------------

#[test]
fn write_member_auto_picks_a_fixed_length_code() {
    let mut s = ser(LE, ExtensibilityKind::Mutable);
    s.write_member_with(1, false, |s| s.serialize_u32(0xAABB_CCDD)).unwrap();
    assert_eq!(body(s), [0x01, 0x00, 0x00, 0x20, 0xDD, 0xCC, 0xBB, 0xAA]);
}

#[test]
fn write_member_auto_falls_back_to_lc4_with_an_inserted_nextint() {
    let mut s = ser(LE, ExtensibilityKind::Mutable);
    s.write_member_with(1, false, |s| {
        for b in [0x11u8, 0x22, 0x33] {
            s.serialize_u8(b)?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(body(s), [0x01, 0x00, 0x00, 0x40, 0x03, 0x00, 0x00, 0x00, 0x11, 0x22, 0x33]);
}

/// LC=6 declares an element count, so NEXTINT coincides with the sequence length
/// field already at the head of the payload — nothing is inserted.
#[test]
fn write_member_seq_mul4_uses_lc6_over_the_length_field() {
    let mut s = ser(LE, ExtensibilityKind::Mutable);
    s.write_member_with_lc(1, false, LcHint::SeqMul4, |s| s.serialize_u32_sequence(&[7, 8]))
        .unwrap();
    let b = body(s);
    assert_eq!(&b[..8], &[0x01, 0x00, 0x00, 0x60, 0x02, 0x00, 0x00, 0x00]);
    assert_eq!(b.len(), 4 + 12);
    let (h, n) = MemberHeader::read(&b, 0, Endianness::LittleEndian).unwrap();
    assert_eq!((h.member_length as usize, n), (b.len() - 4, 4));
}

#[test]
fn write_member_seq_mul8_uses_lc7_over_the_length_field() {
    let mut s = ser(LE, ExtensibilityKind::Mutable);
    s.write_member_with_lc(1, false, LcHint::SeqMul8, |s| s.serialize_f64_sequence(&[1.0, 2.0]))
        .unwrap();
    let b = body(s);
    assert_eq!(&b[..8], &[0x01, 0x00, 0x00, 0x70, 0x02, 0x00, 0x00, 0x00]);
    assert_eq!(b.len(), 4 + 20);
    let (h, n) = MemberHeader::read(&b, 0, Endianness::LittleEndian).unwrap();
    assert_eq!((h.member_length as usize, n), (b.len() - 4, 4));
}

/// LC=5 reuses the value's own DHEADER as NEXTINT, and wins over the fixed-length
/// codes: an 8-byte payload here is LC=5, not LC=3.
#[test]
fn write_member_dheader_hint_uses_lc5_and_reuses_the_payload_dheader() {
    let mut s = ser(LE, ExtensibilityKind::Mutable);
    s.write_member_with_lc(1, false, LcHint::Dheader, |s| {
        let dheader = s.reserve_dheader();
        let start = s.position();
        s.serialize_u32(7)?;
        let size = (s.position() - start) as u32;
        s.write_dheader_at(dheader, size);
        Ok(())
    })
    .unwrap();
    let b = body(s);
    assert_eq!(b, [0x01, 0, 0, 0x50, 0x04, 0, 0, 0, 0x07, 0, 0, 0]);
    let (h, n) = MemberHeader::read(&b, 0, Endianness::LittleEndian).unwrap();
    assert_eq!((h.member_length as usize, n), (b.len() - 4, 4));
}

#[test]
fn write_member_sets_the_must_understand_bit_and_rejects_a_wide_id() {
    let mut s = ser(LE, ExtensibilityKind::Mutable);
    s.write_member_with(1, true, |s| s.serialize_u32(0)).unwrap();
    assert_eq!(&body(s)[..4], &[0x01, 0x00, 0x00, 0xA0]);

    let mut s = ser(LE, ExtensibilityKind::Mutable);
    assert!(matches!(
        s.write_member_with(0x1000_0000, false, |s| s.serialize_u32(0)),
        Err(SerializationError::InvalidMemberId(_))
    ));
}

#[test]
fn consecutive_members_stay_four_byte_aligned() {
    let mut s = ser(LE, ExtensibilityKind::Mutable);
    s.write_member_with(1, false, |s| s.serialize_u8(0x11)).unwrap();
    s.write_member_with(2, false, |s| s.serialize_u8(0x22)).unwrap();
    assert_eq!(
        body(s),
        [0x01, 0, 0, 0x00, 0x11, 0x00, 0x00, 0x00, 0x02, 0, 0, 0x00, 0x22],
        "the second EMHEADER is padded up to a 4-byte boundary"
    );
}

// -- DHEADER -----------------------------------------------------------------

#[test]
fn reserve_dheader_aligns_to_four_first() {
    let mut s = ser(LE, ExtensibilityKind::Final);
    s.serialize_u8(0xFF).unwrap();
    let pos = s.reserve_dheader();
    s.write_dheader_at(pos, 0x0102_0304);
    assert_eq!(body(s), [0xFF, 0x00, 0x00, 0x00, 0x04, 0x03, 0x02, 0x01]);
}

#[test]
fn begin_and_end_struct_backpatch_the_payload_size() {
    let mut s = ser(LE, ExtensibilityKind::Appendable);
    let pos = s.begin_struct().unwrap();
    s.serialize_u32(1).unwrap();
    s.serialize_u16(2).unwrap();
    s.end_struct(pos).unwrap();
    assert_eq!(body(s), [0x06, 0, 0, 0, 0x01, 0, 0, 0, 0x02, 0x00]);
}

/// The DHEADER is what makes APPENDABLE evolution work: a reader that knows fewer
/// members skips the rest, and one that over-reads is a hard error.
#[test]
fn end_struct_skips_unknown_trailing_members() {
    let wire = [0x08, 0, 0, 0, 0x01, 0, 0, 0, 0xDE, 0xAD, 0xBE, 0xEF, 0x63, 0, 0, 0];

    let mut d = Xcdr2Deserializer::new_without_header(&wire, LE);
    let (size, start) = d.begin_struct().unwrap();
    assert_eq!(size, 8);
    assert_eq!(d.deserialize_u32().unwrap(), 1);
    d.end_struct(size, start).unwrap();
    assert_eq!(d.deserialize_u32().unwrap(), 0x63, "the next member starts after the DHEADER span");

    let mut d = Xcdr2Deserializer::new_without_header(&wire, LE);
    let (size, start) = d.begin_struct().unwrap();
    for _ in 0..3 {
        d.deserialize_u32().unwrap();
    }
    assert!(d.end_struct(size, start).is_err());
}

// -- alignment ---------------------------------------------------------------

#[test]
fn xcdr2_caps_alignment_at_four_where_xcdr1_uses_eight() {
    let mut s = ser(LE, ExtensibilityKind::Final);
    s.serialize_u8(1).unwrap();
    s.serialize_u64(0x0102_0304_0506_0708).unwrap();
    assert_eq!(body(s), [1, 0, 0, 0, 8, 7, 6, 5, 4, 3, 2, 1], "1 byte + 3 pad + 8");

    let mut c = CdrSerializer::new(LE);
    c.write_encapsulation_header().unwrap();
    c.serialize_u8(1).unwrap();
    c.serialize_u64(0x0102_0304_0506_0708).unwrap();
    assert_eq!(
        c.into_bytes()[4..],
        [1, 0, 0, 0, 0, 0, 0, 0, 8, 7, 6, 5, 4, 3, 2, 1],
        "1 byte + 7 pad + 8"
    );
}

/// `new` takes the cap from the encapsulation id, not from the deserializer type,
/// so an XCDR1 payload read through `Xcdr2Deserializer` still aligns to 8.
#[test]
fn the_alignment_cap_follows_the_encapsulation_id() {
    let mut s = ser(LE, ExtensibilityKind::Final);
    s.serialize_u8(1).unwrap();
    s.serialize_u64(0x0102_0304_0506_0708).unwrap();
    let xcdr2_wire = s.into_bytes();
    let mut d = Xcdr2Deserializer::new(&xcdr2_wire).unwrap();
    assert_eq!(d.deserialize_u8().unwrap(), 1);
    assert_eq!(d.deserialize_u64().unwrap(), 0x0102_0304_0506_0708);

    let mut c = CdrSerializer::new(LE);
    c.write_encapsulation_header().unwrap();
    c.serialize_u8(1).unwrap();
    c.serialize_u64(0x0102_0304_0506_0708).unwrap();
    let xcdr1_wire = c.into_bytes();
    assert_eq!(xcdr1_wire.len() - xcdr2_wire.len(), 4, "XCDR1 pads 4 bytes more");

    let mut d = Xcdr2Deserializer::new(&xcdr1_wire).unwrap();
    assert_eq!(d.deserialize_u8().unwrap(), 1);
    assert_eq!(d.deserialize_u64().unwrap(), 0x0102_0304_0506_0708);
}

// -- collection framing (DDS-XTypes 7.4.3.5.3-4) -----------------------------

#[test]
fn primitive_sequence_omits_the_dheader() {
    assert_eq!(xcdr_body(&vec![1u32, 2u32]), [2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]);
}

#[test]
fn non_primitive_sequence_carries_a_dheader_of_the_content_size() {
    assert_eq!(
        xcdr_body(&vec!["a".to_string()]),
        [10, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, b'a', 0],
        "DHEADER spans the length field and the elements"
    );
}

#[test]
fn the_string_sequence_override_matches_the_generic_sequence_path() {
    let mut s = ser(LE, ExtensibilityKind::Final);
    s.serialize_string_sequence(&["a".to_string()]).unwrap();
    assert_eq!(body(s), xcdr_body(&vec!["a".to_string()]));
}

#[test]
fn primitive_array_omits_both_the_dheader_and_the_count() {
    assert_eq!(xcdr_body(&[1u32, 2u32]), [1, 0, 0, 0, 2, 0, 0, 0]);
}

#[test]
fn non_primitive_array_carries_a_dheader_but_still_no_count() {
    assert_eq!(
        xcdr_body(&["a".to_string(), "b".to_string()]),
        [14, 0, 0, 0, 2, 0, 0, 0, b'a', 0, 0, 0, 2, 0, 0, 0, b'b', 0]
    );
}

/// A multidimensional IDL array is one flat array (DDS-XTypes 7.4.3.4): one
/// DHEADER for the whole thing when the base type is non-primitive, none at all
/// when it is primitive.
#[test]
fn a_multidimensional_array_frames_once_over_the_flattened_elements() {
    assert_eq!(xcdr_body(&[[1u32, 2u32], [3u32, 4u32]]), xcdr_body(&[1u32, 2, 3, 4]));
    assert_eq!(
        xcdr_body(&[["a".to_string()], ["b".to_string()]]),
        xcdr_body(&["a".to_string(), "b".to_string()])
    );
}

#[test]
fn a_map_frames_only_when_key_or_value_is_non_primitive() {
    let mut primitive = BTreeMap::new();
    primitive.insert(1u32, 2u32);
    assert_eq!(xcdr_body(&primitive), [1, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]);

    let mut with_string = BTreeMap::new();
    with_string.insert(1u32, "a".to_string());
    assert_eq!(xcdr_body(&with_string), [14, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, b'a', 0]);

    // `WChar` is a primitive here; classifying it as a struct would add a DHEADER
    // that the C, C# and Python codecs do not write.
    let mut with_wchar = BTreeMap::new();
    with_wchar.insert(1u32, WChar::from('A'));
    assert_eq!(xcdr_body(&with_wchar), [1, 0, 0, 0, 1, 0, 0, 0, 0x41, 0]);
}

// -- LC=4 retrofit -----------------------------------------------------------

#[test]
fn insert_nextint_slot_shifts_the_payload_without_disturbing_it() {
    let mut s = ser(LE, ExtensibilityKind::Final);
    s.serialize_u32(0x1122_3344).unwrap();
    s.insert_nextint_slot_at(4);
    assert_eq!(body(s), [0, 0, 0, 0, 0x44, 0x33, 0x22, 0x11]);
}
