//! Dynamic serialization - CDR/XCDR2 serialization for DynamicData.
//!
//! This module provides serialization and deserialization of DynamicData
//! based on runtime type information from DynamicType.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use crate::dcps::core::error::{DdsError, DdsResult};
use crate::dcps::topic::type_support::SerializationFormat;
use crate::rtps::common::types::SerializedData;
use crate::serialize::cdr::{
    CdrDeserializer, CdrError, CdrSerializer, ExtensibilityKind, PrimitiveSerialize,
    StringSerialize, Xcdr2Deserializer, Xcdr2Serializer,
};
use crate::serialize::{BufferManager, DeserializerReader};

use super::dynamic_data::{DynamicData, DynamicValue};
use super::dynamic_type::{
    DynamicType, DynamicTypeKind, MemberDescriptor, PrimitiveKind, StructDescriptor,
};

trait ValueDeserializer {
    fn deserialize_bool(&mut self) -> Result<bool, CdrError>;
    fn deserialize_i8(&mut self) -> Result<i8, CdrError>;
    fn deserialize_i16(&mut self) -> Result<i16, CdrError>;
    fn deserialize_i32(&mut self) -> Result<i32, CdrError>;
    fn deserialize_i64(&mut self) -> Result<i64, CdrError>;
    fn deserialize_u8(&mut self) -> Result<u8, CdrError>;
    fn deserialize_u16(&mut self) -> Result<u16, CdrError>;
    fn deserialize_u32(&mut self) -> Result<u32, CdrError>;
    fn deserialize_u64(&mut self) -> Result<u64, CdrError>;
    fn deserialize_f32(&mut self) -> Result<f32, CdrError>;
    fn deserialize_f64(&mut self) -> Result<f64, CdrError>;
    fn deserialize_string(&mut self) -> Result<String, CdrError>;
    fn deserialize_wstring16(&mut self) -> Result<String, CdrError>;
}

impl ValueDeserializer for CdrDeserializer<'_> {
    fn deserialize_bool(&mut self) -> Result<bool, CdrError> {
        CdrDeserializer::deserialize_bool(self)
    }

    fn deserialize_i8(&mut self) -> Result<i8, CdrError> {
        CdrDeserializer::deserialize_i8(self)
    }

    fn deserialize_i16(&mut self) -> Result<i16, CdrError> {
        CdrDeserializer::deserialize_i16(self)
    }

    fn deserialize_i32(&mut self) -> Result<i32, CdrError> {
        CdrDeserializer::deserialize_i32(self)
    }

    fn deserialize_i64(&mut self) -> Result<i64, CdrError> {
        CdrDeserializer::deserialize_i64(self)
    }

    fn deserialize_u8(&mut self) -> Result<u8, CdrError> {
        CdrDeserializer::deserialize_u8(self)
    }

    fn deserialize_u16(&mut self) -> Result<u16, CdrError> {
        CdrDeserializer::deserialize_u16(self)
    }

    fn deserialize_u32(&mut self) -> Result<u32, CdrError> {
        CdrDeserializer::deserialize_u32(self)
    }

    fn deserialize_u64(&mut self) -> Result<u64, CdrError> {
        CdrDeserializer::deserialize_u64(self)
    }

    fn deserialize_f32(&mut self) -> Result<f32, CdrError> {
        CdrDeserializer::deserialize_f32(self)
    }

    fn deserialize_f64(&mut self) -> Result<f64, CdrError> {
        CdrDeserializer::deserialize_f64(self)
    }

    fn deserialize_string(&mut self) -> Result<String, CdrError> {
        CdrDeserializer::deserialize_string(self)
    }

    fn deserialize_wstring16(&mut self) -> Result<String, CdrError> {
        CdrDeserializer::deserialize_wstring16(self)
    }
}

