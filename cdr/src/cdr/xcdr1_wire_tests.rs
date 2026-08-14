//! XCDR1 read-path wire vectors pinned without `#[derive(DdsType)]`.
//!
//! The kernel's own XCDR1 test is a round trip, and the vectors in
//! `cdr/tests/derive_wire/xcdr1.rs` reach the codec through the derive macro, so the
//! code under test also produces the expectation. Neither catches a change applied
//! symmetrically to the writer and the reader — which is what merging the two codecs'
//! readers is. These decode fixed bytes instead.
//!
//! The vectors that carry the most weight put a 64-bit read at an offset that is a
//! multiple of 4 but not of 8. That is the only place XCDR1's 8-byte alignment and
//! XCDR2's 4-byte cap disagree, so a reader that picks up the cap fails here and
//! passes everywhere else.

use bytes::Bytes;

use crate::cdr::{CdrDeserializer, CdrError, PlCdrMemberHeader};

const LE: bool = true;
const BE: bool = false;

fn de(body: &[u8], little_endian: bool) -> CdrDeserializer<'_> {
    CdrDeserializer::new_without_header(body, little_endian)
}

// -- alignment ---------------------------------------------------------------

/// Body offset 4 is 4-aligned but not 8-aligned. The padding is `FF` so a reader that
/// capped alignment at 4 would decode it instead of erroring.
#[test]
fn every_sixty_four_bit_reader_aligns_to_eight() {
    let integer = [
        0x01, 0x00, 0x00, 0x00, // u32 = 1
        0xFF, 0xFF, 0xFF, 0xFF, // padding out to the 8-byte boundary
        0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
    ];
    let mut d = de(&integer, LE);
    assert_eq!(d.deserialize_u32().unwrap(), 1);
    assert_eq!(d.deserialize_u64().unwrap(), 0x0102_0304_0506_0708);

    let mut d = de(&integer, LE);
    assert_eq!(d.deserialize_u32().unwrap(), 1);
    assert_eq!(d.deserialize_i64().unwrap(), 0x0102_0304_0506_0708);

    let float = [
        0x01, 0x00, 0x00, 0x00, //
        0xFF, 0xFF, 0xFF, 0xFF, //
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F,
    ];
    let mut d = de(&float, LE);
    assert_eq!(d.deserialize_u32().unwrap(), 1);
    assert_eq!(d.deserialize_f64().unwrap(), 1.0);
}

#[test]
fn thirty_two_and_sixteen_bit_readers_align_to_their_own_width() {
    let body = [0x01, 0xFF, 0x02, 0x00, 0xFF, 0xFF, 0xFF, 0xFF];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_u8().unwrap(), 1);
    assert_eq!(d.deserialize_u16().unwrap(), 2, "a 16-bit read skips the odd byte");

    let body = [0x01, 0xFF, 0xFF, 0xFF, 0x02, 0x00, 0x00, 0x00];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_u8().unwrap(), 1);
    assert_eq!(d.deserialize_u32().unwrap(), 2);
}

#[test]
fn byte_wide_readers_never_align() {
    let body = [0x01, 0x02, 0x00, 0xFF];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_u8().unwrap(), 1);
    assert_eq!(d.deserialize_i8().unwrap(), 2);
    assert!(!d.deserialize_bool().unwrap());
    assert!(d.deserialize_bool().unwrap(), "any nonzero octet is true");
}

/// Alignment counts from the start of the body. Measuring from the start of the buffer
/// would insert four bytes of padding that the writer never emitted.
#[test]
fn alignment_counts_from_the_body_not_the_encapsulation_header() {
    let wire = [
        0x00, 0x01, 0x00, 0x00, // CDR_LE
        0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
    ];
    let mut d = CdrDeserializer::new(&wire).unwrap();
    assert_eq!(d.deserialize_u64().unwrap(), 0x0102_0304_0506_0708);
}

// -- encapsulation header ----------------------------------------------------

