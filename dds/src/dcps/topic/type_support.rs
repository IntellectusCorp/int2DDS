//! Type Support - Type registration and serialization infrastructure for DDS.
//!
//! This module provides the `DdsType` trait and related infrastructure for making Rust types
//! compatible with DDS communication. Types must implement `DdsType` to be used with Topics,
//! DataWriters, and DataReaders.
//!
//! The `DdsType` derive macro (from `int2dds_derive`) automatically implements the required
//! trait methods for structs, handling serialization, key extraction, and type metadata.
//!
//! # Key Concepts
//!
//! - **DdsType Trait**: Marker and functionality trait for DDS-compatible types
//! - **Serialization**: Conversion between Rust types and wire format (CDR, PL_CDR)
//! - **Key Support**: Identification of key fields for instance management
//! - **Type Registration**: Automatic registration of types with DomainParticipant
//!
//! # Example
//!
//! ```no_run
//! use int2dds::topic::type_support::DdsType;
//!
//! #[derive(DdsType, Clone)]
//! #[dds_type(crate_path = "int2dds")]
//! struct MyData {
//!     #[dds(key)]
//!     id: u32,
//!     value: String,
//! }
//! ```

use std::{
    any::{Any, TypeId},
    fmt::Debug,
    sync::Arc,
};

use crate::{
    common::instance_handle::InstanceHandle,
    dcps::core::error::{DdsError, DdsResult},
    domain::domain_participant::DomainParticipant,
    rtps::common::types::SerializedData,
    topic::sql::ast::Parameter,
};

pub use int2dds_derive::DdsType;

/// Serialization format options for DDS types
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SerializationFormat {
    /// CDR
    #[default]
    Cdr,
    /// XCDR
    Xcdr { extensibility_kind: crate::serialize::xcdr::ExtensibilityKind, use_delimiters: bool },
}

pub trait DdsType: 'static + Send + Sync + Clone + Debug {
    type TypeSupport: TypeSupport + Default;

    fn get_type_support() -> Arc<Self::TypeSupport> {
        Arc::new(Self::TypeSupport::default())
    }

    fn get_type_name() -> String {
        Self::TypeSupport::default().get_type_name().to_string()
    }

    // Convenient type-safe methods
    fn serialize(&self) -> DdsResult<SerializedData> {
        Self::TypeSupport::default().serialize(self as &dyn Any)
    }

    fn deserialize(data: &[u8]) -> DdsResult<Self> {
        let any_box = Self::TypeSupport::default().deserialize(data)?;
        any_box
            .downcast::<Self>()
            .map(|boxed| *boxed)
            .map_err(|_| DdsError::Error("Type downcast failed".to_string()))
    }

    fn get_field_value(&self, field_path: &str) -> DdsResult<Parameter> {
        Self::TypeSupport::default().get_field_value(self as &dyn Any, field_path)
    }

    fn has_field(&self, field_path: &str) -> DdsResult<bool> {
        Ok(Self::TypeSupport::default().has_field(field_path))
    }
}

