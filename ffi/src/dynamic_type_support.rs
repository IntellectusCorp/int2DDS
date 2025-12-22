//! # Dynamic Type Support
//!
//! TypeSupport implementation for runtime-defined types.
//!
//! ## Overview
//!
//! DynamicTypeSupport provides CDR serialization/deserialization for Int2DdsData
//! based on TypeDescriptor definitions. It supports both XCDR v1 and v2.

use std::any::{Any, TypeId};
use std::sync::Arc;

use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::core::error::{DdsError, DdsResult},
    rtps::common::types::SerializedData,
    serialize::{
        cdr::{
            CdrDeserializer, CdrSerializer, CdrSerializerCommon, ExtensibilityKind,
            PrimitiveSerialize, StringSerialize, Xcdr2Deserializer, Xcdr2Serializer,
        },
        core::{BufferManager, DeserializerReader},
    },
    topic::{
        sql::ast::Parameter,
        type_support::{SerializationFormat, TypeSupport},
    },
};

use crate::data::{FieldValue, Int2DdsData};
use crate::type_descriptor::{FieldTypeInfo, Int2DdsTypeDescriptor, Int2DdsXcdrVersion};

/// Dynamic type support - provides TypeSupport for runtime-defined types
#[derive(Debug)]
pub struct DynamicTypeSupport {
    /// Type descriptor defining the structure
    pub descriptor: Arc<Int2DdsTypeDescriptor>,
}

impl DynamicTypeSupport {
    /// Create new DynamicTypeSupport from a TypeDescriptor
    pub fn new(descriptor: Arc<Int2DdsTypeDescriptor>) -> Self {
        Self { descriptor }
    }

    /// Serialize Int2DdsData fields using CDR v1
    fn serialize_cdr(&self, data: &Int2DdsData) -> DdsResult<Vec<u8>> {
        let mut serializer = CdrSerializer::new(true); // true = little endian

        // Write encapsulation header
        serializer.write_encapsulation_header().map_err(|e| DdsError::Error(e.to_string()))?;

        // Serialize each field in order
        for field in &self.descriptor.fields {
            self.serialize_field_cdr(&mut serializer, data, field)?;
        }

        Ok(serializer.into_bytes())
    }

    /// Serialize a single field using CDR v1
    fn serialize_field_cdr(
        &self,
        serializer: &mut CdrSerializer,
        data: &Int2DdsData,
        field: &crate::type_descriptor::FieldDescriptor,
    ) -> DdsResult<()> {
        let value = data
            .get_value(&field.name)
            .ok_or_else(|| DdsError::Error(format!("Missing required field: {}", field.name)))?;

        self.serialize_value_cdr(serializer, value, &field.field_type)
    }

