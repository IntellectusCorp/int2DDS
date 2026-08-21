//! DynamicTypeSupport - TypeSupport implementation for DynamicData.
//!
//! This module provides TypeSupport trait implementation that enables
//! DynamicData to be used with standard DDS infrastructure (DataReader, DataWriter).

use std::any::{Any, TypeId};
use std::sync::Arc;

use crate::{
    common::instance_handle::InstanceHandle,
    dcps::core::error::{DdsError, DdsResult},
    dcps::topic::type_support::{DdsType, FieldAccessor, SerializationFormat, TypeSupport},
    rtps::common::types::SerializedData,
    serialize::xcdr::ExtensibilityKind,
    topic::sql::ast::Parameter,
    xtypes::{CompleteTypeObject, TypeIdentifier, TypeObject, TypeRegistry},
};

use super::dynamic_data::{DynamicData, DynamicValue};
use super::dynamic_serialization::{
    deserialize_dynamic_data, deserialize_key_cdr, key_holder_max_size, serialize_dynamic_data,
    serialize_key_cdr,
};
use super::dynamic_type::DynamicType;

/// TypeSupport implementation for DynamicData.
///
/// This enables DynamicData to be used with DataReader<DynamicData> and DataWriter<DynamicData>.
/// Created from a TypeObject received during discovery.
#[derive(Clone, Debug)]
pub struct DynamicTypeSupport {
    /// The dynamic type descriptor
    dynamic_type: Arc<DynamicType>,
    /// Type name
    type_name: String,
    /// TypeIdentifier for this type
    type_identifier: TypeIdentifier,
    /// Original TypeObject
    type_object: TypeObject,
    /// Transitive closure of nested TypeObjects, so a dynamic DataWriter can
    /// advertise dependencies for cross-participant TypeLookup. Empty when built
    /// without a registry.
    dependencies: Vec<(TypeIdentifier, TypeObject)>,
}

impl DynamicTypeSupport {
    /// Create DynamicTypeSupport from a TypeObject.
    pub fn from_type_object(type_object: TypeObject) -> DdsResult<Self> {
        Self::from_complete_with(type_object, |complete, type_identifier| {
            DynamicType::from_type_object(complete, type_identifier.clone())
                .map_err(|e| DdsError::Error(e.to_string()))
        })
    }

    pub fn from_type_object_with_registry(
        type_object: TypeObject,
        registry: &TypeRegistry,
    ) -> DdsResult<Self> {
        let mut support = Self::from_complete_with(type_object, |complete, type_identifier| {
            DynamicType::from_type_object_with_registry(
                Arc::new(complete),
                type_identifier.clone(),
                registry,
            )
            .map_err(|e| DdsError::Error(e.to_string()))
        })?;
        if let TypeObject::Complete(complete) = &support.type_object {
            support.dependencies = registry.dependency_closure_of(complete);
        }
        Ok(support)
    }

    fn from_complete_with(
        type_object: TypeObject,
        build: impl FnOnce(CompleteTypeObject, &TypeIdentifier) -> DdsResult<DynamicType>,
    ) -> DdsResult<Self> {
        let complete_type_object = match &type_object {
            TypeObject::Complete(complete) => complete.clone(),
            TypeObject::Minimal(_) => {
                return Err(DdsError::Error(
                    "DynamicTypeSupport requires CompleteTypeObject".to_string(),
                ));
            }
        };

        let hash = type_object.compute_hash();
        let type_identifier = TypeIdentifier::CompleteTypeId(hash);
        let type_name = Self::extract_type_name(&complete_type_object);

        let dynamic_type = build(complete_type_object, &type_identifier)?;

        Ok(Self {
            dynamic_type: Arc::new(dynamic_type),
            type_name,
            type_identifier,
            type_object,
            dependencies: Vec::new(),
        })
    }

    /// Create DynamicTypeSupport from a CompleteTypeObject directly.
    pub fn from_complete_type_object(complete: CompleteTypeObject) -> DdsResult<Self> {
        let type_object = TypeObject::Complete(complete);
        Self::from_type_object(type_object)
    }

    fn extract_type_name(complete: &CompleteTypeObject) -> String {
        match complete {
            CompleteTypeObject::Struct(s) => s.header.detail.type_name.clone(),
            CompleteTypeObject::Enum(e) => e.header.detail.type_name.clone(),
            CompleteTypeObject::Union(u) => u.header.type_name.clone(),
            CompleteTypeObject::Alias(a) => a.header.type_name.clone(),
            CompleteTypeObject::Bitmask(b) => b.header.detail.type_name.clone(),
            CompleteTypeObject::Bitset(b) => b.header.type_name.clone(),
        }
    }

