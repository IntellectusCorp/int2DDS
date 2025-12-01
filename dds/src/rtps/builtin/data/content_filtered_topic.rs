//! Content-filtered topic support for RTPS.
//!
//! This module provides content filtering capabilities for topics, allowing
//! DataReaders to subscribe only to samples matching specific filter criteria.
//! Filters are expressed using SQL-like expressions.

use speedy::{Readable, Writable};

use crate::{
    rtps::common::parameters::{ParameterId, ParameterValue, PlCdrParameter},
    serialize::pl_cdr::PlCdrParser,
};

#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub struct ContentFilterProperty {
    pub content_filtered_topic_name: String, // string<256>
    pub related_topic_name: String,          // string<256>
    pub filter_class_name: String,           // string<256>
    pub filter_expression: String,
    pub expression_parameters: Vec<String>,
}

#[allow(dead_code)]
impl ContentFilterProperty {
    pub fn from_serialized_payload(
        payload: &[u8],
        is_big_endian: bool,
    ) -> Result<Option<Self>, String> {
        if payload.is_empty() {
            return Ok(None);
        }

        let parser = PlCdrParser::new(is_big_endian);
        let parameters = parser.parse(payload)?;

        // Find ContentFilterProperty parameter in the list
        Ok(parameters.iter().find_map(|parameter| {
            if parameter.id == ParameterId::PidContentFilterProperty {
                if let ParameterValue::ContentFilterProperty(property) = &parameter.value {
                    return Some(property.clone());
                }
            }
            None
        }))
    }

    pub fn to_serialized_payload(&self, little_endian: bool) -> Result<Vec<u8>, String> {
        use crate::serialize::pl_cdr::PlCdrSerializer;

        // Create parameter with ContentFilterProperty
        let parameter = PlCdrParameter {
            id: ParameterId::PidContentFilterProperty,
            value: ParameterValue::ContentFilterProperty(self.clone()),
        };

        // Serialize using PlCdrSerializer
        let serializer = PlCdrSerializer::new(little_endian);
        serializer.serialize_parameters(&[parameter])
    }
}

pub type FilterResult = Vec<i32>;
// FilterSignature is a 128-bit MD5 checksum (DDS-RTPS 2.5 spec 9.6.4.1)
// MD5 is 128bit = 16bytes, represented as [u8; 16]
// Previously [i32; 4] was used to represent 4 32-bit integers
// Now we use [u8; 16] for accurate byte array transmission over wire
pub type FilterSignature = [u8; 16];
pub type FilterSignatureSequence = Vec<FilterSignature>;

#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub struct ContentFilterInfo {
    pub filter_result: FilterResult,
    pub filter_signatures: FilterSignatureSequence,
}

#[allow(dead_code)]
impl ContentFilterInfo {
    /// Serialize ContentFilterInfo to inline QoS payload bytes
    pub fn to_inline_qos_payload(&self, little_endian: bool) -> Result<Vec<u8>, String> {
        use crate::serialize::pl_cdr::InlineQosSerializer;

        let serializer = InlineQosSerializer::new(!little_endian); // big_endian = !little_endian
        serializer.serialize_content_filter_info(self)
    }
}

/// Calculate filter signature (128-bit MD5 hash) from filter expression
pub(crate) fn calculate_filter_signature(filter_expression: &str) -> FilterSignature {
    let hash = ::md5::compute(filter_expression.as_bytes());
    hash.0
}

#[cfg(test)]
mod tests {

    use crate::serialize::pl_cdr::InlineQosParser;

    #[test]
    fn test_content_filter_info_parsing() {
        let mut test_data = Vec::new();

        // numBitmaps = 2
        test_data.extend_from_slice(&2u32.to_le_bytes());

        // bitmap[0] = 0xFFFFFFFF (all bits set to 1)
        test_data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());

        // bitmap[1] = 0x00000001 (first bit set to 1)
        test_data.extend_from_slice(&0x00000001u32.to_le_bytes());

        // numSignatures = 33
        test_data.extend_from_slice(&33u32.to_le_bytes());

        // 33 MD5 signatures (each 16 bytes)
        for i in 0..33 {
            let mut signature = [0u8; 16];
            signature[0] = i as u8; // Set first byte to index to distinguish each signature
            test_data.extend_from_slice(&signature);
        }

        // Parse using InlineQosParser
        let parser = InlineQosParser::new(false); // little-endian
        let result = parser.parse_content_filter_info(&test_data);

        assert!(result.is_ok(), "Parsing should succeed");

        let content_filter_info = result.unwrap();

        // Verify filter_result (bitmaps)
        assert_eq!(content_filter_info.filter_result.len(), 2);
        assert_eq!(content_filter_info.filter_result[0], -1); // 0xFFFFFFFF as i32
        assert_eq!(content_filter_info.filter_result[1], 1);

        // Verify filter_signatures
        assert_eq!(content_filter_info.filter_signatures.len(), 33);
        for i in 0..33 {
            assert_eq!(content_filter_info.filter_signatures[i][0], i as u8);
        }
    }
}