    /// Serialize a FieldValue using CDR v1
    fn serialize_value_cdr(
        &self,
        serializer: &mut CdrSerializer,
        value: &FieldValue,
        field_type: &FieldTypeInfo,
    ) -> DdsResult<()> {
        match (value, field_type) {
            (FieldValue::Bool(v), FieldTypeInfo::Bool) => {
                serializer.serialize_bool(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Int8(v), FieldTypeInfo::Int8) => {
                serializer.serialize_i8(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::UInt8(v), FieldTypeInfo::UInt8) => {
                serializer.serialize_u8(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Int16(v), FieldTypeInfo::Int16) => {
                serializer.serialize_i16(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::UInt16(v), FieldTypeInfo::UInt16) => {
                serializer.serialize_u16(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Int32(v), FieldTypeInfo::Int32) => {
                serializer.serialize_i32(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::UInt32(v), FieldTypeInfo::UInt32) => {
                serializer.serialize_u32(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Int64(v), FieldTypeInfo::Int64) => {
                serializer.serialize_i64(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::UInt64(v), FieldTypeInfo::UInt64) => {
                serializer.serialize_u64(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Float32(v), FieldTypeInfo::Float32) => {
                serializer.serialize_f32(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Float64(v), FieldTypeInfo::Float64) => {
                serializer.serialize_f64(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::String(v), FieldTypeInfo::String { .. }) => {
                serializer.serialize_string(v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Sequence(v), FieldTypeInfo::Sequence { element_type, .. }) => {
                serializer
                    .serialize_u32(v.len() as u32)
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                for item in v {
                    self.serialize_value_cdr(serializer, item, element_type)?;
                }
            }
            // Optimized Bytes serialization - bulk write without per-element overhead
            (FieldValue::Bytes(bytes), FieldTypeInfo::Bytes { .. }) => {
                serializer
                    .serialize_u32(bytes.len() as u32)
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                // Bulk write: directly extend buffer instead of per-byte serialization
                serializer.buffer_mut().extend_from_slice(bytes);
            }
            // Allow Bytes for Sequence<UInt8> for backward compatibility - bulk write
            (FieldValue::Bytes(bytes), FieldTypeInfo::Sequence { element_type, .. })
                if matches!(element_type.as_ref(), FieldTypeInfo::UInt8) =>
            {
                serializer
                    .serialize_u32(bytes.len() as u32)
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                // Bulk write: directly extend buffer instead of per-byte serialization
                serializer.buffer_mut().extend_from_slice(bytes);
            }
            (FieldValue::Array(v), FieldTypeInfo::Array { element_type, .. }) => {
                for item in v {
                    self.serialize_value_cdr(serializer, item, element_type)?;
                }
            }
            (FieldValue::Struct(nested_data), FieldTypeInfo::Struct { descriptor }) => {
                // Recursively serialize nested struct
                let nested_support = DynamicTypeSupport::new(descriptor.clone());
                for field in &descriptor.fields {
                    nested_support.serialize_field_cdr(serializer, nested_data, field)?;
                }
            }
            _ => {
                return Err(DdsError::Error("Type mismatch during serialization".to_string()));
            }
        }
        Ok(())
    }

    /// Deserialize CDR v1 data into Int2DdsData
    fn deserialize_cdr(&self, data: &[u8]) -> DdsResult<Int2DdsData> {
        if data.len() < 4 {
            return Err(DdsError::Error("Data too short for CDR header".to_string()));
        }

        let mut deserializer =
            CdrDeserializer::new(data).map_err(|e| DdsError::Error(e.to_string()))?;

        let mut result = Int2DdsData::new(self.descriptor.clone());

        // Deserialize each field in order
        for field in &self.descriptor.fields {
            let value = self.deserialize_field_cdr(&mut deserializer, &field.field_type)?;
            result.values.insert(field.name.clone(), value);
        }

        Ok(result)
    }

    /// Deserialize a single field using CDR v1
    fn deserialize_field_cdr(
        &self,
        deserializer: &mut CdrDeserializer,
        field_type: &FieldTypeInfo,
    ) -> DdsResult<FieldValue> {
        match field_type {
            FieldTypeInfo::Bool => {
                let v =
                    deserializer.deserialize_bool().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Bool(v))
            }
            FieldTypeInfo::Int8 => {
                let v =
                    deserializer.deserialize_i8().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Int8(v))
            }
            FieldTypeInfo::UInt8 => {
                let v =
                    deserializer.deserialize_u8().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::UInt8(v))
            }
            FieldTypeInfo::Int16 => {
                let v =
                    deserializer.deserialize_i16().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Int16(v))
            }
            FieldTypeInfo::UInt16 => {
                let v =
                    deserializer.deserialize_u16().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::UInt16(v))
            }
            FieldTypeInfo::Int32 => {
                let v =
                    deserializer.deserialize_i32().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Int32(v))
            }
            FieldTypeInfo::UInt32 => {
                let v =
                    deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::UInt32(v))
            }
            FieldTypeInfo::Int64 => {
                let v =
                    deserializer.deserialize_i64().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Int64(v))
            }
            FieldTypeInfo::UInt64 => {
                let v =
                    deserializer.deserialize_u64().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::UInt64(v))
            }
            FieldTypeInfo::Float32 => {
                let v =
                    deserializer.deserialize_f32().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Float32(v))
            }
            FieldTypeInfo::Float64 => {
                let v =
                    deserializer.deserialize_f64().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Float64(v))
            }
            FieldTypeInfo::String { .. } => {
                let v = deserializer
                    .deserialize_string()
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::String(v))
            }
            FieldTypeInfo::Sequence { element_type, .. } => {
                let len =
                    deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?
                        as usize;

                // Optimization: bulk read for Sequence<UInt8>
                if matches!(element_type.as_ref(), FieldTypeInfo::UInt8) {
                    deserializer
                        .check_available(len)
                        .map_err(|e| DdsError::Error(e.to_string()))?;
                    let start = deserializer.get_position();
                    let bytes = deserializer.get_data()[start..start + len].to_vec();
                    deserializer.set_position(start + len);
                    return Ok(FieldValue::Bytes(Arc::from(bytes)));
                }

                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.deserialize_field_cdr(deserializer, element_type)?);
                }
                Ok(FieldValue::Sequence(items))
            }
            FieldTypeInfo::Array { element_type, length } => {
                let mut items = Vec::with_capacity(*length as usize);
                for _ in 0..*length {
                    items.push(self.deserialize_field_cdr(deserializer, element_type)?);
                }
                Ok(FieldValue::Array(items))
            }
            FieldTypeInfo::Struct { descriptor } => {
                let nested_support = DynamicTypeSupport::new(descriptor.clone());
                let mut nested_data = Int2DdsData::new(descriptor.clone());
                for field in &descriptor.fields {
                    let value =
                        nested_support.deserialize_field_cdr(deserializer, &field.field_type)?;
                    nested_data.values.insert(field.name.clone(), value);
                }
                Ok(FieldValue::Struct(Box::new(nested_data)))
            }
            // Optimized Bytes deserialization - bulk read directly into Arc<[u8]>
            FieldTypeInfo::Bytes { .. } => {
                let len =
                    deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?
                        as usize;
                // Bulk read: directly copy slice instead of per-byte deserialization
                deserializer.check_available(len).map_err(|e| DdsError::Error(e.to_string()))?;
                let start = deserializer.get_position();
                let bytes = deserializer.get_data()[start..start + len].to_vec();
                deserializer.set_position(start + len);
                Ok(FieldValue::Bytes(Arc::from(bytes)))
            }
        }
    }

    /// Serialize Int2DdsData using XCDR v2
    fn serialize_xcdr2(&self, data: &Int2DdsData) -> DdsResult<Vec<u8>> {
        let mut serializer = Xcdr2Serializer::new(true, self.descriptor.extensibility); // true = little endian

        // Write encapsulation header
        serializer.write_encapsulation_header().map_err(|e| DdsError::Error(e.to_string()))?;

        // Serialize each field in order
        for field in &self.descriptor.fields {
            self.serialize_field_xcdr2(&mut serializer, data, field)?;
        }

        Ok(serializer.into_bytes())
    }

    /// Serialize a single field using XCDR v2
    fn serialize_field_xcdr2(
        &self,
        serializer: &mut Xcdr2Serializer,
        data: &Int2DdsData,
        field: &crate::type_descriptor::FieldDescriptor,
    ) -> DdsResult<()> {
        let value = data
            .get_value(&field.name)
            .ok_or_else(|| DdsError::Error(format!("Missing required field: {}", field.name)))?;

        self.serialize_value_xcdr2(serializer, value, &field.field_type)
    }

    /// Serialize a FieldValue using XCDR v2
    fn serialize_value_xcdr2(
        &self,
        serializer: &mut Xcdr2Serializer,
        value: &FieldValue,
        field_type: &FieldTypeInfo,
    ) -> DdsResult<()> {
        match (value, field_type) {
            (FieldValue::Bool(v), FieldTypeInfo::Bool) => {
                serializer.serialize_bool(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Int8(v), FieldTypeInfo::Int8) => {
                serializer.serialize_i8(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::UInt8(v), FieldTypeInfo::UInt8) => {
                serializer.serialize_u8(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Int16(v), FieldTypeInfo::Int16) => {
                serializer.serialize_i16(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::UInt16(v), FieldTypeInfo::UInt16) => {
                serializer.serialize_u16(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Int32(v), FieldTypeInfo::Int32) => {
                serializer.serialize_i32(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::UInt32(v), FieldTypeInfo::UInt32) => {
                serializer.serialize_u32(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Int64(v), FieldTypeInfo::Int64) => {
                serializer.serialize_i64(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::UInt64(v), FieldTypeInfo::UInt64) => {
                serializer.serialize_u64(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Float32(v), FieldTypeInfo::Float32) => {
                serializer.serialize_f32(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Float64(v), FieldTypeInfo::Float64) => {
                serializer.serialize_f64(*v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::String(v), FieldTypeInfo::String { .. }) => {
                serializer.serialize_string(v).map_err(|e| DdsError::Error(e.to_string()))?;
            }
            (FieldValue::Sequence(v), FieldTypeInfo::Sequence { element_type, .. }) => {
                serializer
                    .serialize_u32(v.len() as u32)
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                for item in v {
                    self.serialize_value_xcdr2(serializer, item, element_type)?;
                }
            }
            // Optimized Bytes serialization - bulk write without per-element overhead
            (FieldValue::Bytes(bytes), FieldTypeInfo::Bytes { .. }) => {
                serializer
                    .serialize_u32(bytes.len() as u32)
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                // Bulk write: directly extend buffer instead of per-byte serialization
                serializer.buffer_mut().extend_from_slice(bytes);
            }
            // Allow Bytes for Sequence<UInt8> for backward compatibility - bulk write
            (FieldValue::Bytes(bytes), FieldTypeInfo::Sequence { element_type, .. })
                if matches!(element_type.as_ref(), FieldTypeInfo::UInt8) =>
            {
                serializer
                    .serialize_u32(bytes.len() as u32)
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                // Bulk write: directly extend buffer instead of per-byte serialization
                serializer.buffer_mut().extend_from_slice(bytes);
            }
            (FieldValue::Array(v), FieldTypeInfo::Array { element_type, .. }) => {
                for item in v {
                    self.serialize_value_xcdr2(serializer, item, element_type)?;
                }
            }
            (FieldValue::Struct(nested_data), FieldTypeInfo::Struct { descriptor }) => {
                let nested_support = DynamicTypeSupport::new(descriptor.clone());
                for field in &descriptor.fields {
                    nested_support.serialize_field_xcdr2(serializer, nested_data, field)?;
                }
            }
            _ => {
                return Err(DdsError::Error("Type mismatch during serialization".to_string()));
            }
        }
        Ok(())
    }

    /// Deserialize XCDR v2 data into Int2DdsData
    fn deserialize_xcdr2(&self, data: &[u8]) -> DdsResult<Int2DdsData> {
        if data.len() < 4 {
            return Err(DdsError::Error("Data too short for XCDR2 header".to_string()));
        }

        let mut deserializer =
            Xcdr2Deserializer::new(data).map_err(|e| DdsError::Error(e.to_string()))?;

        let mut result = Int2DdsData::new(self.descriptor.clone());

        // Deserialize each field in order
        for field in &self.descriptor.fields {
            let value = self.deserialize_field_xcdr2(&mut deserializer, &field.field_type)?;
            result.values.insert(field.name.clone(), value);
        }

        Ok(result)
    }

    /// Deserialize a single field using XCDR v2
    fn deserialize_field_xcdr2(
        &self,
        deserializer: &mut Xcdr2Deserializer,
        field_type: &FieldTypeInfo,
    ) -> DdsResult<FieldValue> {
        match field_type {
            FieldTypeInfo::Bool => {
                let v =
                    deserializer.deserialize_bool().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Bool(v))
            }
            FieldTypeInfo::Int8 => {
                let v =
                    deserializer.deserialize_i8().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Int8(v))
            }
            FieldTypeInfo::UInt8 => {
                let v =
                    deserializer.deserialize_u8().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::UInt8(v))
            }
            FieldTypeInfo::Int16 => {
                let v =
                    deserializer.deserialize_i16().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Int16(v))
            }
            FieldTypeInfo::UInt16 => {
                let v =
                    deserializer.deserialize_u16().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::UInt16(v))
            }
            FieldTypeInfo::Int32 => {
                let v =
                    deserializer.deserialize_i32().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Int32(v))
            }
            FieldTypeInfo::UInt32 => {
                let v =
                    deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::UInt32(v))
            }
            FieldTypeInfo::Int64 => {
                let v =
                    deserializer.deserialize_i64().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Int64(v))
            }
            FieldTypeInfo::UInt64 => {
                let v =
                    deserializer.deserialize_u64().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::UInt64(v))
            }
            FieldTypeInfo::Float32 => {
                let v =
                    deserializer.deserialize_f32().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Float32(v))
            }
            FieldTypeInfo::Float64 => {
                let v =
                    deserializer.deserialize_f64().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::Float64(v))
            }
            FieldTypeInfo::String { .. } => {
                let v = deserializer
                    .deserialize_string()
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(FieldValue::String(v))
            }
            FieldTypeInfo::Sequence { element_type, .. } => {
                let len =
                    deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?
                        as usize;

                // Optimization: bulk read for Sequence<UInt8>
                if matches!(element_type.as_ref(), FieldTypeInfo::UInt8) {
                    deserializer
                        .check_available(len)
                        .map_err(|e| DdsError::Error(e.to_string()))?;
                    let start = deserializer.get_position();
                    let bytes = deserializer.get_data()[start..start + len].to_vec();
                    deserializer.set_position(start + len);
                    return Ok(FieldValue::Bytes(Arc::from(bytes)));
                }

                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.deserialize_field_xcdr2(deserializer, element_type)?);
                }
                Ok(FieldValue::Sequence(items))
            }
            FieldTypeInfo::Array { element_type, length } => {
                let mut items = Vec::with_capacity(*length as usize);
                for _ in 0..*length {
                    items.push(self.deserialize_field_xcdr2(deserializer, element_type)?);
                }
                Ok(FieldValue::Array(items))
            }
            FieldTypeInfo::Struct { descriptor } => {
                let nested_support = DynamicTypeSupport::new(descriptor.clone());
                let mut nested_data = Int2DdsData::new(descriptor.clone());
                for field in &descriptor.fields {
                    let value =
                        nested_support.deserialize_field_xcdr2(deserializer, &field.field_type)?;
                    nested_data.values.insert(field.name.clone(), value);
                }
                Ok(FieldValue::Struct(Box::new(nested_data)))
            }
            // Optimized Bytes deserialization - bulk read directly into Arc<[u8]>
            FieldTypeInfo::Bytes { .. } => {
                let len =
                    deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?
                        as usize;
                // Bulk read: directly copy slice instead of per-byte deserialization
                deserializer.check_available(len).map_err(|e| DdsError::Error(e.to_string()))?;
                let start = deserializer.get_position();
                let bytes = deserializer.get_data()[start..start + len].to_vec();
                deserializer.set_position(start + len);
                Ok(FieldValue::Bytes(Arc::from(bytes)))
            }
        }
    }

    /// Serialize only key fields
    fn serialize_key_fields(&self, data: &Int2DdsData) -> DdsResult<Vec<u8>> {
        let key_fields = self.descriptor.key_fields();
        if key_fields.is_empty() {
            return Ok(Vec::new());
        }

        let mut serializer = CdrSerializer::new(true); // true = little endian
        serializer.write_encapsulation_header().map_err(|e| DdsError::Error(e.to_string()))?;

        for field in key_fields {
            if let Some(value) = data.get_value(&field.name) {
                self.serialize_value_cdr(&mut serializer, value, &field.field_type)?;
            }
        }

        Ok(serializer.into_bytes())
    }

    /// Detect encoding from data and deserialize accordingly
    fn auto_deserialize(&self, data: &[u8]) -> DdsResult<Int2DdsData> {
        if data.len() < 2 {
            return Err(DdsError::Error("Data too short for encoding detection".to_string()));
        }

        let encoding_id = u16::from_be_bytes([data[0], data[1]]);

        match encoding_id {
            0x0000 | 0x0001 => self.deserialize_cdr(data),
            0x0006 | 0x0007 | 0x0008 | 0x0009 | 0x000A | 0x000B => self.deserialize_xcdr2(data),
            _ => {
                // Try CDR as fallback
                self.deserialize_cdr(data)
            }
        }
    }
}

