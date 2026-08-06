mod array;
mod primitive;
mod sequence;
mod string;

#[cfg(test)]
#[allow(unused_imports)]
mod cdr_error_tests {
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
    fn test_cdr_insufficient_data() {
        // Empty data should fail
        let result = CdrDeserializer::new(&[]);
        assert!(result.is_err());

        // Only 2 bytes (incomplete header)
        let result = CdrDeserializer::new(&[0x00, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cdr_invalid_encapsulation() {
        // Invalid encapsulation identifier
        let data = [0xFF, 0xFF, 0x00, 0x00];
        let result = CdrDeserializer::new(&data);
        assert!(result.is_err());
    }

    // Tuple Struct Tests
}
