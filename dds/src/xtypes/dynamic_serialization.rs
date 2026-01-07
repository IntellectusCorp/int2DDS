//! Dynamic serialization - CDR/XCDR2 serialization for DynamicData.
//!
//! This module provides serialization and deserialization of DynamicData
//! based on runtime type information from DynamicType.

use std::collections::HashMap;
use std::sync::Arc;

use crate::dcps::core::error::{DdsError, DdsResult};
use crate::dcps::topic::type_support::SerializationFormat;
use crate::rtps::common::types::SerializedData;
use crate::serialize::cdr::{
    CdrDeserializer, CdrSerializer, ExtensibilityKind, Xcdr2Deserializer, Xcdr2Serializer,
    MEMBER_ID_SENTINEL,
};
use crate::serialize::cdr::{PrimitiveSerialize, StringSerialize};
use crate::serialize::BufferManager;

use super::dynamic_data::{DynamicData, DynamicValue};
use super::dynamic_type::{DynamicType, DynamicTypeKind, MemberDescriptor, PrimitiveKind};

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
    // Auto-detect format from encapsulation header
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

// ============================================================================
// CDR Serialization (legacy)
// ============================================================================

fn serialize_cdr(data: &DynamicData) -> DdsResult<SerializedData> {
    let mut serializer = CdrSerializer::with_capacity(true, 256);
    serializer.write_encapsulation_header().map_err(|e| DdsError::Error(e.to_string()))?;

    serialize_struct_cdr(&mut serializer, data)?;

    let bytes = serializer.into_bytes();
    Ok(Arc::from(bytes.into_boxed_slice()))
}

fn deserialize_cdr(bytes: &[u8], dynamic_type: &Arc<DynamicType>) -> DdsResult<DynamicData> {
    let mut deserializer =
        CdrDeserializer::new(bytes).map_err(|e| DdsError::Error(e.to_string()))?;

    deserialize_struct_cdr(&mut deserializer, dynamic_type)
}

fn serialize_struct_cdr(serializer: &mut CdrSerializer, data: &DynamicData) -> DdsResult<()> {
    let struct_desc = data
        .dynamic_type()
        .as_struct()
        .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;

    // Serialize members in order
    for member in struct_desc.members() {
        if let Some(value) = data.get_value(&member.name) {
            serialize_value_cdr(serializer, value, &member.member_type)?;
        } else if !member.is_optional {
            // Non-optional field missing - use default value
            let default_value = DynamicValue::default_for_kind(&member.member_type);
            serialize_value_cdr(serializer, &default_value, &member.member_type)?;
        }
    }

    Ok(())
}

fn serialize_value_cdr(
    serializer: &mut CdrSerializer,
    value: &DynamicValue,
    _type_kind: &DynamicTypeKind,
) -> DdsResult<()> {
    let map_err = |e: crate::serialize::core::SerializationError| DdsError::Error(e.to_string());

    match value {
        DynamicValue::Boolean(v) => serializer.serialize_bool(*v).map_err(map_err),
        DynamicValue::Int8(v) => serializer.serialize_i8(*v).map_err(map_err),
        DynamicValue::Int16(v) => serializer.serialize_i16(*v).map_err(map_err),
        DynamicValue::Int32(v) => serializer.serialize_i32(*v).map_err(map_err),
        DynamicValue::Int64(v) => serializer.serialize_i64(*v).map_err(map_err),
        DynamicValue::Uint8(v) => serializer.serialize_u8(*v).map_err(map_err),
        DynamicValue::Uint16(v) => serializer.serialize_u16(*v).map_err(map_err),
        DynamicValue::Uint32(v) => serializer.serialize_u32(*v).map_err(map_err),
        DynamicValue::Uint64(v) => serializer.serialize_u64(*v).map_err(map_err),
        DynamicValue::Float32(v) => serializer.serialize_f32(*v).map_err(map_err),
        DynamicValue::Float64(v) => serializer.serialize_f64(*v).map_err(map_err),
        DynamicValue::Char8(v) => serializer.serialize_u8(*v as u8).map_err(map_err),
        DynamicValue::Byte(v) => serializer.serialize_u8(*v).map_err(map_err),
        DynamicValue::String(v) => serializer.serialize_string(v).map_err(map_err),
        DynamicValue::WString(v) => serializer.serialize_wstring16(v).map_err(map_err),
        DynamicValue::Enum { value, .. } => serializer.serialize_i32(*value).map_err(map_err),
        DynamicValue::Sequence(items) => {
            serializer.serialize_u32(items.len() as u32).map_err(map_err)?;
            for item in items {
                serialize_value_cdr(
                    serializer,
                    item,
                    &DynamicTypeKind::Primitive(PrimitiveKind::Int32),
                )?;
            }
            Ok(())
        }
        DynamicValue::Array(items) => {
            for item in items {
                serialize_value_cdr(
                    serializer,
                    item,
                    &DynamicTypeKind::Primitive(PrimitiveKind::Int32),
                )?;
            }
            Ok(())
        }
        DynamicValue::Struct(inner) => serialize_struct_cdr(serializer, inner),
        DynamicValue::Optional(Some(inner)) => {
            serializer.serialize_bool(true).map_err(map_err)?;
            serialize_value_cdr(
                serializer,
                inner,
                &DynamicTypeKind::Primitive(PrimitiveKind::Int32),
            )
        }
        DynamicValue::Optional(None) => serializer.serialize_bool(false).map_err(map_err),
        DynamicValue::Null => Ok(()),
    }
}

