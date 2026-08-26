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
        deserialize_dynamic_data, DynamicTypeSupport, FrameLayout, TypeIdentifier, TypeObject,
        TypePlans, TypeRegistry,
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
    /// Compiled layout of the type. Answers the key projection and filter field
    /// access straight from the sample bytes; the shapes it declines — listed on
    /// `int2dds::xtypes::codec_plan` — fall through to `dynamic_key_support`.
    plans: Option<Arc<TypePlans>>,
    /// Canonical key machinery derived from a full TypeObject. When present,
    /// `compute_key` deserializes the sample into a `DynamicData` and delegates to
    /// the shared Rust key path (`serialize_key_cdr`), matching native-Rust/derive
    /// InstanceHandles for every key shape — including composite, float, and nested
    /// members. It is the sole key path: a raw topic without a full TypeObject
    /// yields a NIL InstanceHandle rather than a non-conformant approximation.
    dynamic_key_support: Option<Arc<DynamicTypeSupport>>,
    /// ValueFrame layout for the frame exchange path (`int2dds_topic_frame_*`).
    /// `None` when the type has no full TypeObject or a shape the frame does not
    /// represent; bindings then keep their own codec path.
    frame_layout: Option<Arc<FrameLayout>>,
}

impl RawTypeSupport {
    pub fn new(type_name: String, extensibility: ExtensibilityKind) -> Self {
        Self {
            type_name,
            extensibility,
            has_key: false,
            type_identifier: None,
            type_object: None,
            plans: None,
            dynamic_key_support: None,
            frame_layout: None,
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
            plans: None,
            dynamic_key_support: None,
            frame_layout: None,
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
        // Build the canonical type machinery from the full TypeObject so topics
        // created via the type_info path (C# generated types, Python
        // `_dds_type_info_fields`) compute the same InstanceHandle as native Rust
        // and can read fields for reader-side filtering. Built for keyless topics
        // too, which have no key to compute but may still carry a filter.
        let dynamic_support = if dependencies.is_empty() {
            DynamicTypeSupport::from_type_object(type_object.clone()).ok().map(Arc::new)
        } else {
            let mut registry = TypeRegistry::new();
            for (id, obj) in &dependencies {
                registry.register_type_object_with_id(id, obj.clone());
            }
            DynamicTypeSupport::from_type_object_with_registry(type_object.clone(), &registry)
                .ok()
                .map(Arc::new)
        };
        let plans = dynamic_support
            .as_ref()
            .map(|support| Arc::new(TypePlans::compile(support.dynamic_type())));
        let frame_layout = dynamic_support
            .as_ref()
            .and_then(|support| FrameLayout::compile(support.dynamic_type()))
            .map(Arc::new);
        Self {
            type_name,
            extensibility,
            has_key,
            type_identifier: Some(type_identifier),
            type_object: Some(type_object),
            plans,
            dynamic_key_support: if has_key { dynamic_support } else { None },
            frame_layout,
        }
    }

    pub fn frame_layout(&self) -> Option<Arc<FrameLayout>> {
        self.frame_layout.clone()
    }

    pub fn plans(&self) -> Option<Arc<TypePlans>> {
        self.plans.clone()
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
        // Keep the CDR bytes only when something will read them back: the key
        // projection, or a reader-side filter. A raw topic created without type
        // information carries neither, and gets an empty `Int2DdsData`.
        let need_bytes = self.plans.is_some() || self.dynamic_key_support.is_some();
        Ok(Box::new(crate::data::Int2DdsData {
            cdr_bytes: if need_bytes { Some(data.to_vec()) } else { None },
            plans: self.plans.clone(),
        }))
    }

    fn serialize_key(&self, data: &dyn Any) -> DdsResult<SerializedData> {
        // Canonical RTPS KeyHash CDR (headerless, big-endian, §9.6.4.8) projected
        // out of the full sample bytes — the same projection `compute_key` hashes
        // and native Rust/derive emit. The compiled plan reads it straight off the
        // wire; shapes it does not cover go through the DynamicData machinery, and
        // a topic with no full TypeObject (name-only keyed topic) yields an empty key.
        let int2dds_data = match data.downcast_ref::<crate::data::Int2DdsData>() {
            Some(d) => d,
            None => return Ok(Arc::from(Vec::new())),
        };
        let cdr_bytes = match &int2dds_data.cdr_bytes {
            Some(b) => b,
            None => return Ok(Arc::from(Vec::new())),
        };
        if self.has_key {
            if let Some(Ok(key)) = self.plans.as_ref().and_then(|p| p.serialize_key(cdr_bytes)) {
                return Ok(Arc::from(key));
            }
        }
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

        // Raw FFI path key computation goes exclusively through the spec-compliant
        // RTPS KeyHash projection (§9.6.4.8) that native Rust/derive and the dynamic
        // path also use, whether the compiled plan or the DynamicData machinery
        // performs it. When no full TypeObject is available (name-only keyed topic)
        // the handle is NIL rather than a non-conformant flat-parser approximation.
        if self.has_key {
            if let Some(handle) = self.plans.as_ref().and_then(|p| p.compute_key(cdr_bytes)) {
                return handle;
            }
        }
        if let Some(dts) = &self.dynamic_key_support {
            if let Ok(dyn_data) = deserialize_dynamic_data(cdr_bytes, dts.dynamic_type()) {
                return dts.compute_key(&dyn_data);
            }
        }

        InstanceHandle::NIL
    }

    fn key_info_from_bytes(&self, sample: &[u8]) -> DdsResult<(SerializedData, InstanceHandle)> {
        // The default asks `serialize_key` and `compute_key` in turn, which would
        // walk the sample twice. Both answers come out of one projection here, and
        // the sample never has to be copied into an `Int2DdsData` to be asked.
        if !self.has_key {
            return Ok((Arc::from(Vec::new()), InstanceHandle::NIL));
        }
        if let Some(Ok((key, handle))) = self.plans.as_ref().and_then(|p| p.key_info(sample)) {
            return Ok((Arc::from(key), handle));
        }
        if let Some(dts) = &self.dynamic_key_support {
            if let Ok(dyn_data) = deserialize_dynamic_data(sample, dts.dynamic_type()) {
                return Ok((dts.serialize_key(&dyn_data)?, dts.compute_key(&dyn_data)));
            }
        }
        Ok((Arc::from(Vec::new()), InstanceHandle::NIL))
    }

    fn is_compute_key_provided(&self) -> bool {
        self.has_key
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        self.extensibility
    }

    fn filter_has_field(&self, field_path: &str) -> Option<bool> {
        // The reader-side filter reads fields through the compiled plan; without
        // one (name-only topic, or a shape the plan does not cover) the filter is
        // inert and expressions are accepted unchecked.
        self.plans.as_ref().and_then(|plans| plans.filter_has_field(field_path))
    }

    fn get_type_identifier(&self) -> Option<TypeIdentifier> {
        self.type_identifier.clone()
    }

    fn get_type_object(&self) -> Option<TypeObject> {
        self.type_object.clone()
    }
}
