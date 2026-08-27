#[allow(unused_imports)]
mod xcdr2_tests {
    use int2dds::serialize::cdr::MemberHeader;
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
    use speedy::Endianness;
    use std::collections::HashMap;
    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct SimpleStruct {
        pub x: i32,
        pub y: i32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct AppendableStruct {
        pub id: u32,
        pub name: String,
        pub values: Vec<i32>,
    }

    #[test]
    fn test_xcdr2_extensibility_final() {
        let value: i32 = 42;

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        // PLAIN_CDR2_LE = 0x0007
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x07);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_extensibility_appendable() {
        let value: i32 = 42;

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Appendable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        // DCDR2_LE = 0x0009
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x09);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_extensibility_mutable() {
        let value: i32 = 42;

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        // PL_CDR2_LE = 0x000B
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x0B);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    // Complex Struct Tests with DdsType derive

    #[test]
    fn test_emheader_roundtrip_small_length() {
        for (length, expected_lc) in [(1usize, 0u8), (2, 1), (4, 2), (8, 3)] {
            let header = MemberHeader::new(42, length);
            let mut buffer = Vec::new();
            header.write(&mut buffer, Endianness::LittleEndian).unwrap();

            assert_eq!(buffer.len(), 4, "length={length} must encode as 4 bytes");

            let word = u32::from_le_bytes(buffer[..4].try_into().unwrap());
            assert_eq!((word >> 28) & 0x07, expected_lc as u32);
            assert_eq!(word & 0x0FFF_FFFF, 42);
            assert_eq!(word & 0x8000_0000, 0);

            let (read_header, bytes_consumed) =
                MemberHeader::read(&buffer, 0, Endianness::LittleEndian).unwrap();
            assert_eq!(bytes_consumed, 4);
            assert_eq!(read_header.member_id, 42);
            assert_eq!(read_header.member_length as usize, length);
            assert!(!read_header.must_understand);
        }
    }

    #[test]
    fn test_emheader_roundtrip_large_length() {
        // Test EMHEADER with large length (> 64KB, requires LC=4 extended header)
        let large_length: usize = 100_000; // 100KB
        let header = MemberHeader::new(123, large_length);

        let mut buffer = Vec::new();
        header.write(&mut buffer, Endianness::LittleEndian).unwrap();

        // Verify header is 8 bytes for large lengths (4 + 4 for extended length)
        assert_eq!(buffer.len(), 8);

        // Read it back
        let (read_header, bytes_consumed) =
            MemberHeader::read(&buffer, 0, Endianness::LittleEndian).unwrap();

        assert_eq!(bytes_consumed, 8);
        assert_eq!(read_header.member_id, 123);
        assert_eq!(read_header.member_length, large_length as u32);
    }

    #[test]
    fn test_emheader_must_understand_flag() {
        // Test must_understand flag (bit 31)
        let header = MemberHeader { member_id: 10, member_length: 50, must_understand: true };

        let mut buffer = Vec::new();
        header.write(&mut buffer, Endianness::LittleEndian).unwrap();

        // Read it back
        let (read_header, _) = MemberHeader::read(&buffer, 0, Endianness::LittleEndian).unwrap();

        assert_eq!(read_header.member_id, 10);
        assert_eq!(read_header.member_length, 50);
        assert!(read_header.must_understand);
    }

    #[test]
    fn test_emheader_big_endian() {
        // Test big-endian encoding/decoding
        let header = MemberHeader::new(255, 1000);

        let mut buffer = Vec::new();
        header.write(&mut buffer, Endianness::BigEndian).unwrap();

        let (read_header, _) = MemberHeader::read(&buffer, 0, Endianness::BigEndian).unwrap();

        assert_eq!(read_header.member_id, 255);
        assert_eq!(read_header.member_length, 1000);
    }

    // =============================================================================
    // Mutable Struct Tests
    // =============================================================================

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableStruct {
        #[dds(id = 1)]
        pub id: u32,
        #[dds(id = 2)]
        pub name: String,
        #[dds(id = 3)]
        pub value: f64,
    }

    #[test]
    fn test_mutable_struct_xcdr2() {
        let value = MutableStruct { id: 42, name: "test_mutable".to_string(), value: 3.14159 };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Verify encapsulation header is PL_CDR2_LE (0x000B)
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x0B);

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableStruct::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.id, 42);
        assert_eq!(result.name, "test_mutable");
        assert!((result.value - 3.14159).abs() < 1e-10);
    }

    #[test]
    fn test_mutable_struct_big_endian() {
        let value = MutableStruct { id: 100, name: "big_endian".to_string(), value: 2.71828 };

        // Serialize in big-endian
        let mut serializer = XcdrSerializer::new(false, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Verify encapsulation header is PL_CDR2_BE (0x000A)
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x0A);

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableStruct::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.id, 100);
        assert_eq!(result.name, "big_endian");
        assert!((result.value - 2.71828).abs() < 1e-10);
    }

    // =============================================================================
    // Mutable Struct with Nested Types
    // =============================================================================

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableNestedStruct {
        #[dds(id = 1)]
        pub header: SimpleStruct,
        #[dds(id = 2)]
        pub data: Vec<i32>,
        #[dds(id = 3)]
        pub count: u32,
    }

    #[test]
    fn test_mutable_nested_struct_xcdr2() {
        let value = MutableNestedStruct {
            header: SimpleStruct { x: 10, y: 20 },
            data: vec![1, 2, 3, 4, 5],
            count: 5,
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableNestedStruct::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.header.x, 10);
        assert_eq!(result.header.y, 20);
        assert_eq!(result.data, vec![1, 2, 3, 4, 5]);
        assert_eq!(result.count, 5);
    }

    // =============================================================================
    // Optional Field Tests (Mutable types)
    // =============================================================================

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableWithOptional {
        #[dds(id = 1)]
        pub required_field: u32,
        #[dds(id = 2, optional)]
        pub optional_string: Option<String>,
        #[dds(id = 3, optional)]
        pub optional_value: Option<f64>,
    }

    #[test]
    fn test_mutable_optional_all_present() {
        let value = MutableWithOptional {
            required_field: 123,
            optional_string: Some("hello".to_string()),
            optional_value: Some(99.9),
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithOptional::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.required_field, 123);
        assert_eq!(result.optional_string, Some("hello".to_string()));
        assert_eq!(result.optional_value, Some(99.9));
    }

    #[test]
    fn test_mutable_optional_none_values() {
        let value = MutableWithOptional {
            required_field: 456,
            optional_string: None,
            optional_value: None,
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithOptional::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.required_field, 456);
        assert_eq!(result.optional_string, None);
        assert_eq!(result.optional_value, None);
    }

    #[test]
    fn test_mutable_optional_mixed() {
        let value = MutableWithOptional {
            required_field: 789,
            optional_string: Some("partial".to_string()),
            optional_value: None,
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithOptional::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.required_field, 789);
        assert_eq!(result.optional_string, Some("partial".to_string()));
        assert_eq!(result.optional_value, None);
    }

    // =============================================================================
    // DHEADER Tests (Appendable/Mutable struct size header)
    // =============================================================================

    #[test]
    fn test_appendable_struct_dheader() {
        // Appendable structs use DHEADER for forward compatibility
        let value = AppendableStruct {
            id: 999,
            name: "dheader_test".to_string(),
            values: vec![10, 20, 30],
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Appendable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Verify encapsulation header is DCDR2_LE (0x0009)
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x09);

        // After encap header (4 bytes), there should be a DHEADER (4 bytes)
        // The DHEADER contains the object size
        let dheader_bytes = &bytes[4..8];
        let dheader_size = u32::from_le_bytes([
            dheader_bytes[0],
            dheader_bytes[1],
            dheader_bytes[2],
            dheader_bytes[3],
        ]);

        // Object size should match remaining data
        let object_data_len = bytes.len() - 8; // Total - encap header - dheader
        assert_eq!(dheader_size as usize, object_data_len);

        // Deserialize should still work
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = AppendableStruct::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result, value);
    }

    // ============================================================================
    // Bounded WString Tests
    // ============================================================================

    #[derive(DdsType)]
    #[dds_type(extensibility = "Mutable")]
    struct HashIdStruct {
        #[dds(hashid)]
        pub name: String,
        #[dds(hashid = "custom_field")]
        pub value: u32,
    }

    #[test]
    fn test_hashid_mutable_roundtrip() {
        let original = HashIdStruct { name: "test_name".to_string(), value: 42 };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        original.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = HashIdStruct::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.name, "test_name");
        assert_eq!(result.value, 42);
    }

    // ============================================================================
    // @autoid Tests
    // ============================================================================

    #[derive(DdsType)]
    #[dds_type(extensibility = "Mutable", autoid = "Hash")]
    struct AutoIdHashStruct {
        pub field_a: u32,
        pub field_b: String,
    }

    #[test]
    fn test_autoid_hash_mutable_roundtrip() {
        let original = AutoIdHashStruct { field_a: 123, field_b: "autoid_test".to_string() };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        original.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = AutoIdHashStruct::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.field_a, 123);
        assert_eq!(result.field_b, "autoid_test");
    }

    #[derive(DdsType)]
    #[dds_type(extensibility = "Mutable", autoid = "Sequential")]
    struct AutoIdSeqStruct {
        pub first: u32,
        pub second: f64,
    }

    #[test]
    fn test_autoid_sequential_mutable_roundtrip() {
        let original = AutoIdSeqStruct { first: 999, second: 1.5 };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        original.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = AutoIdSeqStruct::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.first, 999);
        assert_eq!(result.second, 1.5);
    }

    // ============================================================================
    // Struct Inheritance Tests
    // ============================================================================

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableWithSeqU32 {
        #[dds(id = 0)]
        pub values: Vec<u32>,
    }

    #[test]
    fn test_u32_sequence_emheader_auto_lc() {
        use int2dds::serialize::cdr::MemberHeader;

        let value = MutableWithSeqU32 { values: vec![1, 2, 3] };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let payload = &bytes[4..];

        let emheader_bytes = &payload[4..];
        let (header, _) =
            MemberHeader::read(emheader_bytes, 0, speedy::Endianness::LittleEndian).unwrap();

        assert_eq!(header.member_id, 0);
        assert_eq!(header.member_length, 16);

        let emh_word = u32::from_le_bytes([
            emheader_bytes[0],
            emheader_bytes[1],
            emheader_bytes[2],
            emheader_bytes[3],
        ]);
        let lc = (emh_word >> 28) & 0x07;
        assert_eq!(lc, 4);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithSeqU32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.values, vec![1, 2, 3]);
    }

    #[test]
    fn test_u32_sequence_emheader_compacts_to_lc3() {
        // A single-element Vec<u32> payload is 8 bytes (length + element), so the
        // Auto policy compacts to LC=3 with no NEXTINT.
        let value = MutableWithSeqU32 { values: vec![7] };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();

        // 4 encap + 4 struct DHEADER + 4 EMHEADER + 4 length + 4 element = 20
        assert_eq!(bytes.len(), 20);
        assert_eq!(u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]), 0x3000_0000);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithSeqU32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.values, vec![7]);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableWithSeqF64 {
        #[dds(id = 0)]
        pub data: Vec<f64>,
    }

    #[test]
    fn test_f64_sequence_emheader_auto_lc() {
        let value = MutableWithSeqF64 { data: vec![1.0, 2.0] };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let payload = &bytes[4..];

        let emheader_bytes = &payload[4..];
        let emh_word = u32::from_le_bytes([
            emheader_bytes[0],
            emheader_bytes[1],
            emheader_bytes[2],
            emheader_bytes[3],
        ]);
        let lc = (emh_word >> 28) & 0x07;
        assert_eq!(lc, 4);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithSeqF64::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.data, vec![1.0, 2.0]);
    }

    #[test]
    fn test_u32_sequence_wire_bytes_auto() {
        // Mutable Vec<u32> XCDR2 under the Auto LC policy: LC=4 owns its NEXTINT,
        // followed by the sequence's own length word.
        let value = MutableWithSeqU32 { values: vec![1, 2, 3] };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();

        // 4 encap + 4 struct DHEADER + 4 EMHEADER + 4 NEXTINT + 4 length + 3*4 elements = 32
        assert_eq!(bytes.len(), 32);
        // Encap header PL_CDR2_LE
        assert_eq!(&bytes[0..2], &[0x00, 0x0B]);
        // Struct DHEADER = content size after itself = 24
        assert_eq!(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 24);
        // EMHEADER: M=0, LC=4, ID=0 → 0x4000_0000
        assert_eq!(u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]), 0x4000_0000);
        // NEXTINT = member length = 16
        assert_eq!(u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]), 16);
        // Sequence length = 3
        assert_eq!(u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]), 3);
        // Elements
        assert_eq!(u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]), 1);
        assert_eq!(u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]), 2);
        assert_eq!(u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]), 3);

        // Round-trip
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithSeqU32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.values, vec![1, 2, 3]);
    }

    #[test]
    fn test_f64_sequence_wire_bytes_auto() {
        // Mutable Vec<f64> XCDR2 under the Auto LC policy: LC=4 owns its NEXTINT,
        // followed by the sequence's own length word.
        let value = MutableWithSeqF64 { data: vec![1.0, 2.0] };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();

        // 4 encap + 4 struct DHEADER + 4 EMHEADER + 4 NEXTINT + 4 length + 2*8 elements = 36
        assert_eq!(bytes.len(), 36);
        // Struct DHEADER content = 28
        assert_eq!(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 28);
        // EMHEADER: M=0, LC=4, ID=0 → 0x4000_0000
        assert_eq!(u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]), 0x4000_0000);
        // NEXTINT = member length = 20
        assert_eq!(u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]), 20);
        // Sequence length = 2
        assert_eq!(u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]), 2);
        // Elements (XCDR2 max alignment is 4, so f64 sits at offset 20)
        assert_eq!(
            f64::from_le_bytes([
                bytes[20], bytes[21], bytes[22], bytes[23], bytes[24], bytes[25], bytes[26],
                bytes[27],
            ]),
            1.0
        );
        assert_eq!(
            f64::from_le_bytes([
                bytes[28], bytes[29], bytes[30], bytes[31], bytes[32], bytes[33], bytes[34],
                bytes[35],
            ]),
            2.0
        );

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithSeqF64::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.data, vec![1.0, 2.0]);
    }

    #[test]
    fn test_xcdr2_hashmap_dheader_roundtrip() {
        // XCDR2 HashMap: DHEADER + length + pairs
        let mut value: HashMap<String, i32> = HashMap::new();
        value.insert("alpha".to_string(), 10);
        value.insert("beta".to_string(), 20);

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();

        // DHEADER must equal (total - encap header - dheader) = total - 8
        let dheader = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(dheader as usize, bytes.len() - 8);
        // Map length follows DHEADER
        let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        assert_eq!(len, 2);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = HashMap::<String, i32>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_btreemap_dheader_roundtrip() {
        use std::collections::BTreeMap;

        let mut value: BTreeMap<String, i32> = BTreeMap::new();
        value.insert("alpha".to_string(), 10);
        value.insert("beta".to_string(), 20);

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();

        let dheader = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(dheader as usize, bytes.len() - 8);
        let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        assert_eq!(len, 2);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = BTreeMap::<String, i32>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_primitive_map_omits_dheader() {
        use std::collections::BTreeMap;

        let mut hash: HashMap<i32, i32> = HashMap::new();
        hash.insert(1, 10);
        hash.insert(2, 20);
        let mut tree: BTreeMap<i32, i32> = BTreeMap::new();
        tree.insert(1, 10);
        tree.insert(2, 20);

        let serialize = |run: &dyn Fn(&mut XcdrSerializer)| {
            let mut s = XcdrSerializer::new(true, ExtensibilityKind::Final);
            s.write_encapsulation_header().unwrap();
            run(&mut s);
            s.into_bytes()
        };
        let hash_bytes = serialize(&|s| hash.serialize_xcdr(s).unwrap());
        let tree_bytes = serialize(&|s| tree.serialize_xcdr(s).unwrap());

        for bytes in [&hash_bytes, &tree_bytes] {
            let count = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            assert_eq!(count, 2, "primitive map must write count, not a DHEADER");
            assert_eq!(bytes.len(), 24, "primitive map must not contain a DHEADER slot");
        }

        let mut d = XcdrDeserializer::new(&hash_bytes).unwrap();
        assert_eq!(HashMap::<i32, i32>::deserialize_xcdr(&mut d).unwrap(), hash);
        let mut d = XcdrDeserializer::new(&tree_bytes).unwrap();
        assert_eq!(BTreeMap::<i32, i32>::deserialize_xcdr(&mut d).unwrap(), tree);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct ThreeU64Mutable {
        a: u64,
        b: u64,
        c: u64,
    }

    #[test]
    fn xcdr2_mutable_three_u64_serialize_into_round_trip() {
        use int2dds::dcps::topic::type_support::{SerializationFormat, TypeSupport};

        let value = ThreeU64Mutable {
            a: 0xAAAA_AAAA_AAAA_AAAA,
            b: 0xBBBB_BBBB_BBBB_BBBB,
            c: 0xCCCC_CCCC_CCCC_CCCC,
        };
        let format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Mutable,
            use_delimiters: true,
        };
        let ts = ThreeU64Mutable::get_type_support();

        let mut buf = Vec::new();
        ts.serialize_into(&value, &mut buf, Some(&format)).unwrap();

        let got = ts.deserialize(&buf, Some(&format)).unwrap();
        let got = got.downcast_ref::<ThreeU64Mutable>().unwrap();
        assert_eq!(*got, value);
    }

    #[test]
    fn xcdr2_mutable_three_u64_serialize_into_matches_serialize() {
        use int2dds::dcps::topic::type_support::{SerializationFormat, TypeSupport};

        let value = ThreeU64Mutable { a: 1, b: 2, c: 3 };
        let format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Mutable,
            use_delimiters: true,
        };
        let ts = ThreeU64Mutable::get_type_support();

        let serialized = ts.serialize(&value, Some(&format)).unwrap();
        let mut buf = Vec::new();
        ts.serialize_into(&value, &mut buf, Some(&format)).unwrap();

        assert_eq!(&buf[..], &serialized[..]);
    }
}
