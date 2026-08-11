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
    CdrDeserializer, CdrError, CdrSerializer, ExtensibilityKind, PlCdrMemberHeader,
    PrimitiveSerialize, StringSerialize, Xcdr2Deserializer, Xcdr2Serializer,
};
use crate::serialize::{BufferManager, DeserializerReader};

use super::dynamic_data::{DynamicData, DynamicValue};
use super::dynamic_type::{
    DynamicType, DynamicTypeKind, EnumDescriptor, MemberDescriptor, PrimitiveKind,
    StructDescriptor, UnionDescriptor,
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

fn checked_array_len(dimensions: &[u32]) -> DdsResult<u32> {
    dimensions.iter().try_fold(1u32, |acc, &d| acc.checked_mul(d)).ok_or_else(|| {
        DdsError::Error(format!("array dimensions overflow element count: {:?}", dimensions))
    })
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
    // Enum and bitmask are constructed types, not primitives, so collections of them are
    // framed (DDS-XTypes 7.4.3.5.3/7.4.3.5.4). OMG issue DDSXTY14-56 proposes exempting
    // them; it is unresolved, so this follows the spec as written.
    fn is_scalar(kind: &DynamicTypeKind) -> bool {
        matches!(kind, DynamicTypeKind::Primitive(_) | DynamicTypeKind::Bitset(_))
    }
    match kind {
        DynamicTypeKind::TypeRef(inner) => is_scalar(inner.kind()),
        other => is_scalar(other),
    }
}

/// Unwrap a `TypeRef` to its underlying kind (composite metadata), leaving other
/// kinds untouched.
fn resolved_kind(kind: &DynamicTypeKind) -> &DynamicTypeKind {
    match kind {
        DynamicTypeKind::TypeRef(inner) => inner.kind(),
        other => other,
    }
}

/// Resolve a union kind (directly or via `TypeRef`) into its descriptor and the
/// extensibility used to frame it.
fn as_union_kind(kind: &DynamicTypeKind) -> Option<(&UnionDescriptor, ExtensibilityKind)> {
    match kind {
        DynamicTypeKind::Union(desc) => Some((desc, ExtensibilityKind::Final)),
        DynamicTypeKind::TypeRef(inner) => match inner.kind() {
            DynamicTypeKind::Union(desc) => Some((desc, inner.extensibility())),
            _ => None,
        },
        _ => None,
    }
}

/// Wire width in bytes for a packed integer (bitmask/bitset) of `bits` bits.
fn packed_wire_width(bits: u16) -> u8 {
    if bits <= 8 {
        1
    } else if bits <= 16 {
        2
    } else if bits <= 32 {
        4
    } else {
        8
    }
}

fn serialize_packed<S: PrimitiveSerialize>(
    serializer: &mut S,
    bits: u64,
    width: u8,
) -> Result<(), CdrError> {
    match width {
        1 => serializer.serialize_u8(bits as u8),
        2 => serializer.serialize_u16(bits as u16),
        4 => serializer.serialize_u32(bits as u32),
        _ => serializer.serialize_u64(bits),
    }
}

fn deserialize_packed<D: ValueDeserializer>(
    deserializer: &mut D,
    width: u8,
) -> Result<u64, CdrError> {
    Ok(match width {
        1 => deserializer.deserialize_u8()? as u64,
        2 => deserializer.deserialize_u16()? as u64,
        4 => deserializer.deserialize_u32()? as u64,
        _ => deserializer.deserialize_u64()?,
    })
}

/// Extract the integer value of a union discriminator for case-label matching.
fn discriminator_as_i64(value: &DynamicValue) -> DdsResult<i64> {
    Ok(match value {
        DynamicValue::Boolean(v) => *v as i64,
        DynamicValue::Int8(v) => *v as i64,
        DynamicValue::Int16(v) => *v as i64,
        DynamicValue::Int32(v) => *v as i64,
        DynamicValue::Int64(v) => *v,
        DynamicValue::Uint8(v) => *v as i64,
        DynamicValue::Uint16(v) => *v as i64,
        DynamicValue::Uint32(v) => *v as i64,
        DynamicValue::Uint64(v) => *v as i64,
        DynamicValue::Char8(v) => *v as i64,
        DynamicValue::Byte(v) => *v as i64,
        DynamicValue::Enum { value, .. } => *value as i64,
        other => {
            return Err(DdsError::Error(format!(
                "invalid union discriminator value: {}",
                other.type_kind()
            )))
        }
    })
}

fn select_union_member<'a>(
    union_desc: &'a UnionDescriptor,
    discriminator: i64,
) -> DdsResult<&'a super::dynamic_type::UnionMemberDescriptor> {
    union_desc.select_member(discriminator).ok_or_else(|| {
        DdsError::Error(format!("no union member matches discriminator {}", discriminator))
    })
}

fn enum_wire_width(bit_bound: u16) -> u8 {
    if bit_bound <= 8 {
        1
    } else if bit_bound <= 16 {
        2
    } else {
        4
    }
}

fn enum_bit_bound(type_kind: &DynamicTypeKind) -> Option<u16> {
    match type_kind {
        DynamicTypeKind::Enum(desc) => Some(desc.bit_bound()),
        DynamicTypeKind::TypeRef(inner) => match inner.kind() {
            DynamicTypeKind::Enum(desc) => Some(desc.bit_bound()),
            _ => None,
        },
        _ => None,
    }
}

fn serialize_enum_value<S: PrimitiveSerialize>(
    serializer: &mut S,
    value: i32,
    bit_bound: u16,
) -> Result<(), CdrError> {
    match enum_wire_width(bit_bound) {
        1 => serializer.serialize_i8(value as i8),
        2 => serializer.serialize_i16(value as i16),
        _ => serializer.serialize_i32(value),
    }
}

fn deserialize_enum_value<D: ValueDeserializer>(
    deserializer: &mut D,
    bit_bound: u16,
) -> Result<i32, CdrError> {
    Ok(match enum_wire_width(bit_bound) {
        1 => deserializer.deserialize_i8()? as i32,
        2 => deserializer.deserialize_i16()? as i32,
        _ => deserializer.deserialize_i32()?,
    })
}

/// Read an enum value and resolve its literal name into a `DynamicValue::Enum`.
fn deserialize_enum<D: ValueDeserializer>(
    deserializer: &mut D,
    enum_desc: &EnumDescriptor,
) -> DdsResult<DynamicValue> {
    let value = deserialize_enum_value(deserializer, enum_desc.bit_bound()).map_err(cdr_error)?;
    let name = enum_desc
        .get_literal_by_value(value)
        .map(|literal| literal.name.clone())
        .unwrap_or_default();
    Ok(DynamicValue::Enum { name, value })
}

/// Write `f`, framing it with an XCDR2 collection DHEADER when `framed` is true
/// (non-primitive elements), else write it inline.
fn write_collection_framed<F>(serializer: &mut Xcdr2Serializer, framed: bool, f: F) -> DdsResult<()>
where
    F: FnOnce(&mut Xcdr2Serializer) -> DdsResult<()>,
{
    if !framed {
        return f(serializer);
    }
    let dh = serializer.reserve_dheader();
    let start = serializer.position();
    f(serializer)?;
    let size = (serializer.position() - start) as u32;
    serializer.write_dheader_at(dh, size);
    Ok(())
}

fn serialize_atomic_value<S, F>(
    serializer: &mut S,
    value: &DynamicValue,
    type_kind: &DynamicTypeKind,
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
        DynamicValue::Enum { value, .. } => {
            let bit_bound = enum_bit_bound(type_kind).unwrap_or(32);
            serialize_enum_value(serializer, *value, bit_bound).map_err(cdr_error)
        }
        DynamicValue::Struct(inner) => serialize_nested_struct(serializer, inner),
        DynamicValue::Bitmask(bits) => {
            let width = match resolved_kind(type_kind) {
                DynamicTypeKind::Bitmask(desc) => packed_wire_width(desc.bit_bound),
                _ => 8,
            };
            serialize_packed(serializer, *bits, width).map_err(cdr_error)
        }
        DynamicValue::Bitset(bits) => {
            let width = match resolved_kind(type_kind) {
                DynamicTypeKind::Bitset(desc) => packed_wire_width(desc.total_bits()),
                _ => 8,
            };
            serialize_packed(serializer, *bits, width).map_err(cdr_error)
        }
        DynamicValue::Null => Ok(()),
        // Union handled by callers (format-specific framing).
        DynamicValue::Union { .. } => Err(DdsError::Error(
            "serialize_atomic_value called with union; use format-specific path".to_string(),
        )),
        // Sequence/Array/Map/Optional handled by callers with format-specific logic
        DynamicValue::Sequence(_)
        | DynamicValue::Array(_)
        | DynamicValue::Map(_)
        | DynamicValue::Optional(_) => Err(DdsError::Error(
            "serialize_atomic_value called with collection/optional; use format-specific path"
                .to_string(),
        )),
    }
}

fn serialize_value_cdr<F>(
    serializer: &mut CdrSerializer,
    value: &DynamicValue,
    type_kind: &DynamicTypeKind,
    serialize_nested_struct: &mut F,
) -> DdsResult<()>
where
    F: FnMut(&mut CdrSerializer, &DynamicData) -> DdsResult<()>,
{
    match value {
        DynamicValue::Sequence(items) => {
            let element_type = match type_kind {
                DynamicTypeKind::Sequence { element_type, .. } => element_type.as_ref(),
                _ => return Err(DdsError::Error("type mismatch: expected Sequence".to_string())),
            };
            serializer.serialize_u32(items.len() as u32).map_err(cdr_error)?;
            for item in items {
                serialize_value_cdr(serializer, item, element_type, serialize_nested_struct)?;
            }
            Ok(())
        }
        DynamicValue::Array(items) => {
            let element_type = match type_kind {
                DynamicTypeKind::Array { element_type, .. } => element_type.as_ref(),
                _ => return Err(DdsError::Error("type mismatch: expected Array".to_string())),
            };
            for item in items {
                serialize_value_cdr(serializer, item, element_type, serialize_nested_struct)?;
            }
            Ok(())
        }
        DynamicValue::Optional(Some(inner)) => {
            serializer.serialize_bool(true).map_err(cdr_error)?;
            serialize_value_cdr(serializer, inner, type_kind, serialize_nested_struct)
        }
        DynamicValue::Optional(None) => serializer.serialize_bool(false).map_err(cdr_error),
        DynamicValue::Union { discriminator, value } => {
            let (union_desc, _ext) = as_union_kind(type_kind)
                .ok_or_else(|| DdsError::Error("type mismatch: expected Union".to_string()))?;
            serialize_value_cdr(
                serializer,
                discriminator,
                union_desc.discriminator_type(),
                serialize_nested_struct,
            )?;
            let disc = discriminator_as_i64(discriminator)?;
            let member = select_union_member(union_desc, disc)?;
            serialize_value_cdr(serializer, value, &member.member_type, serialize_nested_struct)
        }
        DynamicValue::Map(entries) => {
            let (key_type, value_type) = map_element_types(type_kind)?;
            serializer.serialize_u32(entries.len() as u32).map_err(cdr_error)?;
            for (key, value) in entries {
                serialize_value_cdr(serializer, key, key_type, serialize_nested_struct)?;
                serialize_value_cdr(serializer, value, value_type, serialize_nested_struct)?;
            }
            Ok(())
        }
        other => serialize_atomic_value(serializer, other, type_kind, serialize_nested_struct),
    }
}

/// Resolve the key/value element kinds of a `Map` type, erroring on mismatch.
fn map_element_types(
    type_kind: &DynamicTypeKind,
) -> DdsResult<(&DynamicTypeKind, &DynamicTypeKind)> {
    match type_kind {
        DynamicTypeKind::Map { key_type, value_type, .. } => {
            Ok((key_type.as_ref(), value_type.as_ref()))
        }
        _ => Err(DdsError::Error("type mismatch: expected Map".to_string())),
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
            let write_inner = |s: &mut Xcdr2Serializer| -> DdsResult<()> {
                s.serialize_u32(items.len() as u32).map_err(cdr_error)?;
                for item in items {
                    serialize_value_xcdr2(s, item, element_type, serialize_nested_struct)?;
                }
                Ok(())
            };
            let framed = !is_primitive_kind(element_type) && !serializer.plain_collections();
            write_collection_framed(serializer, framed, write_inner)
        }
        DynamicValue::Array(items) => {
            let element_type = match type_kind {
                DynamicTypeKind::Array { element_type, .. } => element_type.as_ref(),
                _ => return Err(DdsError::Error("type mismatch: expected Array".to_string())),
            };
            let write_inner = |s: &mut Xcdr2Serializer| -> DdsResult<()> {
                for item in items {
                    serialize_value_xcdr2(s, item, element_type, serialize_nested_struct)?;
                }
                Ok(())
            };
            let framed = !is_primitive_kind(element_type) && !serializer.plain_collections();
            write_collection_framed(serializer, framed, write_inner)
        }
        DynamicValue::Optional(Some(inner)) => {
            serializer.serialize_bool(true).map_err(cdr_error)?;
            serialize_value_xcdr2(serializer, inner, type_kind, serialize_nested_struct)
        }
        DynamicValue::Optional(None) => serializer.serialize_bool(false).map_err(cdr_error),
        DynamicValue::Union { discriminator, value } => {
            let (union_desc, ext) = as_union_kind(type_kind)
                .ok_or_else(|| DdsError::Error("type mismatch: expected Union".to_string()))?;
            serialize_union_xcdr2(
                serializer,
                discriminator,
                value,
                union_desc,
                ext,
                serialize_nested_struct,
            )
        }
        DynamicValue::Map(entries) => {
            let (key_type, value_type) = map_element_types(type_kind)?;
            let write_inner = |s: &mut Xcdr2Serializer| -> DdsResult<()> {
                s.serialize_u32(entries.len() as u32).map_err(cdr_error)?;
                for (key, value) in entries {
                    serialize_value_xcdr2(s, key, key_type, serialize_nested_struct)?;
                    serialize_value_xcdr2(s, value, value_type, serialize_nested_struct)?;
                }
                Ok(())
            };
            let framed = !(is_primitive_kind(key_type) && is_primitive_kind(value_type));
            write_collection_framed(serializer, framed, write_inner)
        }
        other => serialize_atomic_value(serializer, other, type_kind, serialize_nested_struct),
    }
}

