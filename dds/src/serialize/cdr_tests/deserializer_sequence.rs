#[cfg(test)]
#[allow(unused_imports)]
mod try_construct_seq_tests {
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
    struct TcWireSeq {
        pub values: Vec<i32>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcSeqDiscard {
        #[dds(bound = 4)]
        pub values: Vec<i32>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcSeqUseDefault {
        #[dds(bound = 4, try_construct = "use_default")]
        pub values: Vec<i32>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcSeqTrim {
        #[dds(bound = 4, try_construct = "trim")]
        pub values: Vec<i32>,
    }
    #[test]
    fn test_try_construct_discard_is_error() {
        let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2, 3, 4, 5, 6, 7, 8] });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcSeqDiscard::deserialize_cdr(&mut deserializer);
        assert!(result.is_err());
    }

    #[test]
    fn test_try_construct_use_default_yields_empty_sequence() {
        let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2, 3, 4, 5, 6, 7, 8] });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcSeqUseDefault::deserialize_cdr(&mut deserializer).unwrap();
        assert!(result.values.is_empty());
    }

    #[test]
    fn test_try_construct_trim_truncates_sequence() {
        let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2, 3, 4, 5, 6, 7, 8] });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcSeqTrim::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.values, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_try_construct_trim_preserves_within_bound() {
        let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2] });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcSeqTrim::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.values, vec![1, 2]);
    }
}