impl TypeSupport for DynamicTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<Int2DdsData>()
    }

    fn get_type_name(&self) -> &str {
        &self.descriptor.type_name
    }

    fn get_field_value(&self, data: &dyn Any, field_path: &str) -> DdsResult<Parameter> {
        let dynamic_data = data
            .downcast_ref::<Int2DdsData>()
            .ok_or_else(|| DdsError::Error("Expected Int2DdsData".to_string()))?;

        match dynamic_data.get_value(field_path) {
            Some(FieldValue::Bool(v)) => Ok(Parameter::IntegerValue(if *v { 1 } else { 0 })),
            Some(FieldValue::Int8(v)) => Ok(Parameter::IntegerValue(*v as i32)),
            Some(FieldValue::UInt8(v)) => Ok(Parameter::IntegerValue(*v as i32)),
            Some(FieldValue::Int16(v)) => Ok(Parameter::IntegerValue(*v as i32)),
            Some(FieldValue::UInt16(v)) => Ok(Parameter::IntegerValue(*v as i32)),
            Some(FieldValue::Int32(v)) => Ok(Parameter::IntegerValue(*v)),
            Some(FieldValue::UInt32(v)) => Ok(Parameter::IntegerValue(*v as i32)),
            Some(FieldValue::Int64(v)) => Ok(Parameter::IntegerValue(*v as i32)),
            Some(FieldValue::UInt64(v)) => Ok(Parameter::IntegerValue(*v as i32)),
            Some(FieldValue::Float32(v)) => Ok(Parameter::FloatValue(*v as f64)),
            Some(FieldValue::Float64(v)) => Ok(Parameter::FloatValue(*v)),
            Some(FieldValue::String(v)) => Ok(Parameter::String(v.clone())),
            Some(_) => Err(DdsError::Unsupported),
            None => Err(DdsError::Error(format!("Field not found: {}", field_path))),
        }
    }

    fn has_field(&self, field_path: &str) -> bool {
        self.descriptor.get_field(field_path).is_some()
    }

    fn serialize(&self, data: &dyn Any) -> DdsResult<SerializedData> {
        let dynamic_data = data
            .downcast_ref::<Int2DdsData>()
            .ok_or_else(|| DdsError::Error("Expected Int2DdsData".to_string()))?;

        // Use XCDR version specified in the type descriptor
        let bytes = match self.descriptor.xcdr_version {
            Int2DdsXcdrVersion::Xcdr1 => self.serialize_cdr(dynamic_data)?,
            Int2DdsXcdrVersion::Xcdr2 => self.serialize_xcdr2(dynamic_data)?,
        };
        Ok(bytes.into())
    }

    fn deserialize(&self, data: &[u8]) -> DdsResult<Box<dyn Any>> {
        let result = self.auto_deserialize(data)?;
        Ok(Box::new(result))
    }

    fn serialize_with_format(
        &self,
        data: &dyn Any,
        format: &SerializationFormat,
    ) -> DdsResult<SerializedData> {
        let dynamic_data = data
            .downcast_ref::<Int2DdsData>()
            .ok_or_else(|| DdsError::Error("Expected Int2DdsData".to_string()))?;

        let bytes = match format {
            SerializationFormat::Cdr => self.serialize_cdr(dynamic_data)?,
            SerializationFormat::Xcdr { .. } => self.serialize_xcdr2(dynamic_data)?,
        };
        Ok(bytes.into())
    }

    fn deserialize_with_format(
        &self,
        data: &[u8],
        format: &SerializationFormat,
    ) -> DdsResult<Box<dyn Any>> {
        let result = match format {
            SerializationFormat::Cdr => self.deserialize_cdr(data)?,
            SerializationFormat::Xcdr { .. } => self.deserialize_xcdr2(data)?,
        };
        Ok(Box::new(result))
    }

    fn serialize_key(&self, data: &dyn Any) -> DdsResult<SerializedData> {
        let dynamic_data = data
            .downcast_ref::<Int2DdsData>()
            .ok_or_else(|| DdsError::Error("Expected Int2DdsData".to_string()))?;

        let bytes = self.serialize_key_fields(dynamic_data)?;
        Ok(bytes.into())
    }

    fn deserialize_key(&self, _serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        // For now, return empty data - key deserialization is complex
        Ok(Box::new(Int2DdsData::new(self.descriptor.clone())))
    }

    fn compute_key(&self, data: &dyn Any) -> InstanceHandle {
        let dynamic_data = match data.downcast_ref::<Int2DdsData>() {
            Some(d) => d,
            None => return InstanceHandle::NIL,
        };

        // Compute key from key fields
        let key_fields = self.descriptor.key_fields();
        if key_fields.is_empty() {
            return InstanceHandle::NIL;
        }

        // Hash computation directly without intermediate Vec allocations
        let mut hash = [0u8; 16];
        let mut hash_idx = 0usize;

        for field in key_fields {
            if let Some(value) = dynamic_data.get_value(&field.name) {
                hash_value_direct(value, &mut hash, &mut hash_idx);
            }
        }

        InstanceHandle::new(hash)
    }

    fn is_compute_key_provided(&self) -> bool {
        self.descriptor.has_key()
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        self.descriptor.extensibility
    }
}