fn serialize_union_xcdr2<F>(
    serializer: &mut Xcdr2Serializer,
    discriminator: &DynamicValue,
    value: &DynamicValue,
    union_desc: &UnionDescriptor,
    extensibility: ExtensibilityKind,
    nested: &mut F,
) -> DdsResult<()>
where
    F: FnMut(&mut Xcdr2Serializer, &DynamicData) -> DdsResult<()>,
{
    let disc = discriminator_as_i64(discriminator)?;
    let member = select_union_member(union_desc, disc)?;
    let disc_type = union_desc.discriminator_type();

    match extensibility {
        ExtensibilityKind::Final => {
            serialize_value_xcdr2(serializer, discriminator, disc_type, nested)?;
            serialize_value_xcdr2(serializer, value, &member.member_type, nested)
        }
        ExtensibilityKind::Appendable => {
            let size_pos = serializer.begin_struct().map_err(cdr_error)?;
            serialize_value_xcdr2(serializer, discriminator, disc_type, nested)?;
            serialize_value_xcdr2(serializer, value, &member.member_type, nested)?;
            serializer.end_struct(size_pos).map_err(cdr_error)
        }
        ExtensibilityKind::Mutable => {
            let size_pos = serializer.begin_struct().map_err(cdr_error)?;
            let branch_id = member.index as u32 + 1;
            let member_type = member.member_type.clone();
            serializer
                .write_member_with(0, false, |s| {
                    serialize_value_xcdr2(s, discriminator, disc_type, nested)
                        .map_err(|e| CdrError::SerializationError(e.to_string()))
                })
                .map_err(cdr_error)?;
            serializer
                .write_member_with(branch_id, false, |s| {
                    serialize_value_xcdr2(s, value, &member_type, nested)
                        .map_err(|e| CdrError::SerializationError(e.to_string()))
                })
                .map_err(cdr_error)?;
            serializer.end_struct(size_pos).map_err(cdr_error)
        }
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
        DynamicTypeKind::Enum(enum_desc) => deserialize_enum(deserializer, enum_desc),
        DynamicTypeKind::Struct(struct_desc) => {
            Err(nested_struct_deserialization_error(struct_desc))
        }
        DynamicTypeKind::Bitmask(desc) => {
            let bits = deserialize_packed(deserializer, packed_wire_width(desc.bit_bound))
                .map_err(cdr_error)?;
            Ok(DynamicValue::Bitmask(bits))
        }
        DynamicTypeKind::Bitset(desc) => {
            let bits = deserialize_packed(deserializer, packed_wire_width(desc.total_bits()))
                .map_err(cdr_error)?;
            Ok(DynamicValue::Bitset(bits))
        }
        DynamicTypeKind::Union(_) => Err(DdsError::Error(
            "Union must be handled by the format-specific deserialization path".to_string(),
        )),
        DynamicTypeKind::TypeRef(_) => Err(DdsError::Error(
            "TypeRef must be handled by the format-specific deserialization path".to_string(),
        )),
        DynamicTypeKind::ExternalType { .. } => Err(external_type_deserialization_error(type_kind)),
        DynamicTypeKind::Sequence { .. }
        | DynamicTypeKind::Array { .. }
        | DynamicTypeKind::Map { .. } => Err(DdsError::Error(
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
            let mut items = Vec::new();
            for _ in 0..len {
                items.push(deserialize_value_cdr(deserializer, element_type)?);
            }
            Ok(DynamicValue::Sequence(items))
        }
        DynamicTypeKind::Array { element_type, dimensions } => {
            let total_size = checked_array_len(dimensions)?;
            let mut items = Vec::new();
            for _ in 0..total_size {
                items.push(deserialize_value_cdr(deserializer, element_type)?);
            }
            Ok(DynamicValue::Array(items))
        }
        DynamicTypeKind::Map { key_type, value_type, .. } => {
            let len = deserializer.deserialize_u32().map_err(cdr_error)? as usize;
            let mut entries = Vec::new();
            for _ in 0..len {
                let key = deserialize_value_cdr(deserializer, key_type)?;
                let value = deserialize_value_cdr(deserializer, value_type)?;
                entries.push((key, value));
            }
            Ok(DynamicValue::Map(entries))
        }
        DynamicTypeKind::TypeRef(inner) => match inner.kind() {
            DynamicTypeKind::Struct(_) => {
                let nested = deserialize_struct_cdr(deserializer, inner)?;
                Ok(DynamicValue::Struct(Box::new(nested)))
            }
            DynamicTypeKind::Enum(enum_desc) => deserialize_enum(deserializer, enum_desc),
            DynamicTypeKind::Union(union_desc) => deserialize_union_cdr(deserializer, union_desc),
            other => deserialize_atomic_value(deserializer, other),
        },
        other => deserialize_atomic_value(deserializer, other),
    }
}

/// Deserialize an XCDR1 (PLAIN) union: discriminator then the selected branch.
fn deserialize_union_cdr(
    deserializer: &mut CdrDeserializer,
    union_desc: &UnionDescriptor,
) -> DdsResult<DynamicValue> {
    let discriminator = deserialize_value_cdr(deserializer, union_desc.discriminator_type())?;
    let disc = discriminator_as_i64(&discriminator)?;
    let member = select_union_member(union_desc, disc)?;
    let value = deserialize_value_cdr(deserializer, &member.member_type)?;
    Ok(DynamicValue::Union { discriminator: Box::new(discriminator), value: Box::new(value) })
}

fn deserialize_value_xcdr2(
    deserializer: &mut Xcdr2Deserializer,
    type_kind: &DynamicTypeKind,
) -> DdsResult<DynamicValue> {
    match type_kind {
        DynamicTypeKind::Sequence { element_type, .. } => {
            if !is_primitive_kind(element_type) && !deserializer.plain_collections() {
                let _ = deserializer.read_dheader().map_err(cdr_error)?;
            }
            let len = deserializer.deserialize_u32().map_err(cdr_error)? as usize;
            let mut items = Vec::new();
            for _ in 0..len {
                items.push(deserialize_value_xcdr2(deserializer, element_type)?);
            }
            Ok(DynamicValue::Sequence(items))
        }
        DynamicTypeKind::Array { element_type, dimensions } => {
            if !is_primitive_kind(element_type) && !deserializer.plain_collections() {
                let _ = deserializer.read_dheader().map_err(cdr_error)?;
            }
            let total_size = checked_array_len(dimensions)?;
            let mut items = Vec::new();
            for _ in 0..total_size {
                items.push(deserialize_value_xcdr2(deserializer, element_type)?);
            }
            Ok(DynamicValue::Array(items))
        }
        DynamicTypeKind::Map { key_type, value_type, .. } => {
            if !(is_primitive_kind(key_type) && is_primitive_kind(value_type)) {
                let _ = deserializer.read_dheader().map_err(cdr_error)?;
            }
            let len = deserializer.deserialize_u32().map_err(cdr_error)? as usize;
            let mut entries = Vec::new();
            for _ in 0..len {
                let key = deserialize_value_xcdr2(deserializer, key_type)?;
                let value = deserialize_value_xcdr2(deserializer, value_type)?;
                entries.push((key, value));
            }
            Ok(DynamicValue::Map(entries))
        }
        DynamicTypeKind::TypeRef(inner) => match inner.kind() {
            DynamicTypeKind::Struct(_) => {
                let nested = deserialize_struct_xcdr(deserializer, inner)?;
                Ok(DynamicValue::Struct(Box::new(nested)))
            }
            DynamicTypeKind::Enum(enum_desc) => deserialize_enum(deserializer, enum_desc),
            DynamicTypeKind::Union(union_desc) => {
                deserialize_union_xcdr2(deserializer, union_desc, inner.extensibility())
            }
            other => deserialize_atomic_value(deserializer, other),
        },
        other => deserialize_atomic_value(deserializer, other),
    }
}

/// Deserialize a union under XCDR2, mirroring `serialize_union_xcdr2` /
/// `generate_union_xcdr_deserialize_impl`.
fn deserialize_union_xcdr2(
    deserializer: &mut Xcdr2Deserializer,
    union_desc: &UnionDescriptor,
    extensibility: ExtensibilityKind,
) -> DdsResult<DynamicValue> {
    let disc_type = union_desc.discriminator_type();
    match extensibility {
        ExtensibilityKind::Final => {
            let discriminator = deserialize_value_xcdr2(deserializer, disc_type)?;
            let disc = discriminator_as_i64(&discriminator)?;
            let member = select_union_member(union_desc, disc)?;
            let value = deserialize_value_xcdr2(deserializer, &member.member_type)?;
            Ok(DynamicValue::Union {
                discriminator: Box::new(discriminator),
                value: Box::new(value),
            })
        }
        ExtensibilityKind::Appendable => {
            let (object_size, start_pos) = deserializer.begin_struct().map_err(cdr_error)?;
            let discriminator = deserialize_value_xcdr2(deserializer, disc_type)?;
            let disc = discriminator_as_i64(&discriminator)?;
            let member = select_union_member(union_desc, disc)?;
            let value = deserialize_value_xcdr2(deserializer, &member.member_type)?;
            deserializer.end_struct(object_size, start_pos).map_err(cdr_error)?;
            Ok(DynamicValue::Union {
                discriminator: Box::new(discriminator),
                value: Box::new(value),
            })
        }
        ExtensibilityKind::Mutable => {
            let (object_size, start_pos) = deserializer.begin_struct().map_err(cdr_error)?;
            let (disc_id, _len) = deserializer.read_member_header().map_err(cdr_error)?;
            if disc_id != 0 {
                return Err(DdsError::Error(format!(
                    "expected union discriminator member id 0, got {}",
                    disc_id
                )));
            }
            let discriminator = deserialize_value_xcdr2(deserializer, disc_type)?;
            let disc = discriminator_as_i64(&discriminator)?;
            let member = select_union_member(union_desc, disc)?;
            let (_branch_id, _branch_len) = deserializer.read_member_header().map_err(cdr_error)?;
            let value = deserialize_value_xcdr2(deserializer, &member.member_type)?;
            deserializer.end_struct(object_size, start_pos).map_err(cdr_error)?;
            Ok(DynamicValue::Union {
                discriminator: Box::new(discriminator),
                value: Box::new(value),
            })
        }
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
        if member.is_optional {
            match deserializer.read_parameter_header().map_err(cdr_error)? {
                PlCdrMemberHeader::Short { length: 0, .. }
                | PlCdrMemberHeader::Long { length: 0, .. } => {}
                PlCdrMemberHeader::Short { .. } | PlCdrMemberHeader::Long { .. } => {
                    let value = deserialize_value_cdr(deserializer, &member.member_type)?;
                    values.insert(member.name.clone(), value);
                }
                PlCdrMemberHeader::Sentinel => {
                    return Err(DdsError::Error(format!(
                        "Unexpected PID_SENTINEL while reading optional field `{}`",
                        member.name
                    )));
                }
            }
        } else {
            let value = deserialize_value_cdr(deserializer, &member.member_type)?;
            values.insert(member.name.clone(), value);
        }
    }
    Ok(values)
}

fn deserialize_struct_members_mutable_cdr(
    deserializer: &mut CdrDeserializer,
    struct_desc: &StructDescriptor,
) -> DdsResult<HashMap<Arc<str>, DynamicValue>> {
    let mut values = HashMap::new();
    while !deserializer.is_at_sentinel() {
        let (member_id, member_length, must_understand) =
            match deserializer.read_parameter_header().map_err(cdr_error)? {
                PlCdrMemberHeader::Sentinel => break,
                PlCdrMemberHeader::Short { pid, length, must_understand } => {
                    (pid as u32, length as u32, must_understand)
                }
                PlCdrMemberHeader::Long { member_id, length, must_understand } => {
                    (member_id, length, must_understand)
                }
            };

        let member_start = deserializer.get_position();
        if let Some(member) = struct_desc.get_member_by_id(member_id) {
            let value = deserialize_value_cdr(deserializer, &member.member_type)?;
            values.insert(member.name.clone(), value);
        } else if must_understand {
            return Err(DdsError::Error(format!("Unknown required member with id {}", member_id)));
        } else {
            deserializer.skip(member_length as usize).map_err(cdr_error)?;
        }

        let consumed = deserializer.get_position() - member_start;
        if consumed < member_length as usize {
            deserializer.skip(member_length as usize - consumed).map_err(cdr_error)?;
        }
    }
    Ok(values)
}

fn deserialize_struct_members_xcdr2(
    deserializer: &mut Xcdr2Deserializer,
    struct_desc: &StructDescriptor,
) -> DdsResult<HashMap<Arc<str>, DynamicValue>> {
    let mut values = HashMap::new();
    for member in struct_desc.members() {
        if member.is_optional {
            let present = deserializer.deserialize_bool().map_err(cdr_error)?;
            if present {
                let value = deserialize_value_xcdr2(deserializer, &member.member_type)?;
                values.insert(member.name.clone(), value);
            }
        } else {
            let value = deserialize_value_xcdr2(deserializer, &member.member_type)?;
            values.insert(member.name.clone(), value);
        }
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
    let dynamic_type = data.dynamic_type();
    let struct_desc = dynamic_type
        .as_struct()
        .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;

    let mut nested = serialize_struct_cdr;

    if matches!(dynamic_type.extensibility(), ExtensibilityKind::Mutable) {
        for member in struct_desc.members() {
            if let Some(value) = member_value_or_default(data, member) {
                let member_id = member.member_id;
                let must_understand = member.is_must_understand;
                let member_type = member.member_type.clone();
                serializer
                    .write_member_with_v1(member_id, must_understand, |s| {
                        serialize_value_cdr(s, &value, &member_type, &mut nested)
                            .map_err(|e| CdrError::SerializationError(e.to_string()))
                    })
                    .map_err(cdr_error)?;
            }
        }
        serializer.end_mutable_struct().map_err(cdr_error)?;
        return Ok(());
    }

    for member in struct_desc.members() {
        if member.is_optional {
            let member_id = member.member_id;
            let must_understand = member.is_must_understand;
            let value = data.get_value(&member.name);
            serializer
                .write_member_with_v1(member_id, must_understand, |s| {
                    if let Some(value) = value {
                        serialize_value_cdr(s, value, &member.member_type, &mut nested)
                            .map_err(|e| CdrError::SerializationError(e.to_string()))?;
                    }
                    Ok(())
                })
                .map_err(cdr_error)?;
        } else if let Some(value) = member_value_or_default(data, member) {
            serialize_value_cdr(serializer, &value, &member.member_type, &mut nested)?;
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

    let values = if matches!(dynamic_type.extensibility(), ExtensibilityKind::Mutable) {
        deserialize_struct_members_mutable_cdr(deserializer, struct_desc)?
    } else {
        deserialize_struct_members_cdr(deserializer, struct_desc)?
    };
    Ok(DynamicData::with_values(dynamic_type.clone(), values))
}

/// Collect a struct's key members in canonical (member_id) order.
///
/// Returns an empty vec for non-struct or keyless types.
fn key_members_ordered(dynamic_type: &DynamicType) -> Vec<&MemberDescriptor> {
    match dynamic_type.as_struct() {
        Some(struct_desc) => {
            let mut members: Vec<&MemberDescriptor> =
                struct_desc.members().iter().filter(|m| m.is_key).collect();
            members.sort_by_key(|m| m.member_id);
            members
        }
        None => Vec::new(),
    }
}

/// True when the sole key member is a `String`. The derive path (`is_unbounded_string`
/// in `key_methods.rs`) and the field-descriptor raw path (`CdrFieldType::String`) both
/// MD5-hash such keys regardless of any `bound`, since neither observes the bound at
/// this point. The dynamic path matches them so all keyed paths agree. `WString` keys
/// are not hashed (they stay in the ≤16 raw rule), matching derive.
fn is_single_string_key(members: &[&MemberDescriptor]) -> bool {
    members.len() == 1
        && matches!(resolved_kind(&members[0].member_type), DynamicTypeKind::String { .. })
}

/// Serialize the key members of `data` as canonical RTPS KeyHash CDR:
/// big-endian, member_id order, no encapsulation header — byte-identical to the
/// derive-generated `serialize_key`.
///
/// Returns `(key_cdr, single_unbounded_string)`; the bool selects the
/// InstanceHandle rule at the call site (`from_key_cdr_hashed` vs `from_key_cdr`).
pub fn serialize_key_cdr(data: &DynamicData) -> DdsResult<(Vec<u8>, bool)> {
    let members = key_members_ordered(data.dynamic_type());
    if members.is_empty() {
        return Ok((Vec::new(), false));
    }
    let single_unbounded_string = is_single_string_key(&members);

    // Per RTPS KeyHash spec (DDSI-RTPS 9.6.4.8 step 4): PLAIN_CDR2 big-endian
    // (max alignment 4), no encapsulation header, Final => no DHEADER. Write the
    // 4-byte header so the alignment math is relative to it, then strip it.
    let mut serializer = Xcdr2Serializer::with_capacity(false, ExtensibilityKind::Final, 64);
    serializer.set_plain_collections(true);
    serializer.write_encapsulation_header().map_err(cdr_error)?;
    let mut nested = serialize_key_holder_struct;
    for member in &members {
        let value = member_value_or_default(data, member).ok_or_else(|| {
            DdsError::Error(format!("Missing key member value for '{}'", member.name))
        })?;
        serialize_value_xcdr2(&mut serializer, &value, &member.member_type, &mut nested)?;
    }

    let mut bytes = serializer.into_bytes();
    bytes.drain(..4);
    if single_unbounded_string {
        while bytes.len() > 1 && bytes[bytes.len() - 1] == 0 && bytes[bytes.len() - 2] == 0 {
            bytes.pop();
        }
    }
    Ok((bytes, single_unbounded_string))
}

/// Deserialize a canonical headerless big-endian key CDR (as produced by
/// [`serialize_key_cdr`]) back into a `DynamicData` holding only the key members.
pub fn deserialize_key_cdr(
    bytes: &[u8],
    dynamic_type: &Arc<DynamicType>,
) -> DdsResult<DynamicData> {
    let mut deserializer = Xcdr2Deserializer::new_without_header(bytes, false);
    deserializer.set_plain_collections(true);
    deserialize_key_holder_struct(&mut deserializer, dynamic_type)
}

/// Projecting nested-struct serializer for the key holder: when the struct has
/// `@key` members, write only those (member_id order); otherwise write all
/// members. This is the recursive key projection required by RTPS KeyHash
/// (DDSI-RTPS 9.6.4.8 step 1/3).
fn serialize_key_holder_struct(
    serializer: &mut Xcdr2Serializer,
    data: &DynamicData,
) -> DdsResult<()> {
    let members = key_members_ordered(data.dynamic_type());
    if members.is_empty() {
        // Nested aggregated type with no @key members: keep all members, but the
        // key holder is FINAL (no DHEADER) regardless of the type's own
        // extensibility, recursing each nested aggregate as its own FINAL holder.
        let struct_desc = data
            .dynamic_type()
            .as_struct()
            .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;
        let mut nested = serialize_key_holder_struct;
        return serialize_members_xcdr2_inline(serializer, data, struct_desc, &mut nested);
    }
    let mut nested = serialize_key_holder_struct;
    for member in &members {
        let value = member_value_or_default(data, member).ok_or_else(|| {
            DdsError::Error(format!("Missing key member value for '{}'", member.name))
        })?;
        serialize_value_xcdr2(serializer, &value, &member.member_type, &mut nested)?;
    }
    Ok(())
}

/// Read side of [`serialize_key_holder_struct`]: reconstruct only the projected
/// key members from a key-holder CDR stream.
fn deserialize_key_holder_struct(
    deserializer: &mut Xcdr2Deserializer,
    dynamic_type: &Arc<DynamicType>,
) -> DdsResult<DynamicData> {
    let members = key_members_ordered(dynamic_type);
    if members.is_empty() {
        // No @key members: read all members inline as FINAL (no DHEADER),
        // mirroring the serialize side, recursing key-holder into nested members.
        let struct_desc = dynamic_type
            .as_struct()
            .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;
        let mut values = HashMap::new();
        for member in struct_desc.members() {
            if member.is_optional {
                let present = deserializer.deserialize_bool().map_err(cdr_error)?;
                if present {
                    let value = deserialize_key_holder_value(deserializer, &member.member_type)?;
                    values.insert(member.name.clone(), value);
                }
            } else {
                let value = deserialize_key_holder_value(deserializer, &member.member_type)?;
                values.insert(member.name.clone(), value);
            }
        }
        return Ok(DynamicData::with_values(dynamic_type.clone(), values));
    }
    let mut values = HashMap::new();
    for member in &members {
        let value = deserialize_key_holder_value(deserializer, &member.member_type)?;
        values.insert(member.name.clone(), value);
    }
    Ok(DynamicData::with_values(dynamic_type.clone(), values))
}

fn deserialize_key_holder_value(
    deserializer: &mut Xcdr2Deserializer,
    kind: &DynamicTypeKind,
) -> DdsResult<DynamicValue> {
    // A nested struct key member is itself projected to its key holder.
    if let DynamicTypeKind::TypeRef(inner) = kind {
        if matches!(inner.kind(), DynamicTypeKind::Struct(_)) {
            let nested = deserialize_key_holder_struct(deserializer, inner)?;
            return Ok(DynamicValue::Struct(Box::new(nested)));
        }
    }
    // A fixed array projects element-wise (FINAL, no framing), recursing into
    // struct elements as their own key holders.
    if let DynamicTypeKind::Array { element_type, dimensions } = kind {
        let count = checked_array_len(dimensions)?;
        let mut items = Vec::with_capacity(count as usize);
        for _ in 0..count {
            items.push(deserialize_key_holder_value(deserializer, element_type)?);
        }
        return Ok(DynamicValue::Array(items));
    }
    deserialize_value_xcdr2(deserializer, kind)
}

/// Maximum PLAIN_CDR2 (max alignment 4) serialized size of the key holder of
/// `dynamic_type`, per RTPS KeyHash step 5. `None` means the key holder has an
/// unbounded maximum size, so the KeyHash is always the MD5 of the actual stream.
pub fn key_holder_max_size(dynamic_type: &DynamicType) -> Option<usize> {
    let members = key_members_ordered(dynamic_type);
    if members.is_empty() {
        // No @key members: the whole aggregated type is the key holder.
        return dynamic_type
            .as_struct()
            .and_then(|d| accumulate_members_max_size(d.members().iter().map(|m| &m.member_type)));
    }
    accumulate_members_max_size(members.iter().map(|m| &m.member_type))
}

fn accumulate_members_max_size<'a, I>(member_types: I) -> Option<usize>
where
    I: Iterator<Item = &'a DynamicTypeKind>,
{
    let mut pos = 0usize;
    for kind in member_types {
        let (align, size) = type_kind_max_size(kind)?;
        pos = align_up(pos, align);
        pos = pos.checked_add(size)?;
    }
    Some(pos)
}

/// `(alignment, maximum serialized size)` of a single member's contribution to the
/// key holder, or `None` if that member has no finite maximum (=> MD5 the holder).
fn type_kind_max_size(kind: &DynamicTypeKind) -> Option<(usize, usize)> {
    match resolved_kind(kind) {
        DynamicTypeKind::Primitive(p) => {
            let s = p.size();
            Some((s.min(4), s))
        }
        DynamicTypeKind::Enum(_) => Some((4, 4)),
        DynamicTypeKind::String { bound: Some(n) } => Some((4, 4 + *n as usize + 1)),
        DynamicTypeKind::WString { bound: Some(n) } => Some((4, 4 + 2 * (*n as usize + 1))),
        DynamicTypeKind::Struct(desc) => {
            // Nested key holder: project to its own @key members (member_id order),
            // or keep all members when it has none.
            let mut key_members: Vec<&MemberDescriptor> =
                desc.members().iter().filter(|m| m.is_key).collect();
            let size = if key_members.is_empty() {
                accumulate_members_max_size(desc.members().iter().map(|m| &m.member_type))?
            } else {
                key_members.sort_by_key(|m| m.member_id);
                accumulate_members_max_size(key_members.iter().map(|m| &m.member_type))?
            };
            Some((4, size))
        }
        // Fixed array of finite-size elements: `N` packed (max-align-4) elements.
        DynamicTypeKind::Array { element_type, dimensions } => {
            let (elem_align, elem_size) = type_kind_max_size(element_type)?;
            let count: usize = dimensions.iter().map(|d| *d as usize).product();
            if count == 0 {
                return Some((elem_align, 0));
            }
            let stride = align_up(elem_size, elem_align);
            let size = stride.checked_mul(count - 1)?.checked_add(elem_size)?;
            Some((elem_align, size))
        }
        // Bounded sequence: `u32` length prefix (align 4) then up to `bound` packed
        // (max-align-4) elements. Unbounded elements collapse the whole holder to MD5.
        DynamicTypeKind::Sequence { element_type, bound: Some(n) } => {
            let (elem_align, elem_size) = type_kind_max_size(element_type)?;
            let n = *n as usize;
            let size = if n == 0 {
                4
            } else {
                let stride = align_up(elem_size, elem_align);
                4usize.checked_add(stride.checked_mul(n - 1)?)?.checked_add(elem_size)?
            };
            Some((4, size))
        }
        // Unbounded string/wstring, unbounded sequences, maps, and anything else
        // with no finite maximum serialized size => hash the actual bytes.
        _ => None,
    }
}

fn align_up(pos: usize, align: usize) -> usize {
    if align <= 1 {
        pos
    } else {
        (pos + align - 1) & !(align - 1)
    }
}

fn serialize_xcdr(
    data: &DynamicData,
    extensibility: ExtensibilityKind,
) -> DdsResult<SerializedData> {
    let mut serializer = Xcdr2Serializer::with_capacity(true, extensibility, 256);
    serializer.write_encapsulation_header().map_err(cdr_error)?;
    serialize_struct_xcdr(&mut serializer, data)?;
    Ok(Arc::from(serializer.into_bytes().into_boxed_slice()))
}

fn deserialize_xcdr(bytes: &[u8], dynamic_type: &Arc<DynamicType>) -> DdsResult<DynamicData> {
    let mut deserializer = Xcdr2Deserializer::new(bytes).map_err(cdr_error)?;
    deserialize_struct_xcdr(&mut deserializer, dynamic_type)
}

/// Serialize struct members inline (Final/Appendable). Optionals are framed by
/// a presence bool per XCDR2; non-optionals are written directly.
fn serialize_members_xcdr2_inline<F>(
    serializer: &mut Xcdr2Serializer,
    data: &DynamicData,
    struct_desc: &StructDescriptor,
    nested: &mut F,
) -> DdsResult<()>
where
    F: FnMut(&mut Xcdr2Serializer, &DynamicData) -> DdsResult<()>,
{
    for member in struct_desc.members() {
        if member.is_optional {
            match data.get_value(&member.name) {
                Some(value) => {
                    serializer.serialize_bool(true).map_err(cdr_error)?;
                    serialize_value_xcdr2(serializer, value, &member.member_type, nested)?;
                }
                None => {
                    serializer.serialize_bool(false).map_err(cdr_error)?;
                }
            }
        } else if let Some(value) = member_value_or_default(data, member) {
            serialize_value_xcdr2(serializer, &value, &member.member_type, nested)?;
        }
    }
    Ok(())
}

fn serialize_struct_xcdr(serializer: &mut Xcdr2Serializer, data: &DynamicData) -> DdsResult<()> {
    let extensibility = data.dynamic_type().extensibility();
    let struct_desc = data
        .dynamic_type()
        .as_struct()
        .ok_or_else(|| DdsError::Error("Expected struct type".to_string()))?;

    let mut nested = serialize_struct_xcdr;

    match extensibility {
        ExtensibilityKind::Final => {
            serialize_members_xcdr2_inline(serializer, data, struct_desc, &mut nested)?;
        }
        ExtensibilityKind::Appendable => {
            let size_pos = serializer.begin_struct().map_err(cdr_error)?;
            serialize_members_xcdr2_inline(serializer, data, struct_desc, &mut nested)?;
            serializer.end_struct(size_pos).map_err(cdr_error)?;
        }
        ExtensibilityKind::Mutable => {
            let size_pos = serializer.begin_struct().map_err(cdr_error)?;

            for member in struct_desc.members() {
                if let Some(value) = member_value_or_default(data, member) {
                    let member_id = member.member_id;
                    let must_understand = member.is_must_understand;
                    let member_type = member.member_type.clone();
                    serializer
                        .write_member_with(member_id, must_understand, |s| {
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
) -> DdsResult<DynamicData> {
    let extensibility = dynamic_type.extensibility();
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
        CollectionElementFlag, CompleteBitfield, CompleteBitflag, CompleteBitmaskType,
        CompleteBitsetType, CompleteEnumeratedLiteral, CompleteEnumeratedType,
        CompleteStructMember, CompleteStructType, CompleteTypeObject, CompleteUnionMember,
        CompleteUnionType, EnumeratedLiteralFlag, EquivalenceHash, MemberFlag,
        PlainCollectionHeader, TryConstructKind, TypeFlag, TypeIdentifier, TypeRegistry,
    };

    fn member_flag(is_optional: bool, is_key: bool, is_must_understand: bool) -> MemberFlag {
        MemberFlag::new(
            TryConstructKind::Discard,
            false,
            is_optional,
            is_must_understand,
            is_key,
            false,
        )
    }

    fn xcdr_format(ext: ExtensibilityKind) -> SerializationFormat {
        SerializationFormat::Xcdr { extensibility_kind: ext, use_delimiters: false }
    }

    fn tf(ext: ExtensibilityKind) -> TypeFlag {
        let x = match ext {
            ExtensibilityKind::Final => crate::xtypes::ExtensibilityKind::Final,
            ExtensibilityKind::Appendable => crate::xtypes::ExtensibilityKind::Appendable,
            ExtensibilityKind::Mutable => crate::xtypes::ExtensibilityKind::Mutable,
        };
        TypeFlag::new(x, false, false)
    }

    fn inner_struct(ext: ExtensibilityKind) -> (CompleteTypeObject, EquivalenceHash) {
        let mut inner = CompleteStructType::new(tf(ext), "Inner".into(), None);
        inner.add_member(CompleteStructMember::new(
            0,
            member_flag(false, false, false),
            TypeIdentifier::Int32,
            "a".to_string(),
        ));
        inner.add_member(CompleteStructMember::new(
            1,
            member_flag(false, false, false),
            TypeIdentifier::Int32,
            "b".to_string(),
        ));
        let obj = CompleteTypeObject::Struct(inner);
        let hash = EquivalenceHash::compute(&obj.serialize());
        (obj, hash)
    }

    /// Build a "Color" enum CompleteTypeObject (RED=0, GREEN=1, BLUE=2).
    fn color_enum() -> (CompleteTypeObject, EquivalenceHash) {
        let mut e = CompleteEnumeratedType::new(tf(ExtensibilityKind::Final), "Color".into(), 32);
        e.add_literal(CompleteEnumeratedLiteral::new(0, EnumeratedLiteralFlag(0), "RED".into()));
        e.add_literal(CompleteEnumeratedLiteral::new(1, EnumeratedLiteralFlag(0), "GREEN".into()));
        e.add_literal(CompleteEnumeratedLiteral::new(2, EnumeratedLiteralFlag(0), "BLUE".into()));
        let obj = CompleteTypeObject::Enum(e);
        let hash = EquivalenceHash::compute(&obj.serialize());
        (obj, hash)
    }

    fn build_with_registry(outer: CompleteTypeObject, registry: &TypeRegistry) -> Arc<DynamicType> {
        Arc::new(
            DynamicType::from_type_object_with_registry(
                Arc::new(outer),
                TypeIdentifier::None,
                registry,
            )
            .unwrap(),
        )
    }

    #[test]
    fn test_nested_struct_roundtrip_all_formats() {
        let run = |label: &str, outer_ext: ExtensibilityKind, format: SerializationFormat| {
            // Inner shares the outer's extensibility: the serializer frames the
            // whole message with one mode, so nested types follow suit.
            let (inner_obj, inner_hash) = inner_struct(outer_ext);
            let mut registry = TypeRegistry::new();
            registry.register_complete(inner_hash, "Inner".into(), inner_obj.clone());

            let mut outer = CompleteStructType::new(tf(outer_ext), "Outer".into(), None);
            outer.add_member(CompleteStructMember::new(
                0,
                member_flag(false, false, false),
                TypeIdentifier::Int32,
                "id".to_string(),
            ));
            outer.add_member(CompleteStructMember::new(
                1,
                member_flag(false, false, false),
                TypeIdentifier::CompleteTypeId(inner_hash),
                "child".to_string(),
            ));
            let outer_dt = build_with_registry(CompleteTypeObject::Struct(outer), &registry);

            // The nested member must have resolved into a TypeRef.
            assert!(matches!(
                outer_dt.get_member("child").unwrap().member_type,
                DynamicTypeKind::TypeRef(_)
            ));

            let inner_dt =
                Arc::new(DynamicType::from_type_object(inner_obj, TypeIdentifier::None).unwrap());
            let mut inner = DynamicData::new(inner_dt);
            inner.set("a", 11i32).unwrap();
            inner.set("b", 22i32).unwrap();

            let mut data = DynamicData::new(outer_dt.clone());
            data.set("id", 7i32).unwrap();
            data.set_value("child", DynamicValue::Struct(Box::new(inner))).unwrap();

            let bytes = serialize_dynamic_data(&data, &format)
                .unwrap_or_else(|e| panic!("{label} serialize: {e:?}"));
            let back = deserialize_dynamic_data(&bytes, &outer_dt)
                .unwrap_or_else(|e| panic!("{label} deserialize ({} bytes): {e:?}", bytes.len()));

            assert_eq!(back.get::<i32>("id").unwrap(), 7);
            match back.get_value("child").unwrap() {
                DynamicValue::Struct(child) => {
                    assert_eq!(child.get::<i32>("a").unwrap(), 11);
                    assert_eq!(child.get::<i32>("b").unwrap(), 22);
                }
                other => panic!("expected nested struct, got {:?}", other),
            }
        };

        // CDR (XCDR1): Final/Appendable (PLAIN_CDR) and Mutable (PL_CDR); the
        // mutable case nests a mutable inner, exercising recursive PL_CDR framing.
        run("cdr-final", ExtensibilityKind::Final, SerializationFormat::Cdr);
        run("cdr-appendable", ExtensibilityKind::Appendable, SerializationFormat::Cdr);
        run("cdr-mutable", ExtensibilityKind::Mutable, SerializationFormat::Cdr);
        // XCDR2: Final/Appendable/Mutable, inner is Appendable (extensibility-fix coverage).
        run("xcdr2-final", ExtensibilityKind::Final, xcdr_format(ExtensibilityKind::Final));
        run(
            "xcdr2-appendable",
            ExtensibilityKind::Appendable,
            xcdr_format(ExtensibilityKind::Appendable),
        );
        run("xcdr2-mutable", ExtensibilityKind::Mutable, xcdr_format(ExtensibilityKind::Mutable));
    }

    #[test]
    fn test_nested_struct_mixed_extensibility_roundtrip() {
        // Outer and inner differ in extensibility. Per XTypes §7.4.3.1 each is
        // framed by its own kind, so the inner must round-trip with (or without)
        // its own DHEADER independent of the enclosing message's mode.
        let run = |label: &str, outer_ext: ExtensibilityKind, inner_ext: ExtensibilityKind| {
            let (inner_obj, inner_hash) = inner_struct(inner_ext);
            let mut registry = TypeRegistry::new();
            registry.register_complete(inner_hash, "Inner".into(), inner_obj.clone());

            let mut outer = CompleteStructType::new(tf(outer_ext), "Outer".into(), None);
            outer.add_member(CompleteStructMember::new(
                0,
                member_flag(false, false, false),
                TypeIdentifier::Int32,
                "id".to_string(),
            ));
            outer.add_member(CompleteStructMember::new(
                1,
                member_flag(false, false, false),
                TypeIdentifier::CompleteTypeId(inner_hash),
                "child".to_string(),
            ));
            let outer_dt = build_with_registry(CompleteTypeObject::Struct(outer), &registry);

            let inner_dt =
                Arc::new(DynamicType::from_type_object(inner_obj, TypeIdentifier::None).unwrap());
            let mut inner = DynamicData::new(inner_dt);
            inner.set("a", 11i32).unwrap();
            inner.set("b", 22i32).unwrap();

            let mut data = DynamicData::new(outer_dt.clone());
            data.set("id", 7i32).unwrap();
            data.set_value("child", DynamicValue::Struct(Box::new(inner))).unwrap();

            // The message-level format follows the outer (top-level) type.
            let format = xcdr_format(outer_ext);
            let bytes = serialize_dynamic_data(&data, &format)
                .unwrap_or_else(|e| panic!("{label} serialize: {e:?}"));
            let back = deserialize_dynamic_data(&bytes, &outer_dt)
                .unwrap_or_else(|e| panic!("{label} deserialize ({} bytes): {e:?}", bytes.len()));

            assert_eq!(back.get::<i32>("id").unwrap(), 7, "{label} id");
            match back.get_value("child").unwrap() {
                DynamicValue::Struct(child) => {
                    assert_eq!(child.get::<i32>("a").unwrap(), 11, "{label} child.a");
                    assert_eq!(child.get::<i32>("b").unwrap(), 22, "{label} child.b");
                }
                other => panic!("{label}: expected nested struct, got {:?}", other),
            }
        };

        run("final/appendable", ExtensibilityKind::Final, ExtensibilityKind::Appendable);
        run("appendable/final", ExtensibilityKind::Appendable, ExtensibilityKind::Final);
        run("final/mutable", ExtensibilityKind::Final, ExtensibilityKind::Mutable);
        run("appendable/mutable", ExtensibilityKind::Appendable, ExtensibilityKind::Mutable);
    }

    #[test]
    fn test_vec_of_struct_roundtrip() {
        let run = |format: SerializationFormat, count: usize| {
            let (inner_obj, inner_hash) = inner_struct(ExtensibilityKind::Final);
            let mut registry = TypeRegistry::new();
            registry.register_complete(inner_hash, "Inner".into(), inner_obj.clone());

            let mut outer =
                CompleteStructType::new(tf(ExtensibilityKind::Final), "HasVec".into(), None);
            outer.add_member(CompleteStructMember::new(
                0,
                member_flag(false, false, false),
                TypeIdentifier::PlainSequenceLarge {
                    header: PlainCollectionHeader::default(),
                    bound: 0,
                    element_identifier: Box::new(TypeIdentifier::CompleteTypeId(inner_hash)),
                },
                "items".to_string(),
            ));
            let outer_dt = build_with_registry(CompleteTypeObject::Struct(outer), &registry);
            let inner_dt =
                Arc::new(DynamicType::from_type_object(inner_obj, TypeIdentifier::None).unwrap());

            let items: Vec<DynamicValue> = (0..count)
                .map(|i| {
                    let mut inner = DynamicData::new(inner_dt.clone());
                    inner.set("a", i as i32).unwrap();
                    inner.set("b", (i as i32) * 10).unwrap();
                    DynamicValue::Struct(Box::new(inner))
                })
                .collect();

            let mut data = DynamicData::new(outer_dt.clone());
            data.set_value("items", DynamicValue::Sequence(items)).unwrap();

            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            let back = deserialize_dynamic_data(&bytes, &outer_dt).unwrap();

            match back.get_value("items").unwrap() {
                DynamicValue::Sequence(items) => {
                    assert_eq!(items.len(), count);
                    for (i, item) in items.iter().enumerate() {
                        match item {
                            DynamicValue::Struct(child) => {
                                assert_eq!(child.get::<i32>("a").unwrap(), i as i32);
                                assert_eq!(child.get::<i32>("b").unwrap(), (i as i32) * 10);
                            }
                            other => panic!("expected struct element, got {:?}", other),
                        }
                    }
                }
                other => panic!("expected sequence, got {:?}", other),
            }
        };

        for count in [0usize, 1, 3] {
            run(SerializationFormat::Cdr, count);
            run(xcdr_format(ExtensibilityKind::Final), count);
        }
    }

    #[test]
    fn test_array_of_struct_roundtrip() {
        let run = |format: SerializationFormat| {
            let (inner_obj, inner_hash) = inner_struct(ExtensibilityKind::Final);
            let mut registry = TypeRegistry::new();
            registry.register_complete(inner_hash, "Inner".into(), inner_obj.clone());

            let mut outer =
                CompleteStructType::new(tf(ExtensibilityKind::Final), "HasArray".into(), None);
            outer.add_member(CompleteStructMember::new(
                0,
                member_flag(false, false, false),
                TypeIdentifier::PlainArrayLarge {
                    header: PlainCollectionHeader::default(),
                    array_bound_seq: vec![2],
                    element_identifier: Box::new(TypeIdentifier::CompleteTypeId(inner_hash)),
                },
                "items".to_string(),
            ));
            let outer_dt = build_with_registry(CompleteTypeObject::Struct(outer), &registry);
            let inner_dt =
                Arc::new(DynamicType::from_type_object(inner_obj, TypeIdentifier::None).unwrap());

            let items: Vec<DynamicValue> = (0..2)
                .map(|i| {
                    let mut inner = DynamicData::new(inner_dt.clone());
                    inner.set("a", i as i32).unwrap();
                    inner.set("b", (i as i32) + 100).unwrap();
                    DynamicValue::Struct(Box::new(inner))
                })
                .collect();

            let mut data = DynamicData::new(outer_dt.clone());
            data.set_value("items", DynamicValue::Array(items)).unwrap();

            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            let back = deserialize_dynamic_data(&bytes, &outer_dt).unwrap();

            match back.get_value("items").unwrap() {
                DynamicValue::Array(items) => {
                    assert_eq!(items.len(), 2);
                    for (i, item) in items.iter().enumerate() {
                        match item {
                            DynamicValue::Struct(child) => {
                                assert_eq!(child.get::<i32>("a").unwrap(), i as i32);
                                assert_eq!(child.get::<i32>("b").unwrap(), (i as i32) + 100);
                            }
                            other => panic!("expected struct element, got {:?}", other),
                        }
                    }
                }
                other => panic!("expected array, got {:?}", other),
            }
        };

        run(SerializationFormat::Cdr);
        run(xcdr_format(ExtensibilityKind::Final));
    }

    fn map_member(
        name: &str,
        member_id: u32,
        key: TypeIdentifier,
        value: TypeIdentifier,
    ) -> CompleteStructMember {
        CompleteStructMember::new(
            member_id,
            member_flag(false, false, false),
            TypeIdentifier::PlainMapLarge {
                header: PlainCollectionHeader::default(),
                bound: 0,
                key_flags: CollectionElementFlag::default(),
                key_identifier: Box::new(key),
                element_identifier: Box::new(value),
            },
            name.to_string(),
        )
    }

    #[test]
    fn test_map_members_roundtrip_all_formats() {
        let run = |label: &str, ext: ExtensibilityKind, format: SerializationFormat| {
            let mut outer = CompleteStructType::new(tf(ext), "MapHolder".into(), None);
            outer.add_member(map_member("ints", 0, TypeIdentifier::Int32, TypeIdentifier::Int32));
            outer.add_member(map_member(
                "named",
                1,
                TypeIdentifier::String8,
                TypeIdentifier::Int32,
            ));
            let dt = Arc::new(
                DynamicType::from_type_object(
                    CompleteTypeObject::Struct(outer),
                    TypeIdentifier::None,
                )
                .unwrap(),
            );

            let ints = vec![
                (DynamicValue::Int32(1), DynamicValue::Int32(100)),
                (DynamicValue::Int32(2), DynamicValue::Int32(200)),
            ];
            let named = vec![
                (DynamicValue::String("x".into()), DynamicValue::Int32(7)),
                (DynamicValue::String("yy".into()), DynamicValue::Int32(8)),
            ];

            let mut data = DynamicData::new(dt.clone());
            data.set_value("ints", DynamicValue::Map(ints.clone())).unwrap();
            data.set_value("named", DynamicValue::Map(named.clone())).unwrap();

            let bytes = serialize_dynamic_data(&data, &format)
                .unwrap_or_else(|e| panic!("{label} serialize: {e:?}"));
            let back = deserialize_dynamic_data(&bytes, &dt)
                .unwrap_or_else(|e| panic!("{label} deserialize ({} bytes): {e:?}", bytes.len()));

            assert_eq!(back.get_value("ints").unwrap(), &DynamicValue::Map(ints), "{label} ints");
            assert_eq!(
                back.get_value("named").unwrap(),
                &DynamicValue::Map(named),
                "{label} named"
            );
        };

        run("cdr-final", ExtensibilityKind::Final, SerializationFormat::Cdr);
        run("cdr-appendable", ExtensibilityKind::Appendable, SerializationFormat::Cdr);
        run("cdr-mutable", ExtensibilityKind::Mutable, SerializationFormat::Cdr);
        run("xcdr2-final", ExtensibilityKind::Final, xcdr_format(ExtensibilityKind::Final));
        run(
            "xcdr2-appendable",
            ExtensibilityKind::Appendable,
            xcdr_format(ExtensibilityKind::Appendable),
        );
        run("xcdr2-mutable", ExtensibilityKind::Mutable, xcdr_format(ExtensibilityKind::Mutable));
    }

    #[test]
    fn test_nested_enum_name_resolution_and_vec_enum_has_dheader() {
        let (enum_obj, enum_hash) = color_enum();
        let mut registry = TypeRegistry::new();
        registry.register_complete(enum_hash, "Color".into(), enum_obj);

        // A struct with a single `tags: sequence<Color>` member, Final.
        let mut outer =
            CompleteStructType::new(tf(ExtensibilityKind::Final), "EnumList".into(), None);
        outer.add_member(CompleteStructMember::new(
            0,
            member_flag(false, false, false),
            TypeIdentifier::CompleteTypeId(enum_hash),
            "color".to_string(),
        ));
        outer.add_member(CompleteStructMember::new(
            1,
            member_flag(false, false, false),
            TypeIdentifier::PlainSequenceLarge {
                header: PlainCollectionHeader::default(),
                bound: 0,
                element_identifier: Box::new(TypeIdentifier::CompleteTypeId(enum_hash)),
            },
            "tags".to_string(),
        ));
        let outer_dt = build_with_registry(CompleteTypeObject::Struct(outer), &registry);

        let mut data = DynamicData::new(outer_dt.clone());
        data.set_value("color", DynamicValue::Enum { name: String::new(), value: 2 }).unwrap();
        data.set_value(
            "tags",
            DynamicValue::Sequence(vec![
                DynamicValue::Enum { name: String::new(), value: 1 },
                DynamicValue::Enum { name: String::new(), value: 0 },
            ]),
        )
        .unwrap();

        let bytes = serialize_dynamic_data(&data, &xcdr_format(ExtensibilityKind::Final)).unwrap();
        // encap(4) + color(4) + DHEADER(4) + count(4) + two 4-byte enums.
        assert_eq!(bytes.len(), 24, "enum is non-primitive, so sequence<enum> is framed");

        let back = deserialize_dynamic_data(&bytes, &outer_dt).unwrap();
        match back.get_value("color").unwrap() {
            DynamicValue::Enum { name, value } => {
                assert_eq!(*value, 2);
                assert_eq!(name, "BLUE", "enum literal name should be resolved");
            }
            other => panic!("expected enum, got {:?}", other),
        }
        match back.get_value("tags").unwrap() {
            DynamicValue::Sequence(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], DynamicValue::Enum { name: "GREEN".into(), value: 1 });
                assert_eq!(items[1], DynamicValue::Enum { name: "RED".into(), value: 0 });
            }
            other => panic!("expected sequence, got {:?}", other),
        }
    }

    fn enum_with_bound(name: &str, bit_bound: u16) -> (CompleteTypeObject, EquivalenceHash) {
        let mut e =
            CompleteEnumeratedType::new(tf(ExtensibilityKind::Final), name.into(), bit_bound);
        e.add_literal(CompleteEnumeratedLiteral::new(0, EnumeratedLiteralFlag(0), "A".into()));
        e.add_literal(CompleteEnumeratedLiteral::new(1, EnumeratedLiteralFlag(0), "B".into()));
        e.add_literal(CompleteEnumeratedLiteral::new(2, EnumeratedLiteralFlag(0), "C".into()));
        let obj = CompleteTypeObject::Enum(e);
        let hash = EquivalenceHash::compute(&obj.serialize());
        (obj, hash)
    }

    #[test]
    fn test_enum_bit_bound_width_and_roundtrip() {
        let run = |bit_bound: u16, width: usize| {
            let (enum_obj, enum_hash) = enum_with_bound("E", bit_bound);
            let mut registry = TypeRegistry::new();
            registry.register_complete(enum_hash, "E".into(), enum_obj);

            let mut outer =
                CompleteStructType::new(tf(ExtensibilityKind::Final), "Holder".into(), None);
            outer.add_member(CompleteStructMember::new(
                0,
                member_flag(false, false, false),
                TypeIdentifier::CompleteTypeId(enum_hash),
                "c".to_string(),
            ));
            let outer_dt = build_with_registry(CompleteTypeObject::Struct(outer), &registry);

            let mut data = DynamicData::new(outer_dt.clone());
            data.set_value("c", DynamicValue::Enum { name: String::new(), value: 2 }).unwrap();

            let bytes =
                serialize_dynamic_data(&data, &xcdr_format(ExtensibilityKind::Final)).unwrap();
            assert_eq!(
                bytes.len(),
                4 + width,
                "bit_bound {} should use {}-byte holder",
                bit_bound,
                width
            );

            let back = deserialize_dynamic_data(&bytes, &outer_dt).unwrap();
            match back.get_value("c").unwrap() {
                DynamicValue::Enum { value, name } => {
                    assert_eq!(*value, 2);
                    assert_eq!(name, "C", "literal name resolved after narrow-width read");
                }
                other => panic!("expected enum, got {:?}", other),
            }
        };
        run(8, 1);
        run(16, 2);
        run(32, 4);
    }

    /// Build "Opt { id: i32, opt: @optional i32 }" with the given extensibility.
    fn optional_type(ext: ExtensibilityKind) -> Arc<DynamicType> {
        let mut s = CompleteStructType::new(tf(ext), "Opt".into(), None);
        s.add_member(CompleteStructMember::new(
            0,
            member_flag(false, false, false),
            TypeIdentifier::Int32,
            "id".to_string(),
        ));
        s.add_member(CompleteStructMember::new(
            1,
            member_flag(true, false, false),
            TypeIdentifier::Int32,
            "opt".to_string(),
        ));
        Arc::new(
            DynamicType::from_type_object(CompleteTypeObject::Struct(s), TypeIdentifier::None)
                .unwrap(),
        )
    }

    #[test]
    fn test_optional_cdr_uses_pid_header() {
        let dt = optional_type(ExtensibilityKind::Final);

        let mut present = DynamicData::new(dt.clone());
        present.set("id", 5i32).unwrap();
        present.set("opt", 9i32).unwrap();
        let present_bytes = serialize_dynamic_data(&present, &SerializationFormat::Cdr).unwrap();

        let mut absent = DynamicData::new(dt.clone());
        absent.set("id", 5i32).unwrap();
        let absent_bytes = serialize_dynamic_data(&absent, &SerializationFormat::Cdr).unwrap();

        // encap(4) + id(4) + PID header(4) [+ value(4) if present].
        assert_eq!(present_bytes.len(), 16);
        assert_eq!(absent_bytes.len(), 12);
        // Absent: a length-0 short PID member header (member_id 1), NOT a bool.
        assert_eq!(&absent_bytes[8..12], &[1, 0, 0, 0]);
        // Present: same header but length 4.
        assert_eq!(&present_bytes[8..12], &[1, 0, 4, 0]);

        let back_present = deserialize_dynamic_data(&present_bytes, &dt).unwrap();
        assert_eq!(back_present.get::<i32>("opt").unwrap(), 9);
        let back_absent = deserialize_dynamic_data(&absent_bytes, &dt).unwrap();
        assert!(back_absent.get_value("opt").is_none());
    }

    #[test]
    fn test_optional_xcdr2_final_uses_presence_bool() {
        let dt = optional_type(ExtensibilityKind::Final);
        let format = xcdr_format(ExtensibilityKind::Final);

        let mut present = DynamicData::new(dt.clone());
        present.set("id", 5i32).unwrap();
        present.set("opt", 9i32).unwrap();
        let present_bytes = serialize_dynamic_data(&present, &format).unwrap();

        let mut absent = DynamicData::new(dt.clone());
        absent.set("id", 5i32).unwrap();
        let absent_bytes = serialize_dynamic_data(&absent, &format).unwrap();

        // The presence bool sits right after encap(4) + id(4).
        assert_eq!(present_bytes[8], 1, "present optional should write bool(true)");
        assert_eq!(absent_bytes[8], 0, "absent optional should write bool(false)");

        let back_present = deserialize_dynamic_data(&present_bytes, &dt).unwrap();
        assert_eq!(back_present.get::<i32>("opt").unwrap(), 9);
        let back_absent = deserialize_dynamic_data(&absent_bytes, &dt).unwrap();
        assert!(back_absent.get_value("opt").is_none());
    }

    #[test]
    fn test_optional_xcdr2_mutable_uses_header_presence() {
        let dt = optional_type(ExtensibilityKind::Mutable);
        let format = xcdr_format(ExtensibilityKind::Mutable);

        let mut present = DynamicData::new(dt.clone());
        present.set("id", 5i32).unwrap();
        present.set("opt", 9i32).unwrap();
        let present_bytes = serialize_dynamic_data(&present, &format).unwrap();

        let mut absent = DynamicData::new(dt.clone());
        absent.set("id", 5i32).unwrap();
        let absent_bytes = serialize_dynamic_data(&absent, &format).unwrap();

        // Presence is expressed by emitting (or not) the member's EMHEADER.
        assert!(present_bytes.len() > absent_bytes.len());

        let back_present = deserialize_dynamic_data(&present_bytes, &dt).unwrap();
        assert_eq!(back_present.get::<i32>("opt").unwrap(), 9);
        let back_absent = deserialize_dynamic_data(&absent_bytes, &dt).unwrap();
        assert!(back_absent.get_value("opt").is_none());
    }

    #[test]
    fn test_cdr_mutable_struct_pl_cdr_roundtrip() {
        let dt = optional_type(ExtensibilityKind::Mutable);

        // Absent optional: only `id` is emitted, terminated by PID_SENTINEL.
        let mut absent = DynamicData::new(dt.clone());
        absent.set("id", 1i32).unwrap();
        let absent_bytes = serialize_dynamic_data(&absent, &SerializationFormat::Cdr).unwrap();

        // Present optional adds another PID member, so the wire is strictly longer.
        let mut present = DynamicData::new(dt.clone());
        present.set("id", 1i32).unwrap();
        present.set("opt", 7i32).unwrap();
        let present_bytes = serialize_dynamic_data(&present, &SerializationFormat::Cdr).unwrap();
        assert!(present_bytes.len() > absent_bytes.len());

        // Both end with PID_SENTINEL (0x3F02, LE) + zero length.
        for bytes in [&absent_bytes, &present_bytes] {
            assert_eq!(bytes[bytes.len() - 4..], [0x02, 0x3F, 0x00, 0x00]);
        }

        let back_absent = deserialize_dynamic_data(&absent_bytes, &dt).unwrap();
        assert_eq!(back_absent.get::<i32>("id").unwrap(), 1);
        assert!(back_absent.get_value("opt").is_none());

        let back_present = deserialize_dynamic_data(&present_bytes, &dt).unwrap();
        assert_eq!(back_present.get::<i32>("id").unwrap(), 1);
        assert_eq!(back_present.get::<i32>("opt").unwrap(), 7);
    }

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

    fn default_member_flag() -> MemberFlag {
        MemberFlag::new(TryConstructKind::Discard, false, false, false, false, true)
    }

    fn union_holder(union_ext: ExtensibilityKind) -> Arc<DynamicType> {
        let mut u = CompleteUnionType::new(
            tf(union_ext),
            member_flag(false, false, false),
            TypeIdentifier::Int32,
            "MyUnion".into(),
        );
        u.add_member(CompleteUnionMember::new(
            1,
            member_flag(false, false, false),
            TypeIdentifier::Int32,
            vec![0],
            "a".into(),
        ));
        u.add_member(CompleteUnionMember::new(
            2,
            member_flag(false, false, false),
            TypeIdentifier::Int32,
            vec![1],
            "b".into(),
        ));
        u.add_member(CompleteUnionMember::new(
            3,
            default_member_flag(),
            TypeIdentifier::Int32,
            vec![],
            "c".into(),
        ));
        let union_obj = CompleteTypeObject::Union(u);
        let union_hash = EquivalenceHash::compute(&union_obj.serialize());

        let mut registry = TypeRegistry::new();
        registry.register_complete(union_hash, "MyUnion".into(), union_obj);

        let mut outer =
            CompleteStructType::new(tf(ExtensibilityKind::Final), "UHolder".into(), None);
        outer.add_member(CompleteStructMember::new(
            0,
            member_flag(false, false, false),
            TypeIdentifier::CompleteTypeId(union_hash),
            "u".to_string(),
        ));
        build_with_registry(CompleteTypeObject::Struct(outer), &registry)
    }

    #[test]
    fn test_union_roundtrip_all_formats() {
        let run = |label: &str, union_ext: ExtensibilityKind, format: SerializationFormat| {
            let outer_dt = union_holder(union_ext);
            assert!(
                matches!(
                    outer_dt.get_member("u").unwrap().member_type,
                    DynamicTypeKind::TypeRef(_)
                ),
                "{label}: union member should resolve to a TypeRef"
            );

            // case a (label 0), case b (label 1), and the default (no label).
            let cases = [
                (DynamicValue::Int32(0), DynamicValue::Int32(11)),
                (DynamicValue::Int32(1), DynamicValue::Int32(22)),
                (DynamicValue::Int32(99), DynamicValue::Int32(33)),
            ];
            for (disc, val) in cases {
                let mut data = DynamicData::new(outer_dt.clone());
                data.set_value(
                    "u",
                    DynamicValue::Union {
                        discriminator: Box::new(disc.clone()),
                        value: Box::new(val.clone()),
                    },
                )
                .unwrap();

                let bytes = serialize_dynamic_data(&data, &format)
                    .unwrap_or_else(|e| panic!("{label} serialize: {e:?}"));
                let back = deserialize_dynamic_data(&bytes, &outer_dt).unwrap_or_else(|e| {
                    panic!("{label} deserialize ({} bytes): {e:?}", bytes.len())
                });

                match back.get_value("u").unwrap() {
                    DynamicValue::Union { discriminator, value } => {
                        assert_eq!(**discriminator, disc, "{label} discriminator");
                        assert_eq!(**value, val, "{label} value");
                    }
                    other => panic!("{label}: expected union, got {:?}", other),
                }
            }
        };

        for ext in
            [ExtensibilityKind::Final, ExtensibilityKind::Appendable, ExtensibilityKind::Mutable]
        {
            run("cdr", ext, SerializationFormat::Cdr);
            run("xcdr2", ext, xcdr_format(ExtensibilityKind::Final));
        }
    }

    /// Build a bitmask `MyBitmask` (bit_bound 16) inside a Final `BMHolder { m }`.
    fn bitmask_holder() -> Arc<DynamicType> {
        let mut bm =
            CompleteBitmaskType::new(tf(ExtensibilityKind::Appendable), "MyBitmask".into(), 16);
        for (pos, name) in [(0u16, "FLAG0"), (1, "FLAG1"), (2, "FLAG2")] {
            bm.add_flag(CompleteBitflag::new(pos, member_flag(false, false, false), name.into()));
        }
        let obj = CompleteTypeObject::Bitmask(bm);
        let hash = EquivalenceHash::compute(&obj.serialize());

        let mut registry = TypeRegistry::new();
        registry.register_complete(hash, "MyBitmask".into(), obj);

        let mut outer =
            CompleteStructType::new(tf(ExtensibilityKind::Final), "BMHolder".into(), None);
        outer.add_member(CompleteStructMember::new(
            0,
            member_flag(false, false, false),
            TypeIdentifier::CompleteTypeId(hash),
            "m".to_string(),
        ));
        build_with_registry(CompleteTypeObject::Struct(outer), &registry)
    }

    #[test]
    fn test_bitmask_roundtrip_uses_bound_width() {
        let outer_dt = bitmask_holder();
        for format in [SerializationFormat::Cdr, xcdr_format(ExtensibilityKind::Final)] {
            let mut data = DynamicData::new(outer_dt.clone());
            data.set_value("m", DynamicValue::Bitmask(0b101)).unwrap();

            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            // encap(4) + u16(2): bit_bound 16 picks a 2-byte holder.
            assert_eq!(bytes.len(), 6, "bit_bound 16 should serialize as u16");

            let back = deserialize_dynamic_data(&bytes, &outer_dt).unwrap();
            assert_eq!(*back.get_value("m").unwrap(), DynamicValue::Bitmask(0b101));
        }
    }

    /// Build a bitset `MyBitset { a: 3 bits, b: 5 bits }` (8 bits → u8 holder)
    /// inside a Final `BSHolder { s }`.
    fn bitset_holder() -> Arc<DynamicType> {
        let mut bs = CompleteBitsetType::new(tf(ExtensibilityKind::Appendable), "MyBitset".into());
        bs.add_field(CompleteBitfield::new(
            0,
            member_flag(false, false, false),
            3,
            TypeIdentifier::Uint8,
            "a".into(),
        ));
        bs.add_field(CompleteBitfield::new(
            3,
            member_flag(false, false, false),
            5,
            TypeIdentifier::Uint8,
            "b".into(),
        ));
        let obj = CompleteTypeObject::Bitset(bs);
        let hash = EquivalenceHash::compute(&obj.serialize());

        let mut registry = TypeRegistry::new();
        registry.register_complete(hash, "MyBitset".into(), obj);

        let mut outer =
            CompleteStructType::new(tf(ExtensibilityKind::Final), "BSHolder".into(), None);
        outer.add_member(CompleteStructMember::new(
            0,
            member_flag(false, false, false),
            TypeIdentifier::CompleteTypeId(hash),
            "s".to_string(),
        ));
        build_with_registry(CompleteTypeObject::Struct(outer), &registry)
    }

    #[test]
    fn test_bitset_roundtrip_uses_total_bits_width() {
        let outer_dt = bitset_holder();
        // a = 5 (3 bits), b = 9 (5 bits) packed at offsets 0 and 3.
        let packed = 5u64 | (9u64 << 3);
        for format in [SerializationFormat::Cdr, xcdr_format(ExtensibilityKind::Final)] {
            let mut data = DynamicData::new(outer_dt.clone());
            data.set_value("s", DynamicValue::Bitset(packed)).unwrap();

            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            // encap(4) + u8(1): 8 total bits picks a 1-byte holder.
            assert_eq!(bytes.len(), 5, "8 total bits should serialize as u8");

            let back = deserialize_dynamic_data(&bytes, &outer_dt).unwrap();
            assert_eq!(*back.get_value("s").unwrap(), DynamicValue::Bitset(packed));
        }
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod fidelity_tests {
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

    fn concrete_xcdr<T: XcdrSerialize>(value: &T, ext: ExtensibilityKind) -> Vec<u8> {
        let mut serializer = Xcdr2Serializer::with_capacity(true, ext, 256);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        serializer.into_bytes()
    }

    fn dynamic_bytes(data: &DynamicData, format: &SerializationFormat) -> Vec<u8> {
        serialize_dynamic_data(data, format).unwrap().to_vec()
    }

    fn xcdr_format(ext: ExtensibilityKind) -> SerializationFormat {
        SerializationFormat::Xcdr { extensibility_kind: ext, use_delimiters: false }
    }

    fn standalone_type<T: HasTypeObject>() -> Arc<DynamicType> {
        Arc::new(
            DynamicType::from_type_object(T::complete_type_object(), T::type_identifier()).unwrap(),
        )
    }

    /// Build a DynamicType for `O` with the nested type `I` resolved via a registry.
    fn nested_types<O: HasTypeObject, I: HasTypeObject>() -> (Arc<DynamicType>, Arc<DynamicType>) {
        let mut registry = TypeRegistry::new();
        let inner_hash = hash_of(&I::type_identifier());
        registry.register_complete(inner_hash, "Inner".into(), I::complete_type_object());

        let outer_dt = Arc::new(
            DynamicType::from_type_object_with_registry(
                Arc::new(O::complete_type_object()),
                O::type_identifier(),
                &registry,
            )
            .unwrap(),
        );
        let inner_dt = standalone_type::<I>();
        (outer_dt, inner_dt)
    }

    fn assert_complete_object_is_struct<T: HasTypeObject>() {
        assert!(matches!(T::complete_type_object(), CompleteTypeObject::Struct(_)));
    }

    macro_rules! nested_family {
        ($inner:ident, $outer:ident, $ext:literal) => {
            #[derive(DdsType)]
            #[dds_type(crate_path = "int2dds", extensibility = $ext)]
            struct $inner {
                a: i32,
                b: i32,
            }

            #[derive(DdsType)]
            #[dds_type(crate_path = "int2dds", extensibility = $ext)]
            struct $outer {
                id: i32,
                child: $inner,
            }
        };
    }

    nested_family!(InnerFinal, OuterFinal, "Final");
    nested_family!(InnerApp, OuterApp, "Appendable");
    nested_family!(InnerMut, OuterMut, "Mutable");

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct OuterFinalChildApp {
        id: i32,
        child: InnerApp,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct OuterAppChildFinal {
        id: i32,
        child: InnerFinal,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct OuterFinalChildMut {
        id: i32,
        child: InnerMut,
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

    fn build_dynamic_nested(
        outer_dt: &Arc<DynamicType>,
        inner_dt: &Arc<DynamicType>,
    ) -> DynamicData {
        let mut child = DynamicData::new(inner_dt.clone());
        child.set("a", 1i32).unwrap();
        child.set("b", 2i32).unwrap();

        let mut outer = DynamicData::new(outer_dt.clone());
        outer.set("id", 7i32).unwrap();
        outer.set_value("child", DynamicValue::Struct(Box::new(child))).unwrap();
        outer
    }

    #[test]
    fn nested_byte_match_cdr_final() {
        assert_complete_object_is_struct::<OuterFinal>();
        let (outer_dt, inner_dt) = nested_types::<OuterFinal, InnerFinal>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterFinal { id: 7, child: InnerFinal { a: 1, b: 2 } };
        assert_eq!(dynamic_bytes(&dynamic, &SerializationFormat::Cdr), concrete_cdr(&concrete));
    }

    #[test]
    fn nested_byte_match_xcdr_final() {
        let (outer_dt, inner_dt) = nested_types::<OuterFinal, InnerFinal>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterFinal { id: 7, child: InnerFinal { a: 1, b: 2 } };
        let format = xcdr_format(ExtensibilityKind::Final);
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Final)
        );
    }

    #[test]
    fn nested_byte_match_cdr_appendable() {
        let (outer_dt, inner_dt) = nested_types::<OuterApp, InnerApp>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterApp { id: 7, child: InnerApp { a: 1, b: 2 } };
        assert_eq!(dynamic_bytes(&dynamic, &SerializationFormat::Cdr), concrete_cdr(&concrete));
    }

    #[test]
    fn nested_byte_match_xcdr_appendable() {
        let (outer_dt, inner_dt) = nested_types::<OuterApp, InnerApp>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterApp { id: 7, child: InnerApp { a: 1, b: 2 } };
        let format = xcdr_format(ExtensibilityKind::Appendable);
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Appendable)
        );
    }

    #[test]
    fn nested_byte_match_xcdr_mutable() {
        let (outer_dt, inner_dt) = nested_types::<OuterMut, InnerMut>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterMut { id: 7, child: InnerMut { a: 1, b: 2 } };
        let format = xcdr_format(ExtensibilityKind::Mutable);
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Mutable)
        );
    }

    #[test]
    fn nested_byte_match_cdr_mutable() {
        let (outer_dt, inner_dt) = nested_types::<OuterMut, InnerMut>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterMut { id: 7, child: InnerMut { a: 1, b: 2 } };
        assert_eq!(dynamic_bytes(&dynamic, &SerializationFormat::Cdr), concrete_cdr(&concrete));
    }

    #[test]
    fn nested_byte_match_xcdr_mixed_final_outer_app_inner() {
        let (outer_dt, inner_dt) = nested_types::<OuterFinalChildApp, InnerApp>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterFinalChildApp { id: 7, child: InnerApp { a: 1, b: 2 } };
        let format = xcdr_format(ExtensibilityKind::Final);
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Final)
        );
    }

    #[test]
    fn nested_byte_match_xcdr_mixed_app_outer_final_inner() {
        let (outer_dt, inner_dt) = nested_types::<OuterAppChildFinal, InnerFinal>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterAppChildFinal { id: 7, child: InnerFinal { a: 1, b: 2 } };
        let format = xcdr_format(ExtensibilityKind::Appendable);
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Appendable)
        );
    }

    #[test]
    fn nested_byte_match_xcdr_mixed_final_outer_mut_inner() {
        let (outer_dt, inner_dt) = nested_types::<OuterFinalChildMut, InnerMut>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterFinalChildMut { id: 7, child: InnerMut { a: 1, b: 2 } };
        let format = xcdr_format(ExtensibilityKind::Final);
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Final)
        );
    }

    #[test]
    fn mixed_inner_appendable_is_delimited() {
        let (mixed_dt, app_inner_dt) = nested_types::<OuterFinalChildApp, InnerApp>();
        let mixed = build_dynamic_nested(&mixed_dt, &app_inner_dt);
        let mixed_bytes = dynamic_bytes(&mixed, &xcdr_format(ExtensibilityKind::Final));

        let (final_dt, final_inner_dt) = nested_types::<OuterFinal, InnerFinal>();
        let uniform = build_dynamic_nested(&final_dt, &final_inner_dt);
        let uniform_bytes = dynamic_bytes(&uniform, &xcdr_format(ExtensibilityKind::Final));

        // Layout: [encap(4) | id(4)] [inner DHEADER(4)] [a(4) | b(4)]. The all-Final
        // case omits the inner DHEADER; the mixed case inserts it before a,b.
        assert_eq!(mixed_bytes.len(), uniform_bytes.len() + 4, "inner DHEADER must be present");
        assert_eq!(mixed_bytes[..8], uniform_bytes[..8], "encap + id prefix unchanged");
        assert_eq!(mixed_bytes[8..12], [8, 0, 0, 0], "inner DHEADER = object size 8 (LE)");
        assert_eq!(mixed_bytes[12..], uniform_bytes[8..], "inner fields a,b unchanged");
    }

    #[test]
    fn mixed_codegen_roundtrip_reads_inner_dheader() {
        let (outer_dt, inner_dt) = nested_types::<OuterFinalChildApp, InnerApp>();
        let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
        let concrete = OuterFinalChildApp { id: 7, child: InnerApp { a: 1, b: 2 } };

        let dyn_bytes = dynamic_bytes(&dynamic, &xcdr_format(ExtensibilityKind::Final));
        let concrete_bytes = concrete_xcdr(&concrete, ExtensibilityKind::Final);
        assert_eq!(dyn_bytes, concrete_bytes, "dynamic and codegen wire must be identical");

        let mut d = XcdrDeserializer::new(&dyn_bytes).unwrap();
        let back = OuterFinalChildApp::deserialize_xcdr(&mut d).unwrap();
        assert_eq!(back.id, 7);
        assert_eq!(back.child.a, 1);
        assert_eq!(back.child.b, 2);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct VecHolderFinal {
        items: Vec<InnerFinal>,
    }

    fn vec_holder_dynamic_type() -> (Arc<DynamicType>, Arc<DynamicType>) {
        let inner_complete = InnerFinal::complete_type_object();
        let inner_hash = EquivalenceHash::compute(&inner_complete.serialize());

        let mut registry = TypeRegistry::new();
        registry.register_complete(inner_hash, "InnerFinal".into(), inner_complete);

        let mut outer = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "VecHolderFinal".into(),
            None,
        );
        outer.add_member(CompleteStructMember::new(
            0,
            MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false),
            TypeIdentifier::PlainSequenceLarge {
                header: PlainCollectionHeader::default(),
                bound: 0,
                element_identifier: Box::new(TypeIdentifier::CompleteTypeId(inner_hash)),
            },
            "items".to_string(),
        ));

        let outer_dt = Arc::new(
            DynamicType::from_type_object_with_registry(
                Arc::new(CompleteTypeObject::Struct(outer)),
                TypeIdentifier::None,
                &registry,
            )
            .unwrap(),
        );
        let inner_dt = standalone_type::<InnerFinal>();
        (outer_dt, inner_dt)
    }

    fn build_dynamic_vec_holder(
        outer_dt: &Arc<DynamicType>,
        inner_dt: &Arc<DynamicType>,
    ) -> DynamicData {
        let make = |a: i32, b: i32| {
            let mut e = DynamicData::new(inner_dt.clone());
            e.set("a", a).unwrap();
            e.set("b", b).unwrap();
            DynamicValue::Struct(Box::new(e))
        };
        let mut outer = DynamicData::new(outer_dt.clone());
        outer.set_value("items", DynamicValue::Sequence(vec![make(10, 20), make(30, 40)])).unwrap();
        outer
    }

    #[test]
    fn vec_of_struct_byte_match_cdr() {
        let (outer_dt, inner_dt) = vec_holder_dynamic_type();
        let dynamic = build_dynamic_vec_holder(&outer_dt, &inner_dt);
        let concrete = VecHolderFinal {
            items: vec![InnerFinal { a: 10, b: 20 }, InnerFinal { a: 30, b: 40 }],
        };
        assert_eq!(dynamic_bytes(&dynamic, &SerializationFormat::Cdr), concrete_cdr(&concrete));
    }

    #[test]
    fn vec_of_struct_byte_match_xcdr_final() {
        let (outer_dt, inner_dt) = vec_holder_dynamic_type();
        let dynamic = build_dynamic_vec_holder(&outer_dt, &inner_dt);
        let concrete = VecHolderFinal {
            items: vec![InnerFinal { a: 10, b: 20 }, InnerFinal { a: 30, b: 40 }],
        };
        let format = xcdr_format(ExtensibilityKind::Final);
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Final)
        );
    }

    fn enum_holder_dynamic_type() -> Arc<DynamicType> {
        use crate::xtypes::HasTypeObject;
        let small8_hash = hash_of(&Small8::type_identifier());
        let small16_hash = hash_of(&Small16::type_identifier());

        let mut registry = TypeRegistry::new();
        registry.register_complete(small8_hash, "Small8".into(), Small8::complete_type_object());
        registry.register_complete(small16_hash, "Small16".into(), Small16::complete_type_object());

        let flag = || MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false);
        let mut outer = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "EnumHolder".into(),
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

        Arc::new(
            DynamicType::from_type_object_with_registry(
                Arc::new(CompleteTypeObject::Struct(outer)),
                TypeIdentifier::None,
                &registry,
            )
            .unwrap(),
        )
    }

    #[test]
    fn codegen_emits_repr_bit_bound() {
        use crate::xtypes::HasTypeObject;
        let bound = |obj: CompleteTypeObject| match obj {
            CompleteTypeObject::Enum(e) => e.header.common.bit_bound,
            other => panic!("expected enum type object, got {:?}", other),
        };
        assert_eq!(bound(Small8::complete_type_object()), 8, "#[repr(u8)] enum -> bit_bound 8");
        assert_eq!(bound(Small16::complete_type_object()), 16, "#[repr(i16)] enum -> bit_bound 16");
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

    #[test]
    fn enum_bit_bound_byte_match_cdr() {
        let dynamic = build_dynamic_enum_holder(&enum_holder_dynamic_type());
        assert_eq!(
            dynamic_bytes(&dynamic, &SerializationFormat::Cdr),
            concrete_cdr(&concrete_enum_holder())
        );
    }

    #[test]
    fn enum_bit_bound_byte_match_xcdr_final() {
        let dynamic = build_dynamic_enum_holder(&enum_holder_dynamic_type());
        let format = xcdr_format(ExtensibilityKind::Final);
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete_enum_holder(), ExtensibilityKind::Final)
        );
    }

    #[test]
    fn enum_bit_bound_codegen_roundtrip() {
        let dynamic = build_dynamic_enum_holder(&enum_holder_dynamic_type());
        let bytes = dynamic_bytes(&dynamic, &xcdr_format(ExtensibilityKind::Final));
        let mut d = XcdrDeserializer::new(&bytes).unwrap();
        let back = EnumHolder::deserialize_xcdr(&mut d).unwrap();
        assert_eq!(back.v8, Small8::C);
        assert_eq!(back.v16, Small16::Z);
        assert_eq!(back.list, vec![Small8::A, Small8::B]);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct OptFinal {
        id: i32,
        #[dds(optional)]
        opt: Option<i32>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct OptApp {
        id: i32,
        #[dds(optional)]
        opt: Option<i32>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct OptMut {
        id: i32,
        #[dds(optional)]
        opt: Option<i32>,
    }

    fn build_dynamic_opt(dt: &Arc<DynamicType>, opt: Option<i32>) -> DynamicData {
        let mut data = DynamicData::new(dt.clone());
        data.set("id", 5i32).unwrap();
        if let Some(v) = opt {
            data.set("opt", v).unwrap();
        }
        data
    }

    #[test]
    fn optional_byte_match_cdr_final() {
        let dt = standalone_type::<OptFinal>();
        for opt in [Some(9i32), None] {
            let dynamic = build_dynamic_opt(&dt, opt);
            let concrete = OptFinal { id: 5, opt };
            assert_eq!(
                dynamic_bytes(&dynamic, &SerializationFormat::Cdr),
                concrete_cdr(&concrete),
                "CDR optional mismatch for {:?}",
                opt
            );
        }
    }

    #[test]
    fn optional_byte_match_xcdr_final() {
        let dt = standalone_type::<OptFinal>();
        let format = xcdr_format(ExtensibilityKind::Final);
        for opt in [Some(9i32), None] {
            let dynamic = build_dynamic_opt(&dt, opt);
            let concrete = OptFinal { id: 5, opt };
            assert_eq!(
                dynamic_bytes(&dynamic, &format),
                concrete_xcdr(&concrete, ExtensibilityKind::Final),
                "XCDR2 Final optional mismatch for {:?}",
                opt
            );
        }
    }

    #[test]
    fn optional_byte_match_cdr_mutable() {
        let dt = standalone_type::<OptMut>();
        for opt in [Some(9i32), None] {
            let dynamic = build_dynamic_opt(&dt, opt);
            let concrete = OptMut { id: 5, opt };
            assert_eq!(
                dynamic_bytes(&dynamic, &SerializationFormat::Cdr),
                concrete_cdr(&concrete),
                "CDR (PL_CDR) mutable optional mismatch for {:?}",
                opt
            );
        }
    }

    #[test]
    fn optional_byte_match_xcdr_appendable() {
        let dt = standalone_type::<OptApp>();
        let format = xcdr_format(ExtensibilityKind::Appendable);
        for opt in [Some(9i32), None] {
            let dynamic = build_dynamic_opt(&dt, opt);
            let concrete = OptApp { id: 5, opt };
            assert_eq!(
                dynamic_bytes(&dynamic, &format),
                concrete_xcdr(&concrete, ExtensibilityKind::Appendable),
                "XCDR2 Appendable optional mismatch for {:?}",
                opt
            );
        }
    }

    #[test]
    fn optional_byte_match_xcdr_mutable() {
        let dt = standalone_type::<OptMut>();
        let format = xcdr_format(ExtensibilityKind::Mutable);
        for opt in [Some(9i32), None] {
            let dynamic = build_dynamic_opt(&dt, opt);
            let concrete = OptMut { id: 5, opt };
            assert_eq!(
                dynamic_bytes(&dynamic, &format),
                concrete_xcdr(&concrete, ExtensibilityKind::Mutable),
                "XCDR2 Mutable optional mismatch for {:?}",
                opt
            );
        }
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct BareOptFinal {
        id: i32,
        opt: Option<i32>,
    }

    #[test]
    fn bare_option_field_is_marked_optional_in_metadata() {
        let CompleteTypeObject::Struct(s) = BareOptFinal::complete_type_object() else {
            panic!("expected a struct type object");
        };
        let opt = s.member_seq.iter().find(|m| m.detail.name == "opt").expect("opt member present");
        assert!(
            opt.common.member_flags.is_optional(),
            "a bare Option<T> field must carry the IS_OPTIONAL member flag"
        );
        let id = s.member_seq.iter().find(|m| m.detail.name == "id").expect("id member present");
        assert!(!id.common.member_flags.is_optional(), "a non-Option field must not be optional");
    }

    #[test]
    fn bare_option_matches_explicit_optional_wire() {
        for opt in [Some(9i32), None] {
            assert_eq!(
                concrete_cdr(&BareOptFinal { id: 5, opt }),
                concrete_cdr(&OptFinal { id: 5, opt }),
                "bare Option<T> must encode identically to #[dds(optional)] (CDR) for {:?}",
                opt
            );
            assert_eq!(
                concrete_xcdr(&BareOptFinal { id: 5, opt }, ExtensibilityKind::Final),
                concrete_xcdr(&OptFinal { id: 5, opt }, ExtensibilityKind::Final),
                "bare Option<T> must encode identically to #[dds(optional)] (XCDR) for {:?}",
                opt
            );
        }
    }

    #[test]
    fn bare_option_byte_match_dynamic() {
        let dt = standalone_type::<BareOptFinal>();
        let xcdr = xcdr_format(ExtensibilityKind::Final);
        for opt in [Some(9i32), None] {
            let dynamic = build_dynamic_opt(&dt, opt);
            let concrete = BareOptFinal { id: 5, opt };
            assert_eq!(
                dynamic_bytes(&dynamic, &SerializationFormat::Cdr),
                concrete_cdr(&concrete),
                "bare Option<T> CDR dynamic/codegen mismatch for {:?}",
                opt
            );
            assert_eq!(
                dynamic_bytes(&dynamic, &xcdr),
                concrete_xcdr(&concrete, ExtensibilityKind::Final),
                "bare Option<T> XCDR2 Final dynamic/codegen mismatch for {:?}",
                opt
            );
        }
    }

    fn final_holder_with_member<I: HasTypeObject>(member_name: &str) -> Arc<DynamicType> {
        let inner_complete = I::complete_type_object();
        let inner_hash = EquivalenceHash::compute(&inner_complete.serialize());

        let mut registry = TypeRegistry::new();
        registry.register_complete(inner_hash, "Inner".into(), inner_complete);

        let mut outer = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "Holder".into(),
            None,
        );
        outer.add_member(CompleteStructMember::new(
            0,
            MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false),
            TypeIdentifier::CompleteTypeId(inner_hash),
            member_name.to_string(),
        ));
        Arc::new(
            DynamicType::from_type_object_with_registry(
                Arc::new(CompleteTypeObject::Struct(outer)),
                TypeIdentifier::None,
                &registry,
            )
            .unwrap(),
        )
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    #[repr(i32)]
    enum UnionFinal {
        A(i32),
        B(i32),
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    #[repr(i32)]
    enum UnionMut {
        A(i32),
        B(i32),
    }

    fn union_data(dt: &Arc<DynamicType>, disc: i32, value: i32) -> DynamicData {
        let mut data = DynamicData::new(dt.clone());
        data.set_value(
            "u",
            DynamicValue::Union {
                discriminator: Box::new(DynamicValue::Int32(disc)),
                value: Box::new(DynamicValue::Int32(value)),
            },
        )
        .unwrap();
        data
    }

    #[test]
    fn union_byte_match_cdr_final() {
        let dt = final_holder_with_member::<UnionFinal>("u");
        assert_eq!(
            dynamic_bytes(&union_data(&dt, 0, 11), &SerializationFormat::Cdr),
            concrete_cdr(&UnionFinal::A(11)),
            "union case A"
        );
        assert_eq!(
            dynamic_bytes(&union_data(&dt, 1, 22), &SerializationFormat::Cdr),
            concrete_cdr(&UnionFinal::B(22)),
            "union case B"
        );
    }

    #[test]
    fn union_byte_match_xcdr_final() {
        let dt = final_holder_with_member::<UnionFinal>("u");
        let format = xcdr_format(ExtensibilityKind::Final);
        assert_eq!(
            dynamic_bytes(&union_data(&dt, 0, 11), &format),
            concrete_xcdr(&UnionFinal::A(11), ExtensibilityKind::Final),
            "union case A"
        );
        assert_eq!(
            dynamic_bytes(&union_data(&dt, 1, 22), &format),
            concrete_xcdr(&UnionFinal::B(22), ExtensibilityKind::Final),
            "union case B"
        );
    }

    #[test]
    fn union_byte_match_cdr_mutable() {
        let dt = final_holder_with_member::<UnionMut>("u");
        assert_eq!(
            dynamic_bytes(&union_data(&dt, 1, 22), &SerializationFormat::Cdr),
            concrete_cdr(&UnionMut::B(22)),
        );
    }

    #[test]
    fn union_byte_match_xcdr_mutable() {
        let dt = final_holder_with_member::<UnionMut>("u");
        let format = xcdr_format(ExtensibilityKind::Final);
        assert_eq!(
            dynamic_bytes(&union_data(&dt, 1, 22), &format),
            concrete_xcdr(&UnionMut::B(22), ExtensibilityKind::Final),
        );
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", bitset)]
    struct BitsetByteMatch {
        #[dds(bitfield = 3)]
        a: u8,
        #[dds(bitfield = 5)]
        b: u8,
    }

    fn bitset_data(dt: &Arc<DynamicType>, packed: u64) -> DynamicData {
        let mut data = DynamicData::new(dt.clone());
        data.set_value("s", DynamicValue::Bitset(packed)).unwrap();
        data
    }

    #[test]
    fn bitset_byte_match_cdr() {
        let dt = final_holder_with_member::<BitsetByteMatch>("s");
        let packed = 5u64 | (9u64 << 3); // a = 5 (3 bits), b = 9 (5 bits)
        assert_eq!(
            dynamic_bytes(&bitset_data(&dt, packed), &SerializationFormat::Cdr),
            concrete_cdr(&BitsetByteMatch { a: 5, b: 9 }),
        );
    }

    #[test]
    fn bitset_byte_match_xcdr_final() {
        let dt = final_holder_with_member::<BitsetByteMatch>("s");
        let packed = 5u64 | (9u64 << 3);
        let format = xcdr_format(ExtensibilityKind::Final);
        assert_eq!(
            dynamic_bytes(&bitset_data(&dt, packed), &format),
            concrete_xcdr(&BitsetByteMatch { a: 5, b: 9 }, ExtensibilityKind::Final),
        );
    }

    fn map_holder_dynamic_type(
        key_id: TypeIdentifier,
        value_id: TypeIdentifier,
        inner: Option<(EquivalenceHash, CompleteTypeObject)>,
    ) -> Arc<DynamicType> {
        let mut registry = TypeRegistry::new();
        if let Some((hash, obj)) = inner {
            registry.register_complete(hash, "Inner".into(), obj);
        }
        let mut outer = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "MapHolder".into(),
            None,
        );
        outer.add_member(CompleteStructMember::new(
            0,
            MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false),
            TypeIdentifier::PlainMapLarge {
                header: PlainCollectionHeader::default(),
                bound: 0,
                key_flags: CollectionElementFlag::default(),
                key_identifier: Box::new(key_id),
                element_identifier: Box::new(value_id),
            },
            "m".to_string(),
        ));
        Arc::new(
            DynamicType::from_type_object_with_registry(
                Arc::new(CompleteTypeObject::Struct(outer)),
                TypeIdentifier::None,
                &registry,
            )
            .unwrap(),
        )
    }

    #[test]
    fn map_primitive_byte_match_cdr_and_xcdr() {
        let dt = map_holder_dynamic_type(TypeIdentifier::Int32, TypeIdentifier::Int32, None);
        let mut data = DynamicData::new(dt.clone());
        data.set_value(
            "m",
            DynamicValue::Map(vec![
                (DynamicValue::Int32(1), DynamicValue::Int32(10)),
                (DynamicValue::Int32(2), DynamicValue::Int32(20)),
            ]),
        )
        .unwrap();

        let map = BTreeMap::from([(1i32, 10i32), (2i32, 20i32)]);

        assert_eq!(dynamic_bytes(&data, &SerializationFormat::Cdr), concrete_cdr(&map), "CDR");
        assert_eq!(
            dynamic_bytes(&data, &xcdr_format(ExtensibilityKind::Final)),
            concrete_xcdr(&map, ExtensibilityKind::Final),
            "XCDR2 (both primitive: no DHEADER)"
        );
    }

    #[test]
    fn map_struct_value_byte_match_cdr_and_xcdr() {
        let inner_complete = InnerFinal::complete_type_object();
        let inner_hash = EquivalenceHash::compute(&inner_complete.serialize());
        let dt = map_holder_dynamic_type(
            TypeIdentifier::Int32,
            TypeIdentifier::CompleteTypeId(inner_hash),
            Some((inner_hash, inner_complete)),
        );
        let inner_dt = standalone_type::<InnerFinal>();

        let entry = |k: i32, a: i32, b: i32| {
            let mut e = DynamicData::new(inner_dt.clone());
            e.set("a", a).unwrap();
            e.set("b", b).unwrap();
            (DynamicValue::Int32(k), DynamicValue::Struct(Box::new(e)))
        };
        let mut data = DynamicData::new(dt.clone());
        data.set_value("m", DynamicValue::Map(vec![entry(1, 10, 11), entry(2, 20, 22)])).unwrap();

        let map = BTreeMap::from([
            (1i32, InnerFinal { a: 10, b: 11 }),
            (2i32, InnerFinal { a: 20, b: 22 }),
        ]);

        assert_eq!(dynamic_bytes(&data, &SerializationFormat::Cdr), concrete_cdr(&map), "CDR");
        assert_eq!(
            dynamic_bytes(&data, &xcdr_format(ExtensibilityKind::Final)),
            concrete_xcdr(&map, ExtensibilityKind::Final),
            "XCDR2 (non-primitive value: DHEADER)"
        );
    }
}