#[test]
fn the_encapsulation_id_selects_the_endianness() {
    let cases: [([u8; 2], u32); 4] = [
        ([0x00, 0x00], 0x0102_0304), // CDR_BE
        ([0x00, 0x01], 0x0403_0201), // CDR_LE
        ([0x00, 0x02], 0x0102_0304), // PL_CDR_BE
        ([0x00, 0x03], 0x0403_0201), // PL_CDR_LE
    ];
    for (id, expected) in cases {
        let wire = [id[0], id[1], 0x00, 0x00, 0x01, 0x02, 0x03, 0x04];
        let mut d = CdrDeserializer::new(&wire).unwrap();
        assert_eq!(d.deserialize_u32().unwrap(), expected, "{id:02X?}");
    }
}

/// The XCDR2 identifiers are not accepted here: the alignment rules differ, so decoding
/// them with this reader would silently produce the wrong offsets.
#[test]
fn an_xcdr2_encapsulation_id_is_rejected() {
    assert!(matches!(
        CdrDeserializer::new(&[0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        Err(CdrError::InvalidEncapsulation(0x0007))
    ));
    assert!(CdrDeserializer::new(&[0x00, 0x01, 0x00]).is_err(), "a header needs four bytes");
}

// -- scalars -----------------------------------------------------------------

#[test]
fn scalars_decode_under_both_endiannesses() {
    let body = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    assert_eq!(de(&body, LE).deserialize_u16().unwrap(), 0x0201);
    assert_eq!(de(&body, BE).deserialize_u16().unwrap(), 0x0102);
    assert_eq!(de(&body, LE).deserialize_u32().unwrap(), 0x0403_0201);
    assert_eq!(de(&body, BE).deserialize_u32().unwrap(), 0x0102_0304);
    assert_eq!(de(&body, LE).deserialize_u64().unwrap(), 0x0807_0605_0403_0201);
    assert_eq!(de(&body, BE).deserialize_u64().unwrap(), 0x0102_0304_0506_0708);

    let ones = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    assert_eq!(de(&ones, LE).deserialize_i16().unwrap(), -1);
    assert_eq!(de(&ones, BE).deserialize_i32().unwrap(), -1);
    assert_eq!(de(&ones, LE).deserialize_i64().unwrap(), -1);

    assert_eq!(de(&[0x00, 0x00, 0x80, 0x3F], LE).deserialize_f32().unwrap(), 1.0);
    assert_eq!(de(&[0x3F, 0x80, 0x00, 0x00], BE).deserialize_f32().unwrap(), 1.0);
    let f64_be = [0x3F, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(de(&f64_be, BE).deserialize_f64().unwrap(), 1.0);
}

#[test]
fn a_truncated_read_errs_instead_of_panicking() {
    assert!(de(&[], LE).deserialize_u8().is_err());
    assert!(de(&[0x01], LE).deserialize_u16().is_err());
    assert!(de(&[0x01, 0x02, 0x03], LE).deserialize_u32().is_err());
    assert!(de(&[0x01, 0x00, 0x00, 0x00], LE).deserialize_u64().is_err());
}

#[test]
fn skip_moves_the_cursor_and_is_bounds_checked() {
    let body = [0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0x00];
    let mut d = de(&body, LE);
    d.skip(4).unwrap();
    assert_eq!(d.deserialize_u32().unwrap(), 1);
    assert!(d.skip(1).is_err());
}

// -- characters and strings --------------------------------------------------

#[test]
fn a_string_carries_its_null_terminator_inside_the_declared_length() {
    let body = [0x03, 0x00, 0x00, 0x00, b'h', b'i', 0x00];
    assert_eq!(de(&body, LE).deserialize_string().unwrap(), "hi");

    let be = [0x00, 0x00, 0x00, 0x03, b'h', b'i', 0x00];
    assert_eq!(de(&be, BE).deserialize_string().unwrap(), "hi");
}

#[test]
fn a_string_without_a_terminator_keeps_every_declared_byte() {
    let body = [0x02, 0x00, 0x00, 0x00, b'h', b'i'];
    assert_eq!(de(&body, LE).deserialize_string().unwrap(), "hi");
}

#[test]
fn a_zero_length_string_consumes_only_its_length_field() {
    let body = [0x00, 0x00, 0x00, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_string().unwrap(), "");
    assert_eq!(d.deserialize_u32().unwrap(), 0xDDCC_BBAA, "the cursor stopped at 4");
}

#[test]
fn a_string_length_field_aligns_to_four() {
    let body = [0xAA, 0xFF, 0xFF, 0xFF, 0x03, 0x00, 0x00, 0x00, b'h', b'i', 0x00];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_u8().unwrap(), 0xAA);
    assert_eq!(d.deserialize_string().unwrap(), "hi");
}

#[test]
fn a_string_rejects_an_overlong_length_and_invalid_utf8() {
    assert!(de(&[0x10, 0x00, 0x00, 0x00, b'a'], LE).deserialize_string().is_err());
    assert!(matches!(
        de(&[0x02, 0x00, 0x00, 0x00, 0xFF, 0x00], LE).deserialize_string(),
        Err(CdrError::InvalidString)
    ));
}

/// The length counts UTF-16 code units, not bytes.
#[test]
fn a_wstring_length_counts_code_units() {
    let body = [0x02, 0x00, 0x00, 0x00, 0x68, 0x00, 0x69, 0x00];
    assert_eq!(de(&body, LE).deserialize_wstring16().unwrap(), "hi");

    let be = [0x00, 0x00, 0x00, 0x02, 0x00, 0x68, 0x00, 0x69];
    assert_eq!(de(&be, BE).deserialize_wstring16().unwrap(), "hi");

    assert_eq!(de(&[0x00, 0x00, 0x00, 0x00], LE).deserialize_wstring16().unwrap(), "");
}

#[test]
fn a_char_is_one_latin1_octet_and_a_wchar_is_a_code_unit() {
    assert_eq!(de(&[0xE9], LE).deserialize_char().unwrap(), 'é');
    assert_eq!(de(&[0xE9, 0x00], LE).deserialize_wchar16().unwrap(), 'é');
    assert_eq!(de(&[0xE9, 0x00, 0x00, 0x00], LE).deserialize_wchar32().unwrap(), 'é');
}

#[test]
fn an_unpaired_surrogate_is_rejected() {
    assert!(matches!(
        de(&[0x00, 0xD8], LE).deserialize_wchar16(),
        Err(CdrError::InvalidWideCharacter)
    ));
    assert!(matches!(
        de(&[0x00, 0xD8, 0x00, 0x00], LE).deserialize_wchar32(),
        Err(CdrError::InvalidWideCharacter)
    ));
}

/// `deserialize_char_array` reads a length prefix; `deserialize_char_array_fixed` takes
/// the count from the caller. The names do not say so.
#[test]
fn the_two_char_array_readers_differ_by_the_length_prefix() {
    let body = [0x02, 0x00, 0x00, 0x00, b'a', b'b'];
    assert_eq!(de(&body, LE).deserialize_char_array().unwrap(), vec!['a', 'b']);
    assert_eq!(de(&body[4..], LE).deserialize_char_array_fixed(2).unwrap(), vec!['a', 'b']);
}

// -- collections -------------------------------------------------------------

/// The 8-byte alignment applies between the count and the elements too.
#[test]
fn a_sixty_four_bit_sequence_pads_between_its_count_and_its_elements() {
    let body = [
        0x01, 0x00, 0x00, 0x00, // count = 1
        0xFF, 0xFF, 0xFF, 0xFF, // padding out to the 8-byte boundary
        0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
    ];
    assert_eq!(de(&body, LE).deserialize_u64_sequence().unwrap(), vec![0x0102_0304_0506_0708]);

    let float = [
        0x01, 0x00, 0x00, 0x00, //
        0xFF, 0xFF, 0xFF, 0xFF, //
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F,
    ];
    assert_eq!(de(&float, LE).deserialize_f64_sequence().unwrap(), vec![1.0]);
}

/// An empty run skips the alignment entirely, matching the write path: a zero-length
/// sequence is its 4-byte count and nothing else.
#[test]
fn an_empty_sequence_does_not_pad_for_its_element_type() {
    let body = [0x00, 0x00, 0x00, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];
    let mut d = de(&body, LE);
    assert!(d.deserialize_u64_sequence().unwrap().is_empty());
    assert_eq!(d.deserialize_u32().unwrap(), 0xDDCC_BBAA, "the cursor stopped at 4");
}

#[test]
fn a_bulk_run_honours_the_declared_endianness() {
    let le = [0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00];
    assert_eq!(de(&le, LE).deserialize_u32_sequence().unwrap(), vec![1, 2]);

    let be = [0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02];
    assert_eq!(de(&be, BE).deserialize_u32_sequence().unwrap(), vec![1, 2]);

    let le16 = [0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00];
    assert_eq!(de(&le16, LE).deserialize_u16_sequence().unwrap(), vec![1, 2]);
}

/// Octet runs are not aligned and not byte-swapped, so they read the same either way.
#[test]
fn an_octet_run_is_neither_aligned_nor_swapped() {
    let body = [0x03, 0x00, 0x00, 0x00, 0x00, 0x01, 0xFF, 0x2A];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_bool_sequence().unwrap(), vec![false, true, true]);
    assert_eq!(d.deserialize_u8().unwrap(), 0x2A, "the cursor stopped at 7");

    let body = [0x02, 0x00, 0x00, 0x00, b'a', b'b'];
    assert_eq!(de(&body, LE).deserialize_char_sequence().unwrap(), vec!['a', 'b']);
}

/// XCDR2 puts a DHEADER in front of a sequence of non-primitive elements. XCDR1 does
/// not, and a merged reader must not start expecting one.
#[test]
fn a_string_sequence_carries_no_dheader() {
    let body = [
        0x01, 0x00, 0x00, 0x00, // count = 1
        0x02, 0x00, 0x00, 0x00, // string length
        b'a', 0x00,
    ];
    assert_eq!(de(&body, LE).deserialize_string_sequence().unwrap(), vec!["a".to_string()]);
}

/// Same divergence on the generic path, which is what `Vec<T>` deserializes through.
#[test]
fn a_generic_sequence_carries_no_dheader_either() {
    let body = [0x01, 0x00, 0x00, 0x00, 0x2A, 0x00, 0x00, 0x00];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_sequence(|d| d.deserialize_u32()).unwrap(), vec![42u32]);
}

