#[cfg(test)]
#[allow(unused_imports)]
mod cdr_string_tests {
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
    use std::collections::HashMap;
    #[test]
    fn test_cdr_string() {
        // empty string
        let value = String::new();
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = String::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);

        // ascii string
        let value = "Hello, World!".to_string();
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = String::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    // Vec (Sequence) Tests - CDR

    #[test]
    fn test_cdr_wstring() {
        let value = WString::from("Hello, World!");

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = WString::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.as_str(), "Hello, World!");
    }

    // Endianness Tests - CDR

    #[test]
    fn test_xcdr2_string() {
        let value = "Hello, XCDR2!".to_string();

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = String::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    // XCDR2 Extensibility Tests

    #[derive(DdsType)]
    struct BoundedWStringStruct {
        #[dds(bound = 5)]
        pub ws: WString,
    }

    #[test]
    fn test_bounded_wstring_within_bound() {
        let value = BoundedWStringStruct { ws: WString::from("Hello") };
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        CdrSerialize::serialize_cdr(&value, &mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = BoundedWStringStruct::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.ws.as_str(), "Hello");
    }

    // ============================================================================
    // @external (Box<T>) Tests
    // ============================================================================
}
