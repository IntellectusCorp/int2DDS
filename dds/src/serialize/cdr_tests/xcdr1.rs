#[cfg(test)]
#[allow(unused_imports)]
mod xcdr1_tests {
    use crate::serialize::cdr::xcdr1::PID_SENTINEL;
    use crate::serialize::cdr::MemberHeader;
    use crate::{
        dcps::topic::type_support::{DdsType, FieldAccessor},
        serialize::{
            cdr::{
                CdrDeserialize, CdrDeserializer, CdrSerialize, CdrSerializer, ExtensibilityKind,
                XcdrDeserialize, XcdrDeserializer, XcdrSerialize, XcdrSerializer,
            },
            BufferManager, DeserializerReader, WChar, WString,
        },
    };
    use speedy::Endianness;
    use std::collections::HashMap;
    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableV1Simple {
        pub x: u32,
        pub y: u16,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableV1Padded {
        pub a: u16,
        pub b: u32,
    }

    /// `a` is two bytes, so the next member header needs two bytes of padding. XTypes
    /// counts only the member itself in the parameter length; RTI's default compliance
    /// mask counts the padding too (`parameter_length_with_padding`, documented by RTI
    /// as non-compliant). Both have to decode.
    fn padded_member_wire(declared_len_of_a: u16) -> Vec<u8> {
        padded_member_wire_with(declared_len_of_a, [0x00, 0x00])
    }

    fn padded_member_wire_with(declared_len_of_a: u16, padding: [u8; 2]) -> Vec<u8> {
        let mut bytes = vec![0x00, 0x03, 0x00, 0x00];
        bytes.extend_from_slice(&0x0000u16.to_le_bytes());
        bytes.extend_from_slice(&declared_len_of_a.to_le_bytes());
        bytes.extend_from_slice(&0xBEEFu16.to_le_bytes());
        bytes.extend_from_slice(&padding);
        bytes.extend_from_slice(&0x0001u16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&0x11223344u32.to_le_bytes());
        bytes.extend_from_slice(&PID_SENTINEL.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes
    }

    #[test]
    fn test_xcdr1_reads_both_parameter_length_conventions() {
        for declared_len in [2u16, 4u16] {
            let bytes = padded_member_wire(declared_len);
            let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
            let result = MutableV1Padded::deserialize_cdr(&mut deserializer).unwrap();
            assert_eq!(result.a, 0xBEEF, "declared_len {}", declared_len);
            assert_eq!(result.b, 0x11223344, "declared_len {}", declared_len);
        }
    }

    /// The spec does not constrain the content of inter-member padding, so a peer that
    /// leaves it uninitialised can put sentinel-looking bytes there. Only the aligned
    /// header position decides where the member list ends.
    #[test]
    fn test_xcdr1_sentinel_lookalike_padding_is_not_end_of_struct() {
        let bytes = padded_member_wire_with(2, PID_SENTINEL.to_le_bytes());
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = MutableV1Padded::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.a, 0xBEEF);
        assert_eq!(result.b, 0x11223344);
    }

    #[test]
    fn test_xcdr1_parameter_length_excludes_trailing_padding() {
        let value = MutableV1Padded { a: 0xBEEF, b: 0x11223344 };

        let mut serializer = CdrSerializer::new_mutable(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let payload = &serializer.into_bytes()[4..];
        assert_eq!(u16::from_le_bytes([payload[0], payload[1]]) & 0x3FFF, 0);
        assert_eq!(u16::from_le_bytes([payload[2], payload[3]]), 2);
        assert_eq!(u16::from_le_bytes([payload[8], payload[9]]) & 0x3FFF, 1);
        assert_eq!(payload, &padded_member_wire(2)[4..]);
    }

    #[test]
    fn test_xcdr1_mutable_roundtrip() {
        let value = MutableV1Simple { x: 42, y: 7 };

        let mut serializer = CdrSerializer::new_mutable(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x03);

        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = MutableV1Simple::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.x, 42);
        assert_eq!(result.y, 7);
    }

    #[test]
    fn test_xcdr1_mutable_wire_format() {
        let value = MutableV1Simple { x: 0x12345678, y: 0xABCD };

        let mut serializer = CdrSerializer::new_mutable(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let payload = &bytes[4..];

        let pid0 = u16::from_le_bytes([payload[0], payload[1]]);
        let len0 = u16::from_le_bytes([payload[2], payload[3]]);
        assert_eq!(pid0 & 0x3FFF, 0);
        assert_eq!(len0, 4);
        assert_eq!(
            u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]),
            0x12345678
        );

        let pid1 = u16::from_le_bytes([payload[8], payload[9]]);
        let len1 = u16::from_le_bytes([payload[10], payload[11]]);
        assert_eq!(pid1 & 0x3FFF, 1);
        assert_eq!(len1, 2);
        assert_eq!(u16::from_le_bytes([payload[12], payload[13]]), 0xABCD);

        let sentinel_offset = 16;
        let sentinel_pid =
            u16::from_le_bytes([payload[sentinel_offset], payload[sentinel_offset + 1]]);
        assert_eq!(sentinel_pid & 0x3FFF, 0x3F02 & 0x3FFF);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableV1WithOptional {
        pub required_val: u32,
        #[dds(optional)]
        pub optional_val: Option<u16>,
    }

    #[test]
    fn test_xcdr1_mutable_optional_present() {
        let value = MutableV1WithOptional { required_val: 10, optional_val: Some(20) };

        let mut serializer = CdrSerializer::new_mutable(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = MutableV1WithOptional::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.required_val, 10);
        assert_eq!(result.optional_val, Some(20));
    }

    #[test]
    fn test_xcdr1_mutable_optional_absent() {
        let value = MutableV1WithOptional { required_val: 99, optional_val: None };

        let mut serializer = CdrSerializer::new_mutable(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = MutableV1WithOptional::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.required_val, 99);
        assert_eq!(result.optional_val, None);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableV1WithId {
        #[dds(id = 100)]
        pub a: u32,
        #[dds(id = 200)]
        pub b: u64,
    }

    #[test]
    fn test_xcdr1_mutable_explicit_ids() {
        let value = MutableV1WithId { a: 1, b: 2 };

        let mut serializer = CdrSerializer::new_mutable(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        let payload = &bytes[4..];
        let pid0 = u16::from_le_bytes([payload[0], payload[1]]);
        assert_eq!(pid0 & 0x3FFF, 100);
        let pid1_offset = 4 + 4;
        let pid1 = u16::from_le_bytes([payload[pid1_offset], payload[pid1_offset + 1]]);
        assert_eq!(pid1 & 0x3FFF, 200);

        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = MutableV1WithId::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.a, 1);
        assert_eq!(result.b, 2);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct FinalV1WithOptional {
        pub required_val: u32,
        #[dds(optional)]
        pub optional_val: Option<u16>,
    }

    #[test]
    fn test_xcdr1_final_optional_present_roundtrip() {
        let value = FinalV1WithOptional { required_val: 0x12345678, optional_val: Some(0xABCD) };

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = FinalV1WithOptional::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.required_val, 0x12345678);
        assert_eq!(result.optional_val, Some(0xABCD));
    }

    #[test]
    fn test_xcdr1_final_optional_absent_roundtrip() {
        let value = FinalV1WithOptional { required_val: 0x12345678, optional_val: None };

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = FinalV1WithOptional::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.required_val, 0x12345678);
        assert_eq!(result.optional_val, None);
    }

    #[test]
    fn test_xcdr1_final_optional_wire_format_present() {
        let value = FinalV1WithOptional { required_val: 0x12345678, optional_val: Some(0xABCD) };

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        // Encap: CDR_LE = 0x00 0x01
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x01);
        // required_val (positional, no header)
        assert_eq!(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 0x12345678);
        // ShortMemberHeader for optional_val (member_id=1, length=2)
        let pid = u16::from_le_bytes([bytes[8], bytes[9]]);
        let len = u16::from_le_bytes([bytes[10], bytes[11]]);
        assert_eq!(pid & 0x3FFF, 1);
        assert_eq!(len, 2);
        // Inner payload (u16 LE 0xABCD)
        assert_eq!(u16::from_le_bytes([bytes[12], bytes[13]]), 0xABCD);
        // No sentinel for Final
        assert_eq!(bytes.len(), 14);
    }

