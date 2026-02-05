#![allow(clippy::only_used_in_recursion)]
#![allow(clippy::manual_range_patterns)]

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
            PrimitiveSerialize, SequenceSerialize, StringSerialize, Xcdr2Deserializer,
            Xcdr2Serializer,
        },
        core::{BufferManager, DeserializerReader},
    },
    topic::{
        sql::ast::Parameter,
        type_support::{SerializationFormat, TypeSupport},
    },
    xtypes::{
        CollectionElementFlag, CommonStructMember, CompleteMemberDetail, CompleteStructMember,
        CompleteStructType, EquivalenceKind as XtypesEquivalenceKind, MemberFlag,
        MinimalStructMember, MinimalStructType, PlainCollectionHeader, TryConstructKind, TypeFlag,
        TypeIdentifier, TypeObject,
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

    /// Estimate the serialized size of data for buffer pre-allocation.
    /// This avoids repeated Vec reallocations during serialization.
    fn estimate_serialized_size(&self, data: &Int2DdsData) -> usize {
        let mut size = 4; // CDR encapsulation header (4 bytes)
        for (index, field) in self.descriptor.fields.iter().enumerate() {
            if let Some(value) = data.get_value_by_index(index) {
                size += Self::estimate_value_size(value, &field.field_type);
            }
        }
        size
    }

    /// Estimate the serialized size of a single field value
    fn estimate_value_size(value: &FieldValue, field_type: &FieldTypeInfo) -> usize {
        match (value, field_type) {
            (FieldValue::Bool(_), FieldTypeInfo::Bool) => 1,
            (FieldValue::Int8(_), FieldTypeInfo::Int8) => 1,
            (FieldValue::UInt8(_), FieldTypeInfo::UInt8) => 1,
            (FieldValue::Int16(_), FieldTypeInfo::Int16) => 2 + 1, // +1 for potential alignment
            (FieldValue::UInt16(_), FieldTypeInfo::UInt16) => 2 + 1,
            (FieldValue::Int32(_), FieldTypeInfo::Int32) => 4 + 3, // +3 for potential alignment
            (FieldValue::UInt32(_), FieldTypeInfo::UInt32) => 4 + 3,
            (FieldValue::Int64(_), FieldTypeInfo::Int64) => 8 + 7, // +7 for potential alignment
            (FieldValue::UInt64(_), FieldTypeInfo::UInt64) => 8 + 7,
            (FieldValue::Float32(_), FieldTypeInfo::Float32) => 4 + 3,
            (FieldValue::Float64(_), FieldTypeInfo::Float64) => 8 + 7,
            (FieldValue::String(s), FieldTypeInfo::String { .. }) => {
                4 + 3 + s.len() + 1 // length(4) + alignment(3) + data + null terminator
            }
            (FieldValue::Bytes(bytes), FieldTypeInfo::Bytes { .. }) => {
                4 + bytes.len() // length prefix + data
            }
            (FieldValue::Bytes(bytes), FieldTypeInfo::Sequence { element_type, .. })
                if matches!(element_type.as_ref(), FieldTypeInfo::UInt8) =>
            {
                4 + bytes.len()
            }
            (FieldValue::Sequence(v), FieldTypeInfo::Sequence { element_type, .. }) => {
                let element_size = Self::estimate_element_type_size(element_type);
                4 + 7 + v.len() * element_size // length + alignment + elements
            }
            (FieldValue::Array(v), FieldTypeInfo::Array { element_type, .. }) => {
                let element_size = Self::estimate_element_type_size(element_type);
                7 + v.len() * element_size // alignment + elements (no length prefix)
            }
            (FieldValue::Struct(nested_data), FieldTypeInfo::Struct { descriptor }) => {
                let mut size = 0;
                for (index, field) in descriptor.fields.iter().enumerate() {
                    if let Some(value) = nested_data.get_value_by_index(index) {
                        size += Self::estimate_value_size(value, &field.field_type);
                    }
                }
                size
            }
            _ => 16, // Default estimate for unknown types
        }
    }

    /// Estimate size of a single element based on type info
    fn estimate_element_type_size(element_type: &FieldTypeInfo) -> usize {
        match element_type {
            FieldTypeInfo::Bool | FieldTypeInfo::Int8 | FieldTypeInfo::UInt8 => 1,
            FieldTypeInfo::Int16 | FieldTypeInfo::UInt16 => 2 + 1,
            FieldTypeInfo::Int32 | FieldTypeInfo::UInt32 | FieldTypeInfo::Float32 => 4 + 3,
            FieldTypeInfo::Int64 | FieldTypeInfo::UInt64 | FieldTypeInfo::Float64 => 8 + 7,
            FieldTypeInfo::String { max_length } => 4 + *max_length as usize,
            FieldTypeInfo::Bytes { max_length } => 4 + *max_length as usize,
            _ => 32, // Conservative estimate for nested types
        }
    }

    /// Serialize Int2DdsData fields using CDR v1
    fn serialize_cdr(&self, data: &Int2DdsData) -> DdsResult<Vec<u8>> {
        // Pre-allocate buffer based on estimated size to avoid reallocations
        let estimated_size = self.estimate_serialized_size(data);
        let mut serializer = CdrSerializer::with_capacity(true, estimated_size);

        // Write encapsulation header
        serializer.write_encapsulation_header().map_err(|e| DdsError::Error(e.to_string()))?;

        // Serialize each field in order
        for (index, field) in self.descriptor.fields.iter().enumerate() {
            self.serialize_field_cdr(&mut serializer, data, index, field)?;
        }

        Ok(serializer.into_bytes())
    }

    /// Serialize a single field using CDR v1
    fn serialize_field_cdr(
        &self,
        serializer: &mut CdrSerializer,
        data: &Int2DdsData,
        index: usize,
        field: &crate::type_descriptor::FieldDescriptor,
    ) -> DdsResult<()> {
        let value = data
            .get_value_by_index(index)
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
                // Optimized bulk serialization for primitive type sequences
                match element_type.as_ref() {
                    FieldTypeInfo::Int8 => {
                        let values: Vec<i8> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Int8(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_i8_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Int16 => {
                        let values: Vec<i16> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Int16(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_i16_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Int32 => {
                        let values: Vec<i32> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Int32(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_i32_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Int64 => {
                        let values: Vec<i64> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Int64(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_i64_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::UInt16 => {
                        let values: Vec<u16> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::UInt16(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_u16_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::UInt32 => {
                        let values: Vec<u32> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::UInt32(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_u32_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::UInt64 => {
                        let values: Vec<u64> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::UInt64(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_u64_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Float32 => {
                        let values: Vec<f32> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::Float32(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_f32_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Float64 => {
                        let values: Vec<f64> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::Float64(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_f64_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Bool => {
                        let values: Vec<bool> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Bool(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_bool_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    // For complex types (String, Struct, nested Sequence), use element-by-element serialization
                    _ => {
                        serializer
                            .serialize_u32(v.len() as u32)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                        for item in v {
                            self.serialize_value_cdr(serializer, item, element_type)?;
                        }
                    }
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
                // Serialize nested struct fields directly without creating new DynamicTypeSupport
                for (index, field) in descriptor.fields.iter().enumerate() {
                    if let Some(value) = nested_data.get_value_by_index(index) {
                        self.serialize_value_cdr(serializer, value, &field.field_type)?;
                    } else {
                        return Err(DdsError::Error(format!(
                            "Missing required field: {}",
                            field.name
                        )));
                    }
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
        for (index, field) in self.descriptor.fields.iter().enumerate() {
            let value = self.deserialize_field_cdr(&mut deserializer, &field.field_type)?;
            result.values[index] = Some(value);
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

                // Optimization: bulk read for Sequence<UInt8> - direct slice to Arc conversion
                if matches!(element_type.as_ref(), FieldTypeInfo::UInt8) {
                    deserializer
                        .check_available(len)
                        .map_err(|e| DdsError::Error(e.to_string()))?;
                    let start = deserializer.get_position();
                    // Direct conversion from slice to Arc - avoids intermediate Vec allocation
                    let bytes: Arc<[u8]> = Arc::from(&deserializer.get_data()[start..start + len]);
                    deserializer.set_position(start + len);
                    return Ok(FieldValue::Bytes(bytes));
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
                // Deserialize nested struct directly without creating new DynamicTypeSupport
                let mut nested_data = Int2DdsData::new(descriptor.clone());
                for (index, field) in descriptor.fields.iter().enumerate() {
                    let value = self.deserialize_field_cdr(deserializer, &field.field_type)?;
                    nested_data.values[index] = Some(value);
                }
                Ok(FieldValue::Struct(Box::new(nested_data)))
            }
            // Optimized Bytes deserialization - direct slice to Arc conversion
            FieldTypeInfo::Bytes { .. } => {
                let len =
                    deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?
                        as usize;
                // Direct conversion from slice to Arc - avoids intermediate Vec allocation
                deserializer.check_available(len).map_err(|e| DdsError::Error(e.to_string()))?;
                let start = deserializer.get_position();
                let bytes: Arc<[u8]> = Arc::from(&deserializer.get_data()[start..start + len]);
                deserializer.set_position(start + len);
                Ok(FieldValue::Bytes(bytes))
            }
        }
    }

    /// Serialize Int2DdsData using XCDR v2
    fn serialize_xcdr2(&self, data: &Int2DdsData) -> DdsResult<Vec<u8>> {
        // Pre-allocate buffer based on estimated size to avoid reallocations
        let estimated_size = self.estimate_serialized_size(data);
        let mut serializer =
            Xcdr2Serializer::with_capacity(true, self.descriptor.extensibility, estimated_size);

        // Write encapsulation header
        serializer.write_encapsulation_header().map_err(|e| DdsError::Error(e.to_string()))?;

        // For Appendable/Mutable: write DHEADER
        let size_pos = match self.descriptor.extensibility {
            ExtensibilityKind::Final => None,
            ExtensibilityKind::Appendable | ExtensibilityKind::Mutable => {
                Some(serializer.begin_struct().map_err(|e| DdsError::Error(e.to_string()))?)
            }
        };

        let is_mutable = matches!(self.descriptor.extensibility, ExtensibilityKind::Mutable);

        // Serialize each field
        for (index, field) in self.descriptor.fields.iter().enumerate() {
            // Check if field has value
            let value = data.get_value_by_index(index);

            if is_mutable {
                // For Mutable types: write EMHEADER before each field
                if field.is_optional {
                    // Optional field: only serialize if value exists
                    if let Some(val) = value {
                        // Reserve space for EMHEADER
                        let emheader_pos = serializer.reserve_dheader();
                        let field_start = serializer.position();

                        // Serialize the field value
                        self.serialize_value_xcdr2(&mut serializer, val, &field.field_type)?;

                        // Calculate field length and backpatch EMHEADER
                        let field_len = (serializer.position() - field_start) as u32;
                        let emheader = (field.member_id << 16) | (field_len & 0xFFFF);
                        serializer.write_dheader_at(emheader_pos, emheader);
                    }
                    // If None, don't write anything (skip this field)
                } else {
                    // Required field: must have value
                    let val = value.ok_or_else(|| {
                        DdsError::Error(format!("Missing required field: {}", field.name))
                    })?;

                    // Reserve space for EMHEADER
                    let emheader_pos = serializer.reserve_dheader();
                    let field_start = serializer.position();

                    // Serialize the field
                    self.serialize_value_xcdr2(&mut serializer, val, &field.field_type)?;

                    // Calculate field length and backpatch EMHEADER
                    let field_len = (serializer.position() - field_start) as u32;
                    let emheader = (field.member_id << 16) | (field_len & 0xFFFF);
                    serializer.write_dheader_at(emheader_pos, emheader);
                }
            } else {
                // For Final/Appendable: serialize field directly
                let val = value.ok_or_else(|| {
                    DdsError::Error(format!("Missing required field: {}", field.name))
                })?;
                self.serialize_value_xcdr2(&mut serializer, val, &field.field_type)?;
            }
        }

        // For Appendable/Mutable: backpatch DHEADER
        if let Some(size_pos) = size_pos {
            serializer.end_struct(size_pos).map_err(|e| DdsError::Error(e.to_string()))?;
        }

        Ok(serializer.into_bytes())
    }

    /// Serialize a single field using XCDR v2
    fn _serialize_field_xcdr2(
        &self,
        serializer: &mut Xcdr2Serializer,
        data: &Int2DdsData,
        index: usize,
        field: &crate::type_descriptor::FieldDescriptor,
    ) -> DdsResult<()> {
        let value = data
            .get_value_by_index(index)
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
                // Optimized bulk serialization for primitive type sequences
                match element_type.as_ref() {
                    FieldTypeInfo::Int8 => {
                        let values: Vec<i8> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Int8(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_i8_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Int16 => {
                        let values: Vec<i16> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Int16(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_i16_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Int32 => {
                        let values: Vec<i32> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Int32(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_i32_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Int64 => {
                        let values: Vec<i64> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Int64(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_i64_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::UInt16 => {
                        let values: Vec<u16> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::UInt16(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_u16_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::UInt32 => {
                        let values: Vec<u32> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::UInt32(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_u32_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::UInt64 => {
                        let values: Vec<u64> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::UInt64(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_u64_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Float32 => {
                        let values: Vec<f32> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::Float32(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_f32_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Float64 => {
                        let values: Vec<f64> = v
                            .iter()
                            .filter_map(|fv| {
                                if let FieldValue::Float64(n) = fv {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        serializer
                            .serialize_f64_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    FieldTypeInfo::Bool => {
                        let values: Vec<bool> = v
                            .iter()
                            .filter_map(
                                |fv| if let FieldValue::Bool(n) = fv { Some(*n) } else { None },
                            )
                            .collect();
                        serializer
                            .serialize_bool_sequence(&values)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                    // For complex types (String, Struct, nested Sequence), use element-by-element serialization
                    _ => {
                        serializer
                            .serialize_u32(v.len() as u32)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                        for item in v {
                            self.serialize_value_xcdr2(serializer, item, element_type)?;
                        }
                    }
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
                // Serialize nested struct fields directly without creating new DynamicTypeSupport
                for (index, field) in descriptor.fields.iter().enumerate() {
                    if let Some(value) = nested_data.get_value_by_index(index) {
                        self.serialize_value_xcdr2(serializer, value, &field.field_type)?;
                    } else {
                        return Err(DdsError::Error(format!(
                            "Missing required field: {}",
                            field.name
                        )));
                    }
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
        use int2dds::serialize::cdr::is_sentinel_member_id;

        if data.len() < 4 {
            return Err(DdsError::Error("Data too short for XCDR2 header".to_string()));
        }

        let mut deserializer =
            Xcdr2Deserializer::new(data).map_err(|e| DdsError::Error(e.to_string()))?;

        let mut result = Int2DdsData::new(self.descriptor.clone());

        // For Appendable/Mutable: read DHEADER
        let (object_size, start_position) = match self.descriptor.extensibility {
            ExtensibilityKind::Final => (0, 0),
            ExtensibilityKind::Appendable | ExtensibilityKind::Mutable => {
                deserializer.begin_struct().map_err(|e| DdsError::Error(e.to_string()))?
            }
        };

        let is_mutable = matches!(self.descriptor.extensibility, ExtensibilityKind::Mutable);

        if is_mutable {
            // Mutable: read by member_id
            let object_end = start_position + object_size as usize;
            let mut fields_read = vec![false; self.descriptor.fields.len()];

            while deserializer.get_position() < object_end {
                let (member_id, member_length) = deserializer
                    .read_member_header()
                    .map_err(|e| DdsError::Error(e.to_string()))?;

                // Check for sentinel
                if is_sentinel_member_id(member_id) {
                    break;
                }

                let member_start = deserializer.get_position();

                // Find field by member_id
                if let Some((index, field)) = self
                    .descriptor
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, f)| f.member_id == member_id)
                {
                    let value =
                        self.deserialize_field_xcdr2(&mut deserializer, &field.field_type)?;
                    result.values[index] = Some(value);
                    fields_read[index] = true;

                    // Skip remaining bytes if needed
                    let consumed = deserializer.get_position() - member_start;
                    if consumed < member_length as usize {
                        deserializer
                            .skip((member_length as usize) - consumed)
                            .map_err(|e| DdsError::Error(e.to_string()))?;
                    }
                } else {
                    // Unknown member_id - skip for forward compatibility
                    deserializer
                        .skip_member(member_length)
                        .map_err(|e| DdsError::Error(e.to_string()))?;
                }
            }

            // Check that all required fields were read
            for (index, field) in self.descriptor.fields.iter().enumerate() {
                if !field.is_optional && !fields_read[index] {
                    return Err(DdsError::Error(format!("Missing required field: {}", field.name)));
                }
            }
        } else {
            // Final/Appendable: read in order
            for (index, field) in self.descriptor.fields.iter().enumerate() {
                let value = self.deserialize_field_xcdr2(&mut deserializer, &field.field_type)?;
                result.values[index] = Some(value);
            }
        }

        // For Appendable/Mutable: validate and skip remaining bytes
        if !matches!(self.descriptor.extensibility, ExtensibilityKind::Final) {
            deserializer
                .end_struct(object_size, start_position)
                .map_err(|e| DdsError::Error(e.to_string()))?;
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

                // Optimization: bulk read for Sequence<UInt8> - direct slice to Arc conversion
                if matches!(element_type.as_ref(), FieldTypeInfo::UInt8) {
                    deserializer
                        .check_available(len)
                        .map_err(|e| DdsError::Error(e.to_string()))?;
                    let start = deserializer.get_position();
                    // Direct conversion from slice to Arc - avoids intermediate Vec allocation
                    let bytes: Arc<[u8]> = Arc::from(&deserializer.get_data()[start..start + len]);
                    deserializer.set_position(start + len);
                    return Ok(FieldValue::Bytes(bytes));
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
                // Deserialize nested struct directly without creating new DynamicTypeSupport
                let mut nested_data = Int2DdsData::new(descriptor.clone());
                for (index, field) in descriptor.fields.iter().enumerate() {
                    let value = self.deserialize_field_xcdr2(deserializer, &field.field_type)?;
                    nested_data.values[index] = Some(value);
                }
                Ok(FieldValue::Struct(Box::new(nested_data)))
            }
            // Optimized Bytes deserialization - direct slice to Arc conversion
            FieldTypeInfo::Bytes { .. } => {
                let len =
                    deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?
                        as usize;
                // Direct conversion from slice to Arc - avoids intermediate Vec allocation
                deserializer.check_available(len).map_err(|e| DdsError::Error(e.to_string()))?;
                let start = deserializer.get_position();
                let bytes: Arc<[u8]> = Arc::from(&deserializer.get_data()[start..start + len]);
                deserializer.set_position(start + len);
                Ok(FieldValue::Bytes(bytes))
            }
        }
    }

    /// Serialize only key fields
    fn serialize_key_fields(&self, data: &Int2DdsData) -> DdsResult<Vec<u8>> {
        let mut serializer = CdrSerializer::new(true); // true = little endian
        serializer.write_encapsulation_header().map_err(|e| DdsError::Error(e.to_string()))?;

        for (index, field) in self.descriptor.fields.iter().enumerate() {
            if !field.is_key {
                continue;
            }
            if let Some(value) = data.get_value_by_index(index) {
                self.serialize_value_cdr(&mut serializer, value, &field.field_type)?;
            }
        }

        Ok(serializer.into_bytes())
    }

    /// Detect encoding from data and deserialize accordingly
    fn auto_deserialize(&self, data: &[u8]) -> DdsResult<Int2DdsData> {
        if data.len() < 2 {
            eprintln!("DynamicTypeSupport::auto_deserialize: data too short (len={})", data.len());
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

    // ========================================================================
    // XTypes TypeObject Generation
    // ========================================================================

    /// Convert FieldTypeInfo to TypeIdentifier
    fn field_type_to_type_identifier(field_type: &FieldTypeInfo) -> TypeIdentifier {
        match field_type {
            FieldTypeInfo::Bool => TypeIdentifier::Boolean,
            FieldTypeInfo::Int8 => TypeIdentifier::Int8,
            FieldTypeInfo::UInt8 => TypeIdentifier::Uint8,
            FieldTypeInfo::Int16 => TypeIdentifier::Int16,
            FieldTypeInfo::UInt16 => TypeIdentifier::Uint16,
            FieldTypeInfo::Int32 => TypeIdentifier::Int32,
            FieldTypeInfo::UInt32 => TypeIdentifier::Uint32,
            FieldTypeInfo::Int64 => TypeIdentifier::Int64,
            FieldTypeInfo::UInt64 => TypeIdentifier::Uint64,
            FieldTypeInfo::Float32 => TypeIdentifier::Float32,
            FieldTypeInfo::Float64 => TypeIdentifier::Float64,
            FieldTypeInfo::String { max_length } => {
                if *max_length == 0 || *max_length == u32::MAX {
                    TypeIdentifier::String8
                } else if *max_length <= 255 {
                    TypeIdentifier::String8Small { bound: *max_length as u8 }
                } else {
                    TypeIdentifier::String8Large { bound: *max_length }
                }
            }
            FieldTypeInfo::Bytes { max_length } => {
                // Bytes are serialized as octet sequence
                // max_length = 0 means unbounded sequence
                let header = PlainCollectionHeader {
                    equiv_kind: XtypesEquivalenceKind::Minimal,
                    element_flags: CollectionElementFlag(0),
                };
                if *max_length > 0 && *max_length <= 255 {
                    TypeIdentifier::PlainSequenceSmall {
                        header,
                        bound: *max_length as u8,
                        element_identifier: Box::new(TypeIdentifier::Byte),
                    }
                } else {
                    // max_length == 0 (unbounded) or max_length > 255
                    TypeIdentifier::PlainSequenceLarge {
                        header,
                        bound: *max_length,
                        element_identifier: Box::new(TypeIdentifier::Byte),
                    }
                }
            }
            FieldTypeInfo::Sequence { element_type, max_length } => {
                let header = PlainCollectionHeader {
                    equiv_kind: XtypesEquivalenceKind::Minimal,
                    element_flags: CollectionElementFlag(0),
                };
                let element_id = Self::field_type_to_type_identifier(element_type);
                if *max_length > 0 && *max_length <= 255 {
                    TypeIdentifier::PlainSequenceSmall {
                        header,
                        bound: *max_length as u8,
                        element_identifier: Box::new(element_id),
                    }
                } else {
                    TypeIdentifier::PlainSequenceLarge {
                        header,
                        bound: *max_length,
                        element_identifier: Box::new(element_id),
                    }
                }
            }
            FieldTypeInfo::Array { element_type, length } => {
                let header = PlainCollectionHeader {
                    equiv_kind: XtypesEquivalenceKind::Minimal,
                    element_flags: CollectionElementFlag(0),
                };
                let element_id = Self::field_type_to_type_identifier(element_type);
                if *length <= 255 {
                    TypeIdentifier::PlainArraySmall {
                        header,
                        array_bound_seq: vec![*length as u8],
                        element_identifier: Box::new(element_id),
                    }
                } else {
                    TypeIdentifier::PlainArrayLarge {
                        header,
                        array_bound_seq: vec![*length],
                        element_identifier: Box::new(element_id),
                    }
                }
            }
            FieldTypeInfo::Struct { descriptor } => {
                // Nested struct: compute hash from its minimal type
                let nested_support = DynamicTypeSupport::new(descriptor.clone());
                let minimal = nested_support.build_minimal_struct_type();
                TypeIdentifier::MinimalTypeId(minimal.compute_hash())
            }
        }
    }

    /// Build MinimalStructType from descriptor
    fn build_minimal_struct_type(&self) -> MinimalStructType {
        let extensibility = match self.descriptor.extensibility {
            ExtensibilityKind::Final => int2dds::xtypes::ExtensibilityKind::Final,
            ExtensibilityKind::Appendable => int2dds::xtypes::ExtensibilityKind::Appendable,
            ExtensibilityKind::Mutable => int2dds::xtypes::ExtensibilityKind::Mutable,
        };
        let type_flags = TypeFlag::new(extensibility, false, false);
        let mut minimal = MinimalStructType::new(type_flags, None);

        for field in &self.descriptor.fields {
            let member_flags = MemberFlag::new(
                TryConstructKind::Discard,
                false,             // is_external
                field.is_optional, // is_optional
                false,             // is_must_understand
                field.is_key,      // is_key
                false,             // is_default
            );
            let type_id = Self::field_type_to_type_identifier(&field.field_type);
            let member =
                MinimalStructMember::new(field.member_id, member_flags, type_id, &field.name);
            minimal.add_member(member);
        }

        minimal
    }

    /// Build CompleteStructType from descriptor
    fn build_complete_struct_type(&self) -> CompleteStructType {
        let extensibility = match self.descriptor.extensibility {
            ExtensibilityKind::Final => int2dds::xtypes::ExtensibilityKind::Final,
            ExtensibilityKind::Appendable => int2dds::xtypes::ExtensibilityKind::Appendable,
            ExtensibilityKind::Mutable => int2dds::xtypes::ExtensibilityKind::Mutable,
        };
        let type_flags = TypeFlag::new(extensibility, false, false);
        let mut complete =
            CompleteStructType::new(type_flags, self.descriptor.type_name.clone(), None);

        for field in &self.descriptor.fields {
            let member_flags = MemberFlag::new(
                TryConstructKind::Discard,
                false,
                field.is_optional,
                false,
                field.is_key,
                false,
            );
            let type_id = Self::field_type_to_type_identifier(&field.field_type);
            let common = CommonStructMember {
                member_id: field.member_id,
                member_flags,
                member_type_id: type_id,
            };
            let detail = CompleteMemberDetail {
                name: field.name.clone(),
                ann_builtin: None,
                ann_custom: Vec::new(),
            };
            complete.add_member(CompleteStructMember { common, detail });
        }

        complete
    }

    /// Build TypeObject from descriptor (uses CompleteTypeObject for XTypes compatibility)
    pub fn build_type_object(&self) -> TypeObject {
        let complete = self.build_complete_struct_type();
        TypeObject::Complete(int2dds::xtypes::CompleteTypeObject::Struct(complete))
    }

    /// Build TypeIdentifier from descriptor (uses CompleteTypeObject hash for consistency)
    pub fn build_type_identifier(&self) -> TypeIdentifier {
        let type_obj = self.build_type_object();
        TypeIdentifier::CompleteTypeId(type_obj.compute_hash())
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

    fn serialize(
        &self,
        data: &dyn Any,
        format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData> {
        let dynamic_data = data
            .downcast_ref::<Int2DdsData>()
            .ok_or_else(|| DdsError::Error("Expected Int2DdsData".to_string()))?;

        let bytes_result = match format {
            Some(SerializationFormat::Cdr) => self.serialize_cdr(dynamic_data)?,
            Some(SerializationFormat::Xcdr { .. }) => self.serialize_xcdr2(dynamic_data)?,
            None => {
                // Use XCDR version specified in the type descriptor
                match self.descriptor.xcdr_version {
                    Int2DdsXcdrVersion::Xcdr1 => self.serialize_cdr(dynamic_data)?,
                    Int2DdsXcdrVersion::Xcdr2 => self.serialize_xcdr2(dynamic_data)?,
                }
            }
        };
        if bytes_result.len() < 2 {
            eprintln!(
                "DynamicTypeSupport::serialize: serialized data too short (len={})",
                bytes_result.len()
            );
            return Err(DdsError::Error("Serialized data too short".to_string()));
        }
        Ok(bytes_result.into())
    }

    fn deserialize(
        &self,
        data: &[u8],
        format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        let result = match format {
            Some(SerializationFormat::Cdr) => self.deserialize_cdr(data)?,
            Some(SerializationFormat::Xcdr { .. }) => self.deserialize_xcdr2(data)?,
            None => self.auto_deserialize(data)?,
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
        if !self.descriptor.fields.iter().any(|field| field.is_key) {
            return InstanceHandle::NIL;
        }

        // Hash computation directly without intermediate Vec allocations
        let mut hash = [0u8; 16];
        let mut hash_idx = 0usize;

        for (index, field) in self.descriptor.fields.iter().enumerate() {
            if !field.is_key {
                continue;
            }
            if let Some(value) = dynamic_data.get_value_by_index(index) {
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

    fn get_type_identifier(&self) -> Option<TypeIdentifier> {
        Some(self.build_type_identifier())
    }

    fn get_type_object(&self) -> Option<TypeObject> {
        Some(self.build_type_object())
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
            for value in nested.values.iter().flatten() {
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
        data.set_value("id", FieldValue::UInt32(123)).unwrap();
        data.set_value("message", FieldValue::String("hello".to_string())).unwrap();
        data.set_value("value", FieldValue::Float64(3.14)).unwrap();

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
        data.set_value("id", FieldValue::UInt32(456)).unwrap();
        data.set_value("message", FieldValue::String("world".to_string())).unwrap();
        data.set_value("value", FieldValue::Float64(2.71)).unwrap();

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
        data.set_value("id", FieldValue::UInt32(789)).unwrap();
        data.set_value("message", FieldValue::String("test".to_string())).unwrap();
        data.set_value("value", FieldValue::Float64(1.0)).unwrap();

        let key_bytes = support.serialize_key_fields(&data).unwrap();
        assert!(!key_bytes.is_empty());
    }

    #[test]
    fn test_compute_key() {
        let desc = create_test_descriptor();
        let support = DynamicTypeSupport::new(desc.clone());

        let mut data = Int2DdsData::new(desc);
        data.set_value("id", FieldValue::UInt32(100)).unwrap();
        data.set_value("message", FieldValue::String("msg".to_string())).unwrap();
        data.set_value("value", FieldValue::Float64(0.0)).unwrap();

        let handle = support.compute_key(&data as &dyn Any);
        assert_ne!(handle, InstanceHandle::NIL);
    }
}
