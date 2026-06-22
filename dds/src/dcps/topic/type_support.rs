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
//! #[derive(DdsType)]
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
    xtypes::{TypeIdentifier, TypeObject},
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
    type FieldAccessor: FieldAccessor + Default;

    fn get_type_support() -> Arc<Self::TypeSupport> {
        Arc::new(Self::TypeSupport::default())
    }

    fn get_type_name() -> String {
        Self::TypeSupport::default().get_type_name().to_string()
    }

    // Convenient type-safe methods
    fn serialize(&self) -> DdsResult<SerializedData> {
        Self::TypeSupport::default().serialize(self as &dyn Any, None)
    }

    fn deserialize(data: &[u8]) -> DdsResult<Self> {
        let any_box = Self::TypeSupport::default().deserialize(data, None)?;
        any_box
            .downcast::<Self>()
            .map(|boxed| *boxed)
            .map_err(|_| DdsError::Error("Type downcast failed".to_string()))
    }

    fn get_field_value(&self, field_path: &str) -> DdsResult<Parameter> {
        Self::FieldAccessor::default().get_field_value(self as &dyn Any, field_path)
    }

    fn has_field(&self, field_path: &str) -> DdsResult<bool> {
        Ok(Self::FieldAccessor::default().has_field(field_path))
    }
}

pub trait FieldAccessor: Send + Sync + 'static {
    fn get_field_value(&self, data: &dyn Any, field_path: &str) -> DdsResult<Parameter>;
    fn has_field(&self, field_path: &str) -> bool;
}