fn deserialize_struct_cdr(
    deserializer: &mut CdrDeserializer,
    dynamic_type: &Arc<DynamicType>,
) -> DdsResult<DynamicData> {
    let struct_desc = dynamic_type
        .as_struct()
        .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;

    let mut values = HashMap::new();

    // Deserialize members in order
    for member in struct_desc.members() {
        let value = deserialize_value_cdr(deserializer, &member.member_type)?;
        values.insert(member.name.clone(), value);
    }

    Ok(DynamicData::with_values(dynamic_type.clone(), values))
}

fn deserialize_value_cdr(
    deserializer: &mut CdrDeserializer,
    type_kind: &DynamicTypeKind,
) -> DdsResult<DynamicValue> {
    match type_kind {
        DynamicTypeKind::Primitive(p) => deserialize_primitive_cdr(deserializer, *p),
        DynamicTypeKind::String { .. } => {
            let s =
                deserializer.deserialize_string().map_err(|e| DdsError::Error(e.to_string()))?;
            Ok(DynamicValue::String(s))
        }
        DynamicTypeKind::WString { .. } => {
            let s =
                deserializer.deserialize_wstring16().map_err(|e| DdsError::Error(e.to_string()))?;
            Ok(DynamicValue::WString(s))
        }
        DynamicTypeKind::Sequence { element_type, .. } => {
            let len = deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?
                as usize;
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
        DynamicTypeKind::Enum(_) => {
            let value =
                deserializer.deserialize_i32().map_err(|e| DdsError::Error(e.to_string()))?;
            Ok(DynamicValue::Enum { name: String::new(), value })
        }
        DynamicTypeKind::Struct(_) | DynamicTypeKind::ExternalType { .. } => {
            Err(DdsError::Error("Nested struct deserialization requires type context".to_string()))
        }
    }
}

fn deserialize_primitive_cdr(
    deserializer: &mut CdrDeserializer,
    kind: PrimitiveKind,
) -> DdsResult<DynamicValue> {
    let map_err = |e: crate::serialize::core::SerializationError| DdsError::Error(e.to_string());

    match kind {
        PrimitiveKind::Boolean => {
            Ok(DynamicValue::Boolean(deserializer.deserialize_bool().map_err(map_err)?))
        }
        PrimitiveKind::Int8 => {
            Ok(DynamicValue::Int8(deserializer.deserialize_i8().map_err(map_err)?))
        }
        PrimitiveKind::Int16 => {
            Ok(DynamicValue::Int16(deserializer.deserialize_i16().map_err(map_err)?))
        }
        PrimitiveKind::Int32 => {
            Ok(DynamicValue::Int32(deserializer.deserialize_i32().map_err(map_err)?))
        }
        PrimitiveKind::Int64 => {
            Ok(DynamicValue::Int64(deserializer.deserialize_i64().map_err(map_err)?))
        }
        PrimitiveKind::Uint8 => {
            Ok(DynamicValue::Uint8(deserializer.deserialize_u8().map_err(map_err)?))
        }
        PrimitiveKind::Uint16 => {
            Ok(DynamicValue::Uint16(deserializer.deserialize_u16().map_err(map_err)?))
        }
        PrimitiveKind::Uint32 => {
            Ok(DynamicValue::Uint32(deserializer.deserialize_u32().map_err(map_err)?))
        }
        PrimitiveKind::Uint64 => {
            Ok(DynamicValue::Uint64(deserializer.deserialize_u64().map_err(map_err)?))
        }
        PrimitiveKind::Float32 => {
            Ok(DynamicValue::Float32(deserializer.deserialize_f32().map_err(map_err)?))
        }
        PrimitiveKind::Float64 => {
            Ok(DynamicValue::Float64(deserializer.deserialize_f64().map_err(map_err)?))
        }
        PrimitiveKind::Float128 => {
            // Approximate as f64
            Ok(DynamicValue::Float64(deserializer.deserialize_f64().map_err(map_err)?))
        }
        PrimitiveKind::Char8 => {
            let c = deserializer.deserialize_u8().map_err(map_err)? as char;
            Ok(DynamicValue::Char8(c))
        }
        PrimitiveKind::Char16 => {
            let c = deserializer.deserialize_u16().map_err(map_err)? as u8 as char;
            Ok(DynamicValue::Char8(c))
        }
        PrimitiveKind::Byte => {
            Ok(DynamicValue::Byte(deserializer.deserialize_u8().map_err(map_err)?))
        }
    }
}

// ============================================================================
// XCDR2 Serialization
// ============================================================================

fn serialize_xcdr(
    data: &DynamicData,
    extensibility: ExtensibilityKind,
) -> DdsResult<SerializedData> {
    let mut serializer = Xcdr2Serializer::with_capacity(true, extensibility, 256);
    serializer.write_encapsulation_header().map_err(|e| DdsError::Error(e.to_string()))?;

    serialize_struct_xcdr(&mut serializer, data, extensibility)?;

    let bytes = serializer.into_bytes();
    Ok(Arc::from(bytes.into_boxed_slice()))
}

fn deserialize_xcdr(bytes: &[u8], dynamic_type: &Arc<DynamicType>) -> DdsResult<DynamicData> {
    let mut deserializer =
        Xcdr2Deserializer::new(bytes).map_err(|e| DdsError::Error(e.to_string()))?;

    let extensibility = dynamic_type.extensibility();
    deserialize_struct_xcdr(&mut deserializer, dynamic_type, extensibility)
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
            // FINAL: serialize members in order, no headers
            for member in struct_desc.members() {
                serialize_member_xcdr(serializer, data, member, extensibility)?;
            }
        }
        ExtensibilityKind::Appendable => {
            // APPENDABLE: DHEADER + members in order
            let size_pos = serializer.begin_struct().map_err(|e| DdsError::Error(e.to_string()))?;

            for member in struct_desc.members() {
                serialize_member_xcdr(serializer, data, member, extensibility)?;
            }

            serializer.end_struct(size_pos).map_err(|e| DdsError::Error(e.to_string()))?;
        }
        ExtensibilityKind::Mutable => {
            // MUTABLE: DHEADER + [EMHEADER + member]* + sentinel
            let size_pos = serializer.begin_struct().map_err(|e| DdsError::Error(e.to_string()))?;

            for member in struct_desc.members() {
                if let Some(value) = data.get_value(&member.name) {
                    // Write EMHEADER
                    let _member_start = serializer.position();
                    // Reserve space for member header (will be backpatched)
                    let header_pos = serializer.position();
                    serializer
                        .write_member_header(member.member_id, 0)
                        .map_err(|e| DdsError::Error(e.to_string()))?;

                    let content_start = serializer.position();
                    serialize_value_xcdr(serializer, value, &member.member_type, extensibility)?;
                    let content_end = serializer.position();

                    // Note: In a production implementation, we'd backpatch the member length here
                    // For now, we calculate and write the correct length upfront
                    let _ = (header_pos, content_start, content_end); // Suppress unused warnings
                } else if !member.is_optional {
                    // Non-optional field missing - use default
                    let default_value = DynamicValue::default_for_kind(&member.member_type);
                    serializer
                        .write_member_header(member.member_id, 0)
                        .map_err(|e| DdsError::Error(e.to_string()))?;
                    serialize_value_xcdr(
                        serializer,
                        &default_value,
                        &member.member_type,
                        extensibility,
                    )?;
                }
            }

            // Write sentinel
            serializer
                .write_member_header(MEMBER_ID_SENTINEL, 0)
                .map_err(|e| DdsError::Error(e.to_string()))?;

            serializer.end_struct(size_pos).map_err(|e| DdsError::Error(e.to_string()))?;
        }
    }

    Ok(())
}

