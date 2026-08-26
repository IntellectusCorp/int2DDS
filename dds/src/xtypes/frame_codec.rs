//! ValueFrame codec — the flat exchange buffer between language bindings and the
//! kernel.
//!
//! A frame is one contiguous little-endian buffer: a fixed slot region whose
//! offsets are compiled from the type once, followed by an arena holding
//! variable-sized content (strings, sequences) referenced by `(offset, length)`
//! slots. Bindings pack/unpack it with bulk primitives; the kernel converts it
//! to/from `DynamicData` and lets the dynamic codec own every wire rule — this
//! module contains none.
//!
//! Coverage is structs (any extensibility) of fixed-width scalars, enums,
//! bitmask/bitset, utf-8 strings, arrays/sequences of scalars, and nested
//! structs (flattened inline). Everything else — union, map, optional members,
//! wstring/char16/float128, collections of non-scalars — compiles to `None` and
//! the binding keeps its own codec path.
//!
//! The binding computes the same layout independently from its generated type
//! description; `schema_hash` (FNV-1a 64 over the canonical layout string) makes
//! any divergence a loud creation-time error instead of a silent wire error.

use std::collections::HashMap;
use std::sync::Arc;

use crate::dcps::core::error::{DdsError, DdsResult};

use super::dynamic_data::{DynamicData, DynamicValue};
use super::dynamic_type::{DynamicType, DynamicTypeKind, EnumDescriptor, PrimitiveKind};

#[derive(Debug, Clone)]
enum ScalarSlot {
    Bool,
    I8,
    U8,
    Byte,
    Char8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    /// 4-byte slot regardless of the enum's wire `bit_bound`.
    Enum(EnumDescriptor),
    /// 8-byte slot regardless of the packed wire width.
    Bitmask,
    Bitset,
}

impl ScalarSlot {
    fn width(&self) -> u32 {
        match self {
            ScalarSlot::Bool
            | ScalarSlot::I8
            | ScalarSlot::U8
            | ScalarSlot::Byte
            | ScalarSlot::Char8 => 1,
            ScalarSlot::I16 | ScalarSlot::U16 => 2,
            ScalarSlot::I32 | ScalarSlot::U32 | ScalarSlot::F32 | ScalarSlot::Enum(_) => 4,
            ScalarSlot::I64
            | ScalarSlot::U64
            | ScalarSlot::F64
            | ScalarSlot::Bitmask
            | ScalarSlot::Bitset => 8,
        }
    }

    fn tag(&self) -> &'static str {
        match self {
            ScalarSlot::Bool => "bool",
            ScalarSlot::I8 => "i8",
            ScalarSlot::U8 => "u8",
            ScalarSlot::Byte => "byte",
            ScalarSlot::Char8 => "char8",
            ScalarSlot::I16 => "i16",
            ScalarSlot::U16 => "u16",
            ScalarSlot::I32 => "i32",
            ScalarSlot::U32 => "u32",
            ScalarSlot::I64 => "i64",
            ScalarSlot::U64 => "u64",
            ScalarSlot::F32 => "f32",
            ScalarSlot::F64 => "f64",
            ScalarSlot::Enum(_) => "enum",
            ScalarSlot::Bitmask => "bitmask",
            ScalarSlot::Bitset => "bitset",
        }
    }

    fn from_primitive(kind: PrimitiveKind) -> Option<ScalarSlot> {
        match kind {
            PrimitiveKind::Boolean => Some(ScalarSlot::Bool),
            PrimitiveKind::Byte => Some(ScalarSlot::Byte),
            PrimitiveKind::Int8 => Some(ScalarSlot::I8),
            PrimitiveKind::Int16 => Some(ScalarSlot::I16),
            PrimitiveKind::Int32 => Some(ScalarSlot::I32),
            PrimitiveKind::Int64 => Some(ScalarSlot::I64),
            PrimitiveKind::Uint8 => Some(ScalarSlot::U8),
            PrimitiveKind::Uint16 => Some(ScalarSlot::U16),
            PrimitiveKind::Uint32 => Some(ScalarSlot::U32),
            PrimitiveKind::Uint64 => Some(ScalarSlot::U64),
            PrimitiveKind::Float32 => Some(ScalarSlot::F32),
            PrimitiveKind::Float64 => Some(ScalarSlot::F64),
            PrimitiveKind::Char8 => Some(ScalarSlot::Char8),
            // The dynamic codec reads these asymmetrically; not represented.
            PrimitiveKind::Float128 | PrimitiveKind::Char16 => None,
        }
    }
}

#[derive(Debug)]
enum SlotShape {
    Scalar(ScalarSlot),
    /// 8-byte `(offset: u32, byte_len: u32)` slot; utf-8 bytes in the arena.
    Utf8String,
    /// 8-byte `(offset: u32, count: u32)` slot; elements packed in the arena.
    PrimSeq(ScalarSlot),
    /// Elements inline in the slot region (multi-dimensional arrays flattened).
    PrimArray(ScalarSlot, u32),
    /// Nested struct flattened inline.
    Nested(Arc<DynamicType>, Vec<FieldSlot>),
}

