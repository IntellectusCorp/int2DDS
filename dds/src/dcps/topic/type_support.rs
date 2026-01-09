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