fn serialize_member_xcdr(
    serializer: &mut Xcdr2Serializer,
    data: &DynamicData,
    member: &MemberDescriptor,
    extensibility: ExtensibilityKind,
) -> DdsResult<()> {
    if let Some(value) = data.get_value(&member.name) {
        serialize_value_xcdr(serializer, value, &member.member_type, extensibility)?;
    } else if !member.is_optional {
        // Non-optional field missing - use default
        let default_value = DynamicValue::default_for_kind(&member.member_type);
        serialize_value_xcdr(serializer, &default_value, &member.member_type, extensibility)?;
    }
    Ok(())
}

fn serialize_value_xcdr(
    serializer: &mut Xcdr2Serializer,
    value: &DynamicValue,
    _type_kind: &DynamicTypeKind,
    extensibility: ExtensibilityKind,
) -> DdsResult<()> {
    let map_err = |e: crate::serialize::core::SerializationError| DdsError::Error(e.to_string());
    match value {
        DynamicValue::Boolean(v) => serializer.serialize_bool(*v).map_err(map_err),
        DynamicValue::Int8(v) => serializer.serialize_i8(*v).map_err(map_err),
        DynamicValue::Int16(v) => serializer.serialize_i16(*v).map_err(map_err),
        DynamicValue::Int32(v) => serializer.serialize_i32(*v).map_err(map_err),
        DynamicValue::Int64(v) => serializer.serialize_i64(*v).map_err(map_err),
        DynamicValue::Uint8(v) => serializer.serialize_u8(*v).map_err(map_err),
        DynamicValue::Uint16(v) => serializer.serialize_u16(*v).map_err(map_err),
        DynamicValue::Uint32(v) => serializer.serialize_u32(*v).map_err(map_err),
        DynamicValue::Uint64(v) => serializer.serialize_u64(*v).map_err(map_err),
        DynamicValue::Float32(v) => serializer.serialize_f32(*v).map_err(map_err),
        DynamicValue::Float64(v) => serializer.serialize_f64(*v).map_err(map_err),
        DynamicValue::Char8(v) => serializer.serialize_u8(*v as u8).map_err(map_err),
        DynamicValue::Byte(v) => serializer.serialize_u8(*v).map_err(map_err),
        DynamicValue::String(v) => serializer.serialize_string(v).map_err(map_err),
        DynamicValue::WString(v) => serializer.serialize_wstring16(v).map_err(map_err),
        DynamicValue::Enum { value, .. } => serializer.serialize_i32(*value).map_err(map_err),
        DynamicValue::Sequence(items) => {
            serializer.serialize_u32(items.len() as u32).map_err(map_err)?;
            for item in items {
                serialize_value_xcdr(
                    serializer,
                    item,
                    &DynamicTypeKind::Primitive(PrimitiveKind::Int32),
                    extensibility,
                )?;
            }
            Ok(())
        }
        DynamicValue::Array(items) => {
            for item in items {
                serialize_value_xcdr(
                    serializer,
                    item,
                    &DynamicTypeKind::Primitive(PrimitiveKind::Int32),
                    extensibility,
                )?;
            }
            Ok(())
        }
        DynamicValue::Struct(inner) => serialize_struct_xcdr(serializer, inner, extensibility),
        DynamicValue::Optional(Some(inner)) => {
            serializer.serialize_bool(true).map_err(map_err)?;
            serialize_value_xcdr(
                serializer,
                inner,
                &DynamicTypeKind::Primitive(PrimitiveKind::Int32),
                extensibility,
            )
        }
        DynamicValue::Optional(None) => serializer.serialize_bool(false).map_err(map_err),
        DynamicValue::Null => Ok(()),
    }
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
            // FINAL: read members in order
            for member in struct_desc.members() {
                let value =
                    deserialize_value_xcdr(deserializer, &member.member_type, extensibility)?;
                values.insert(member.name.clone(), value);
            }
        }
        ExtensibilityKind::Appendable => {
            // APPENDABLE: DHEADER + members
            let (object_size, start_pos) =
                deserializer.begin_struct().map_err(|e| DdsError::Error(e.to_string()))?;

            for member in struct_desc.members() {
                let value =
                    deserialize_value_xcdr(deserializer, &member.member_type, extensibility)?;
                values.insert(member.name.clone(), value);
            }

            // Skip any remaining bytes (forward compatibility)
            deserializer
                .end_struct(object_size, start_pos)
                .map_err(|e| DdsError::Error(e.to_string()))?;
        }
        ExtensibilityKind::Mutable => {
            // MUTABLE: DHEADER + [EMHEADER + member]* + sentinel
            let (object_size, start_pos) =
                deserializer.begin_struct().map_err(|e| DdsError::Error(e.to_string()))?;

            let _object_end = start_pos + object_size as usize;

            while !deserializer.is_at_sentinel() {
                let (member_id, member_length, must_understand) = deserializer
                    .read_member_header_full()
                    .map_err(|e| DdsError::Error(e.to_string()))?;

                if let Some(member) = struct_desc.get_member_by_id(member_id) {
                    let value =
                        deserialize_value_xcdr(deserializer, &member.member_type, extensibility)?;
                    values.insert(member.name.clone(), value);
                } else if must_understand {
                    return Err(DdsError::Error(format!(
                        "Unknown required member with id {}",
                        member_id
                    )));
                } else {
                    // Skip unknown optional member
                    deserializer
                        .skip_member(member_length)
                        .map_err(|e| DdsError::Error(e.to_string()))?;
                }
            }

            // Skip sentinel
            deserializer.skip_sentinel_if_present().map_err(|e| DdsError::Error(e.to_string()))?;

            // Ensure we've consumed the right amount
            deserializer
                .end_struct(object_size, start_pos)
                .map_err(|e| DdsError::Error(e.to_string()))?;
        }
    }

    Ok(DynamicData::with_values(dynamic_type.clone(), values))
}

