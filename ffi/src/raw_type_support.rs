//! Lightweight TypeSupport for the raw bytes FFI path.
//!
//! `RawTypeSupport` implements the `TypeSupport` trait with stub serialize/deserialize
//! methods. It is used when C users handle serialization themselves via
//! `int2dds_datawriter_write_serialized()` / `int2dds_datareader_take_serialized()`.
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
    topic::type_support::{FieldAccessor, SerializationFormat, TypeSupport},
    xtypes::{
        deserialize_dynamic_data, DynamicTypeSupport, TypeIdentifier, TypeObject, TypeRegistry,
    },
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
    all_fields: Option<Arc<Vec<crate::data::CdrFieldDescriptor>>>,
    /// Canonical key machinery derived from a full TypeObject. When present,
    /// `compute_key` deserializes the sample into a `DynamicData` and delegates to
    /// the shared Rust key path (`serialize_key_cdr`), matching native-Rust/derive
    /// InstanceHandles for every key shape — including composite, float, and nested
    /// members. It is the sole key path: a raw topic without a full TypeObject
    /// yields a NIL InstanceHandle rather than a non-conformant approximation.
    dynamic_key_support: Option<Arc<DynamicTypeSupport>>,
}

impl RawTypeSupport {
    pub fn new(type_name: String, extensibility: ExtensibilityKind) -> Self {
        Self {
            type_name,
            extensibility,
            has_key: false,
            type_identifier: None,
            type_object: None,
            all_fields: None,
            dynamic_key_support: None,
        }
    }

    pub fn new_with_key(
        type_name: String,
        extensibility: ExtensibilityKind,
        has_key: bool,
    ) -> Self {
        Self {
            type_name,
            extensibility,
            has_key,
            type_identifier: None,
            type_object: None,
            all_fields: None,
            dynamic_key_support: None,
        }
    }

    /// Create a RawTypeSupport with pre-built TypeIdentifier and TypeObject.
    ///
    /// This enables DDS-XTypes discovery parameters (0x0075/0x0069, 0x0072) to be sent
    /// during endpoint matching.
    pub fn with_type_info(
        type_name: String,
        extensibility: ExtensibilityKind,
        has_key: bool,
        type_identifier: TypeIdentifier,
        type_object: TypeObject,
    ) -> Self {
        Self::with_type_info_and_deps(
            type_name,
            extensibility,
            has_key,
            type_identifier,
            type_object,
            Vec::new(),
        )
    }

    /// Like [`with_type_info`](Self::with_type_info), but with a dependency closure of
    /// nested `TypeObject`s so the canonical key machinery can resolve composite
    /// (nested-struct) key members. Each dependency is registered under its own
    /// content-hash `CompleteTypeId` — the same identifier the parent member references
    /// and the derive macro emits — so `serialize_key_cdr` recurses into nested keys
    /// exactly like native Rust.
    pub fn with_type_info_and_deps(
        type_name: String,
        extensibility: ExtensibilityKind,
        has_key: bool,
        type_identifier: TypeIdentifier,
        type_object: TypeObject,
        dependencies: Vec<(TypeIdentifier, TypeObject)>,
    ) -> Self {
        // Build the canonical key machinery from the full TypeObject so keyed
        // topics created via the type_info path (C# generated types, Python
        // `_dds_type_info_fields`) compute the same InstanceHandle as native Rust.
        let dynamic_key_support = if has_key {
            if dependencies.is_empty() {
                DynamicTypeSupport::from_type_object(type_object.clone()).ok().map(Arc::new)
            } else {
                let mut registry = TypeRegistry::new();
                for (id, obj) in &dependencies {
                    registry.register_type_object_with_id(id, obj.clone());
                }
                DynamicTypeSupport::from_type_object_with_registry(type_object.clone(), &registry)
                    .ok()
                    .map(Arc::new)
            }
        } else {
            None
        };
        Self {
            type_name,
            extensibility,
            has_key,
            type_identifier: Some(type_identifier),
            type_object: Some(type_object),
            all_fields: None,
            dynamic_key_support,
        }
    }

