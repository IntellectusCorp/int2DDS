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
};

/// Lightweight TypeSupport for raw bytes FFI path.
///
/// Serialize/deserialize methods return errors since they are never called
/// when using `write_serialized()` / `take_serialized()`.
pub struct RawTypeSupport {
    type_name: String,
    extensibility: ExtensibilityKind,
    has_key: bool,
}

impl RawTypeSupport {
    pub fn new(type_name: String, extensibility: ExtensibilityKind, has_key: bool) -> Self {
        Self { type_name, extensibility, has_key }
    }
    pub fn new_with_key(
        type_name: String,
        extensibility: ExtensibilityKind,
        has_key: bool,
    ) -> Self {
        Self { type_name, extensibility, has_key }
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
        Err(DdsError::Error("RawTypeSupport: use take_serialized() instead".to_string()))
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
}
