pub(crate) mod read;

use std::borrow::Cow;

use crate::cdr::{CdrDeserializer, CdrError, Xcdr2Deserializer};
use read::{forward_cdr_read, CdrRead};

impl<'a> CdrRead for CdrDeserializer<'a> {
    fn bytes_at(&self, offset: usize, len: usize) -> Cow<'_, [u8]> {
        self.input.bytes(offset, len)
    }
}

impl<'a> CdrRead for Xcdr2Deserializer<'a> {
    fn bytes_at(&self, offset: usize, len: usize) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.data[offset..offset + len])
    }

    fn read_collection_dheader(&mut self) -> Result<(), CdrError> {
        self.read_dheader()?;
        Ok(())
    }
}

forward_cdr_read!(CdrDeserializer);
forward_cdr_read!(Xcdr2Deserializer);

#[cfg(test)]
#[allow(unused_imports)]
mod cdr_error_tests {
    use crate::{
        cdr::{
            CdrDeserialize, CdrDeserializer, CdrSerialize, CdrSerializer, ExtensibilityKind,
            XcdrDeserialize, XcdrDeserializer, XcdrSerialize, XcdrSerializer,
        },
        BufferManager, DeserializerReader, WChar, WString,
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

#[cfg(test)]
mod robustness_tests {
    use crate::cdr::{CdrDeserializer, Xcdr2Deserializer};

    // 4-byte LE length prefix of 0xFFFFFFFF plus a near-empty body: the declared
    // element count cannot fit, so deserialization must Err rather than OOM.
    const HUGE_LEN: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00];

    #[test]
    fn cdr_sequences_reject_huge_length() {
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u16_sequence()
            .is_err());
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u32_sequence()
            .is_err());
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u64_sequence()
            .is_err());
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_f64_sequence()
            .is_err());
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_string_sequence()
            .is_err());
    }

    #[test]
    fn xcdr2_sequences_reject_huge_length() {
        assert!(Xcdr2Deserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u32_sequence()
            .is_err());
        assert!(Xcdr2Deserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u64_sequence()
            .is_err());
    }

    #[test]
    fn generic_sequence_with_huge_length_errs_without_oom() {
        let mut d = CdrDeserializer::new_without_header(&HUGE_LEN, true);
        let r: Result<Vec<u32>, _> = d.deserialize_sequence(|de| de.deserialize_u32());
        assert!(r.is_err());
    }
}
