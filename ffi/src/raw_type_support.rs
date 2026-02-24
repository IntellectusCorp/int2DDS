//! Lightweight TypeSupport for the raw bytes FFI path.
//!
//! `RawTypeSupport` implements the `TypeSupport` trait with stub serialize/deserialize
//! methods. It is used when C users handle serialization themselves via
//! `int2dds_write_serialized()` / `int2dds_take_serialized()`.
//!
//! This allows topic creation and DDS discovery without the full dynamic type system.

use std::any::{Any, TypeId};
use std::sync::Arc;

use int2dds::{
    common::instance_handle::InstanceHandle,
    core::error::{DdsError, DdsResult},
    rtps::common::types::SerializedData,
    serialize::cdr::ExtensibilityKind,
    topic::sql::ast::Parameter,
    topic::type_support::{SerializationFormat, TypeSupport},
    xtypes::{TypeIdentifier, TypeObject},
};

/// Lightweight TypeSupport for raw bytes FFI path.
///
/// Serialize/deserialize methods return errors since they are never called
/// when using `write_serialized()` / `take_serialized()`.
pub struct RawTypeSupport {
    type_name: String,
    extensibility: ExtensibilityKind,
    has_key: bool,
    type_identifier: Option<TypeIdentifier>,
    type_object: Option<TypeObject>,
}

impl RawTypeSupport {
    pub fn new(type_name: String, extensibility: ExtensibilityKind) -> Self {
        Self { type_name, extensibility, has_key: false, type_identifier: None, type_object: None }
    }

    /// Create a RawTypeSupport with pre-built TypeIdentifier and TypeObject.
    ///
    /// This enables DDS-XTypes discovery parameters (0x0069, 0x0072) to be sent
    /// during endpoint matching.
    pub fn with_type_info(
        type_name: String,
        extensibility: ExtensibilityKind,
        has_key: bool,
        type_identifier: TypeIdentifier,
        type_object: TypeObject,
    ) -> Self {
        Self {
            type_name,
            extensibility,
            has_key,
            type_identifier: Some(type_identifier),
            type_object: Some(type_object),
        }
    }
}

impl TypeSupport for RawTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<crate::data::Int2DdsData>()
    }

    fn get_type_name(&self) -> &str {
        &self.type_name
    }

    fn get_field_value(&self, _data: &dyn Any, _field_path: &str) -> DdsResult<Parameter> {
        Err(DdsError::Error(
            "RawTypeSupport: field access not supported in raw bytes mode".to_string(),
        ))
    }

    fn has_field(&self, _field_path: &str) -> bool {
        false
    }

    fn serialize(
        &self,
        _data: &dyn Any,
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData> {
        Err(DdsError::Error("RawTypeSupport: use write_serialized() instead".to_string()))
    }

    fn deserialize(
        &self,
        _data: &[u8],
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        // Return a dummy Int2DdsData so that the DDS internal key extraction
        // (update_instance_state) succeeds and data is stored in the cache.
        // Actual deserialization is done by C users via take_serialized().
        Ok(Box::new(crate::data::Int2DdsData))
    }

    fn serialize_key(&self, _data: &dyn Any) -> DdsResult<SerializedData> {
        Ok(Arc::from(Vec::new()))
    }

    fn deserialize_key(&self, _serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        Err(DdsError::Error("RawTypeSupport: use take_serialized() for key access".to_string()))
    }

    fn compute_key(&self, _data: &dyn Any) -> InstanceHandle {
        InstanceHandle::NIL
    }

    fn is_compute_key_provided(&self) -> bool {
        self.has_key
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        self.extensibility
    }

    fn get_type_identifier(&self) -> Option<TypeIdentifier> {
        self.type_identifier.clone()
    }

    fn get_type_object(&self) -> Option<TypeObject> {
        self.type_object.clone()
    }
}