impl ValueDeserializer for Xcdr2Deserializer<'_> {
    fn deserialize_bool(&mut self) -> Result<bool, CdrError> {
        Xcdr2Deserializer::deserialize_bool(self)
    }

    fn deserialize_i8(&mut self) -> Result<i8, CdrError> {
        Xcdr2Deserializer::deserialize_i8(self)
    }

    fn deserialize_i16(&mut self) -> Result<i16, CdrError> {
        Xcdr2Deserializer::deserialize_i16(self)
    }

    fn deserialize_i32(&mut self) -> Result<i32, CdrError> {
        Xcdr2Deserializer::deserialize_i32(self)
    }

    fn deserialize_i64(&mut self) -> Result<i64, CdrError> {
        Xcdr2Deserializer::deserialize_i64(self)
    }

    fn deserialize_u8(&mut self) -> Result<u8, CdrError> {
        Xcdr2Deserializer::deserialize_u8(self)
    }

    fn deserialize_u16(&mut self) -> Result<u16, CdrError> {
        Xcdr2Deserializer::deserialize_u16(self)
    }

    fn deserialize_u32(&mut self) -> Result<u32, CdrError> {
        Xcdr2Deserializer::deserialize_u32(self)
    }

    fn deserialize_u64(&mut self) -> Result<u64, CdrError> {
        Xcdr2Deserializer::deserialize_u64(self)
    }

    fn deserialize_f32(&mut self) -> Result<f32, CdrError> {
        Xcdr2Deserializer::deserialize_f32(self)
    }

    fn deserialize_f64(&mut self) -> Result<f64, CdrError> {
        Xcdr2Deserializer::deserialize_f64(self)
    }

    fn deserialize_string(&mut self) -> Result<String, CdrError> {
        Xcdr2Deserializer::deserialize_string(self)
    }

    fn deserialize_wstring16(&mut self) -> Result<String, CdrError> {
        Xcdr2Deserializer::deserialize_wstring16(self)
    }
}

#[inline]
fn cdr_error(error: CdrError) -> DdsError {
    DdsError::Error(error.to_string())
}

fn nested_struct_deserialization_error(struct_desc: &StructDescriptor) -> DdsError {
    DdsError::Error(format!(
        "Nested struct deserialization is not supported yet for '{}': runtime nested type metadata is unavailable",
        struct_desc
            .members()
            .first()
            .map(|member| member.name.as_ref())
            .unwrap_or("anonymous struct")
    ))
}

fn external_type_deserialization_error(type_kind: &DynamicTypeKind) -> DdsError {
    DdsError::Error(format!("External type deserialization is not supported yet: {:?}", type_kind))
}

fn member_value_or_default<'a>(
    data: &'a DynamicData,
    member: &MemberDescriptor,
) -> Option<Cow<'a, DynamicValue>> {
    if let Some(value) = data.get_value(&member.name) {
        Some(Cow::Borrowed(value))
    } else if !member.is_optional {
        Some(Cow::Owned(DynamicValue::default_for_kind(&member.member_type)))
    } else {
        None
    }
}

fn is_primitive_kind(kind: &DynamicTypeKind) -> bool {
    matches!(kind, DynamicTypeKind::Primitive(_) | DynamicTypeKind::Enum(_))
}

