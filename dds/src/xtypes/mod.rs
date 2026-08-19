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

mod codec_plan;
mod dynamic_data;
mod dynamic_serialization;
mod dynamic_type;
mod dynamic_type_support;
mod type_compatibility;
mod type_lookup;
mod type_object;
mod type_object_v1;
mod type_object_xcdr;
mod type_registry;

pub use type_compatibility::{
    check_structural_compatibility, complete_key_erased, complete_key_holder,
    evaluate_structural_compatibility, minimal_key_erased, minimal_key_holder, TypeCompatibility,
    TypeCompatibilityError, TypeCompatibilityResult, TypeResolver,
};
pub use type_object::*;
pub use type_object_v1::TypeObjectV1;
pub use type_object_xcdr::{deserialize_type_object, serialize_type_object, spec_hash};

// Dynamic type support
pub use codec_plan::TypePlans;
pub use dynamic_data::{DynamicData, DynamicValue, FromDynamicValue, IntoDynamicValue};
pub use dynamic_serialization::{deserialize_dynamic_data, serialize_dynamic_data};
pub use dynamic_type::{
    DynamicType, DynamicTypeError, DynamicTypeKind, EnumDescriptor, EnumLiteralDescriptor,
    MemberDescriptor, PrimitiveKind, StructDescriptor,
};
pub use dynamic_type_support::DynamicTypeSupport;
pub use type_lookup::{
    chunk_dependencies, continuation_point_for, continuation_point_index, GetTypeDependenciesIn,
    GetTypeDependenciesOut, GetTypesIn, GetTypesOut, ReplyHeader, RequestHeader, SampleIdentity,
    TypeLookupCall, TypeLookupReply, TypeLookupRequest, TypeLookupReturn,
    MAX_DEPENDENCIES_PER_REPLY,
};
pub use type_registry::{
    build_minimal_closure, new_shared_registry, SharedTypeRegistry, TypeRegistry,
};
