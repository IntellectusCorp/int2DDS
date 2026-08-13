/// Pins the `IS_PRIMITIVE` classification that decides whether an XCDR2 collection
/// carries a DHEADER. The three IDL generators treat everything outside
/// `String|WString|Struct|Enum|Bitmask|Sequence|Array|Map` as primitive, so any Rust
/// type classified differently diverges from C/C#/Python on the wire.
#[cfg(test)]
mod element_classification_tests {
    use crate::serialize::cdr::*;
    use crate::serialize::core::BufferManager;
    use crate::serialize::WChar;
    use int2dds_derive::DdsType;
    use std::collections::{BTreeMap, HashMap};

    #[derive(DdsType)]
    #[dds_type(bitmask, bit_bound = 8, crate_path = "crate")]
    #[repr(u8)]
    enum Flags {
        #[dds(position = 0)]
        F0 = 1,
    }

    fn encode_xcdr2<T: XcdrSerialize>(value: &T) -> Vec<u8> {
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        serializer.into_bytes()
    }

    #[test]
    fn map_of_wchar_omits_dheader() {
        let mut value: BTreeMap<i32, WChar> = BTreeMap::new();
        value.insert(1, WChar::from('A'));

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x00, 0x07, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x41, 0x00,
        ];
        assert_eq!(encode_xcdr2(&value), expected);
    }

    /// Bitmask is a constructed type, not a primitive, so the map is framed
    /// (DDS-XTypes 7.4.3.5.4; see the unresolved OMG issue DDSXTY14-56).
    #[test]
    fn map_of_bitmask_keeps_dheader() {
        let mut value: BTreeMap<i32, FlagsValue> = BTreeMap::new();
        value.insert(1, FlagsValue::from(Flags::F0));

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x00, 0x07, 0x00, 0x00,
            0x09, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x01,
        ];
        assert_eq!(encode_xcdr2(&value), expected);
    }

    /// Control: a genuinely non-primitive value must keep its DHEADER.
    #[test]
    fn map_of_string_keeps_dheader() {
        let mut value: BTreeMap<i32, String> = BTreeMap::new();
        value.insert(1, "ab".to_string());

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x00, 0x07, 0x00, 0x00,
            0x0F, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x03, 0x00, 0x00, 0x00,
            b'a', b'b', 0x00,
        ];
        assert_eq!(encode_xcdr2(&value), expected);
    }

    /// `HashMap` carries its own copy of the condition, so pin it too.
    #[test]
    fn hash_map_of_wchar_omits_dheader() {
        let mut value: HashMap<i32, WChar> = HashMap::new();
        value.insert(1, WChar::from('A'));

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x00, 0x07, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x41, 0x00,
        ];
        assert_eq!(encode_xcdr2(&value), expected);

        let mut deserializer = XcdrDeserializer::new(&expected).unwrap();
        let result = HashMap::<i32, WChar>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn map_of_wchar_round_trips() {
        let mut value: BTreeMap<i32, WChar> = BTreeMap::new();
        value.insert(1, WChar::from('A'));
        value.insert(2, WChar::from('Z'));

        let bytes = encode_xcdr2(&value);
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = BTreeMap::<i32, WChar>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    enum Color {
        Red,
        Blue,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate", extensibility = "Final")]
    struct Pt {
        x: i32,
        y: i32,
    }

    /// Round-trips and length checks cannot tell a correct DHEADER from one that
    /// forgot to count the 4-byte element count, so pin the value itself. Per
    /// DDS-XTypes 7.4.3.5.4 the sequence DHEADER spans length + elements.
    #[test]
    fn sequence_of_enum_dheader_counts_length_and_elements() {
        let value = vec![Color::Red, Color::Blue];

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x00, 0x07, 0x00, 0x00,
            0x0C, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
        ];
        assert_eq!(encode_xcdr2(&value), expected);

        let mut deserializer = XcdrDeserializer::new(&expected).unwrap();
        assert_eq!(Vec::<Color>::deserialize_xcdr(&mut deserializer).unwrap(), value);
    }

    #[test]
    fn sequence_of_struct_dheader_counts_length_and_elements() {
        let value = vec![Pt { x: 1, y: 2 }, Pt { x: 3, y: 4 }];

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x00, 0x07, 0x00, 0x00,
            0x14, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00,
            0x03, 0x00, 0x00, 0x00,
            0x04, 0x00, 0x00, 0x00,
        ];
        assert_eq!(encode_xcdr2(&value), expected);

        let mut deserializer = XcdrDeserializer::new(&expected).unwrap();
        assert_eq!(Vec::<Pt>::deserialize_xcdr(&mut deserializer).unwrap(), value);
    }

    /// `long a[2][3]` parses to a nested `[[i32; 3]; 2]`, but XTypes sees one array of a
    /// primitive base type: the wire stays flat, with no DHEADER on either dimension.
    #[test]
    fn multidim_primitive_array_stays_flat() {
        let value: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]];
        assert_eq!(encode_xcdr2(&value).len(), 4 + 24);
    }

    /// The same nesting with a non-primitive base type is still one array, so it carries
    /// exactly one DHEADER spanning every element -- not one per dimension.
    #[test]
    fn multidim_struct_array_frames_once() {
        let value: [[Pt; 1]; 2] = [[Pt { x: 1, y: 2 }], [Pt { x: 3, y: 4 }]];

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x00, 0x07, 0x00, 0x00,
            0x10, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00,
            0x03, 0x00, 0x00, 0x00,
            0x04, 0x00, 0x00, 0x00,
        ];
        assert_eq!(encode_xcdr2(&value), expected);

        let mut deserializer = XcdrDeserializer::new(&expected).unwrap();
        assert_eq!(<[[Pt; 1]; 2]>::deserialize_xcdr(&mut deserializer).unwrap(), value);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate", extensibility = "Appendable")]
    struct APt {
        x: i32,
        y: i32,
    }

    /// The array DHEADER spans the elements including each element's own framing. These
    /// are the bytes the Python and C# generators emit for the same `Pt row[2]` member,
    /// so they pin cross-language agreement, not just self-consistency.
    #[test]
    fn array_of_appendable_struct_frames_element_framing_too() {
        let value: [APt; 2] = [APt { x: 1, y: 2 }, APt { x: 3, y: 4 }];

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x00, 0x07, 0x00, 0x00,
            0x18, 0x00, 0x00, 0x00,
            0x08, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00,
            0x08, 0x00, 0x00, 0x00,
            0x03, 0x00, 0x00, 0x00,
            0x04, 0x00, 0x00, 0x00,
        ];
        assert_eq!(encode_xcdr2(&value), expected);

        let mut deserializer = XcdrDeserializer::new(&expected).unwrap();
        assert_eq!(<[APt; 2]>::deserialize_xcdr(&mut deserializer).unwrap(), value);
    }

    /// An array is not a primitive type, so a sequence of arrays is framed even when the
    /// arrays themselves are not (DDS-XTypes 7.4.3.5.4: PSEQUENCE is primitive elements only).
    #[test]
    fn sequence_of_primitive_arrays_keeps_dheader() {
        let value: Vec<[i32; 2]> = vec![[1, 2], [3, 4]];

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x00, 0x07, 0x00, 0x00,
            0x14, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00,
            0x03, 0x00, 0x00, 0x00,
            0x04, 0x00, 0x00, 0x00,
        ];
        assert_eq!(encode_xcdr2(&value), expected);

        let mut deserializer = XcdrDeserializer::new(&expected).unwrap();
        assert_eq!(Vec::<[i32; 2]>::deserialize_xcdr(&mut deserializer).unwrap(), value);
    }
}