fn serialize_atomic_value<S, F>(
    serializer: &mut S,
    value: &DynamicValue,
    serialize_nested_struct: &mut F,
) -> DdsResult<()>
where
    S: PrimitiveSerialize + StringSerialize,
    F: FnMut(&mut S, &DynamicData) -> DdsResult<()>,
{
    match value {
        DynamicValue::Boolean(v) => serializer.serialize_bool(*v).map_err(cdr_error),
        DynamicValue::Int8(v) => serializer.serialize_i8(*v).map_err(cdr_error),
        DynamicValue::Int16(v) => serializer.serialize_i16(*v).map_err(cdr_error),
        DynamicValue::Int32(v) => serializer.serialize_i32(*v).map_err(cdr_error),
        DynamicValue::Int64(v) => serializer.serialize_i64(*v).map_err(cdr_error),
        DynamicValue::Uint8(v) => serializer.serialize_u8(*v).map_err(cdr_error),
        DynamicValue::Uint16(v) => serializer.serialize_u16(*v).map_err(cdr_error),
        DynamicValue::Uint32(v) => serializer.serialize_u32(*v).map_err(cdr_error),
        DynamicValue::Uint64(v) => serializer.serialize_u64(*v).map_err(cdr_error),
        DynamicValue::Float32(v) => serializer.serialize_f32(*v).map_err(cdr_error),
        DynamicValue::Float64(v) => serializer.serialize_f64(*v).map_err(cdr_error),
        DynamicValue::Char8(v) => serializer.serialize_u8(*v as u8).map_err(cdr_error),
        DynamicValue::Byte(v) => serializer.serialize_u8(*v).map_err(cdr_error),
        DynamicValue::String(v) => serializer.serialize_string(v).map_err(cdr_error),
        DynamicValue::WString(v) => serializer.serialize_wstring16(v).map_err(cdr_error),
        DynamicValue::Enum { value, .. } => serializer.serialize_i32(*value).map_err(cdr_error),
        DynamicValue::Struct(inner) => serialize_nested_struct(serializer, inner),
        DynamicValue::Null => Ok(()),
        // Sequence/Array/Optional handled by callers with format-specific logic
        DynamicValue::Sequence(_) | DynamicValue::Array(_) | DynamicValue::Optional(_) => {
            Err(DdsError::Error(
                "serialize_atomic_value called with collection/optional; use format-specific path"
                    .to_string(),
            ))
        }
    }
}

fn serialize_value_cdr<F>(
    serializer: &mut CdrSerializer,
    value: &DynamicValue,
    serialize_nested_struct: &mut F,
) -> DdsResult<()>
where
    F: FnMut(&mut CdrSerializer, &DynamicData) -> DdsResult<()>,
{
    match value {
        DynamicValue::Sequence(items) => {
            serializer.serialize_u32(items.len() as u32).map_err(cdr_error)?;
            for item in items {
                serialize_value_cdr(serializer, item, serialize_nested_struct)?;
            }
            Ok(())
        }
        DynamicValue::Array(items) => {
            for item in items {
                serialize_value_cdr(serializer, item, serialize_nested_struct)?;
            }
            Ok(())
        }
        DynamicValue::Optional(Some(inner)) => {
            serializer.serialize_bool(true).map_err(cdr_error)?;
            serialize_value_cdr(serializer, inner, serialize_nested_struct)
        }
        DynamicValue::Optional(None) => serializer.serialize_bool(false).map_err(cdr_error),
        other => serialize_atomic_value(serializer, other, serialize_nested_struct),
    }
}

fn serialize_value_xcdr2<F>(
    serializer: &mut Xcdr2Serializer,
    value: &DynamicValue,
    type_kind: &DynamicTypeKind,
    serialize_nested_struct: &mut F,
) -> DdsResult<()>
where
    F: FnMut(&mut Xcdr2Serializer, &DynamicData) -> DdsResult<()>,
{
    match value {
        DynamicValue::Sequence(items) => {
            let element_type = match type_kind {
                DynamicTypeKind::Sequence { element_type, .. } => element_type.as_ref(),
                _ => return Err(DdsError::Error("type mismatch: expected Sequence".to_string())),
            };
            let mut write_inner = |s: &mut Xcdr2Serializer| -> DdsResult<()> {
                s.serialize_u32(items.len() as u32).map_err(cdr_error)?;
                for item in items {
                    serialize_value_xcdr2(s, item, element_type, serialize_nested_struct)?;
                }
                Ok(())
            };
            if is_primitive_kind(element_type) {
                write_inner(serializer)
            } else {
                let dh = serializer.reserve_dheader();
                let start = serializer.position();
                write_inner(serializer)?;
                let size = (serializer.position() - start) as u32;
                serializer.write_dheader_at(dh, size);
                Ok(())
            }
        }
        DynamicValue::Array(items) => {
            let element_type = match type_kind {
                DynamicTypeKind::Array { element_type, .. } => element_type.as_ref(),
                _ => return Err(DdsError::Error("type mismatch: expected Array".to_string())),
            };
            let mut write_inner = |s: &mut Xcdr2Serializer| -> DdsResult<()> {
                for item in items {
                    serialize_value_xcdr2(s, item, element_type, serialize_nested_struct)?;
                }
                Ok(())
            };
            if is_primitive_kind(element_type) {
                write_inner(serializer)
            } else {
                let dh = serializer.reserve_dheader();
                let start = serializer.position();
                write_inner(serializer)?;
                let size = (serializer.position() - start) as u32;
                serializer.write_dheader_at(dh, size);
                Ok(())
            }
        }
        DynamicValue::Optional(Some(inner)) => {
            serializer.serialize_bool(true).map_err(cdr_error)?;
            serialize_value_xcdr2(serializer, inner, type_kind, serialize_nested_struct)
        }
        DynamicValue::Optional(None) => serializer.serialize_bool(false).map_err(cdr_error),
        other => serialize_atomic_value(serializer, other, serialize_nested_struct),
    }
}