#[test]
fn a_fixed_array_has_no_count_but_still_aligns() {
    let body = [
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // u8 then padding to 8
        0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
    ];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_u8().unwrap(), 1);
    assert_eq!(d.deserialize_u64_array(1).unwrap(), vec![0x0102_0304_0506_0708]);
}

#[test]
fn an_optional_is_a_boolean_flag_followed_by_the_value() {
    let body = [0x01, 0xFF, 0xFF, 0xFF, 0x2A, 0x00, 0x00, 0x00];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_optional(|d| d.deserialize_u32()).unwrap(), Some(42u32));

    let mut d = de(&[0x00], LE);
    assert_eq!(d.deserialize_optional(|d| d.deserialize_u32()).unwrap(), None);
}

// -- PL_CDR parameter headers ------------------------------------------------

#[test]
fn a_short_parameter_header_splits_the_flag_bit_from_the_id() {
    let body = [0x05, 0x00, 0x04, 0x00, 0xDD, 0xCC, 0xBB, 0xAA];
    let mut d = de(&body, LE);
    assert_eq!(
        d.read_parameter_header().unwrap(),
        PlCdrMemberHeader::Short { pid: 5, length: 4, must_understand: false }
    );
    assert_eq!(d.deserialize_u32().unwrap(), 0xAABB_CCDD);

    assert_eq!(
        de(&[0x05, 0x40, 0x04, 0x00], LE).read_parameter_header().unwrap(),
        PlCdrMemberHeader::Short { pid: 5, length: 4, must_understand: true }
    );
    assert_eq!(
        de(&[0x40, 0x05, 0x00, 0x04], BE).read_parameter_header().unwrap(),
        PlCdrMemberHeader::Short { pid: 5, length: 4, must_understand: true }
    );
}

