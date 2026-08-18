#[allow(unused_imports)]
mod cdr_array_tests {
    use int2dds::{
        dcps::topic::type_support::{DdsType, FieldAccessor},
        serialize::{
            cdr::{
                CdrDeserialize, CdrDeserializer, CdrSerialize, CdrSerializer, ExtensibilityKind,
                XcdrDeserialize, XcdrDeserializer, XcdrSerialize, XcdrSerializer,
            },
            BufferManager, DeserializerReader, WChar, WString,
        },
    };
    use std::collections::HashMap;
    #[test]
    fn test_cdr_array_i32() {
        let value: [i32; 5] = [1, 2, 3, 4, 5];

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = <[i32; 5]>::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_cdr_multidim_array_row_major() {
        let value: [[u16; 3]; 2] = [[1, 2, 3], [4, 5, 6]];

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let expected: &[u8] =
            &[0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00];
        assert_eq!(&bytes[4..], expected);

        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = <[[u16; 3]; 2]>::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_multidim_array_row_major() {
        let value: [[u16; 3]; 2] = [[1, 2, 3], [4, 5, 6]];

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let expected: &[u8] =
            &[0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00];
        assert_eq!(&bytes[4..], expected);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = <[[u16; 3]; 2]>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_3d_array_row_major() {
        let value: [[[u8; 2]; 3]; 2] = [[[1, 2], [3, 4], [5, 6]], [[7, 8], [9, 10], [11, 12]]];

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = <[[[u8; 2]; 3]; 2]>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);

        let payload = &bytes[4..];
        let expected: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        assert_eq!(payload, expected);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct StringArrayHolder {
        names: [String; 2],
    }

    fn string_array_value() -> StringArrayHolder {
        StringArrayHolder { names: ["ab".to_string(), "cd".to_string()] }
    }

    /// `string a[2]` has non-primitive elements, so XCDR2 frames it with a DHEADER of the
    /// element payload size and **no** element count (DDS-XTypes 7.4.3.5.3). A round-trip
    /// test cannot see this: assert the exact bytes.
    #[test]
    fn test_xcdr2_string_array_dheader_without_count() {
        let value = string_array_value();

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let expected: &[u8] = &[
            0x0F, 0x00, 0x00, 0x00, // DHEADER = 15 element payload bytes, no count follows
            0x03, 0x00, 0x00, 0x00, b'a', b'b', 0x00, // "ab"
            0x00, // pad to 4
            0x03, 0x00, 0x00, 0x00, b'c', b'd', 0x00, // "cd"
        ];
        assert_eq!(&bytes[4..], expected);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = StringArrayHolder::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.names, value.names);
    }

    /// The derive-routed `serialize_string_array` path and the generic `[T; N]` impl must
    /// agree byte for byte; they framed differently before.
    #[test]
    fn test_xcdr2_string_array_matches_generic_impl() {
        let value = string_array_value();

        let mut derived = XcdrSerializer::new(true, ExtensibilityKind::Final);
        value.serialize_xcdr(&mut derived).unwrap();

        let mut generic = XcdrSerializer::new(true, ExtensibilityKind::Final);
        value.names.serialize_xcdr(&mut generic).unwrap();

        assert_eq!(derived.into_bytes(), generic.into_bytes());
    }

    /// `serialize_string_array` / `deserialize_string_array` are the derive macro's per-field
    /// helpers — `deserialize_string_array` is what the XCDR2 interop fallback decoder uses.
    /// They must frame the array exactly like the generic `[T; N]` impls above.
    #[test]
    fn test_xcdr2_string_array_helpers_frame_like_generic_impl() {
        use int2dds::serialize::cdr::ArraySerialize;

        let names = ["ab".to_string(), "cd".to_string()];

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        serializer.serialize_string_array(&names).unwrap();

        let bytes = serializer.into_bytes();
        let expected: &[u8] = &[
            0x0F, 0x00, 0x00, 0x00, // DHEADER = 15 element payload bytes, no count follows
            0x03, 0x00, 0x00, 0x00, b'a', b'b', 0x00, // "ab"
            0x00, // pad to 4
            0x03, 0x00, 0x00, 0x00, b'c', b'd', 0x00, // "cd"
        ];
        assert_eq!(&bytes[4..], expected);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        assert_eq!(deserializer.deserialize_string_array(2).unwrap(), names);
    }

    /// The same helpers under XCDR1, where no collection frames.
    #[test]
    fn test_cdr_string_array_helpers_have_no_dheader() {
        use int2dds::serialize::cdr::ArraySerialize;

        let names = ["ab".to_string(), "cd".to_string()];

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        serializer.serialize_string_array(&names).unwrap();

        let bytes = serializer.into_bytes();
        let expected: &[u8] = &[
            0x03, 0x00, 0x00, 0x00, b'a', b'b', 0x00, //
            0x00, //
            0x03, 0x00, 0x00, 0x00, b'c', b'd', 0x00,
        ];
        assert_eq!(&bytes[4..], expected);

        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        assert_eq!(deserializer.deserialize_string_array(2).unwrap(), names);
    }

    /// `TypeSupport::deserialize` reads fields through `deserialize_string_array`, while
    /// `serialize` goes through the generic `[T; N]` impl. The two framed the array
    /// differently, so this is the pair that actually breaks.
    #[test]
    fn test_xcdr2_string_array_type_support_round_trip() {
        use int2dds::dcps::topic::type_support::TypeSupport;

        let value = string_array_value();
        let ts = StringArrayHolder::get_type_support();
        let fmt = xcdr2_format(ExtensibilityKind::Final);

        let serialized = ts.serialize(&value, Some(&fmt)).unwrap();
        let expected: &[u8] = &[
            0x0F, 0x00, 0x00, 0x00, // DHEADER = 15 element payload bytes, no count follows
            0x03, 0x00, 0x00, 0x00, b'a', b'b', 0x00, // "ab"
            0x00, // pad to 4
            0x03, 0x00, 0x00, 0x00, b'c', b'd', 0x00, // "cd"
        ];
        assert_eq!(&serialized[4..], expected);

        let got = ts.deserialize(&serialized, Some(&fmt)).unwrap();
        let got = got.downcast_ref::<StringArrayHolder>().unwrap();
        assert_eq!(got.names, value.names);
    }

    /// Same pairing under XCDR1, where neither side frames.
    #[test]
    fn test_cdr_string_array_type_support_round_trip() {
        use int2dds::dcps::topic::type_support::TypeSupport;

        let value = string_array_value();
        let ts = StringArrayHolder::get_type_support();
        let fmt = xcdr1_format();

        let serialized = ts.serialize(&value, Some(&fmt)).unwrap();
        let expected: &[u8] = &[
            0x03, 0x00, 0x00, 0x00, b'a', b'b', 0x00, //
            0x00, //
            0x03, 0x00, 0x00, 0x00, b'c', b'd', 0x00,
        ];
        assert_eq!(&serialized[4..], expected);

        let got = ts.deserialize(&serialized, Some(&fmt)).unwrap();
        let got = got.downcast_ref::<StringArrayHolder>().unwrap();
        assert_eq!(got.names, value.names);
    }

    /// XCDR1 has no DHEADER for any collection.
    #[test]
    fn test_cdr_string_array_has_no_dheader() {
        let value = string_array_value();

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let expected: &[u8] = &[
            0x03, 0x00, 0x00, 0x00, b'a', b'b', 0x00, //
            0x00, //
            0x03, 0x00, 0x00, 0x00, b'c', b'd', 0x00,
        ];
        assert_eq!(&bytes[4..], expected);

        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = StringArrayHolder::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.names, value.names);
    }

    // HashMap Tests - CDR

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct MatrixFinal {
        a: u32,
        b: u32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct MatrixAppendable {
        a: u32,
        b: u32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MatrixMutable {
        a: u32,
        b: u32,
    }

    fn xcdr1_format() -> int2dds::dcps::topic::type_support::SerializationFormat {
        int2dds::dcps::topic::type_support::SerializationFormat::Cdr
    }

    fn xcdr2_format(
        ext: ExtensibilityKind,
    ) -> int2dds::dcps::topic::type_support::SerializationFormat {
        int2dds::dcps::topic::type_support::SerializationFormat::Xcdr {
            extensibility_kind: ext,
            use_delimiters: !matches!(ext, ExtensibilityKind::Final),
        }
    }

    macro_rules! matrix_case {
        ($name:ident, $ty:ident, $field_setter:expr, $field_getter:expr, $format:expr, $encap:expr) => {
            #[test]
            fn $name() {
                use int2dds::dcps::topic::type_support::TypeSupport;

                let value: $ty = $field_setter;
                let ts = <$ty>::get_type_support();
                let fmt = $format;

                // serialize → bytes
                let serialized = ts.serialize(&value, Some(&fmt)).unwrap();
                assert_eq!(
                    &serialized[0..2],
                    &$encap,
                    concat!(stringify!($name), ": serialize() encap mismatch"),
                );
                let got_a = ts.deserialize(&serialized, Some(&fmt)).unwrap();
                let got_a = got_a.downcast_ref::<$ty>().unwrap();
                $field_getter(got_a, &value);

                // serialize_into → buffer
                let mut buf = Vec::new();
                ts.serialize_into(&value, &mut buf, Some(&fmt)).unwrap();
                assert_eq!(
                    &buf[0..2],
                    &$encap,
                    concat!(stringify!($name), ": serialize_into() encap mismatch"),
                );
                assert_eq!(
                    &buf[..],
                    &serialized[..],
                    concat!(stringify!($name), ": serialize_into() differs from serialize()"),
                );
                let got_b = ts.deserialize(&buf, Some(&fmt)).unwrap();
                let got_b = got_b.downcast_ref::<$ty>().unwrap();
                $field_getter(got_b, &value);
            }
        };
    }

    fn matrix_check(got: &MatrixFinal, want: &MatrixFinal) {
        assert_eq!(got.a, want.a);
        assert_eq!(got.b, want.b);
    }
    fn matrix_check_a(got: &MatrixAppendable, want: &MatrixAppendable) {
        assert_eq!(got.a, want.a);
        assert_eq!(got.b, want.b);
    }
    fn matrix_check_m(got: &MatrixMutable, want: &MatrixMutable) {
        assert_eq!(got.a, want.a);
        assert_eq!(got.b, want.b);
    }

    matrix_case!(
        matrix_xcdr1_final,
        MatrixFinal,
        MatrixFinal { a: 0x1111_1111, b: 0x2222_2222 },
        matrix_check,
        xcdr1_format(),
        [0x00, 0x01]
    );
    matrix_case!(
        matrix_xcdr1_appendable,
        MatrixAppendable,
        MatrixAppendable { a: 0x3333_3333, b: 0x4444_4444 },
        matrix_check_a,
        xcdr1_format(),
        [0x00, 0x01]
    );
    matrix_case!(
        matrix_xcdr1_mutable,
        MatrixMutable,
        MatrixMutable { a: 0x5555_5555, b: 0x6666_6666 },
        matrix_check_m,
        xcdr1_format(),
        [0x00, 0x03]
    );
    matrix_case!(
        matrix_xcdr2_final,
        MatrixFinal,
        MatrixFinal { a: 0x7777_7777, b: 0x8888_8888 },
        matrix_check,
        xcdr2_format(ExtensibilityKind::Final),
        [0x00, 0x07]
    );
    matrix_case!(
        matrix_xcdr2_appendable,
        MatrixAppendable,
        MatrixAppendable { a: 0x9999_9999, b: 0xAAAA_AAAA },
        matrix_check_a,
        xcdr2_format(ExtensibilityKind::Appendable),
        [0x00, 0x09]
    );
    matrix_case!(
        matrix_xcdr2_mutable,
        MatrixMutable,
        MatrixMutable { a: 0xBBBB_BBBB, b: 0xCCCC_CCCC },
        matrix_check_m,
        xcdr2_format(ExtensibilityKind::Mutable),
        [0x00, 0x0B]
    );
}