fn deserialize_atomic_value<D>(
    deserializer: &mut D,
    type_kind: &DynamicTypeKind,
) -> DdsResult<DynamicValue>
where
    D: ValueDeserializer,
{
    match type_kind {
        DynamicTypeKind::Primitive(kind) => deserialize_primitive(deserializer, *kind),
        DynamicTypeKind::String { .. } => {
            Ok(DynamicValue::String(deserializer.deserialize_string().map_err(cdr_error)?))
        }
        DynamicTypeKind::WString { .. } => {
            Ok(DynamicValue::WString(deserializer.deserialize_wstring16().map_err(cdr_error)?))
        }
        DynamicTypeKind::Enum(_) => {
            let value = deserializer.deserialize_i32().map_err(cdr_error)?;
            Ok(DynamicValue::Enum { name: String::new(), value })
        }
        DynamicTypeKind::Struct(struct_desc) => {
            Err(nested_struct_deserialization_error(struct_desc))
        }
        DynamicTypeKind::ExternalType { .. } => Err(external_type_deserialization_error(type_kind)),
        DynamicTypeKind::Sequence { .. } | DynamicTypeKind::Array { .. } => Err(DdsError::Error(
            "deserialize_atomic_value called with collection; use format-specific path".to_string(),
        )),
    }
}

fn deserialize_value_cdr(
    deserializer: &mut CdrDeserializer,
    type_kind: &DynamicTypeKind,
) -> DdsResult<DynamicValue> {
    match type_kind {
        DynamicTypeKind::Sequence { element_type, .. } => {
            let len = deserializer.deserialize_u32().map_err(cdr_error)? as usize;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(deserialize_value_cdr(deserializer, element_type)?);
            }
            Ok(DynamicValue::Sequence(items))
        }
        DynamicTypeKind::Array { element_type, dimensions } => {
            let total_size: u32 = dimensions.iter().product();
            let mut items = Vec::with_capacity(total_size as usize);
            for _ in 0..total_size {
                items.push(deserialize_value_cdr(deserializer, element_type)?);
            }
            Ok(DynamicValue::Array(items))
        }
        other => deserialize_atomic_value(deserializer, other),
    }
}

fn deserialize_value_xcdr2(
    deserializer: &mut Xcdr2Deserializer,
    type_kind: &DynamicTypeKind,
) -> DdsResult<DynamicValue> {
    match type_kind {
        DynamicTypeKind::Sequence { element_type, .. } => {
            if !is_primitive_kind(element_type) {
                let _ = deserializer.read_dheader().map_err(cdr_error)?;
            }
            let len = deserializer.deserialize_u32().map_err(cdr_error)? as usize;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(deserialize_value_xcdr2(deserializer, element_type)?);
            }
            Ok(DynamicValue::Sequence(items))
        }
        DynamicTypeKind::Array { element_type, dimensions } => {
            if !is_primitive_kind(element_type) {
                let _ = deserializer.read_dheader().map_err(cdr_error)?;
            }
            let total_size: u32 = dimensions.iter().product();
            let mut items = Vec::with_capacity(total_size as usize);
            for _ in 0..total_size {
                items.push(deserialize_value_xcdr2(deserializer, element_type)?);
            }
            Ok(DynamicValue::Array(items))
        }
        other => deserialize_atomic_value(deserializer, other),
    }
}