    /// Get the DynamicType descriptor.
    pub fn dynamic_type(&self) -> &Arc<DynamicType> {
        &self.dynamic_type
    }

    /// Create a new DynamicData instance for this type.
    pub fn create_data(&self) -> DynamicData {
        DynamicData::new(self.dynamic_type.clone())
    }

    /// Get the TypeIdentifier.
    pub fn type_identifier(&self) -> &TypeIdentifier {
        &self.type_identifier
    }

    /// Get the original TypeObject.
    pub fn type_object_ref(&self) -> &TypeObject {
        &self.type_object
    }
}

impl Default for DynamicTypeSupport {
    fn default() -> Self {
        // Create a placeholder DynamicTypeSupport
        // This is required by DdsType trait but should not be used directly
        let empty_struct = crate::xtypes::CompleteStructType::new(
            crate::xtypes::TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "DynamicData".to_string(),
            None,
        );
        let type_object = TypeObject::Complete(CompleteTypeObject::Struct(empty_struct));

        Self::from_type_object(type_object).unwrap_or_else(|_| {
            // Fallback - should never happen
            Self {
                dynamic_type: Arc::new(
                    DynamicType::from_type_object(
                        CompleteTypeObject::Struct(crate::xtypes::CompleteStructType::default()),
                        TypeIdentifier::None,
                    )
                    .unwrap(),
                ),
                type_name: "DynamicData".to_string(),
                type_identifier: TypeIdentifier::None,
                type_object: TypeObject::Complete(CompleteTypeObject::Struct(
                    crate::xtypes::CompleteStructType::default(),
                )),
                dependencies: Vec::new(),
            }
        })
    }
}

impl FieldAccessor for DynamicTypeSupport {
    fn get_field_value(&self, data: &dyn Any, field_path: &str) -> DdsResult<Parameter> {
        let dynamic_data = data
            .downcast_ref::<DynamicData>()
            .ok_or_else(|| DdsError::Error("Expected DynamicData type".to_string()))?;

        let value = dynamic_data
            .get_value(field_path)
            .ok_or_else(|| DdsError::Error(format!("Field not found: {}", field_path)))?;

        dynamic_value_to_parameter(value)
    }

    fn has_field(&self, field_path: &str) -> bool {
        if let Some(struct_desc) = self.dynamic_type.as_struct() {
            let parts: Vec<&str> = field_path.split('.').collect();
            if parts.is_empty() {
                return false;
            }
            struct_desc.get_member(parts[0]).is_some()
        } else {
            false
        }
    }
}