/// Helper to XOR a byte into the hash buffer
#[inline(always)]
fn hash_byte(hash: &mut [u8; 16], hash_idx: &mut usize, byte: u8) {
    hash[*hash_idx % 16] ^= byte;
    *hash_idx += 1;
}

/// Helper to XOR a byte slice into the hash buffer
#[inline(always)]
fn hash_bytes(hash: &mut [u8; 16], hash_idx: &mut usize, bytes: &[u8]) {
    for &b in bytes {
        hash[*hash_idx % 16] ^= b;
        *hash_idx += 1;
    }
}

/// Optimized hash function that directly XORs value bytes into hash buffer
/// Avoids all intermediate Vec<u8> allocations
fn hash_value_direct(value: &FieldValue, hash: &mut [u8; 16], hash_idx: &mut usize) {
    match value {
        FieldValue::Bool(v) => hash_byte(hash, hash_idx, *v as u8),
        FieldValue::Int8(v) => hash_byte(hash, hash_idx, *v as u8),
        FieldValue::UInt8(v) => hash_byte(hash, hash_idx, *v),
        FieldValue::Int16(v) => hash_bytes(hash, hash_idx, &v.to_le_bytes()),
        FieldValue::UInt16(v) => hash_bytes(hash, hash_idx, &v.to_le_bytes()),
        FieldValue::Int32(v) => hash_bytes(hash, hash_idx, &v.to_le_bytes()),
        FieldValue::UInt32(v) => hash_bytes(hash, hash_idx, &v.to_le_bytes()),
        FieldValue::Int64(v) => hash_bytes(hash, hash_idx, &v.to_le_bytes()),
        FieldValue::UInt64(v) => hash_bytes(hash, hash_idx, &v.to_le_bytes()),
        FieldValue::Float32(v) => hash_bytes(hash, hash_idx, &v.to_le_bytes()),
        FieldValue::Float64(v) => hash_bytes(hash, hash_idx, &v.to_le_bytes()),
        FieldValue::String(v) => hash_bytes(hash, hash_idx, v.as_bytes()),
        FieldValue::Bytes(bytes) => hash_bytes(hash, hash_idx, bytes),
        FieldValue::Sequence(items) | FieldValue::Array(items) => {
            for item in items {
                hash_value_direct(item, hash, hash_idx);
            }
        }
        FieldValue::Struct(nested) => {
            for value in nested.values.values() {
                hash_value_direct(value, hash, hash_idx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_descriptor::FieldTypeInfo;

    fn create_test_descriptor() -> Arc<Int2DdsTypeDescriptor> {
        let mut desc = Int2DdsTypeDescriptor::new("TestType");
        desc.add_field("id", FieldTypeInfo::UInt32, true);
        desc.add_field("message", FieldTypeInfo::String { max_length: 256 }, false);
        desc.add_field("value", FieldTypeInfo::Float64, false);
        Arc::new(desc)
    }

    #[test]
    fn test_serialize_deserialize_cdr() {
        let desc = create_test_descriptor();
        let support = DynamicTypeSupport::new(desc.clone());

        let mut data = Int2DdsData::new(desc);
        data.values.insert("id".to_string(), FieldValue::UInt32(123));
        data.values.insert("message".to_string(), FieldValue::String("hello".to_string()));
        data.values.insert("value".to_string(), FieldValue::Float64(3.14));

        // Serialize
        let serialized = support.serialize_cdr(&data).unwrap();
        assert!(!serialized.is_empty());

        // Deserialize
        let deserialized = support.deserialize_cdr(&serialized).unwrap();

        // Verify
        assert!(matches!(deserialized.get_value("id"), Some(FieldValue::UInt32(123))));
        assert!(
            matches!(deserialized.get_value("message"), Some(FieldValue::String(s)) if s == "hello")
        );
    }

    #[test]
    fn test_serialize_deserialize_xcdr2() {
        let desc = create_test_descriptor();
        let support = DynamicTypeSupport::new(desc.clone());

        let mut data = Int2DdsData::new(desc);
        data.values.insert("id".to_string(), FieldValue::UInt32(456));
        data.values.insert("message".to_string(), FieldValue::String("world".to_string()));
        data.values.insert("value".to_string(), FieldValue::Float64(2.71));

        // Serialize
        let serialized = support.serialize_xcdr2(&data).unwrap();
        assert!(!serialized.is_empty());

        // Deserialize
        let deserialized = support.deserialize_xcdr2(&serialized).unwrap();

        // Verify
        assert!(matches!(deserialized.get_value("id"), Some(FieldValue::UInt32(456))));
        assert!(
            matches!(deserialized.get_value("message"), Some(FieldValue::String(s)) if s == "world")
        );
    }

    #[test]
    fn test_key_serialization() {
        let desc = create_test_descriptor();
        let support = DynamicTypeSupport::new(desc.clone());

        let mut data = Int2DdsData::new(desc);
        data.values.insert("id".to_string(), FieldValue::UInt32(789));
        data.values.insert("message".to_string(), FieldValue::String("test".to_string()));
        data.values.insert("value".to_string(), FieldValue::Float64(1.0));

        let key_bytes = support.serialize_key_fields(&data).unwrap();
        assert!(!key_bytes.is_empty());
    }

    #[test]
    fn test_compute_key() {
        let desc = create_test_descriptor();
        let support = DynamicTypeSupport::new(desc.clone());

        let mut data = Int2DdsData::new(desc);
        data.values.insert("id".to_string(), FieldValue::UInt32(100));
        data.values.insert("message".to_string(), FieldValue::String("msg".to_string()));
        data.values.insert("value".to_string(), FieldValue::Float64(0.0));

        let handle = support.compute_key(&data as &dyn Any);
        assert_ne!(handle, InstanceHandle::NIL);
    }
}