pub trait TypeSupport: Send + Sync + 'static {
    fn type_id(&self) -> TypeId;
    fn get_type_name(&self) -> &str;
    fn get_field_value(&self, data: &dyn Any, field_path: &str) -> DdsResult<Parameter>;
    fn has_field(&self, field_path: &str) -> bool;

    // Default serialization (CDR format)
    fn serialize(&self, data: &dyn Any) -> DdsResult<SerializedData>;
    fn deserialize(&self, data: &[u8]) -> DdsResult<Box<dyn Any>>;

    // Format-specific serialization (CDR or XCDR)
    fn serialize_with_format(
        &self,
        data: &dyn Any,
        format: &SerializationFormat,
    ) -> DdsResult<SerializedData>;
    fn deserialize_with_format(
        &self,
        data: &[u8],
        format: &SerializationFormat,
    ) -> DdsResult<Box<dyn Any>>;

    // Key handling
    fn serialize_key(&self, data: &dyn Any) -> DdsResult<SerializedData>;
    fn deserialize_key(&self, serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>>;
    fn compute_key(&self, data: &dyn Any) -> InstanceHandle;
    fn is_compute_key_provided(&self) -> bool;
    fn get_extensibility_kind(&self) -> crate::serialize::xcdr::ExtensibilityKind;

    fn serialize_key_and_non_key(
        &self,
        data: &dyn Any,
    ) -> DdsResult<(SerializedData, SerializedData)> {
        let key_data = self.serialize_key(data)?;
        let full_data = self.serialize(data)?;
        Ok((key_data, full_data))
    }

    /// Get serialized size estimate for the type
    // Todo:()
    fn get_serialized_size_bound(&self) -> Option<usize> {
        None
    }

    fn register_type(
        self: Arc<Self>,
        participant: &mut DomainParticipant,
        type_name: &str,
    ) -> DdsResult<()>
    where
        Self: Sized,
    {
        let type_support: Arc<dyn TypeSupport> = self;
        participant.register_type(type_support, type_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::debug;
    use speedy::{Readable, Writable};

    #[derive(DdsType, Readable, Writable)]
    #[dds_type(crate_path = "crate")]
    struct HelloWorldType {
        index: u32,
        message: String,
    }

    #[test]
    #[ignore]
    fn test_manual_type_registration() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).try_init();

        // Verify we can get the type support directly from DdsType
        let type_support = HelloWorldType::get_type_support();
        assert_eq!(type_support.get_type_name(), "HelloWorldType");

        // Create test data
        let hello = HelloWorldType { index: 123, message: "Test message".to_string() };

        // Serialize
        let serialized = hello.serialize().unwrap();

        // Test direct type deserialization
        let result = HelloWorldType::deserialize(&serialized);
        assert!(result.is_ok());

        let deserialized_hello = result.unwrap();
        assert_eq!(deserialized_hello.index, 123);
        assert_eq!(deserialized_hello.message, "Test message");
    }

    #[test]
    #[ignore]
    fn test_serialize_deserialize_symmetry() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).try_init();
        // Test 1: Simple data
        let data1 = HelloWorldType { index: 42, message: "Hello, DDS!".to_string() };

        // Serialize using instance method
        let serialized1 = data1.serialize().unwrap();

        // Deserialize using static method
        let deserialized1 = HelloWorldType::deserialize(&serialized1).unwrap();

        // Verify symmetry
        assert_eq!(data1.index, deserialized1.index);
        assert_eq!(data1.message, deserialized1.message);

        // Test 2: Empty string
        let data2 = HelloWorldType { index: 0, message: "".to_string() };

        let serialized3 = data2.serialize().unwrap();
        let deserialized3 = HelloWorldType::deserialize(&serialized3).unwrap();

        assert_eq!(data2.index, deserialized3.index);
        assert_eq!(data2.message, deserialized3.message);

        // Test 3: Large data
        let data3 = HelloWorldType { index: u32::MAX, message: "A".repeat(1000) };

        let serialized4 = data3.serialize().unwrap();
        let deserialized4 = HelloWorldType::deserialize(&serialized4).unwrap();

        assert_eq!(data3.index, deserialized4.index);
        assert_eq!(data3.message, deserialized4.message);
    }

    #[derive(DdsType, Readable, Writable)]
    #[dds_type(crate_path = "crate")]
    struct UnifiedHelloWorldType {
        #[dds(key)]
        index: u32,
        message: String,
    }

    #[test]
    #[ignore]
    fn test_unified_cdr_xcdr_serialization() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).try_init();
        use crate::serialize::xcdr::ExtensibilityKind;

        let test_data =
            UnifiedHelloWorldType { index: 42, message: "Unified CDR/XCDR Test".to_string() };

        let type_support = UnifiedHelloWorldType::get_type_support();

        // Test CDR serialization (default)
        let cdr_serialized = type_support.serialize(&test_data as &dyn std::any::Any).unwrap();
        debug!("CDR serialized data: {:02X?}", &cdr_serialized[..8]);

        // Test XCDR serialization (Final extensibility)
        let xcdr_format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Final,
            use_delimiters: false,
        };
        let xcdr_serialized = type_support
            .serialize_with_format(&test_data as &dyn std::any::Any, &xcdr_format)
            .unwrap();
        debug!("XCDR serialized data: {:02X?}", &xcdr_serialized[..8]);

        // Header validation
        assert_eq!(cdr_serialized[0], 0x00);
        assert_eq!(cdr_serialized[1], 0x01); // CDR LE

        assert_eq!(xcdr_serialized[0], 0x00);
        assert_eq!(xcdr_serialized[1], 0x07); // XCDR2 LE

        // Deserialization test
        let cdr_deserialized_any = type_support.deserialize(&cdr_serialized).unwrap();
        let cdr_deserialized = cdr_deserialized_any.downcast::<UnifiedHelloWorldType>().unwrap();
        assert_eq!(cdr_deserialized.index, test_data.index);
        assert_eq!(cdr_deserialized.message, test_data.message);

        let xcdr_deserialized_any =
            type_support.deserialize_with_format(&xcdr_serialized, &xcdr_format).unwrap();
        let xcdr_deserialized = xcdr_deserialized_any.downcast::<UnifiedHelloWorldType>().unwrap();
        assert_eq!(xcdr_deserialized.index, test_data.index);
        assert_eq!(xcdr_deserialized.message, test_data.message);

        debug!("✓ Unified CDR/XCDR test passed!");
    }

    #[derive(DdsType, Readable, Writable)]
    #[dds_type(crate_path = "crate")]
    struct UnifiedPaddingTestType {
        small: u8,
        medium: u16,
        large: u64,
        tiny: u8,
        big: u32,
        text: String,
    }

    #[test]
    #[ignore]
    fn test_xcdr_extensibility_modes() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).try_init();
        use crate::serialize::xcdr::ExtensibilityKind;

        let test_data = UnifiedPaddingTestType {
            small: 0xAB,
            medium: 0x1234,
            large: 0x123456789ABCDEF0,
            tiny: 0xCD,
            big: 0x87654321,
            text: "Ext Test".to_string(),
        };

        let type_support = UnifiedPaddingTestType::get_type_support();

        // XCDR extensibility mode test (currently only Final is supported)
        let extensibility_modes = [
            ("Final", ExtensibilityKind::Final, false),
            // TODO: Appendable and Mutable need additional implementation
        ];

        for (mode_name, extensibility_kind, use_delimiters) in extensibility_modes.iter() {
            let format = SerializationFormat::Xcdr {
                extensibility_kind: *extensibility_kind,
                use_delimiters: *use_delimiters,
            };

            let serialized = type_support
                .serialize_with_format(&test_data as &dyn std::any::Any, &format)
                .unwrap();
            let deserialized_any =
                type_support.deserialize_with_format(&serialized, &format).unwrap();
            let deserialized = deserialized_any.downcast::<UnifiedPaddingTestType>().unwrap();

            assert_eq!(deserialized.small, test_data.small, "Failed for {}", mode_name);
            assert_eq!(deserialized.medium, test_data.medium, "Failed for {}", mode_name);
            assert_eq!(deserialized.large, test_data.large, "Failed for {}", mode_name);
            assert_eq!(deserialized.tiny, test_data.tiny, "Failed for {}", mode_name);
            assert_eq!(deserialized.big, test_data.big, "Failed for {}", mode_name);
            assert_eq!(deserialized.text, test_data.text, "Failed for {}", mode_name);

            debug!("✓ XCDR {} extensibility test passed", mode_name);
        }
    }

    #[derive(DdsType, Readable, Writable)]
    #[dds_type(crate_path = "crate")]
    struct SimpleArrayTest {
        data: [u8; 4],
    }

    #[derive(DdsType, Readable, Writable)]
    #[dds_type(crate_path = "crate")]
    struct MixedArrayTest {
        id: u32,
        bytes: [u8; 2],
        name: String,
    }

    #[test]
    #[ignore]
    fn test_simple_array_compilation() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).try_init();
        debug!("=== Simple [u8; N] array compilation test ===");

        // 1. Test struct creation capability
        let simple = SimpleArrayTest { data: [0xAA, 0xBB, 0xCC, 0xDD] };
        debug!("✓ SimpleArrayTest created successfully: {:?}", simple);

        let mixed = MixedArrayTest { id: 12345, bytes: [0x11, 0x22], name: "test".to_string() };
        debug!("✓ MixedArrayTest created successfully: {:?}", mixed);

        // 2. Verify DdsType trait method calls
        debug!("\n2. Checking TypeSupport...");
        let simple_type_support = SimpleArrayTest::get_type_support();
        debug!("✓ SimpleArrayTest TypeSupport: {}", simple_type_support.get_type_name());

        let mixed_type_support = MixedArrayTest::get_type_support();
        debug!("✓ MixedArrayTest TypeSupport: {}", mixed_type_support.get_type_name());
    }

    #[derive(DdsType, Readable, Writable)]
    #[dds_type(crate_path = "crate")]
    struct ArrayTestType {
        id: u32,
        data: [u8; 4], // Fixed size array test
        name: String,
    }

    #[test]
    #[ignore]
    fn test_u8_array_support() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).try_init();
        debug!("=== Testing [u8; N] Array Support ===");

        let test_data = ArrayTestType {
            id: 12345,
            data: [0xAA, 0xBB, 0xCC, 0xDD],
            name: "ArrayTest".to_string(),
        };

        debug!(
            "Original data: id={}, data={:02X?}, name={}",
            test_data.id, test_data.data, test_data.name
        );

        // Attempt serialization
        match test_data.serialize() {
            Ok(serialized) => {
                debug!("✓ Serialization successful: {} bytes", serialized.len());
                debug!(
                    "  Serialized bytes: {:02X?}",
                    &serialized.as_ref()[..std::cmp::min(serialized.len(), 32)]
                );

                // Attempt deserialization
                match ArrayTestType::deserialize(&serialized) {
                    Ok(deserialized) => {
                        debug!("✓ Deserialization successful");
                        debug!(
                            "Deserialized data: id={}, data={:02X?}, name={}",
                            deserialized.id, deserialized.data, deserialized.name
                        );

                        // Data integrity verification
                        assert_eq!(deserialized.id, test_data.id, "ID mismatch!");
                        assert_eq!(deserialized.data, test_data.data, "Array data mismatch!");
                        assert_eq!(deserialized.name, test_data.name, "Name mismatch!");

                        debug!("✓ All data integrity checks passed!");
                    }
                    Err(e) => {
                        debug!("✗ Deserialization failed: {:?}", e);
                        panic!("Deserialization should succeed");
                    }
                }
            }
            Err(e) => {
                debug!("✗ Serialization failed: {:?}", e);
                panic!("Serialization should succeed");
            }
        }

        debug!("=== [u8; N] Array Test Complete ===");
    }

    #[test]
    #[ignore]
    fn test_rtps_key_hash_calculation() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).try_init();

        debug!("=== Testing RTPS KeyHash Calculation ===");

        // Test with a type that has a key
        let data1 = UnifiedHelloWorldType { index: 12345, message: "Test message".to_string() };

        let type_support = UnifiedHelloWorldType::get_type_support();
        let instance_handle1 = type_support.compute_key(&data1 as &dyn std::any::Any);

        debug!("Key value: {}", data1.index);
        debug!("Computed InstanceHandle: {:02X?}", instance_handle1.value());

        // Different instance with the same key value
        let data2 = UnifiedHelloWorldType {
            index: 12345, // Same key
            message: "Different message".to_string(),
        };

        let instance_handle2 = type_support.compute_key(&data2 as &dyn std::any::Any);

        // Same keys should produce the same instance handle
        assert_eq!(
            instance_handle1, instance_handle2,
            "Same keys should produce same instance handles"
        );

        // Test with a different key value
        let data3 = UnifiedHelloWorldType {
            index: 67890, // Different key
            message: "Another message".to_string(),
        };

        let instance_handle3 = type_support.compute_key(&data3 as &dyn std::any::Any);

        // Different keys should produce different instance handles
        assert_ne!(
            instance_handle1, instance_handle3,
            "Different keys should produce different instance handles"
        );

        debug!("✓ Key consistency test passed");

        // RTPS standard format test
        let test_value = 0x12345678u32;
        let test_data =
            UnifiedHelloWorldType { index: test_value, message: "Format test".to_string() };

        let test_handle = type_support.compute_key(&test_data as &dyn std::any::Any);

        debug!("Test key value: 0x{:08X}", test_value);
        debug!("RTPS KeyHash: {:02X?}", test_handle.value());

        // u32 key is serialized as CDR BE, so it should be [0x12, 0x34, 0x56, 0x78, ...]
        assert_eq!(test_handle.value()[0], 0x12);
        assert_eq!(test_handle.value()[1], 0x34);
        assert_eq!(test_handle.value()[2], 0x56);
        assert_eq!(test_handle.value()[3], 0x78);

        // The rest should be zero-padded
        for i in 4..16 {
            assert_eq!(test_handle.value()[i], 0x00, "Padding byte at index {} should be zero", i);
        }

        debug!("✓ RTPS KeyHash format test passed");

        // Key serialization test
        match type_support.serialize_key(&test_data as &dyn std::any::Any) {
            Ok(serialized_key) => {
                debug!("Serialized key: {:02X?}", serialized_key.as_ref());
                debug!("Serialized key size: {} bytes", serialized_key.len());

                // u32 key should be serialized as CDR Big-Endian to 4 bytes
                assert_eq!(serialized_key.len(), 4, "u32 key should serialize to 4 bytes");
                assert_eq!(
                    serialized_key.as_ref(),
                    &[0x12, 0x34, 0x56, 0x78],
                    "CDR BE serialization should match expected bytes"
                );
            }
            Err(e) => {
                panic!("Key serialization failed: {:?}", e);
            }
        }

        debug!("✓ Key serialization test passed");
        debug!("=== RTPS KeyHash Calculation Test Complete ===");
    }

    #[derive(DdsType, Readable, Writable)]
    #[dds_type(crate_path = "crate")]
    struct CharacterTestType {
        ascii_char: char, // Latin-1 character (8-bit)
        id: u32,
        name: String,
    }

    #[test]
    #[ignore]
    fn test_char_serialization() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).try_init();
        debug!("=== Testing char Serialization Support ===");

        let test_data = CharacterTestType {
            ascii_char: 'A', // Character within Latin-1 range
            id: 12345,
            name: "CharTest".to_string(),
        };

        debug!(
            "Original data: ascii_char='{}' (0x{:02X}), id={}, name={}",
            test_data.ascii_char, test_data.ascii_char as u32, test_data.id, test_data.name
        );

        // Serialization test
        match test_data.serialize() {
            Ok(serialized) => {
                debug!("✓ Serialization successful: {} bytes", serialized.len());
                debug!(
                    "  Serialized bytes: {:02X?}",
                    &serialized.as_ref()[..std::cmp::min(serialized.len(), 32)]
                );

                // Deserialization test
                match CharacterTestType::deserialize(&serialized) {
                    Ok(deserialized) => {
                        debug!("✓ Deserialization successful");
                        debug!(
                            "Deserialized data: ascii_char='{}' (0x{:02X}), id={}, name={}",
                            deserialized.ascii_char,
                            deserialized.ascii_char as u32,
                            deserialized.id,
                            deserialized.name
                        );

                        // Data integrity verification
                        assert_eq!(
                            deserialized.ascii_char, test_data.ascii_char,
                            "ASCII char mismatch!"
                        );
                        assert_eq!(deserialized.id, test_data.id, "ID mismatch!");
                        assert_eq!(deserialized.name, test_data.name, "Name mismatch!");

                        debug!("✓ All character data integrity checks passed!");
                    }
                    Err(e) => {
                        debug!("✗ Deserialization failed: {:?}", e);
                        panic!("Character deserialization should succeed");
                    }
                }
            }
            Err(e) => {
                debug!("✗ Serialization failed: {:?}", e);
                panic!("Character serialization should succeed");
            }
        }

        debug!("=== char Serialization Test Complete ===");
    }

    #[derive(DdsType, Readable, Writable)]
    #[dds_type(crate_path = "crate")]
    struct DataPacket {
        id: u32,
        payload: Vec<u8>,
        timestamp: u64,
    }

    #[test]
    #[ignore]
    fn test_vec_u8_serialization() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).try_init();
        debug!("=== Testing Vec<u8> Serialization Support ===");

        let test_data = DataPacket {
            id: 42,
            payload: vec![0x01, 0x02, 0x03, 0x04, 0xFF, 0xAB, 0xCD, 0xEF],
            timestamp: 1234567890,
        };

        debug!(
            "Original data: id={}, payload={:02X?}, timestamp={}",
            test_data.id, test_data.payload, test_data.timestamp
        );

        let serialized = test_data.serialize().unwrap();
        debug!("✓ Serialization successful: {} bytes", serialized.len());

        let deserialized = DataPacket::deserialize(&serialized).unwrap();
        debug!("✓ Deserialization successful");

        assert_eq!(deserialized.id, test_data.id);
        assert_eq!(deserialized.payload, test_data.payload);
        assert_eq!(deserialized.timestamp, test_data.timestamp);
        debug!("✓ All Vec<u8> data integrity checks passed!");

        // Test empty vector
        let empty_data = DataPacket { id: 100, payload: vec![], timestamp: 9876543210 };

        let empty_serialized = empty_data.serialize().unwrap();
        let empty_deserialized = DataPacket::deserialize(&empty_serialized).unwrap();

        assert_eq!(empty_deserialized.id, empty_data.id);
        assert_eq!(empty_deserialized.payload, empty_data.payload);
        assert_eq!(empty_deserialized.timestamp, empty_data.timestamp);
        debug!("✓ Empty Vec<u8> test passed!");

        debug!("=== Vec<u8> Serialization Test Complete ===");
    }
}