#[derive(Debug)]
struct FieldSlot {
    name: Arc<str>,
    offset: u32,
    shape: SlotShape,
}

/// A struct type's compiled frame layout.
#[derive(Debug)]
pub struct FrameLayout {
    root: Arc<DynamicType>,
    fields: Vec<FieldSlot>,
    fixed_size: u32,
    schema_hash: u64,
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn scalar_of(kind: &DynamicTypeKind) -> Option<ScalarSlot> {
    match kind {
        DynamicTypeKind::Primitive(p) => ScalarSlot::from_primitive(*p),
        DynamicTypeKind::Enum(desc) => Some(ScalarSlot::Enum(desc.clone())),
        DynamicTypeKind::Bitmask(_) => Some(ScalarSlot::Bitmask),
        DynamicTypeKind::Bitset(_) => Some(ScalarSlot::Bitset),
        DynamicTypeKind::TypeRef(t) => match t.kind() {
            DynamicTypeKind::Struct(_) => None,
            other => scalar_of(other),
        },
        _ => None,
    }
}

impl FrameLayout {
    /// Compile the layout, or `None` when the type has a shape the frame does
    /// not represent (the binding then keeps its own codec path).
    pub fn compile(root: &Arc<DynamicType>) -> Option<FrameLayout> {
        let desc = root.as_struct()?;
        let mut cursor: u32 = 0;
        let fields = Self::walk(desc.members(), &mut cursor)?;
        let mut canon = String::new();
        Self::describe(&fields, "", &mut canon);
        let canon = format!("frame-v0;fixed={cursor};{canon}");
        Some(FrameLayout {
            root: root.clone(),
            fields,
            fixed_size: cursor,
            schema_hash: fnv1a64(canon.as_bytes()),
        })
    }

    fn walk(
        members: &[super::dynamic_type::MemberDescriptor],
        cursor: &mut u32,
    ) -> Option<Vec<FieldSlot>> {
        let mut fields = Vec::with_capacity(members.len());
        for member in members {
            if member.is_optional {
                return None;
            }
            let offset = *cursor;
            let shape = Self::shape_of(&member.member_type, cursor)?;
            fields.push(FieldSlot { name: member.name.clone(), offset, shape });
        }
        Some(fields)
    }

    fn shape_of(kind: &DynamicTypeKind, cursor: &mut u32) -> Option<SlotShape> {
        if let Some(scalar) = scalar_of(kind) {
            *cursor = cursor.checked_add(scalar.width())?;
            return Some(SlotShape::Scalar(scalar));
        }
        match kind {
            DynamicTypeKind::String { .. } => {
                *cursor = cursor.checked_add(8)?;
                Some(SlotShape::Utf8String)
            }
            DynamicTypeKind::Sequence { element_type, .. } => {
                let elem = scalar_of(element_type)?;
                if matches!(elem, ScalarSlot::Bitmask | ScalarSlot::Bitset) {
                    return None;
                }
                *cursor = cursor.checked_add(8)?;
                Some(SlotShape::PrimSeq(elem))
            }
            DynamicTypeKind::Array { element_type, dimensions } => {
                let elem = scalar_of(element_type)?;
                if matches!(elem, ScalarSlot::Bitmask | ScalarSlot::Bitset) {
                    return None;
                }
                let mut total: u32 = 1;
                for dim in dimensions {
                    total = total.checked_mul(*dim)?;
                }
                *cursor = cursor.checked_add(elem.width().checked_mul(total)?)?;
                Some(SlotShape::PrimArray(elem, total))
            }
            DynamicTypeKind::TypeRef(t) => match t.kind() {
                DynamicTypeKind::Struct(desc) => {
                    let fields = Self::walk(desc.members(), cursor)?;
                    Some(SlotShape::Nested(t.clone(), fields))
                }
                other => Self::shape_of(other, cursor),
            },
            _ => None,
        }
    }

    fn describe(fields: &[FieldSlot], prefix: &str, out: &mut String) {
        for field in fields {
            let path = if prefix.is_empty() {
                field.name.to_string()
            } else {
                format!("{prefix}.{}", field.name)
            };
            match &field.shape {
                SlotShape::Scalar(s) => {
                    out.push_str(&format!("{path}:{}:{};", s.tag(), field.offset))
                }
                SlotShape::Utf8String => out.push_str(&format!("{path}:str:{};", field.offset)),
                SlotShape::PrimSeq(e) => {
                    out.push_str(&format!("{path}:seq[{}]:{};", e.tag(), field.offset))
                }
                SlotShape::PrimArray(e, n) => {
                    out.push_str(&format!("{path}:arr[{};{n}]:{};", e.tag(), field.offset))
                }
                SlotShape::Nested(_, sub) => Self::describe(sub, &path, out),
            }
        }
    }

    pub fn fixed_size(&self) -> u32 {
        self.fixed_size
    }

    pub fn schema_hash(&self) -> u64 {
        self.schema_hash
    }

    pub fn root_type(&self) -> &Arc<DynamicType> {
        &self.root
    }