fn deserialize_primitive<D>(deserializer: &mut D, kind: PrimitiveKind) -> DdsResult<DynamicValue>
where
    D: ValueDeserializer,
{
    match kind {
        PrimitiveKind::Boolean => {
            Ok(DynamicValue::Boolean(deserializer.deserialize_bool().map_err(cdr_error)?))
        }
        PrimitiveKind::Int8 => {
            Ok(DynamicValue::Int8(deserializer.deserialize_i8().map_err(cdr_error)?))
        }
        PrimitiveKind::Int16 => {
            Ok(DynamicValue::Int16(deserializer.deserialize_i16().map_err(cdr_error)?))
        }
        PrimitiveKind::Int32 => {
            Ok(DynamicValue::Int32(deserializer.deserialize_i32().map_err(cdr_error)?))
        }
        PrimitiveKind::Int64 => {
            Ok(DynamicValue::Int64(deserializer.deserialize_i64().map_err(cdr_error)?))
        }
        PrimitiveKind::Uint8 => {
            Ok(DynamicValue::Uint8(deserializer.deserialize_u8().map_err(cdr_error)?))
        }
        PrimitiveKind::Uint16 => {
            Ok(DynamicValue::Uint16(deserializer.deserialize_u16().map_err(cdr_error)?))
        }
        PrimitiveKind::Uint32 => {
            Ok(DynamicValue::Uint32(deserializer.deserialize_u32().map_err(cdr_error)?))
        }
        PrimitiveKind::Uint64 => {
            Ok(DynamicValue::Uint64(deserializer.deserialize_u64().map_err(cdr_error)?))
        }
        PrimitiveKind::Float32 => {
            Ok(DynamicValue::Float32(deserializer.deserialize_f32().map_err(cdr_error)?))
        }
        PrimitiveKind::Float64 => {
            Ok(DynamicValue::Float64(deserializer.deserialize_f64().map_err(cdr_error)?))
        }
        PrimitiveKind::Float128 => {
            Ok(DynamicValue::Float64(deserializer.deserialize_f64().map_err(cdr_error)?))
        }
        PrimitiveKind::Char8 => {
            Ok(DynamicValue::Char8(deserializer.deserialize_u8().map_err(cdr_error)? as char))
        }
        PrimitiveKind::Char16 => Ok(DynamicValue::Char8(
            deserializer.deserialize_u16().map_err(cdr_error)? as u8 as char,
        )),
        PrimitiveKind::Byte => {
            Ok(DynamicValue::Byte(deserializer.deserialize_u8().map_err(cdr_error)?))
        }
    }
}

fn deserialize_struct_members_cdr(
    deserializer: &mut CdrDeserializer,
    struct_desc: &StructDescriptor,
) -> DdsResult<HashMap<Arc<str>, DynamicValue>> {
    let mut values = HashMap::new();
    for member in struct_desc.members() {
        let value = deserialize_value_cdr(deserializer, &member.member_type)?;
        values.insert(member.name.clone(), value);
    }
    Ok(values)
}

fn deserialize_struct_members_xcdr2(
    deserializer: &mut Xcdr2Deserializer,
    struct_desc: &StructDescriptor,
) -> DdsResult<HashMap<Arc<str>, DynamicValue>> {
    let mut values = HashMap::new();
    for member in struct_desc.members() {
        let value = deserialize_value_xcdr2(deserializer, &member.member_type)?;
        values.insert(member.name.clone(), value);
    }
    Ok(values)
}

/// Serialize DynamicData to bytes using the appropriate format.
pub fn serialize_dynamic_data(
    data: &DynamicData,
    format: &SerializationFormat,
) -> DdsResult<SerializedData> {
    match format {
        SerializationFormat::Cdr => serialize_cdr(data),
        SerializationFormat::Xcdr { extensibility_kind, .. } => {
            serialize_xcdr(data, *extensibility_kind)
        }
    }
}