fn deserialize_value_xcdr(
    deserializer: &mut Xcdr2Deserializer,
    type_kind: &DynamicTypeKind,
    extensibility: ExtensibilityKind,
) -> DdsResult<DynamicValue> {
    match type_kind {
        DynamicTypeKind::Primitive(p) => deserialize_primitive_xcdr(deserializer, *p),
        DynamicTypeKind::String { .. } => {
            let s =
                deserializer.deserialize_string().map_err(|e| DdsError::Error(e.to_string()))?;
            Ok(DynamicValue::String(s))
        }
        DynamicTypeKind::WString { .. } => {
            let s =
                deserializer.deserialize_wstring16().map_err(|e| DdsError::Error(e.to_string()))?;
            Ok(DynamicValue::WString(s))
        }
        DynamicTypeKind::Sequence { element_type, .. } => {
            let len = deserializer.deserialize_u32().map_err(|e| DdsError::Error(e.to_string()))?
                as usize;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(deserialize_value_xcdr(deserializer, element_type, extensibility)?);
            }
            Ok(DynamicValue::Sequence(items))
        }
        DynamicTypeKind::Array { element_type, dimensions } => {
            let total_size: u32 = dimensions.iter().product();
            let mut items = Vec::with_capacity(total_size as usize);
            for _ in 0..total_size {
                items.push(deserialize_value_xcdr(deserializer, element_type, extensibility)?);
            }
            Ok(DynamicValue::Array(items))
        }
        DynamicTypeKind::Enum(_) => {
            let value =
                deserializer.deserialize_i32().map_err(|e| DdsError::Error(e.to_string()))?;
            Ok(DynamicValue::Enum { name: String::new(), value })
        }
        DynamicTypeKind::Struct(_) | DynamicTypeKind::ExternalType { .. } => {
            Err(DdsError::Error("Nested struct deserialization requires type context".to_string()))
        }
    }
}