    /// Set all field descriptors for get_field_value() / has_field() support.
    pub fn set_all_fields(&mut self, fields: Vec<crate::data::CdrFieldDescriptor>) {
        self.all_fields = Some(Arc::new(fields));
    }
}

impl FieldAccessor for RawTypeSupport {
    fn get_field_value(&self, _data: &dyn Any, _field_path: &str) -> DdsResult<Parameter> {
        Err(DdsError::Error(
            "RawTypeSupport: field access not supported in raw bytes mode".to_string(),
        ))
    }

    fn has_field(&self, _field_path: &str) -> bool {
        false
    }
}

impl TypeSupport for RawTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<crate::data::Int2DdsData>()
    }

    fn get_type_name(&self) -> &str {
        &self.type_name
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
        data: &[u8],
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        // Store CDR bytes and field metadata when configured (Python binding).
        // Otherwise return empty Int2DdsData (existing behavior for C/C# bindings).
        let need_bytes = self.all_fields.is_some() || self.dynamic_key_support.is_some();
        Ok(Box::new(crate::data::Int2DdsData {
            cdr_bytes: if need_bytes { Some(data.to_vec()) } else { None },
            field_descriptors: self.all_fields.clone(),
            extensibility: self.extensibility,
        }))
    }

    fn serialize_key(&self, data: &dyn Any) -> DdsResult<SerializedData> {
        // Canonical RTPS KeyHash CDR (headerless, big-endian, §9.6.4.8) derived
        // from the full sample bytes via the shared DynamicData key machinery —
        // the same projection `compute_key` hashes and native Rust/derive emit.
        // No full TypeObject (name-only keyed topic) => empty key.
        let int2dds_data = match data.downcast_ref::<crate::data::Int2DdsData>() {
            Some(d) => d,
            None => return Ok(Arc::from(Vec::new())),
        };
        let cdr_bytes = match &int2dds_data.cdr_bytes {
            Some(b) => b,
            None => return Ok(Arc::from(Vec::new())),
        };
        if let Some(dts) = &self.dynamic_key_support {
            if let Ok(dyn_data) = deserialize_dynamic_data(cdr_bytes, dts.dynamic_type()) {
                return dts.serialize_key(&dyn_data);
            }
        }
        Ok(Arc::from(Vec::new()))
    }

    fn deserialize_key(&self, serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        // Reconstruct the key as DynamicData via the canonical key machinery so the wire
        // serializedKey path (serialize_key_payload) works on the raw FFI path, which has
        // no typed value — e.g. dispose/unregister from stored key bytes. A keyed raw topic
        // always carries this (creation is rejected otherwise, see topic.rs); the guard is
        // defensive against a keyed topic that reached here without a full TypeObject.
        match &self.dynamic_key_support {
            Some(dts) => dts.deserialize_key(serialized_key),
            None => Err(DdsError::PreconditionNotMet),
        }
    }

    fn serialize_key_payload(
        &self,
        data: &dyn Any,
        format: &SerializationFormat,
    ) -> DdsResult<SerializedData> {
        match &self.dynamic_key_support {
            Some(dts) => dts.serialize_key_payload(data, format),
            None => Err(DdsError::PreconditionNotMet),
        }
    }

    fn compute_key(&self, data: &dyn Any) -> InstanceHandle {
        let int2dds_data = match data.downcast_ref::<crate::data::Int2DdsData>() {
            Some(d) => d,
            None => return InstanceHandle::NIL,
        };

        let cdr_bytes = match &int2dds_data.cdr_bytes {
            Some(b) => b,
            None => return InstanceHandle::NIL,
        };

        // Raw FFI path key computation goes exclusively through the shared
        // DynamicData key machinery — the spec-compliant RTPS KeyHash projection
        // (§9.6.4.8) that native Rust/derive and the dynamic path also use. When no
        // full TypeObject is available (name-only keyed topic) the handle is NIL
        // rather than a non-conformant flat-parser approximation.
        if let Some(dts) = &self.dynamic_key_support {
            if let Ok(dyn_data) = deserialize_dynamic_data(cdr_bytes, dts.dynamic_type()) {
                return dts.compute_key(&dyn_data);
            }
        }

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