/// Deserialize bytes into DynamicData using the appropriate format.
pub fn deserialize_dynamic_data(
    bytes: &[u8],
    dynamic_type: &Arc<DynamicType>,
) -> DdsResult<DynamicData> {
    if bytes.len() < 4 {
        return Err(DdsError::Error("Insufficient data for encapsulation header".to_string()));
    }

    let encap_id = u16::from_be_bytes([bytes[0], bytes[1]]);
    let is_xcdr2 = matches!(encap_id, 0x0006..=0x000B);

    if is_xcdr2 {
        deserialize_xcdr(bytes, dynamic_type)
    } else {
        deserialize_cdr(bytes, dynamic_type)
    }
}

fn serialize_cdr(data: &DynamicData) -> DdsResult<SerializedData> {
    let mut serializer = CdrSerializer::with_capacity(true, 256);
    serializer.write_encapsulation_header().map_err(cdr_error)?;
    serialize_struct_cdr(&mut serializer, data)?;
    Ok(Arc::from(serializer.into_bytes().into_boxed_slice()))
}

fn deserialize_cdr(bytes: &[u8], dynamic_type: &Arc<DynamicType>) -> DdsResult<DynamicData> {
    let mut deserializer = CdrDeserializer::new(bytes).map_err(cdr_error)?;
    deserialize_struct_cdr(&mut deserializer, dynamic_type)
}

fn serialize_struct_cdr(serializer: &mut CdrSerializer, data: &DynamicData) -> DdsResult<()> {
    let struct_desc = data
        .dynamic_type()
        .as_struct()
        .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;

    let mut nested = serialize_struct_cdr;
    for member in struct_desc.members() {
        if let Some(value) = member_value_or_default(data, member) {
            serialize_value_cdr(serializer, &value, &mut nested)?;
        }
    }

    Ok(())
}

fn deserialize_struct_cdr(
    deserializer: &mut CdrDeserializer,
    dynamic_type: &Arc<DynamicType>,
) -> DdsResult<DynamicData> {
    let struct_desc = dynamic_type
        .as_struct()
        .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;

    let values = deserialize_struct_members_cdr(deserializer, struct_desc)?;
    Ok(DynamicData::with_values(dynamic_type.clone(), values))
}

fn serialize_xcdr(
    data: &DynamicData,
    extensibility: ExtensibilityKind,
) -> DdsResult<SerializedData> {
    let mut serializer = Xcdr2Serializer::with_capacity(true, extensibility, 256);
    serializer.write_encapsulation_header().map_err(cdr_error)?;
    serialize_struct_xcdr(&mut serializer, data, extensibility)?;
    Ok(Arc::from(serializer.into_bytes().into_boxed_slice()))
}

fn deserialize_xcdr(bytes: &[u8], dynamic_type: &Arc<DynamicType>) -> DdsResult<DynamicData> {
    let mut deserializer = Xcdr2Deserializer::new(bytes).map_err(cdr_error)?;
    deserialize_struct_xcdr(&mut deserializer, dynamic_type, dynamic_type.extensibility())
}