    /// Read a frame into `DynamicData` (binding → kernel direction).
    pub fn to_dynamic(&self, frame: &[u8]) -> DdsResult<DynamicData> {
        if frame.len() < self.fixed_size as usize {
            return Err(DdsError::Error(format!(
                "frame too short: {} < fixed region {}",
                frame.len(),
                self.fixed_size
            )));
        }
        let values = Self::read_struct(&self.fields, frame)?;
        Ok(DynamicData::with_values(self.root.clone(), values))
    }

    fn read_struct(
        fields: &[FieldSlot],
        frame: &[u8],
    ) -> DdsResult<HashMap<Arc<str>, DynamicValue>> {
        let mut values = HashMap::with_capacity(fields.len());
        for field in fields {
            let value = match &field.shape {
                SlotShape::Scalar(s) => read_scalar(s, frame, field.offset as usize)?,
                SlotShape::Utf8String => {
                    let (off, len) = read_ref_slot(frame, field.offset as usize)?;
                    let bytes = arena_slice(frame, off, len, 1, &field.name)?;
                    DynamicValue::String(String::from_utf8(bytes.to_vec()).map_err(|_| {
                        DdsError::Error(format!("frame field '{}': invalid utf-8", field.name))
                    })?)
                }
                SlotShape::PrimSeq(elem) => {
                    let (off, count) = read_ref_slot(frame, field.offset as usize)?;
                    let bytes = arena_slice(frame, off, count, elem.width(), &field.name)?;
                    DynamicValue::Sequence(read_elems(elem, bytes, count as usize)?)
                }
                SlotShape::PrimArray(elem, count) => {
                    let width = elem.width() as usize;
                    let start = field.offset as usize;
                    let bytes = &frame[start..start + width * *count as usize];
                    DynamicValue::Array(read_elems(elem, bytes, *count as usize)?)
                }
                SlotShape::Nested(ty, sub) => {
                    let values = Self::read_struct(sub, frame)?;
                    DynamicValue::Struct(Box::new(DynamicData::with_values(ty.clone(), values)))
                }
            };
            values.insert(field.name.clone(), value);
        }
        Ok(values)
    }

    /// Write `DynamicData` as a frame (kernel → binding direction). Members the
    /// data does not hold are written as defaults, mirroring the dynamic
    /// serializer's missing-member rule.
    pub fn from_dynamic(&self, data: &DynamicData) -> DdsResult<Vec<u8>> {
        let mut frame = vec![0u8; self.fixed_size as usize];
        Self::write_struct(&self.fields, Some(data), &mut frame)?;
        Ok(frame)
    }

    fn write_struct(
        fields: &[FieldSlot],
        data: Option<&DynamicData>,
        frame: &mut Vec<u8>,
    ) -> DdsResult<()> {
        for field in fields {
            let value = data.and_then(|d| d.get_value(&field.name));
            match (&field.shape, value) {
                (SlotShape::Scalar(s), value) => {
                    write_scalar(s, value, frame, field.offset as usize, &field.name)?
                }
                (SlotShape::Utf8String, value) => {
                    let text = match value {
                        Some(DynamicValue::String(s)) => s.as_str(),
                        None | Some(DynamicValue::Null) => "",
                        Some(other) => return Err(shape_mismatch(&field.name, "String", other)),
                    };
                    write_ref_slot(frame, field.offset as usize, text.len(), &field.name)?;
                    frame.extend_from_slice(text.as_bytes());
                }
                (SlotShape::PrimSeq(elem), value) => {
                    let empty = Vec::new();
                    let items = match value {
                        Some(DynamicValue::Sequence(items)) => items,
                        None | Some(DynamicValue::Null) => &empty,
                        Some(other) => return Err(shape_mismatch(&field.name, "Sequence", other)),
                    };
                    write_ref_slot(frame, field.offset as usize, items.len(), &field.name)?;
                    for item in items {
                        append_elem(elem, item, frame, &field.name)?;
                    }
                }
                (SlotShape::PrimArray(elem, count), value) => {
                    let mut at = field.offset as usize;
                    let items = match value {
                        Some(DynamicValue::Array(items)) => {
                            if items.len() != *count as usize {
                                return Err(DdsError::Error(format!(
                                    "frame field '{}': array has {} elements, type says {count}",
                                    field.name,
                                    items.len()
                                )));
                            }
                            Some(items)
                        }
                        None | Some(DynamicValue::Null) => None,
                        Some(other) => return Err(shape_mismatch(&field.name, "Array", other)),
                    };
                    if let Some(items) = items {
                        for item in items {
                            write_elem_at(elem, item, frame, &mut at, &field.name)?;
                        }
                    }
                }
                (SlotShape::Nested(_, sub), value) => {
                    let nested = match value {
                        Some(DynamicValue::Struct(inner)) => Some(&**inner),
                        None | Some(DynamicValue::Null) => None,
                        Some(other) => return Err(shape_mismatch(&field.name, "Struct", other)),
                    };
                    Self::write_struct(sub, nested, frame)?;
                }
            }
        }
        Ok(())
    }
}

fn shape_mismatch(name: &str, want: &str, got: &DynamicValue) -> DdsError {
    DdsError::Error(format!("frame field '{name}': expected {want}, got {}", got.type_kind()))
}

fn read_ref_slot(frame: &[u8], offset: usize) -> DdsResult<(u32, u32)> {
    let bytes = frame
        .get(offset..offset + 8)
        .ok_or_else(|| DdsError::Error("frame ref slot out of bounds".to_string()))?;
    let off = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    Ok((off, len))
}

fn write_ref_slot(frame: &mut [u8], offset: usize, count: usize, name: &str) -> DdsResult<()> {
    let arena_off = u32::try_from(frame.len())
        .map_err(|_| DdsError::Error(format!("frame field '{name}': frame exceeds u32 range")))?;
    let count = u32::try_from(count)
        .map_err(|_| DdsError::Error(format!("frame field '{name}': length exceeds u32 range")))?;
    frame[offset..offset + 4].copy_from_slice(&arena_off.to_le_bytes());
    frame[offset + 4..offset + 8].copy_from_slice(&count.to_le_bytes());
    Ok(())
}

fn arena_slice<'a>(
    frame: &'a [u8],
    offset: u32,
    count: u32,
    elem_width: u32,
    name: &str,
) -> DdsResult<&'a [u8]> {
    let byte_len = (count as usize)
        .checked_mul(elem_width as usize)
        .ok_or_else(|| DdsError::Error(format!("frame field '{name}': arena length overflows")))?;
    let start = offset as usize;
    frame.get(start..start + byte_len).ok_or_else(|| {
        DdsError::Error(format!(
            "frame field '{name}': arena reference {start}+{byte_len} exceeds frame {}",
            frame.len()
        ))
    })
}