pub trait TypeSupport: Send + Sync + 'static {
    fn type_id(&self) -> TypeId;
    fn get_type_name(&self) -> &str;

    // Serialization with optional format override.
    // When format is None, the implementation uses its own default behavior.
    fn serialize(
        &self,
        data: &dyn Any,
        format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData>;
    fn deserialize(
        &self,
        data: &[u8],
        format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>>;

    // Deserialize from non-contiguous fragment chunks (scatter-gather receive
    // path). Default materializes the chunks into a contiguous buffer and
    // delegates to `deserialize`; the derive macro overrides this to read classic
    // CDR directly across chunks. Slice order is fragment order.
    fn deserialize_chained(
        &self,
        chunks: &[bytes::Bytes],
        format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        let mut buf = Vec::with_capacity(total);
        for c in chunks {
            buf.extend_from_slice(c);
        }
        self.deserialize(&buf, format)
    }

    /// Serialize into an existing buffer, reusing its capacity.
    /// The buffer is cleared and filled with serialized data (including encapsulation header).
    /// Default implementation delegates to `serialize()` (no buffer reuse).
    /// Derive macro generates an optimized version that reuses the buffer's capacity.
    fn serialize_into(
        &self,
        data: &dyn Any,
        buffer: &mut Vec<u8>,
        format: Option<&SerializationFormat>,
    ) -> DdsResult<()> {
        let serialized = self.serialize(data, format)?;
        buffer.clear();
        buffer.extend_from_slice(&serialized);
        Ok(())
    }

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
        let full_data = self.serialize(data, None)?;
        Ok((key_data, full_data))
    }

    /// Get serialized size estimate for the type
    // Todo:()
    fn get_serialized_size_bound(&self) -> Option<usize> {
        None
    }

    /// Get the TypeIdentifier for this type (DDS-XTypes).
    /// Returns None if the type does not support XTypes.
    fn get_type_identifier(&self) -> Option<TypeIdentifier> {
        None
    }

    /// Get the TypeObject for this type (DDS-XTypes).
    /// Returns None if the type does not support XTypes.
    fn get_type_object(&self) -> Option<TypeObject> {
        None
    }

    /// Get this type's `TypeObject` plus the transitive
    /// closure of every nested composite it references.
    fn get_type_object_closure(&self) -> Vec<(TypeIdentifier, TypeObject)> {
        match (self.get_type_identifier(), self.get_type_object()) {
            (Some(id), Some(obj)) => vec![(id, obj)],
            _ => Vec::new(),
        }
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

/// Autoref specialization helper for dot-notation nested field access.
///
/// Allows derive macro generated code to delegate field access into nested struct
/// fields without knowing at macro expansion time whether a field type implements
/// `DdsType`. Uses the autoref specialization pattern:
/// - If `T: DdsType`, inherent methods on `NestedAccessor<T>` are resolved first.
/// - Otherwise, auto-ref finds the fallback trait impl on `&NestedAccessor<T>`.
pub mod nested_access {
    use super::*;

    pub struct NestedAccessor<T>(pub core::marker::PhantomData<T>);

    /// Fallback for types that do NOT implement DdsType.
    pub trait NestedAccessFallback {
        fn nested_has_field(&self, _rest: &str) -> bool {
            false
        }
        fn nested_get_field_value(&self, _data: &dyn Any, rest: &str) -> DdsResult<Parameter> {
            Err(DdsError::Error(format!("Type has no nested field '{}'", rest)))
        }
    }

    impl<T> NestedAccessFallback for NestedAccessor<T> {}

    /// Preferred path for types that DO implement DdsType.
    impl<T: DdsType> NestedAccessor<T> {
        pub fn nested_has_field(&self, rest: &str) -> bool {
            T::FieldAccessor::default().has_field(rest)
        }

        pub fn nested_get_field_value(&self, data: &dyn Any, rest: &str) -> DdsResult<Parameter> {
            T::FieldAccessor::default().get_field_value(data, rest)
        }
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod keyhash_tests {
    use super::*;
    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct SingleU32Key {
        #[dds(key)]
        pub id: u32,
        pub data: f64,
    }

    #[test]
    fn test_keyhash_u32_big_endian() {
        use crate::dcps::topic::type_support::TypeSupport;

        let value = SingleU32Key { id: 42, data: 1.0 };
        let type_support = SingleU32Key::get_type_support();
        let key_bytes = type_support.serialize_key(&value).unwrap();

        assert_eq!(&*key_bytes, &[0x00, 0x00, 0x00, 0x2A]);

        let instance_handle = type_support.compute_key(&value);
        let handle_bytes = instance_handle.value();
        let mut expected = [0u8; 16];
        expected[..4].copy_from_slice(&[0x00, 0x00, 0x00, 0x2A]);
        assert_eq!(handle_bytes, &expected);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct MultiKeyStruct {
        #[dds(key)]
        pub a: u16,
        #[dds(key)]
        pub b: u32,
        pub c: f64,
    }

    #[test]
    fn test_keyhash_multi_key_big_endian_order() {
        use crate::dcps::topic::type_support::TypeSupport;

        let value = MultiKeyStruct { a: 1, b: 2, c: 99.0 };
        let type_support = MultiKeyStruct::get_type_support();
        let key_bytes = type_support.serialize_key(&value).unwrap();

        assert_eq!(&*key_bytes, &[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,]);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct LargeKeyStruct {
        #[dds(key)]
        pub a: u64,
        #[dds(key)]
        pub b: u64,
        #[dds(key)]
        pub c: u64,
    }

    #[test]
    fn test_keyhash_large_key_uses_md5() {
        use crate::dcps::topic::type_support::TypeSupport;

        let value = LargeKeyStruct { a: 1, b: 2, c: 3 };
        let type_support = LargeKeyStruct::get_type_support();
        let key_bytes = type_support.serialize_key(&value).unwrap();

        assert_eq!(key_bytes.len(), 24);

        let instance_handle = type_support.compute_key(&value);
        let expected_md5 = md5::compute(&*key_bytes);
        assert_eq!(instance_handle.value(), &expected_md5.0);
    }

    // ============================================================================
    // LC 6/7 EMHEADER Optimization Tests
    // ============================================================================
}