fn serialize_struct_xcdr(
    serializer: &mut Xcdr2Serializer,
    data: &DynamicData,
    extensibility: ExtensibilityKind,
) -> DdsResult<()> {
    let struct_desc = data
        .dynamic_type()
        .as_struct()
        .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;

    match extensibility {
        ExtensibilityKind::Final => {
            let mut nested = |serializer: &mut Xcdr2Serializer, inner: &DynamicData| {
                serialize_struct_xcdr(serializer, inner, extensibility)
            };
            for member in struct_desc.members() {
                if let Some(value) = member_value_or_default(data, member) {
                    serialize_value_xcdr2(serializer, &value, &member.member_type, &mut nested)?;
                }
            }
        }
        ExtensibilityKind::Appendable => {
            let size_pos = serializer.begin_struct().map_err(cdr_error)?;
            let mut nested = |serializer: &mut Xcdr2Serializer, inner: &DynamicData| {
                serialize_struct_xcdr(serializer, inner, extensibility)
            };
            for member in struct_desc.members() {
                if let Some(value) = member_value_or_default(data, member) {
                    serialize_value_xcdr2(serializer, &value, &member.member_type, &mut nested)?;
                }
            }
            serializer.end_struct(size_pos).map_err(cdr_error)?;
        }
        ExtensibilityKind::Mutable => {
            let size_pos = serializer.begin_struct().map_err(cdr_error)?;

            for member in struct_desc.members() {
                if let Some(value) = member_value_or_default(data, member) {
                    let member_id = member.member_id;
                    let member_type = member.member_type.clone();
                    serializer
                        .write_member_with(member_id, false, |s| {
                            let mut nested = |s: &mut Xcdr2Serializer, inner: &DynamicData| {
                                serialize_struct_xcdr(s, inner, extensibility)
                            };
                            serialize_value_xcdr2(s, &value, &member_type, &mut nested)
                                .map_err(|e| CdrError::SerializationError(e.to_string()))
                        })
                        .map_err(cdr_error)?;
                }
            }

            serializer.end_struct(size_pos).map_err(cdr_error)?;
        }
    }

    Ok(())
}

