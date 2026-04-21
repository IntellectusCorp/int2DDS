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