fn read_elems(elem: &ScalarSlot, bytes: &[u8], count: usize) -> DdsResult<Vec<DynamicValue>> {
    let width = elem.width() as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(decode_scalar(elem, &bytes[i * width..(i + 1) * width])?);
    }
    Ok(out)
}

fn read_scalar(slot: &ScalarSlot, frame: &[u8], offset: usize) -> DdsResult<DynamicValue> {
    let width = slot.width() as usize;
    let bytes = frame
        .get(offset..offset + width)
        .ok_or_else(|| DdsError::Error("frame scalar slot out of bounds".to_string()))?;
    decode_scalar(slot, bytes)
}

fn decode_scalar(slot: &ScalarSlot, b: &[u8]) -> DdsResult<DynamicValue> {
    Ok(match slot {
        ScalarSlot::Bool => DynamicValue::Boolean(b[0] != 0),
        ScalarSlot::I8 => DynamicValue::Int8(b[0] as i8),
        ScalarSlot::U8 => DynamicValue::Uint8(b[0]),
        ScalarSlot::Byte => DynamicValue::Byte(b[0]),
        ScalarSlot::Char8 => DynamicValue::Char8(b[0] as char),
        ScalarSlot::I16 => DynamicValue::Int16(i16::from_le_bytes(b.try_into().unwrap())),
        ScalarSlot::U16 => DynamicValue::Uint16(u16::from_le_bytes(b.try_into().unwrap())),
        ScalarSlot::I32 => DynamicValue::Int32(i32::from_le_bytes(b.try_into().unwrap())),
        ScalarSlot::U32 => DynamicValue::Uint32(u32::from_le_bytes(b.try_into().unwrap())),
        ScalarSlot::I64 => DynamicValue::Int64(i64::from_le_bytes(b.try_into().unwrap())),
        ScalarSlot::U64 => DynamicValue::Uint64(u64::from_le_bytes(b.try_into().unwrap())),
        ScalarSlot::F32 => DynamicValue::Float32(f32::from_le_bytes(b.try_into().unwrap())),
        ScalarSlot::F64 => DynamicValue::Float64(f64::from_le_bytes(b.try_into().unwrap())),
        ScalarSlot::Enum(desc) => {
            let value = i32::from_le_bytes(b.try_into().unwrap());
            let name = desc.get_literal_by_value(value).map(|l| l.name.clone()).unwrap_or_default();
            DynamicValue::Enum { name, value }
        }
        ScalarSlot::Bitmask => DynamicValue::Bitmask(u64::from_le_bytes(b.try_into().unwrap())),
        ScalarSlot::Bitset => DynamicValue::Bitset(u64::from_le_bytes(b.try_into().unwrap())),
    })
}

fn write_scalar(
    slot: &ScalarSlot,
    value: Option<&DynamicValue>,
    frame: &mut [u8],
    offset: usize,
    name: &str,
) -> DdsResult<()> {
    let width = slot.width() as usize;
    let mut at = offset;
    match value {
        None | Some(DynamicValue::Null) => {
            frame[offset..offset + width].fill(0);
            Ok(())
        }
        Some(value) => encode_scalar_at(slot, value, frame, &mut at, name),
    }
}

fn append_elem(
    elem: &ScalarSlot,
    value: &DynamicValue,
    frame: &mut Vec<u8>,
    name: &str,
) -> DdsResult<()> {
    let mut at = frame.len();
    frame.resize(at + elem.width() as usize, 0);
    encode_scalar_at(elem, value, frame, &mut at, name)
}