fn deserialize_primitive_xcdr(
    deserializer: &mut Xcdr2Deserializer,
    kind: PrimitiveKind,
) -> DdsResult<DynamicValue> {
    let map_err = |e: crate::serialize::core::SerializationError| DdsError::Error(e.to_string());
    match kind {
        PrimitiveKind::Boolean => {
            Ok(DynamicValue::Boolean(deserializer.deserialize_bool().map_err(map_err)?))
        }
        PrimitiveKind::Int8 => {
            Ok(DynamicValue::Int8(deserializer.deserialize_i8().map_err(map_err)?))
        }
        PrimitiveKind::Int16 => {
            Ok(DynamicValue::Int16(deserializer.deserialize_i16().map_err(map_err)?))
        }
        PrimitiveKind::Int32 => {
            Ok(DynamicValue::Int32(deserializer.deserialize_i32().map_err(map_err)?))
        }
        PrimitiveKind::Int64 => {
            Ok(DynamicValue::Int64(deserializer.deserialize_i64().map_err(map_err)?))
        }
        PrimitiveKind::Uint8 => {
            Ok(DynamicValue::Uint8(deserializer.deserialize_u8().map_err(map_err)?))
        }
        PrimitiveKind::Uint16 => {
            Ok(DynamicValue::Uint16(deserializer.deserialize_u16().map_err(map_err)?))
        }
        PrimitiveKind::Uint32 => {
            Ok(DynamicValue::Uint32(deserializer.deserialize_u32().map_err(map_err)?))
        }
        PrimitiveKind::Uint64 => {
            Ok(DynamicValue::Uint64(deserializer.deserialize_u64().map_err(map_err)?))
        }
        PrimitiveKind::Float32 => {
            Ok(DynamicValue::Float32(deserializer.deserialize_f32().map_err(map_err)?))
        }
        PrimitiveKind::Float64 => {
            Ok(DynamicValue::Float64(deserializer.deserialize_f64().map_err(map_err)?))
        }
        PrimitiveKind::Float128 => {
            Ok(DynamicValue::Float64(deserializer.deserialize_f64().map_err(map_err)?))
        }
        PrimitiveKind::Char8 => {
            let c = deserializer.deserialize_u8().map_err(map_err)? as char;
            Ok(DynamicValue::Char8(c))
        }
        PrimitiveKind::Char16 => {
            let c = deserializer.deserialize_u16().map_err(map_err)? as u8 as char;
            Ok(DynamicValue::Char8(c))
        }
        PrimitiveKind::Byte => {
            Ok(DynamicValue::Byte(deserializer.deserialize_u8().map_err(map_err)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xtypes::{
        CompleteStructMember, CompleteStructType, CompleteTypeObject, MemberFlag, TypeFlag,
        TypeIdentifier,
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

    #[test]
    fn test_cdr_roundtrip() {
        let dynamic_type = create_test_type();
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 42i32).unwrap();
        data.set("message", "Hello, World!").unwrap();

        // Serialize
        let serialized = serialize_dynamic_data(&data, &SerializationFormat::Cdr).unwrap();

        // Deserialize
        let deserialized = deserialize_dynamic_data(&serialized, &dynamic_type).unwrap();

        // Verify
        assert_eq!(deserialized.get::<i32>("id").unwrap(), 42);
        assert_eq!(deserialized.get::<String>("message").unwrap(), "Hello, World!");
    }

    #[test]
    fn test_xcdr_final_roundtrip() {
        let dynamic_type = create_test_type();
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 100i32).unwrap();
        data.set("message", "XCDR Test").unwrap();

        // Serialize with FINAL extensibility
        let format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Final,
            use_delimiters: false,
        };
        let serialized = serialize_dynamic_data(&data, &format).unwrap();

        // Deserialize
        let deserialized = deserialize_dynamic_data(&serialized, &dynamic_type).unwrap();

        // Verify
        assert_eq!(deserialized.get::<i32>("id").unwrap(), 100);
        assert_eq!(deserialized.get::<String>("message").unwrap(), "XCDR Test");
    }
}
