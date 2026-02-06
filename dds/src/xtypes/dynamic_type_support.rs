//! DynamicTypeSupport - TypeSupport implementation for DynamicData.
//!
//! This module provides TypeSupport trait implementation that enables
//! DynamicData to be used with standard DDS infrastructure (DataReader, DataWriter).

use std::any::{Any, TypeId};
use std::sync::Arc;

use crate::{
    common::instance_handle::InstanceHandle,
    dcps::core::error::{DdsError, DdsResult},
    dcps::topic::type_support::{DdsType, SerializationFormat, TypeSupport},
    rtps::common::types::SerializedData,
    serialize::xcdr::ExtensibilityKind,
    topic::sql::ast::Parameter,
    xtypes::{CompleteTypeObject, TypeIdentifier, TypeObject},
};

use super::dynamic_data::{DynamicData, DynamicValue};
use super::dynamic_serialization::{deserialize_dynamic_data, serialize_dynamic_data};
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
}

impl DynamicTypeSupport {
    /// Create DynamicTypeSupport from a TypeObject.
    ///
    /// This is the primary way to create DynamicTypeSupport, typically
    /// from a TypeObject received during discovery.
    pub fn from_type_object(type_object: TypeObject) -> DdsResult<Self> {
        let complete_type_object = match &type_object {
            TypeObject::Complete(complete) => complete.clone(),
            TypeObject::Minimal(_) => {
                return Err(DdsError::Error(
                    "DynamicTypeSupport requires CompleteTypeObject".to_string(),
                ));
            }
        };

        // Compute TypeIdentifier from the hash
        let hash = type_object.compute_hash();
        let type_identifier = TypeIdentifier::CompleteTypeId(hash);
        let type_name = Self::extract_type_name(&complete_type_object);

        let dynamic_type =
            DynamicType::from_type_object(complete_type_object, type_identifier.clone())
                .map_err(|e| DdsError::Error(e.to_string()))?;

        Ok(Self { dynamic_type: Arc::new(dynamic_type), type_name, type_identifier, type_object })
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
            }
        })
    }
}

impl TypeSupport for DynamicTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<DynamicData>()
    }

    fn get_type_name(&self) -> &str {
        &self.type_name
    }

    fn get_field_value(&self, data: &dyn Any, field_path: &str) -> DdsResult<Parameter> {
        let dynamic_data = data
            .downcast_ref::<DynamicData>()
            .ok_or_else(|| DdsError::Error("Expected DynamicData type".to_string()))?;

        let value = dynamic_data
            .get_value(field_path)
            .ok_or_else(|| DdsError::Error(format!("Field not found: {}", field_path)))?;

        // Convert DynamicValue to Parameter
        dynamic_value_to_parameter(value)
    }

    fn has_field(&self, field_path: &str) -> bool {
        // Check if the field exists in the type
        if let Some(struct_desc) = self.dynamic_type.as_struct() {
            // Handle nested paths
            let parts: Vec<&str> = field_path.split('.').collect();
            if parts.is_empty() {
                return false;
            }

            // For simple paths, just check if member exists
            struct_desc.get_member(parts[0]).is_some()
        } else {
            false
        }
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

        // Serialize only key fields
        let key_members = self.dynamic_type.key_members();
        if key_members.is_empty() {
            return Ok(Arc::from(Vec::new().into_boxed_slice()));
        }

        // Create a new DynamicData with only key fields
        let mut key_data = DynamicData::new(self.dynamic_type.clone());
        for member in &key_members {
            if let Some(value) = dynamic_data.get_value(&member.name) {
                let _ = key_data.set_value(&member.name, value.clone());
            }
        }

        // Serialize key data
        serialize_dynamic_data(&key_data, &SerializationFormat::Cdr)
    }

    fn deserialize_key(&self, serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        if serialized_key.is_empty() {
            return Ok(Box::new(DynamicData::new(self.dynamic_type.clone())));
        }

        // Deserialize key data
        let key_data = deserialize_dynamic_data(serialized_key, &self.dynamic_type)?;
        Ok(Box::new(key_data))
    }

    fn compute_key(&self, data: &dyn Any) -> InstanceHandle {
        let dynamic_data = match data.downcast_ref::<DynamicData>() {
            Some(d) => d,
            None => return InstanceHandle::NIL,
        };

        let key_values = dynamic_data.get_key_values();
        if key_values.is_empty() {
            return InstanceHandle::NIL;
        }

        // Compute MD5 hash of key values
        let mut hasher_data = Vec::new();
        for (name, value) in key_values {
            hasher_data.extend_from_slice(name.as_bytes());
            hasher_data.push(0); // Separator
                                 // Simple value serialization for hashing
            match value {
                DynamicValue::Int32(v) => hasher_data.extend_from_slice(&v.to_le_bytes()),
                DynamicValue::Int64(v) => hasher_data.extend_from_slice(&v.to_le_bytes()),
                DynamicValue::Uint32(v) => hasher_data.extend_from_slice(&v.to_le_bytes()),
                DynamicValue::Uint64(v) => hasher_data.extend_from_slice(&v.to_le_bytes()),
                DynamicValue::String(s) => hasher_data.extend_from_slice(s.as_bytes()),
                _ => {}
            }
        }

        if hasher_data.is_empty() {
            return InstanceHandle::NIL;
        }

        let hash = md5::compute(&hasher_data);
        InstanceHandle::new(hash.0)
    }

    fn is_compute_key_provided(&self) -> bool {
        // Check if any key members exist
        !self.dynamic_type.key_members().is_empty()
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        self.dynamic_type.extensibility()
    }

    fn get_type_identifier(&self) -> Option<TypeIdentifier> {
        Some(self.type_identifier.clone())
    }

    fn get_type_object(&self) -> Option<TypeObject> {
        Some(self.type_object.clone())
    }
}

/// Implement DdsType for DynamicData
impl DdsType for DynamicData {
    type TypeSupport = DynamicTypeSupport;

    fn get_type_name() -> String {
        "DynamicData".to_string()
    }
}

/// Convert DynamicValue to SQL Parameter for content filtering.
fn dynamic_value_to_parameter(value: &DynamicValue) -> DdsResult<Parameter> {
    match value {
        DynamicValue::Boolean(v) => Ok(Parameter::IntegerValue(if *v { 1 } else { 0 })),
        DynamicValue::Int8(v) => Ok(Parameter::IntegerValue(*v as i32)),
        DynamicValue::Int16(v) => Ok(Parameter::IntegerValue(*v as i32)),
        DynamicValue::Int32(v) => Ok(Parameter::IntegerValue(*v)),
        DynamicValue::Int64(v) => Ok(Parameter::IntegerValue(*v as i32)),
        DynamicValue::Uint8(v) => Ok(Parameter::IntegerValue(*v as i32)),
        DynamicValue::Uint16(v) => Ok(Parameter::IntegerValue(*v as i32)),
        DynamicValue::Uint32(v) => Ok(Parameter::IntegerValue(*v as i32)),
        DynamicValue::Uint64(v) => Ok(Parameter::IntegerValue(*v as i32)),
        DynamicValue::Float32(v) => Ok(Parameter::FloatValue(*v as f64)),
        DynamicValue::Float64(v) => Ok(Parameter::FloatValue(*v)),
        DynamicValue::String(v) => Ok(Parameter::String(v.clone())),
        DynamicValue::WString(v) => Ok(Parameter::String(v.clone())),
        DynamicValue::Char8(v) => Ok(Parameter::CharValue(*v)),
        DynamicValue::Byte(v) => Ok(Parameter::IntegerValue(*v as i32)),
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