fn write_elem_at(
    elem: &ScalarSlot,
    value: &DynamicValue,
    frame: &mut [u8],
    at: &mut usize,
    name: &str,
) -> DdsResult<()> {
    encode_scalar_at(elem, value, frame, at, name)
}

fn encode_scalar_at(
    slot: &ScalarSlot,
    value: &DynamicValue,
    frame: &mut [u8],
    at: &mut usize,
    name: &str,
) -> DdsResult<()> {
    let offset = *at;
    *at += slot.width() as usize;
    let dst = &mut frame[offset..offset + slot.width() as usize];
    match (slot, value) {
        (ScalarSlot::Bool, DynamicValue::Boolean(v)) => dst[0] = u8::from(*v),
        (ScalarSlot::I8, DynamicValue::Int8(v)) => dst[0] = *v as u8,
        (ScalarSlot::U8, DynamicValue::Uint8(v)) => dst[0] = *v,
        (ScalarSlot::Byte, DynamicValue::Byte(v)) => dst[0] = *v,
        (ScalarSlot::Char8, DynamicValue::Char8(v)) => dst[0] = *v as u8,
        (ScalarSlot::I16, DynamicValue::Int16(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (ScalarSlot::U16, DynamicValue::Uint16(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (ScalarSlot::I32, DynamicValue::Int32(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (ScalarSlot::U32, DynamicValue::Uint32(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (ScalarSlot::I64, DynamicValue::Int64(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (ScalarSlot::U64, DynamicValue::Uint64(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (ScalarSlot::F32, DynamicValue::Float32(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (ScalarSlot::F64, DynamicValue::Float64(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (ScalarSlot::Enum(_), DynamicValue::Enum { value, .. }) => {
            dst.copy_from_slice(&value.to_le_bytes())
        }
        (ScalarSlot::Bitmask, DynamicValue::Bitmask(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (ScalarSlot::Bitset, DynamicValue::Bitset(v)) => dst.copy_from_slice(&v.to_le_bytes()),
        (slot, other) => return Err(shape_mismatch(name, slot.tag(), other)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcps::topic::type_support::SerializationFormat;
    use crate::serialize::cdr::ExtensibilityKind;
    use crate::xtypes::{
        deserialize_dynamic_data, plain_collection_equiv_kind, serialize_dynamic_data,
        CollectionElementFlag, CompleteBitflag, CompleteBitmaskType, CompleteEnumeratedLiteral,
        CompleteEnumeratedType, CompleteStructMember, CompleteStructType, CompleteTypeObject,
        CompleteUnionMember, CompleteUnionType, EnumeratedLiteralFlag, EquivalenceHash, MemberFlag,
        PlainCollectionHeader, TryConstructKind, TypeFlag, TypeIdentifier, TypeRegistry,
    };

    fn member_flag(is_optional: bool, is_key: bool) -> MemberFlag {
        MemberFlag::new(TryConstructKind::Discard, false, is_optional, false, is_key, false)
    }

    fn tf(ext: ExtensibilityKind) -> TypeFlag {
        let kind = match ext {
            ExtensibilityKind::Final => crate::xtypes::ExtensibilityKind::Final,
            ExtensibilityKind::Appendable => crate::xtypes::ExtensibilityKind::Appendable,
            ExtensibilityKind::Mutable => crate::xtypes::ExtensibilityKind::Mutable,
        };
        TypeFlag::new(kind, false, false)
    }

    struct Field {
        name: &'static str,
        id: u32,
        type_id: TypeIdentifier,
        is_key: bool,
        is_optional: bool,
    }

    fn field(name: &'static str, id: u32, type_id: TypeIdentifier) -> Field {
        Field { name, id, type_id, is_key: false, is_optional: false }
    }

    fn key(name: &'static str, id: u32, type_id: TypeIdentifier) -> Field {
        Field { is_key: true, ..field(name, id, type_id) }
    }

    fn struct_object(name: &str, ext: ExtensibilityKind, fields: Vec<Field>) -> CompleteTypeObject {
        let mut desc = CompleteStructType::new(tf(ext), name.into(), None);
        for f in fields {
            desc.add_member(CompleteStructMember::new(
                f.id,
                member_flag(f.is_optional, f.is_key),
                f.type_id,
                f.name.to_string(),
            ));
        }
        CompleteTypeObject::Struct(desc)
    }

    fn build(object: CompleteTypeObject) -> Arc<DynamicType> {
        Arc::new(DynamicType::from_type_object(object, TypeIdentifier::None).unwrap())
    }

    fn build_with(object: CompleteTypeObject, registry: &TypeRegistry) -> Arc<DynamicType> {
        Arc::new(
            DynamicType::from_type_object_with_registry(
                Arc::new(object),
                TypeIdentifier::None,
                registry,
            )
            .unwrap(),
        )
    }

    fn register(
        registry: &mut TypeRegistry,
        name: &str,
        obj: CompleteTypeObject,
    ) -> TypeIdentifier {
        let hash = EquivalenceHash::compute(&obj.serialize());
        registry.register_complete(hash, name.into(), obj);
        TypeIdentifier::CompleteTypeId(hash)
    }

    fn sequence_of(element: TypeIdentifier) -> TypeIdentifier {
        TypeIdentifier::PlainSequenceSmall {
            header: PlainCollectionHeader {
                equiv_kind: plain_collection_equiv_kind(&element),
                element_flags: CollectionElementFlag::default(),
            },
            bound: 0,
            element_identifier: Box::new(element),
        }
    }

    fn array_of(element: TypeIdentifier, bounds: Vec<u8>) -> TypeIdentifier {
        TypeIdentifier::PlainArraySmall {
            header: PlainCollectionHeader {
                equiv_kind: plain_collection_equiv_kind(&element),
                element_flags: CollectionElementFlag::default(),
            },
            array_bound_seq: bounds,
            element_identifier: Box::new(element),
        }
    }

    fn color_enum() -> CompleteTypeObject {
        let mut e = CompleteEnumeratedType::new(tf(ExtensibilityKind::Final), "Color".into(), 32);
        e.add_literal(CompleteEnumeratedLiteral::new(0, EnumeratedLiteralFlag(0), "RED".into()));
        e.add_literal(CompleteEnumeratedLiteral::new(1, EnumeratedLiteralFlag(0), "GREEN".into()));
        e.add_literal(CompleteEnumeratedLiteral::new(2, EnumeratedLiteralFlag(0), "BLUE".into()));
        CompleteTypeObject::Enum(e)
    }

    fn flags_bitmask() -> CompleteTypeObject {
        let mut b = CompleteBitmaskType::new(tf(ExtensibilityKind::Final), "Flags".into(), 8);
        b.add_flag(CompleteBitflag::new(0, MemberFlag::default(), "A".into()));
        b.add_flag(CompleteBitflag::new(1, MemberFlag::default(), "B".into()));
        CompleteTypeObject::Bitmask(b)
    }

    fn formats(ext: ExtensibilityKind) -> Vec<(&'static str, SerializationFormat)> {
        vec![
            ("xcdr1", SerializationFormat::Cdr),
            ("xcdr2", SerializationFormat::Xcdr { extensibility_kind: ext, use_delimiters: false }),
        ]
    }

    fn assert_roundtrip(label: &str, layout: &FrameLayout, data: &DynamicData) {
        let frame = layout.from_dynamic(data).unwrap_or_else(|e| panic!("{label}: pack: {e:?}"));
        let back = layout.to_dynamic(&frame).unwrap_or_else(|e| panic!("{label}: unpack: {e:?}"));
        assert_eq!(&back, data, "{label}: frame roundtrip changed the sample");
        for (format_name, format) in formats(data.dynamic_type().extensibility()) {
            let want = serialize_dynamic_data(data, &format).unwrap();
            let got = serialize_dynamic_data(&back, &format).unwrap();
            assert_eq!(got, want, "{label}/{format_name}: wire bytes differ");
        }
    }

    fn scalar_type(ext: ExtensibilityKind) -> Arc<DynamicType> {
        build(struct_object(
            "Scalars",
            ext,
            vec![
                key("id", 0, TypeIdentifier::Uint32),
                field("a", 1, TypeIdentifier::Int32),
                field("flag", 2, TypeIdentifier::Boolean),
                field("small", 3, TypeIdentifier::Uint8),
                field("half", 4, TypeIdentifier::Int16),
                field("big", 5, TypeIdentifier::Int64),
                field("ratio", 6, TypeIdentifier::Float32),
                field("x", 7, TypeIdentifier::Float64),
            ],
        ))
    }

    fn scalar_data(dynamic_type: &Arc<DynamicType>) -> DynamicData {
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 7u32).unwrap();
        data.set("a", -3i32).unwrap();
        data.set("flag", true).unwrap();
        data.set("small", 0xabu8).unwrap();
        data.set("half", 513i16).unwrap();
        data.set("big", -1_234_567_890_123i64).unwrap();
        data.set("ratio", 1.5f32).unwrap();
        data.set("x", -2.25f64).unwrap();
        data
    }

    #[test]
    fn scalar_layout_offsets_are_packed() {
        let layout = FrameLayout::compile(&scalar_type(ExtensibilityKind::Appendable)).unwrap();
        // u32 + i32 + bool + u8 + i16 + i64 + f32 + f64, packed with no padding.
        assert_eq!(layout.fixed_size(), 4 + 4 + 1 + 1 + 2 + 8 + 4 + 8);
    }

    #[test]
    fn schema_hash_is_stable_and_shape_sensitive() {
        let a = FrameLayout::compile(&scalar_type(ExtensibilityKind::Appendable)).unwrap();
        let b = FrameLayout::compile(&scalar_type(ExtensibilityKind::Appendable)).unwrap();
        assert_eq!(a.schema_hash(), b.schema_hash());

        let widened = build(struct_object(
            "Scalars",
            ExtensibilityKind::Appendable,
            vec![key("id", 0, TypeIdentifier::Uint32), field("a", 1, TypeIdentifier::Int64)],
        ));
        let widened = FrameLayout::compile(&widened).unwrap();
        assert_ne!(a.schema_hash(), widened.schema_hash());
    }

    #[test]
    fn scalar_roundtrip_every_extensibility() {
        for ext in
            [ExtensibilityKind::Final, ExtensibilityKind::Appendable, ExtensibilityKind::Mutable]
        {
            let dynamic_type = scalar_type(ext);
            let layout = FrameLayout::compile(&dynamic_type).unwrap();
            assert_roundtrip(&format!("scalars/{ext:?}"), &layout, &scalar_data(&dynamic_type));
        }
    }

    #[test]
    fn string_and_sequence_roundtrip() {
        let dynamic_type = build(struct_object(
            "Coll",
            ExtensibilityKind::Appendable,
            vec![
                key("id", 0, TypeIdentifier::Uint32),
                field("joints", 1, sequence_of(TypeIdentifier::Float64)),
                field("codes", 2, sequence_of(TypeIdentifier::Int32)),
                field("name", 3, TypeIdentifier::String8Small { bound: 128 }),
            ],
        ));
        let layout = FrameLayout::compile(&dynamic_type).unwrap();
        assert_eq!(layout.fixed_size(), 4 + 8 + 8 + 8);

        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 7u32).unwrap();
        data.set_value(
            "joints",
            DynamicValue::Sequence(
                (0..64).map(|i| DynamicValue::Float64(f64::from(i) * 0.5)).collect(),
            ),
        )
        .unwrap();
        data.set_value("codes", DynamicValue::Sequence((0..16).map(DynamicValue::Int32).collect()))
            .unwrap();
        data.set("name", "robot-arm-controller-01".to_string()).unwrap();
        assert_roundtrip("collections", &layout, &data);
    }

    #[test]
    fn enum_bitmask_and_multidim_array_roundtrip() {
        let mut registry = TypeRegistry::new();
        let enum_id = register(&mut registry, "Color", color_enum());
        let mask_id = register(&mut registry, "Flags", flags_bitmask());
        let dynamic_type = build_with(
            struct_object(
                "Mixed",
                ExtensibilityKind::Final,
                vec![
                    field("color", 0, enum_id.clone()),
                    field("flags", 1, mask_id),
                    field("grid", 2, array_of(TypeIdentifier::Int32, vec![2, 3])),
                    field("palette", 3, sequence_of(enum_id)),
                ],
            ),
            &registry,
        );
        let layout = FrameLayout::compile(&dynamic_type).unwrap();
        assert_eq!(layout.fixed_size(), 4 + 8 + 24 + 8);

        let mut data = DynamicData::new(dynamic_type.clone());
        data.set_value("color", DynamicValue::Enum { name: "GREEN".into(), value: 1 }).unwrap();
        data.set_value("flags", DynamicValue::Bitmask(0b10)).unwrap();
        data.set_value("grid", DynamicValue::Array((0..6).map(DynamicValue::Int32).collect()))
            .unwrap();
        data.set_value(
            "palette",
            DynamicValue::Sequence(vec![
                DynamicValue::Enum { name: "BLUE".into(), value: 2 },
                DynamicValue::Enum { name: "RED".into(), value: 0 },
            ]),
        )
        .unwrap();
        assert_roundtrip("mixed", &layout, &data);
    }

    #[test]
    fn nested_struct_flattens_inline() {
        let mut registry = TypeRegistry::new();
        let inner_id = register(
            &mut registry,
            "Inner",
            struct_object(
                "Inner",
                ExtensibilityKind::Final,
                vec![
                    field("a", 0, TypeIdentifier::Int32),
                    field("label", 1, TypeIdentifier::String8Small { bound: 0 }),
                ],
            ),
        );
        let dynamic_type = build_with(
            struct_object(
                "Outer",
                ExtensibilityKind::Final,
                vec![
                    key("id", 0, TypeIdentifier::Uint32),
                    field("child", 1, inner_id),
                    field("tail", 2, TypeIdentifier::Int16),
                ],
            ),
            &registry,
        );
        let layout = FrameLayout::compile(&dynamic_type).unwrap();
        assert_eq!(layout.fixed_size(), 4 + (4 + 8) + 2);

        let inner_type = dynamic_type
            .get_member("child")
            .unwrap()
            .member_type
            .as_type_ref()
            .expect("Inner unresolved")
            .clone();
        let mut inner = DynamicData::new(inner_type);
        inner.set("a", 11i32).unwrap();
        inner.set("label", "leaf".to_string()).unwrap();
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 9u32).unwrap();
        data.set_value("child", DynamicValue::Struct(Box::new(inner))).unwrap();
        data.set("tail", -2i16).unwrap();
        assert_roundtrip("nested", &layout, &data);
    }

    #[test]
    fn missing_values_pack_as_defaults() {
        let dynamic_type = build(struct_object(
            "Coll",
            ExtensibilityKind::Appendable,
            vec![
                key("id", 0, TypeIdentifier::Uint32),
                field("name", 1, TypeIdentifier::String8Small { bound: 0 }),
                field("codes", 2, sequence_of(TypeIdentifier::Int32)),
            ],
        ));
        let layout = FrameLayout::compile(&dynamic_type).unwrap();
        let frame = layout.from_dynamic(&DynamicData::new(dynamic_type)).unwrap();
        let back = layout.to_dynamic(&frame).unwrap();
        assert_eq!(back.get_value("id"), Some(&DynamicValue::Uint32(0)));
        assert_eq!(back.get_value("name"), Some(&DynamicValue::String(String::new())));
        assert_eq!(back.get_value("codes"), Some(&DynamicValue::Sequence(Vec::new())));
    }

    #[test]
    fn unsupported_shapes_do_not_compile() {
        let mut registry = TypeRegistry::new();
        let inner_id = register(
            &mut registry,
            "Inner",
            struct_object(
                "Inner",
                ExtensibilityKind::Final,
                vec![field("a", 0, TypeIdentifier::Int32)],
            ),
        );

        let mut union_desc = CompleteUnionType::new(
            tf(ExtensibilityKind::Final),
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "U".into(),
        );
        union_desc.add_member(CompleteUnionMember::new(
            1,
            Default::default(),
            TypeIdentifier::Int32,
            vec![1],
            "x".to_string(),
        ));
        let union_id = register(&mut registry, "U", CompleteTypeObject::Union(union_desc));

        let unsupported = [
            ("optional member", {
                let mut desc =
                    CompleteStructType::new(tf(ExtensibilityKind::Final), "T".into(), None);
                desc.add_member(CompleteStructMember::new(
                    0,
                    member_flag(true, false),
                    TypeIdentifier::Int32,
                    "opt".to_string(),
                ));
                build_with(CompleteTypeObject::Struct(desc), &registry)
            }),
            (
                "union member",
                build_with(
                    struct_object("T", ExtensibilityKind::Final, vec![field("u", 0, union_id)]),
                    &registry,
                ),
            ),
            (
                "wstring",
                build(struct_object(
                    "T",
                    ExtensibilityKind::Final,
                    vec![field("w", 0, TypeIdentifier::String16Small { bound: 0 })],
                )),
            ),
            (
                "sequence of string",
                build(struct_object(
                    "T",
                    ExtensibilityKind::Final,
                    vec![field("s", 0, sequence_of(TypeIdentifier::String8Small { bound: 0 }))],
                )),
            ),
            (
                "sequence of struct",
                build_with(
                    struct_object(
                        "T",
                        ExtensibilityKind::Final,
                        vec![field("s", 0, sequence_of(inner_id))],
                    ),
                    &registry,
                ),
            ),
            (
                "char16",
                build(struct_object(
                    "T",
                    ExtensibilityKind::Final,
                    vec![field("c", 0, TypeIdentifier::Char16)],
                )),
            ),
        ];
        for (label, dynamic_type) in unsupported {
            assert!(FrameLayout::compile(&dynamic_type).is_none(), "{label} must not compile");
        }
    }

    #[test]
    fn malformed_frames_are_refused() {
        let dynamic_type = build(struct_object(
            "Coll",
            ExtensibilityKind::Final,
            vec![
                key("id", 0, TypeIdentifier::Uint32),
                field("name", 1, TypeIdentifier::String8Small { bound: 0 }),
            ],
        ));
        let layout = FrameLayout::compile(&dynamic_type).unwrap();

        assert!(layout.to_dynamic(&[0u8; 3]).is_err(), "truncated fixed region");

        // name slot points past the end of the frame.
        let mut frame = vec![0u8; layout.fixed_size() as usize];
        frame[4..8].copy_from_slice(&100u32.to_le_bytes());
        frame[8..12].copy_from_slice(&4u32.to_le_bytes());
        assert!(layout.to_dynamic(&frame).is_err(), "arena reference out of bounds");

        // name bytes are not valid utf-8.
        let mut data = DynamicData::new(dynamic_type);
        data.set("name", "ok".to_string()).unwrap();
        let mut frame = layout.from_dynamic(&data).unwrap();
        let len = frame.len();
        frame[len - 1] = 0xff;
        frame[len - 2] = 0xfe;
        assert!(layout.to_dynamic(&frame).is_err(), "invalid utf-8 accepted");
    }

    /// The wire representation the frame path produces must be what the dynamic
    /// path produces for the same values — checked here across formats for a
    /// keyed sample built through the frame direction only.
    #[test]
    fn frame_built_sample_matches_dynamic_wire_bytes() {
        let dynamic_type = scalar_type(ExtensibilityKind::Appendable);
        let layout = FrameLayout::compile(&dynamic_type).unwrap();
        let reference = scalar_data(&dynamic_type);

        let frame = layout.from_dynamic(&reference).unwrap();
        let from_frame = layout.to_dynamic(&frame).unwrap();
        for (format_name, format) in formats(ExtensibilityKind::Appendable) {
            let bytes = serialize_dynamic_data(&from_frame, &format).unwrap();
            let want = serialize_dynamic_data(&reference, &format).unwrap();
            assert_eq!(bytes, want, "{format_name}");
            let redecoded = deserialize_dynamic_data(&bytes, &dynamic_type).unwrap();
            assert_eq!(layout.from_dynamic(&redecoded).unwrap(), frame, "{format_name}: reframe");
        }
    }
}