/// PID_EXTENDED (0x3F01) declares a fixed 8-byte body holding the real id and length.
#[test]
fn a_pid_extended_header_carries_a_32_bit_id_and_length() {
    let body = [
        0x01, 0x3F, 0x08, 0x00, // PID_EXTENDED, length 8
        0x00, 0x00, 0x00, 0x10, // member_id = 0x1000_0000
        0x04, 0x00, 0x00, 0x00, // member_length = 4
    ];
    let mut d = de(&body, LE);
    assert_eq!(
        d.read_parameter_header().unwrap(),
        PlCdrMemberHeader::Long { member_id: 0x1000_0000, length: 4, must_understand: false }
    );

    let flagged = [0x01, 0x7F, 0x08, 0x00, 0x00, 0x00, 0x00, 0x10, 0x04, 0x00, 0x00, 0x00];
    assert_eq!(
        de(&flagged, LE).read_parameter_header().unwrap(),
        PlCdrMemberHeader::Long { member_id: 0x1000_0000, length: 4, must_understand: true }
    );
}

#[test]
fn the_sentinel_ends_the_parameter_list() {
    assert_eq!(
        de(&[0x02, 0x3F, 0x00, 0x00], LE).read_parameter_header().unwrap(),
        PlCdrMemberHeader::Sentinel
    );
    assert_eq!(
        de(&[0x3F, 0x02, 0x00, 0x00], BE).read_parameter_header().unwrap(),
        PlCdrMemberHeader::Sentinel
    );
}