fn deserialize_struct_xcdr(
    deserializer: &mut Xcdr2Deserializer,
    dynamic_type: &Arc<DynamicType>,
    extensibility: ExtensibilityKind,
) -> DdsResult<DynamicData> {
    let struct_desc = dynamic_type
        .as_struct()
        .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;

    let mut values = HashMap::new();

    match extensibility {
        ExtensibilityKind::Final => {
            values = deserialize_struct_members_xcdr2(deserializer, struct_desc)?;
        }
        ExtensibilityKind::Appendable => {
            let (object_size, start_pos) = deserializer.begin_struct().map_err(cdr_error)?;
            values = deserialize_struct_members_xcdr2(deserializer, struct_desc)?;
            deserializer.end_struct(object_size, start_pos).map_err(cdr_error)?;
        }
        ExtensibilityKind::Mutable => {
            let (object_size, start_pos) = deserializer.begin_struct().map_err(cdr_error)?;
            let object_end = start_pos + object_size as usize;

            while deserializer.get_position() < object_end {
                let (member_id, member_length, must_understand) =
                    deserializer.read_member_header_full().map_err(cdr_error)?;

                if let Some(member) = struct_desc.get_member_by_id(member_id) {
                    let value = deserialize_value_xcdr2(deserializer, &member.member_type)?;
                    values.insert(member.name.clone(), value);
                } else if must_understand {
                    return Err(DdsError::Error(format!(
                        "Unknown required member with id {}",
                        member_id
                    )));
                } else {
                    deserializer.skip_member(member_length).map_err(cdr_error)?;
                }
            }

            deserializer.end_struct(object_size, start_pos).map_err(cdr_error)?;
        }
    }

    Ok(DynamicData::with_values(dynamic_type.clone(), values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xtypes::{
        CompleteStructMember, CompleteStructType, CompleteTypeObject, EquivalenceHash, MemberFlag,
        PlainCollectionHeader, TypeFlag, TypeIdentifier,
    };

    fn create_test_type() -> Arc<DynamicType> {
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

        let type_object = CompleteTypeObject::Struct(struct_type);
        Arc::new(DynamicType::from_type_object(type_object, TypeIdentifier::None).unwrap())
    }

    fn create_collection_type() -> Arc<DynamicType> {
        let mut struct_type = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "CollectionStruct".to_string(),
            None,
        );

        struct_type.add_member(CompleteStructMember::new(
            0,
            MemberFlag::new(
                crate::xtypes::TryConstructKind::Discard,
                false,
                false,
                false,
                false,
                false,
            ),
            TypeIdentifier::PlainSequenceLarge {
                header: PlainCollectionHeader::default(),
                bound: 0,
                element_identifier: Box::new(TypeIdentifier::Int32),
            },
            "numbers".to_string(),
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
            TypeIdentifier::PlainArrayLarge {
                header: PlainCollectionHeader::default(),
                array_bound_seq: vec![3],
                element_identifier: Box::new(TypeIdentifier::Boolean),
            },
            "flags".to_string(),
        ));

        let type_object = CompleteTypeObject::Struct(struct_type);
        Arc::new(DynamicType::from_type_object(type_object, TypeIdentifier::None).unwrap())
    }

    fn create_external_member_type() -> Arc<DynamicType> {
        let mut struct_type = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "HasExternalMember".to_string(),
            None,
        );

        struct_type.add_member(CompleteStructMember::new(
            0,
            MemberFlag::new(
                crate::xtypes::TryConstructKind::Discard,
                false,
                false,
                false,
                false,
                false,
            ),
            TypeIdentifier::MinimalTypeId(EquivalenceHash::zero()),
            "child".to_string(),
        ));

        let type_object = CompleteTypeObject::Struct(struct_type);
        Arc::new(DynamicType::from_type_object(type_object, TypeIdentifier::None).unwrap())
    }

    #[test]
    fn test_cdr_roundtrip() {
        let dynamic_type = create_test_type();
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 42i32).unwrap();
        data.set("message", "Hello, World!").unwrap();

        let serialized = serialize_dynamic_data(&data, &SerializationFormat::Cdr).unwrap();
        let deserialized = deserialize_dynamic_data(&serialized, &dynamic_type).unwrap();

        assert_eq!(deserialized.get::<i32>("id").unwrap(), 42);
        assert_eq!(deserialized.get::<String>("message").unwrap(), "Hello, World!");
    }

    #[test]
    fn test_xcdr_final_roundtrip() {
        let dynamic_type = create_test_type();
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 100i32).unwrap();
        data.set("message", "XCDR Test").unwrap();

        let format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Final,
            use_delimiters: false,
        };
        let serialized = serialize_dynamic_data(&data, &format).unwrap();
        let deserialized = deserialize_dynamic_data(&serialized, &dynamic_type).unwrap();

        assert_eq!(deserialized.get::<i32>("id").unwrap(), 100);
        assert_eq!(deserialized.get::<String>("message").unwrap(), "XCDR Test");
    }

    #[test]
    fn test_collection_roundtrip_in_both_formats() {
        let dynamic_type = create_collection_type();
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set_value(
            "numbers",
            DynamicValue::Sequence(vec![
                DynamicValue::Int32(1),
                DynamicValue::Int32(2),
                DynamicValue::Int32(3),
            ]),
        )
        .unwrap();
        data.set_value(
            "flags",
            DynamicValue::Array(vec![
                DynamicValue::Boolean(true),
                DynamicValue::Boolean(false),
                DynamicValue::Boolean(true),
            ]),
        )
        .unwrap();

        for format in [
            SerializationFormat::Cdr,
            SerializationFormat::Xcdr {
                extensibility_kind: ExtensibilityKind::Final,
                use_delimiters: false,
            },
        ] {
            let serialized = serialize_dynamic_data(&data, &format).unwrap();
            let deserialized = deserialize_dynamic_data(&serialized, &dynamic_type).unwrap();

            assert_eq!(deserialized.get::<Vec<i32>>("numbers").unwrap(), vec![1, 2, 3]);
            assert_eq!(deserialized.get::<Vec<bool>>("flags").unwrap(), vec![true, false, true]);
        }
    }

    #[test]
    fn test_external_type_deserialization_remains_explicitly_unsupported() {
        let dynamic_type = create_external_member_type();

        let inner_type = create_test_type();
        let mut inner = DynamicData::new(inner_type);
        inner.set("id", 7i32).unwrap();
        inner.set("message", "child").unwrap();

        let mut outer = DynamicData::new(dynamic_type.clone());
        outer.set_value("child", DynamicValue::Struct(Box::new(inner))).unwrap();

        let serialized = serialize_dynamic_data(&outer, &SerializationFormat::Cdr).unwrap();
        let error = deserialize_dynamic_data(&serialized, &dynamic_type).unwrap_err();

        assert!(
            format!("{:?}", error).contains("External type deserialization is not supported yet")
        );
    }
}