impl TypeSupport for DynamicTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<DynamicData>()
    }

    fn get_type_name(&self) -> &str {
        &self.type_name
    }

    fn serialize(
        &self,
        data: &dyn Any,
        format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData> {
        let dynamic_data = data
            .downcast_ref::<DynamicData>()
            .ok_or_else(|| DdsError::Error("Expected DynamicData type".to_string()))?;

        let default_format = SerializationFormat::Cdr;
        let format = format.unwrap_or(&default_format);
        serialize_dynamic_data(dynamic_data, format)
    }

    fn deserialize(
        &self,
        data: &[u8],
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        let dynamic_data = deserialize_dynamic_data(data, &self.dynamic_type)?;
        Ok(Box::new(dynamic_data))
    }

    fn serialize_key(&self, data: &dyn Any) -> DdsResult<SerializedData> {
        let dynamic_data = data
            .downcast_ref::<DynamicData>()
            .ok_or_else(|| DdsError::Error("Expected DynamicData type".to_string()))?;

        // Canonical RTPS KeyHash CDR: big-endian, member order, no encapsulation header.
        let (key_cdr, _single) = serialize_key_cdr(dynamic_data)?;
        Ok(Arc::from(key_cdr.into_boxed_slice()))
    }

    fn deserialize_key(&self, serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        if serialized_key.is_empty() {
            return Ok(Box::new(DynamicData::new(self.dynamic_type.clone())));
        }

        let key_data = deserialize_key_cdr(serialized_key, &self.dynamic_type)?;
        Ok(Box::new(key_data))
    }

    fn serialize_key_payload(
        &self,
        data: &dyn Any,
        format: &SerializationFormat,
    ) -> DdsResult<SerializedData> {
        // Wrap the canonical KeyHash body (big-endian, max-align-4, headerless FINAL
        // key-holder projection) as a wire serializedKey with the representation-correct
        // encapsulation id, so peers frame it as the topic's representation instead of
        // the default's always-CDR_BE. This is the raw/dynamic path used when no typed
        // value is available (e.g. FFI dispose from stored key bytes). Byte-exact XCDR1
        // 8-byte alignment and nested DELIMITED framing are deferred; the reader's
        // primary instance match is the 16-byte KeyHash inline QoS, not this body.
        let body = self.serialize_key(data)?;
        let mut payload = Vec::with_capacity(body.len() + 8);
        match format {
            SerializationFormat::Cdr => {
                payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // CDR_BE
                payload.extend_from_slice(&body);
            }
            SerializationFormat::Xcdr { extensibility_kind, .. } => {
                if matches!(extensibility_kind, ExtensibilityKind::Final) {
                    payload.extend_from_slice(&[0x00, 0x06, 0x00, 0x00]); // PLAIN_CDR2_BE
                    payload.extend_from_slice(&body);
                } else {
                    payload.extend_from_slice(&[0x00, 0x08, 0x00, 0x00]); // DELIMITED_CDR2_BE
                    payload.extend_from_slice(&(body.len() as u32).to_be_bytes()); // DHEADER
                    payload.extend_from_slice(&body);
                }
            }
        }
        Ok(Arc::from(payload.into_boxed_slice()))
    }

    fn compute_key(&self, data: &dyn Any) -> InstanceHandle {
        let dynamic_data = match data.downcast_ref::<DynamicData>() {
            Some(d) => d,
            None => return InstanceHandle::NIL,
        };

        match serialize_key_cdr(dynamic_data) {
            Ok((key_cdr, _single)) if !key_cdr.is_empty() => {
                // RTPS KeyHash step 5: the raw-vs-MD5 decision is made on the key
                // holder's *maximum* serialized size, not the actual length.
                match key_holder_max_size(&self.dynamic_type) {
                    Some(n) if n <= 16 => InstanceHandle::from_key_cdr(&key_cdr),
                    _ => InstanceHandle::from_key_cdr_hashed(&key_cdr),
                }
            }
            _ => InstanceHandle::NIL,
        }
    }

    fn is_compute_key_provided(&self) -> bool {
        // Check if any key members exist
        !self.dynamic_type.key_members().is_empty()
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        self.dynamic_type.extensibility()
    }

    fn filter_has_field(&self, field_path: &str) -> Option<bool> {
        Some(FieldAccessor::has_field(self, field_path))
    }

    fn get_type_identifier(&self) -> Option<TypeIdentifier> {
        Some(self.type_identifier.clone())
    }

    fn get_type_object(&self) -> Option<TypeObject> {
        Some(self.type_object.clone())
    }

    fn get_type_object_closure(&self) -> Vec<(TypeIdentifier, TypeObject)> {
        let mut out = Vec::with_capacity(1 + self.dependencies.len());
        out.push((self.type_identifier.clone(), self.type_object.clone()));
        out.extend(self.dependencies.iter().cloned());
        out
    }
}

/// Implement DdsType for DynamicData
impl DdsType for DynamicData {
    type TypeSupport = DynamicTypeSupport;
    type FieldAccessor = DynamicTypeSupport;

    fn get_type_name() -> String {
        "DynamicData".to_string()
    }
}