    #[test]
    fn test_xcdr1_final_optional_wire_format_absent() {
        let value = FinalV1WithOptional { required_val: 0x12345678, optional_val: None };

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        // ShortMemberHeader for optional_val with length=0
        let pid = u16::from_le_bytes([bytes[8], bytes[9]]);
        let len = u16::from_le_bytes([bytes[10], bytes[11]]);
        assert_eq!(pid & 0x3FFF, 1);
        assert_eq!(len, 0);
        assert_eq!(bytes.len(), 12);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct AppendableV1WithOptional {
        pub required_val: u32,
        #[dds(optional)]
        pub optional_val: Option<u32>,
    }

    #[test]
    fn test_xcdr1_appendable_optional_roundtrip() {
        let value =
            AppendableV1WithOptional { required_val: 0xDEADBEEF, optional_val: Some(0xCAFEBABE) };

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = AppendableV1WithOptional::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.required_val, 0xDEADBEEF);
        assert_eq!(result.optional_val, Some(0xCAFEBABE));

        // Absent case
        let value = AppendableV1WithOptional { required_val: 1, optional_val: None };
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = AppendableV1WithOptional::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.required_val, 1);
        assert_eq!(result.optional_val, None);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct FinalV1MixedOptional {
        pub before: u32,
        #[dds(optional)]
        pub middle: Option<i32>,
        pub after: u16,
    }

