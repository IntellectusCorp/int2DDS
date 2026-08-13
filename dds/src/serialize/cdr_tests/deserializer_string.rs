#[cfg(test)]
#[allow(unused_imports)]
mod try_construct_string_tests {
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
    fn encode_cdr<T: CdrSerialize>(value: &T) -> Vec<u8> {
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();
        serializer.into_bytes()
    }
    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcWireString {
        pub s: String,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcStringTrim {
        #[dds(bound = 5, try_construct = "trim")]
        pub s: String,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcStringUseDefault {
        #[dds(bound = 5, try_construct = "use_default")]
        pub s: String,
    }
    #[test]
    fn test_try_construct_string_trim() {
        let bytes = encode_cdr(&TcWireString { s: "Hello World!".to_string() });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcStringTrim::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.s, "Hello");
    }

    #[test]
    fn test_try_construct_string_use_default() {
        let bytes = encode_cdr(&TcWireString { s: "Hello World!".to_string() });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcStringUseDefault::deserialize_cdr(&mut deserializer).unwrap();
        assert!(result.s.is_empty());
    }
}