/// Convert DynamicValue to SQL Parameter for content filtering.
fn dynamic_value_to_parameter(value: &DynamicValue) -> DdsResult<Parameter> {
    match value {
        DynamicValue::Boolean(v) => Ok(Parameter::IntegerValue(if *v { 1 } else { 0 })),
        DynamicValue::Int8(v) => Ok(Parameter::IntegerValue(*v as i128)),
        DynamicValue::Int16(v) => Ok(Parameter::IntegerValue(*v as i128)),
        DynamicValue::Int32(v) => Ok(Parameter::IntegerValue(*v as i128)),
        DynamicValue::Int64(v) => Ok(Parameter::IntegerValue(*v as i128)),
        DynamicValue::Uint8(v) => Ok(Parameter::IntegerValue(*v as i128)),
        DynamicValue::Uint16(v) => Ok(Parameter::IntegerValue(*v as i128)),
        DynamicValue::Uint32(v) => Ok(Parameter::IntegerValue(*v as i128)),
        DynamicValue::Uint64(v) => Ok(Parameter::IntegerValue(*v as i128)),
        DynamicValue::Float32(v) => Ok(Parameter::FloatValue(*v as f64)),
        DynamicValue::Float64(v) => Ok(Parameter::FloatValue(*v)),
        DynamicValue::String(v) => Ok(Parameter::String(v.clone())),
        DynamicValue::WString(v) => Ok(Parameter::String(v.clone())),
        DynamicValue::Char8(v) => Ok(Parameter::CharValue(*v)),
        DynamicValue::Byte(v) => Ok(Parameter::IntegerValue(*v as i128)),
        DynamicValue::Enum { name, value: _ } => {
            Ok(Parameter::EnumeratedValue { type_name: None, value: name.clone() })
        }
        _ => Err(DdsError::Error("Cannot convert complex DynamicValue to Parameter".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xtypes::{CompleteStructMember, CompleteStructType, MemberFlag, TypeFlag};

    fn create_test_type_object() -> TypeObject {
        let mut struct_type = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "TestStruct".to_string(),
            None,
        );

        struct_type.add_member(CompleteStructMember::new(
            0,
            MemberFlag::new(
                crate::xtypes::TryConstructKind::Discard,
                false,
                false,
                false,
                true,
                false,
            ),
            TypeIdentifier::Int32,
            "id".to_string(),
        ));
        struct_type.add_member(CompleteStructMember::new(
            1,
            MemberFlag::new(
                crate::xtypes::TryConstructKind::Discard,
                false,
                false,
                false,
                false,
                false,
            ),
            TypeIdentifier::String8,
            "message".to_string(),
        ));

        TypeObject::Complete(CompleteTypeObject::Struct(struct_type))
    }

    #[test]
    fn test_dynamic_type_support_creation() {
        let type_object = create_test_type_object();
        let type_support = DynamicTypeSupport::from_type_object(type_object).unwrap();

        assert_eq!(type_support.get_type_name(), "TestStruct");
        assert!(type_support.is_compute_key_provided());
    }

    #[test]
    fn test_create_data() {
        let type_object = create_test_type_object();
        let type_support = DynamicTypeSupport::from_type_object(type_object).unwrap();

        let data = type_support.create_data();
        assert_eq!(data.type_name(), "TestStruct");
    }

    #[test]
    fn test_serialize_deserialize() {
        let type_object = create_test_type_object();
        let type_support = DynamicTypeSupport::from_type_object(type_object).unwrap();

        let mut data = type_support.create_data();
        data.set("id", 42i32).unwrap();
        data.set("message", "Hello!").unwrap();

        // Serialize
        let serialized = type_support.serialize(&data as &dyn Any, None).unwrap();

        // Deserialize
        let deserialized_any = type_support.deserialize(&serialized, None).unwrap();
        let deserialized = deserialized_any.downcast_ref::<DynamicData>().unwrap();

        assert_eq!(deserialized.get::<i32>("id").unwrap(), 42);
        assert_eq!(deserialized.get::<String>("message").unwrap(), "Hello!");
    }

    #[test]
    fn test_has_field() {
        let type_object = create_test_type_object();
        let type_support = DynamicTypeSupport::from_type_object(type_object).unwrap();

        assert!(type_support.has_field("id"));
        assert!(type_support.has_field("message"));
        assert!(!type_support.has_field("nonexistent"));
    }

    #[test]
    fn test_compute_key() {
        let type_object = create_test_type_object();
        let type_support = DynamicTypeSupport::from_type_object(type_object).unwrap();

        let mut data = type_support.create_data();
        data.set("id", 123i32).unwrap();
        data.set("message", "test").unwrap();

        let key = type_support.compute_key(&data as &dyn Any);
        assert_ne!(key, InstanceHandle::NIL);
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod nested_enum_width_tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use crate::dcps::topic::type_support::{DdsType, SerializationFormat};
    use crate::serialize::cdr::{
        CdrSerialize, CdrSerializer, ExtensibilityKind, PrimitiveSerialize, Xcdr2Serializer,
        XcdrDeserialize, XcdrDeserializer, XcdrSerialize,
    };
    use crate::serialize::{BufferManager, DeserializerReader};
    use crate::xtypes::{
        serialize_dynamic_data, CollectionElementFlag, CompleteStructMember, CompleteStructType,
        CompleteTypeObject, DynamicData, DynamicType, DynamicTypeKind, DynamicTypeSupport,
        DynamicValue, EquivalenceHash, HasTypeObject, MemberFlag, PlainCollectionHeader,
        TryConstructKind, TypeFlag, TypeIdentifier, TypeObject, TypeRegistry,
    };
    fn hash_of(id: &TypeIdentifier) -> EquivalenceHash {
        match id {
            TypeIdentifier::CompleteTypeId(h) | TypeIdentifier::MinimalTypeId(h) => *h,
            other => panic!("expected a hash-based type identifier, got {:?}", other),
        }
    }

    fn concrete_cdr<T: CdrSerialize>(value: &T) -> Vec<u8> {
        let mut serializer = CdrSerializer::with_capacity(true, 256);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();
        serializer.into_bytes()
    }

    fn dynamic_bytes(data: &DynamicData, format: &SerializationFormat) -> Vec<u8> {
        serialize_dynamic_data(data, format).unwrap().to_vec()
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds")]
    #[repr(u8)]
    enum Small8 {
        A,
        B,
        C,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds")]
    #[repr(i16)]
    enum Small16 {
        X,
        Y,
        Z,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct EnumHolder {
        v8: Small8,
        v16: Small16,
        list: Vec<Small8>,
    }

    fn build_dynamic_enum_holder(dt: &Arc<DynamicType>) -> DynamicData {
        let e = |value: i32| DynamicValue::Enum { name: String::new(), value };
        let mut data = DynamicData::new(dt.clone());
        data.set_value("v8", e(2)).unwrap();
        data.set_value("v16", e(2)).unwrap();
        data.set_value("list", DynamicValue::Sequence(vec![e(0), e(1)])).unwrap();
        data
    }

    fn concrete_enum_holder() -> EnumHolder {
        EnumHolder { v8: Small8::C, v16: Small16::Z, list: vec![Small8::A, Small8::B] }
    }

    fn enum_holder_type_object() -> TypeObject {
        let small8_hash = hash_of(&Small8::type_identifier());
        let small16_hash = hash_of(&Small16::type_identifier());
        let flag = || MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false);

        let mut outer = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "EnumHolderSupport".into(),
            None,
        );
        outer.add_member(CompleteStructMember::new(
            0,
            flag(),
            TypeIdentifier::CompleteTypeId(small8_hash),
            "v8".to_string(),
        ));
        outer.add_member(CompleteStructMember::new(
            1,
            flag(),
            TypeIdentifier::CompleteTypeId(small16_hash),
            "v16".to_string(),
        ));
        outer.add_member(CompleteStructMember::new(
            2,
            flag(),
            TypeIdentifier::PlainSequenceLarge {
                header: PlainCollectionHeader::default(),
                bound: 0,
                element_identifier: Box::new(TypeIdentifier::CompleteTypeId(small8_hash)),
            },
            "list".to_string(),
        ));
        TypeObject::Complete(CompleteTypeObject::Struct(outer))
    }

    fn enum_holder_registry() -> TypeRegistry {
        let mut registry = TypeRegistry::new();
        registry.register_complete(
            hash_of(&Small8::type_identifier()),
            "Small8".into(),
            Small8::complete_type_object(),
        );
        registry.register_complete(
            hash_of(&Small16::type_identifier()),
            "Small16".into(),
            Small16::complete_type_object(),
        );
        registry
    }

    #[test]
    fn support_with_registry_resolves_nested_enum_width() {
        let support = DynamicTypeSupport::from_type_object_with_registry(
            enum_holder_type_object(),
            &enum_holder_registry(),
        )
        .unwrap();
        let dynamic = build_dynamic_enum_holder(support.dynamic_type());
        assert_eq!(
            dynamic_bytes(&dynamic, &SerializationFormat::Cdr),
            concrete_cdr(&concrete_enum_holder())
        );
    }

    #[test]
    fn support_without_registry_does_not_resolve_nested_enum_width() {
        let support = DynamicTypeSupport::from_type_object(enum_holder_type_object()).unwrap();
        let dynamic = build_dynamic_enum_holder(support.dynamic_type());
        let codegen = concrete_cdr(&concrete_enum_holder());

        match serialize_dynamic_data(&dynamic, &SerializationFormat::Cdr) {
            Ok(bytes) => assert_ne!(bytes.to_vec(), codegen),
            Err(_) => {}
        }
    }
}