    #[test]
    fn test_xcdr1_final_optional_mixed_roundtrip() {
        let value = FinalV1MixedOptional { before: 11, middle: Some(-22), after: 33 };
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = FinalV1MixedOptional::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.before, 11);
        assert_eq!(result.middle, Some(-22));
        assert_eq!(result.after, 33);

        let value = FinalV1MixedOptional { before: 11, middle: None, after: 33 };
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = FinalV1MixedOptional::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.before, 11);
        assert_eq!(result.middle, None);
        assert_eq!(result.after, 33);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct FinalV1OptionalString {
        pub leading: u32,
        #[dds(optional)]
        pub text: Option<String>,
        pub trailing: u32,
    }

    #[test]
    fn test_xcdr1_final_optional_string_roundtrip() {
        let value =
            FinalV1OptionalString { leading: 7, text: Some("hello".to_string()), trailing: 9 };
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = FinalV1OptionalString::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.leading, 7);
        assert_eq!(result.text.as_deref(), Some("hello"));
        assert_eq!(result.trailing, 9);

        let value = FinalV1OptionalString { leading: 7, text: None, trailing: 9 };
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = FinalV1OptionalString::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.leading, 7);
        assert_eq!(result.text, None);
        assert_eq!(result.trailing, 9);
    }

    // ============================================================================
    // Bitmask Tests
    // ============================================================================

    #[allow(non_upper_case_globals)]
    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct U64Mutable {
        a: u64,
        b: u64,
    }

    #[test]
    fn xcdr1_mutable_encap_header_is_pl_cdr_le() {
        use crate::dcps::topic::type_support::{SerializationFormat, TypeSupport};

        let value = U64Mutable { a: 1, b: 2 };
        let ts = U64Mutable::get_type_support();
        let bytes = ts.serialize(&value, Some(&SerializationFormat::Cdr)).unwrap();
        assert_eq!(
            &bytes[0..2],
            &[0x00, 0x03],
            "XCDR1 + Mutable must emit PL_CDR LE encap (0x0003)"
        );
    }

    #[test]
    fn xcdr1_mutable_serialize_round_trip() {
        use crate::dcps::topic::type_support::{SerializationFormat, TypeSupport};

        let value = U64Mutable { a: 0xAAAA, b: 0xBBBB };
        let ts = U64Mutable::get_type_support();
        let bytes = ts.serialize(&value, Some(&SerializationFormat::Cdr)).unwrap();
        let got = ts.deserialize(&bytes, Some(&SerializationFormat::Cdr)).unwrap();
        let got = got.downcast_ref::<U64Mutable>().unwrap();
        assert_eq!(got.a, value.a);
        assert_eq!(got.b, value.b);
    }

    #[test]
    fn xcdr1_mutable_serialize_into_round_trip() {
        use crate::dcps::topic::type_support::{SerializationFormat, TypeSupport};

        let value = U64Mutable { a: 0xCCCC, b: 0xDDDD };
        let ts = U64Mutable::get_type_support();
        let mut buf = Vec::new();
        ts.serialize_into(&value, &mut buf, Some(&SerializationFormat::Cdr)).unwrap();
        assert_eq!(
            &buf[0..2],
            &[0x00, 0x03],
            "XCDR1 + Mutable serialize_into must emit PL_CDR LE encap"
        );
        let got = ts.deserialize(&buf, Some(&SerializationFormat::Cdr)).unwrap();
        let got = got.downcast_ref::<U64Mutable>().unwrap();
        assert_eq!(got.a, value.a);
        assert_eq!(got.b, value.b);
    }

    #[test]
    fn xcdr1_mutable_serialize_into_matches_serialize() {
        use crate::dcps::topic::type_support::{SerializationFormat, TypeSupport};

        let value = U64Mutable { a: 7, b: 8 };
        let ts = U64Mutable::get_type_support();
        let serialized = ts.serialize(&value, Some(&SerializationFormat::Cdr)).unwrap();
        let mut buf = Vec::new();
        ts.serialize_into(&value, &mut buf, Some(&SerializationFormat::Cdr)).unwrap();
        assert_eq!(&buf[..], &serialized[..]);
    }
}