#[test]
fn a_parameter_header_aligns_to_four_before_it_is_read() {
    let body = [0xAA, 0xFF, 0xFF, 0xFF, 0x05, 0x00, 0x00, 0x00];
    let mut d = de(&body, LE);
    assert_eq!(d.deserialize_u8().unwrap(), 0xAA);
    assert_eq!(
        d.read_parameter_header().unwrap(),
        PlCdrMemberHeader::Short { pid: 5, length: 0, must_understand: false }
    );
}

/// A member whose declared length excludes its trailing padding leaves the cursor
/// unaligned, so the peek has to round up before it looks — otherwise it reads padding.
#[test]
fn the_sentinel_peek_aligns_before_it_looks() {
    let body = [
        0x05, 0x00, 0x01, 0x00, // pid 5, length 1
        0xAA, 0xFF, 0xFF, 0xFF, // the member, then padding
        0x02, 0x3F, 0x00, 0x00, // sentinel
    ];
    let mut d = de(&body, LE);
    d.read_parameter_header().unwrap();
    d.skip(1).unwrap();
    assert!(d.is_at_sentinel(), "position 5 must round up to 8 before peeking");
    assert_eq!(d.read_parameter_header().unwrap(), PlCdrMemberHeader::Sentinel);

    assert!(!de(&[0x05, 0x00, 0x00, 0x00], LE).is_at_sentinel());
    assert!(!de(&[0x02, 0x3F], LE).is_at_sentinel(), "a truncated peek is not a sentinel");
}

// -- chained input -----------------------------------------------------------

/// The fragment path must agree with the contiguous one at an 8-byte boundary, and both
/// reads here straddle a chunk edge.
#[test]
fn chained_input_matches_contiguous_across_an_eight_byte_boundary() {
    let wire = [
        0x00, 0x01, 0x00, 0x00, // CDR_LE
        0x01, 0x00, 0x00, 0x00, // u32 = 1, body 0..4
        0xFF, 0xFF, 0xFF, 0xFF, // padding, body 4..8
        0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // u64, body 8..16
    ];

    let mut contiguous = CdrDeserializer::new(&wire).unwrap();
    assert_eq!(contiguous.deserialize_u32().unwrap(), 1);
    assert_eq!(contiguous.deserialize_u64().unwrap(), 0x0102_0304_0506_0708);

    // Splits at absolute 5 and 14: the u32 spans the first edge, the u64 the second.
    let chunks: Vec<Bytes> =
        [&wire[..5], &wire[5..14], &wire[14..]].iter().map(|c| Bytes::copy_from_slice(c)).collect();
    let mut chained = CdrDeserializer::new_chained(&chunks).unwrap();
    assert_eq!(chained.deserialize_u32().unwrap(), 1);
    assert_eq!(chained.deserialize_u64().unwrap(), 0x0102_0304_0506_0708);
}
