//! DDS-XTYPES (Extensible Types) support.
//!
//! This module provides type metadata and type compatibility checking
//! as defined in the DDS-XTYPES specification.
//!
//! # Key Components
//!
//! - [`TypeObject`] - Complete type description
//! - [`TypeIdentifier`] - Compact type reference with hash
//! - [`EquivalenceHash`] - 14-byte MD5-based type hash
//! - [`DynamicType`] - Runtime type descriptor from TypeObject
//! - [`DynamicData`] - Runtime data container for dynamic types
//! - [`DynamicTypeSupport`] - TypeSupport for DynamicData
//!
//! # Type Compatibility
//!
//! DDS-XTYPES enables type evolution and compatibility checking between
//! different versions of the same type. The [`TypeObject`] contains full
//! type information while [`TypeIdentifier`] provides a compact hash for
//! efficient matching.
//!
//! # Dynamic Types
//!
//! DynamicType and DynamicData enable type-agnostic communication where
//! subscribers can receive and inspect data without compile-time type knowledge.
//! This is useful for bridging, logging, and interoperability scenarios.
//!
//! ## Example
//!
//! ```ignore
//! use int2dds::xtypes::{DynamicTypeSupport, DynamicData};
//!
//! // Create from TypeObject received during discovery
//! let type_support = DynamicTypeSupport::from_type_object(type_object)?;
//!
//! // Create and populate data
//! let mut data = type_support.create_data();
//! data.set("id", 42i32)?;
//! data.set("message", "Hello")?;
//!
//! // Access fields
//! let id: i32 = data.get("id")?;
//! let msg: String = data.get("message")?;
//! ```

mod type_object;
mod dynamic_type;
mod dynamic_data;
mod dynamic_serialization;
mod dynamic_type_support;

pub use type_object::*;

// Dynamic type support
pub use dynamic_type::{
    DynamicType, DynamicTypeKind, DynamicTypeError,
    PrimitiveKind, StructDescriptor, MemberDescriptor,
    EnumDescriptor, EnumLiteralDescriptor,
};
pub use dynamic_data::{
    DynamicData, DynamicValue,
    FromDynamicValue, IntoDynamicValue,
};
pub use dynamic_serialization::{
    serialize_dynamic_data, deserialize_dynamic_data,
};
pub use dynamic_type_support::DynamicTypeSupport;
