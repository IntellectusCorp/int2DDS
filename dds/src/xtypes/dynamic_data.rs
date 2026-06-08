//! DynamicData - Runtime data container for dynamic types.
//!
//! This module provides runtime data storage and access for types
//! described by DynamicType, enabling type-agnostic data handling.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::xtypes::dynamic_type::{DynamicType, DynamicTypeError, DynamicTypeKind, PrimitiveKind};

/// Dynamic data container - stores field values at runtime.
#[derive(Debug, Clone)]
pub struct DynamicData {
    dynamic_type: Arc<DynamicType>,
    values: HashMap<Arc<str>, DynamicValue>,
}

impl PartialEq for DynamicData {
    fn eq(&self, other: &Self) -> bool {
        self.dynamic_type.type_name() == other.dynamic_type.type_name()
            && self.values == other.values
    }
}

impl DynamicData {
    /// Create a new DynamicData instance for the given type.
    pub fn new(dynamic_type: Arc<DynamicType>) -> Self {
        Self { dynamic_type, values: HashMap::new() }
    }

    /// Create a new DynamicData with pre-populated values.
    pub fn with_values(
        dynamic_type: Arc<DynamicType>,
        values: HashMap<Arc<str>, DynamicValue>,
    ) -> Self {
        Self { dynamic_type, values }
    }

    /// Get the type descriptor.
    pub fn dynamic_type(&self) -> &Arc<DynamicType> {
        &self.dynamic_type
    }

    /// Get the type name.
    pub fn type_name(&self) -> &str {
        self.dynamic_type.type_name()
    }

    /// Get a field value with type conversion.
    pub fn get<T: FromDynamicValue>(&self, field: &str) -> Result<T, DynamicTypeError> {
        let value = self
            .values
            .get(field)
            .ok_or_else(|| DynamicTypeError::FieldNotFound(field.to_string()))?;
        T::from_dynamic(value)
    }

    /// Set a field value with type conversion.
    pub fn set<T: IntoDynamicValue>(
        &mut self,
        field: &str,
        value: T,
    ) -> Result<(), DynamicTypeError> {
        // Verify field exists in type
        if let Some(struct_desc) = self.dynamic_type.as_struct() {
            if struct_desc.get_member(field).is_none() {
                return Err(DynamicTypeError::FieldNotFound(field.to_string()));
            }
        }
        self.values.insert(Arc::from(field), value.into_dynamic());
        Ok(())
    }

    /// Get a nested field value using dot notation (e.g., "position.x").
    pub fn get_nested<T: FromDynamicValue>(&self, path: &str) -> Result<T, DynamicTypeError> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return Err(DynamicTypeError::FieldNotFound(path.to_string()));
        }

        let mut current_value = self
            .values
            .get(parts[0])
            .ok_or_else(|| DynamicTypeError::FieldNotFound(parts[0].to_string()))?;

        for part in &parts[1..] {
            match current_value {
                DynamicValue::Struct(inner) => {
                    current_value = inner
                        .values
                        .get(*part)
                        .ok_or_else(|| DynamicTypeError::FieldNotFound(part.to_string()))?;
                }
                _ => {
                    return Err(DynamicTypeError::InvalidOperation(format!(
                        "Cannot access field '{}' on non-struct value",
                        part
                    )));
                }
            }
        }

        T::from_dynamic(current_value)
    }

    /// Get a raw DynamicValue by field name.
    pub fn get_value(&self, field: &str) -> Option<&DynamicValue> {
        self.values.get(field)
    }

    /// Get a mutable raw DynamicValue by field name.
    pub fn get_value_mut(&mut self, field: &str) -> Option<&mut DynamicValue> {
        self.values.get_mut(field)
    }

    /// Set a raw DynamicValue by field name.
    pub fn set_value(&mut self, field: &str, value: DynamicValue) -> Result<(), DynamicTypeError> {
        // Verify field exists in type
        if let Some(struct_desc) = self.dynamic_type.as_struct() {
            if struct_desc.get_member(field).is_none() {
                return Err(DynamicTypeError::FieldNotFound(field.to_string()));
            }
        }
        self.values.insert(Arc::from(field), value);
        Ok(())
    }

    /// Check if a field has a value set.
    pub fn has_value(&self, field: &str) -> bool {
        self.values.contains_key(field)
    }

    /// Clear a field value.
    pub fn clear_value(&mut self, field: &str) {
        self.values.remove(field);
    }

    /// Clear all field values.
    pub fn clear_all(&mut self) {
        self.values.clear();
    }

    /// Iterate over all field values.
    pub fn iter_fields(&self) -> impl Iterator<Item = (&str, &DynamicValue)> {
        self.values.iter().map(|(k, v)| (k.as_ref(), v))
    }

    /// Get all key field values.
    pub fn get_key_values(&self) -> Vec<(&str, &DynamicValue)> {
        let key_members = self.dynamic_type.key_members();
        key_members
            .iter()
            .filter_map(|m| self.values.get(&*m.name).map(|v| (m.name.as_ref(), v)))
            .collect()
    }

    /// Get the number of set values.
    pub fn value_count(&self) -> usize {
        self.values.len()
    }

    /// Get all values as a HashMap reference.
    pub fn values(&self) -> &HashMap<Arc<str>, DynamicValue> {
        &self.values
    }

    /// Take ownership of all values.
    pub fn into_values(self) -> HashMap<Arc<str>, DynamicValue> {
        self.values
    }
}

/// Represents any DDS value at runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicValue {
    Boolean(bool),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Uint8(u8),
    Uint16(u16),
    Uint32(u32),
    Uint64(u64),
    Float32(f32),
    Float64(f64),
    Char8(char),
    Byte(u8),
    /// UTF-8 string
    String(String),
    /// Wide string
    WString(String),
    /// Enum value with name and numeric value
    Enum {
        name: String,
        value: i32,
    },
    /// Union value: the discriminator and the selected branch value
    Union {
        discriminator: Box<DynamicValue>,
        value: Box<DynamicValue>,
    },
    /// Bitmask value (packed set of flag bits)
    Bitmask(u64),
    /// Bitset value (packed named bitfields)
    Bitset(u64),
    /// Nested struct
    Struct(Box<DynamicData>),
    /// Sequence (dynamic array)
    Sequence(Vec<DynamicValue>),
    /// Fixed-size array
    Array(Vec<DynamicValue>),
    /// Map (ordered key-value pairs)
    Map(Vec<(DynamicValue, DynamicValue)>),
    /// Optional value
    Optional(Option<Box<DynamicValue>>),
    /// Null/unset value
    Null,
}

impl DynamicValue {
    /// Get the type kind of this value.
    pub fn type_kind(&self) -> &'static str {
        match self {
            DynamicValue::Boolean(_) => "Boolean",
            DynamicValue::Int8(_) => "Int8",
            DynamicValue::Int16(_) => "Int16",
            DynamicValue::Int32(_) => "Int32",
            DynamicValue::Int64(_) => "Int64",
            DynamicValue::Uint8(_) => "Uint8",
            DynamicValue::Uint16(_) => "Uint16",
            DynamicValue::Uint32(_) => "Uint32",
            DynamicValue::Uint64(_) => "Uint64",
            DynamicValue::Float32(_) => "Float32",
            DynamicValue::Float64(_) => "Float64",
            DynamicValue::Char8(_) => "Char8",
            DynamicValue::Byte(_) => "Byte",
            DynamicValue::String(_) => "String",
            DynamicValue::WString(_) => "WString",
            DynamicValue::Enum { .. } => "Enum",
            DynamicValue::Union { .. } => "Union",
            DynamicValue::Bitmask(_) => "Bitmask",
            DynamicValue::Bitset(_) => "Bitset",
            DynamicValue::Struct(_) => "Struct",
            DynamicValue::Sequence(_) => "Sequence",
            DynamicValue::Array(_) => "Array",
            DynamicValue::Map(_) => "Map",
            DynamicValue::Optional(_) => "Optional",
            DynamicValue::Null => "Null",
        }
    }

    /// Check if this is a primitive value.
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            DynamicValue::Boolean(_)
                | DynamicValue::Int8(_)
                | DynamicValue::Int16(_)
                | DynamicValue::Int32(_)
                | DynamicValue::Int64(_)
                | DynamicValue::Uint8(_)
                | DynamicValue::Uint16(_)
                | DynamicValue::Uint32(_)
                | DynamicValue::Uint64(_)
                | DynamicValue::Float32(_)
                | DynamicValue::Float64(_)
                | DynamicValue::Char8(_)
                | DynamicValue::Byte(_)
        )
    }

    /// Check if this is a null value.
    pub fn is_null(&self) -> bool {
        matches!(self, DynamicValue::Null)
    }

    /// Create a default value for a given type kind.
    pub fn default_for_kind(kind: &DynamicTypeKind) -> Self {
        match kind {
            DynamicTypeKind::Primitive(p) => match p {
                PrimitiveKind::Boolean => DynamicValue::Boolean(false),
                PrimitiveKind::Int8 => DynamicValue::Int8(0),
                PrimitiveKind::Int16 => DynamicValue::Int16(0),
                PrimitiveKind::Int32 => DynamicValue::Int32(0),
                PrimitiveKind::Int64 => DynamicValue::Int64(0),
                PrimitiveKind::Uint8 => DynamicValue::Uint8(0),
                PrimitiveKind::Uint16 => DynamicValue::Uint16(0),
                PrimitiveKind::Uint32 => DynamicValue::Uint32(0),
                PrimitiveKind::Uint64 => DynamicValue::Uint64(0),
                PrimitiveKind::Float32 => DynamicValue::Float32(0.0),
                PrimitiveKind::Float64 => DynamicValue::Float64(0.0),
                PrimitiveKind::Float128 => DynamicValue::Float64(0.0), // Approximate
                PrimitiveKind::Char8 => DynamicValue::Char8('\0'),
                PrimitiveKind::Char16 => DynamicValue::Char8('\0'),
                PrimitiveKind::Byte => DynamicValue::Byte(0),
            },
            DynamicTypeKind::String { .. } => DynamicValue::String(String::new()),
            DynamicTypeKind::WString { .. } => DynamicValue::WString(String::new()),
            DynamicTypeKind::Sequence { .. } => DynamicValue::Sequence(Vec::new()),
            DynamicTypeKind::Map { .. } => DynamicValue::Map(Vec::new()),
            DynamicTypeKind::Array { dimensions, element_type } => {
                let total_size: u32 = dimensions.iter().product();
                let default_element = Self::default_for_kind(element_type);
                DynamicValue::Array(vec![default_element; total_size as usize])
            }
            DynamicTypeKind::Struct(_) => DynamicValue::Null, // Cannot create without type info
            DynamicTypeKind::Enum(_) => DynamicValue::Enum { name: String::new(), value: 0 },
            DynamicTypeKind::Union(_) => DynamicValue::Null, // Cannot create without a selection
            DynamicTypeKind::Bitmask(_) => DynamicValue::Bitmask(0),
            DynamicTypeKind::Bitset(_) => DynamicValue::Bitset(0),
            DynamicTypeKind::ExternalType { .. } => DynamicValue::Null,
            DynamicTypeKind::TypeRef(inner) => match inner.kind() {
                DynamicTypeKind::Enum(_) => DynamicValue::Enum { name: String::new(), value: 0 },
                DynamicTypeKind::Bitmask(_) => DynamicValue::Bitmask(0),
                DynamicTypeKind::Bitset(_) => DynamicValue::Bitset(0),
                _ => DynamicValue::Null,
            },
        }
    }
}

impl fmt::Display for DynamicValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DynamicValue::Boolean(v) => write!(f, "{}", v),
            DynamicValue::Int8(v) => write!(f, "{}", v),
            DynamicValue::Int16(v) => write!(f, "{}", v),
            DynamicValue::Int32(v) => write!(f, "{}", v),
            DynamicValue::Int64(v) => write!(f, "{}", v),
            DynamicValue::Uint8(v) => write!(f, "{}", v),
            DynamicValue::Uint16(v) => write!(f, "{}", v),
            DynamicValue::Uint32(v) => write!(f, "{}", v),
            DynamicValue::Uint64(v) => write!(f, "{}", v),
            DynamicValue::Float32(v) => write!(f, "{}", v),
            DynamicValue::Float64(v) => write!(f, "{}", v),
            DynamicValue::Char8(v) => write!(f, "'{}'", v),
            DynamicValue::Byte(v) => write!(f, "0x{:02x}", v),
            DynamicValue::String(v) => write!(f, "\"{}\"", v),
            DynamicValue::WString(v) => write!(f, "L\"{}\"", v),
            DynamicValue::Enum { name, value } => write!(f, "{}({})", name, value),
            DynamicValue::Union { discriminator, value } => {
                write!(f, "union({} => {})", discriminator, value)
            }
            DynamicValue::Bitmask(bits) => write!(f, "bitmask(0x{:x})", bits),
            DynamicValue::Bitset(bits) => write!(f, "bitset(0x{:x})", bits),
            DynamicValue::Struct(data) => write!(f, "{} {{ ... }}", data.type_name()),
            DynamicValue::Sequence(items) => write!(f, "[{} items]", items.len()),
            DynamicValue::Array(items) => write!(f, "[{} items]", items.len()),
            DynamicValue::Map(entries) => write!(f, "{{{} entries}}", entries.len()),
            DynamicValue::Optional(Some(v)) => write!(f, "Some({})", v),
            DynamicValue::Optional(None) => write!(f, "None"),
            DynamicValue::Null => write!(f, "null"),
        }
    }
}

/// Trait for types that can be converted from DynamicValue.
pub trait FromDynamicValue: Sized {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError>;
}

/// Trait for types that can be converted to DynamicValue.
pub trait IntoDynamicValue {
    fn into_dynamic(self) -> DynamicValue;
}

impl FromDynamicValue for bool {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Boolean(v) => Ok(*v),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to bool",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for i8 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Int8(v) => Ok(*v),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to i8",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for i16 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Int16(v) => Ok(*v),
            DynamicValue::Int8(v) => Ok(*v as i16),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to i16",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for i32 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Int32(v) => Ok(*v),
            DynamicValue::Int16(v) => Ok(*v as i32),
            DynamicValue::Int8(v) => Ok(*v as i32),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to i32",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for i64 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Int64(v) => Ok(*v),
            DynamicValue::Int32(v) => Ok(*v as i64),
            DynamicValue::Int16(v) => Ok(*v as i64),
            DynamicValue::Int8(v) => Ok(*v as i64),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to i64",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for u8 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Uint8(v) => Ok(*v),
            DynamicValue::Byte(v) => Ok(*v),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to u8",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for u16 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Uint16(v) => Ok(*v),
            DynamicValue::Uint8(v) => Ok(*v as u16),
            DynamicValue::Byte(v) => Ok(*v as u16),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to u16",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for u32 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Uint32(v) => Ok(*v),
            DynamicValue::Uint16(v) => Ok(*v as u32),
            DynamicValue::Uint8(v) => Ok(*v as u32),
            DynamicValue::Byte(v) => Ok(*v as u32),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to u32",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for u64 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Uint64(v) => Ok(*v),
            DynamicValue::Uint32(v) => Ok(*v as u64),
            DynamicValue::Uint16(v) => Ok(*v as u64),
            DynamicValue::Uint8(v) => Ok(*v as u64),
            DynamicValue::Byte(v) => Ok(*v as u64),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to u64",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for f32 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Float32(v) => Ok(*v),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to f32",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for f64 {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Float64(v) => Ok(*v),
            DynamicValue::Float32(v) => Ok(*v as f64),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to f64",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for char {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Char8(v) => Ok(*v),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to char",
                value.type_kind()
            ))),
        }
    }
}

impl FromDynamicValue for String {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::String(v) => Ok(v.clone()),
            DynamicValue::WString(v) => Ok(v.clone()),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to String",
                value.type_kind()
            ))),
        }
    }
}

impl<T: FromDynamicValue> FromDynamicValue for Vec<T> {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Sequence(items) | DynamicValue::Array(items) => {
                items.iter().map(T::from_dynamic).collect()
            }
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to Vec",
                value.type_kind()
            ))),
        }
    }
}

impl<K, V> FromDynamicValue for HashMap<K, V>
where
    K: FromDynamicValue + Eq + std::hash::Hash,
    V: FromDynamicValue,
{
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Map(entries) => entries
                .iter()
                .map(|(k, v)| Ok((K::from_dynamic(k)?, V::from_dynamic(v)?)))
                .collect(),
            _ => Err(DynamicTypeError::ConversionError(format!(
                "Cannot convert {} to HashMap",
                value.type_kind()
            ))),
        }
    }
}

impl<T: FromDynamicValue> FromDynamicValue for Option<T> {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        match value {
            DynamicValue::Optional(opt) => match opt {
                Some(v) => Ok(Some(T::from_dynamic(v)?)),
                None => Ok(None),
            },
            DynamicValue::Null => Ok(None),
            other => Ok(Some(T::from_dynamic(other)?)),
        }
    }
}

impl FromDynamicValue for DynamicValue {
    fn from_dynamic(value: &DynamicValue) -> Result<Self, DynamicTypeError> {
        Ok(value.clone())
    }
}

impl IntoDynamicValue for bool {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Boolean(self)
    }
}

impl IntoDynamicValue for i8 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Int8(self)
    }
}

impl IntoDynamicValue for i16 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Int16(self)
    }
}

impl IntoDynamicValue for i32 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Int32(self)
    }
}

impl IntoDynamicValue for i64 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Int64(self)
    }
}

impl IntoDynamicValue for u8 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Uint8(self)
    }
}

impl IntoDynamicValue for u16 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Uint16(self)
    }
}

impl IntoDynamicValue for u32 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Uint32(self)
    }
}

impl IntoDynamicValue for u64 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Uint64(self)
    }
}

impl IntoDynamicValue for f32 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Float32(self)
    }
}

impl IntoDynamicValue for f64 {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Float64(self)
    }
}

impl IntoDynamicValue for char {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Char8(self)
    }
}

impl IntoDynamicValue for String {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::String(self)
    }
}

impl IntoDynamicValue for &str {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::String(self.to_string())
    }
}

impl<T: IntoDynamicValue> IntoDynamicValue for Vec<T> {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Sequence(self.into_iter().map(|v| v.into_dynamic()).collect())
    }
}

impl<K, V> IntoDynamicValue for HashMap<K, V>
where
    K: IntoDynamicValue,
    V: IntoDynamicValue,
{
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Map(
            self.into_iter().map(|(k, v)| (k.into_dynamic(), v.into_dynamic())).collect(),
        )
    }
}

impl<T: IntoDynamicValue> IntoDynamicValue for Option<T> {
    fn into_dynamic(self) -> DynamicValue {
        match self {
            Some(v) => DynamicValue::Optional(Some(Box::new(v.into_dynamic()))),
            None => DynamicValue::Optional(None),
        }
    }
}

impl IntoDynamicValue for DynamicValue {
    fn into_dynamic(self) -> DynamicValue {
        self
    }
}

impl IntoDynamicValue for DynamicData {
    fn into_dynamic(self) -> DynamicValue {
        DynamicValue::Struct(Box::new(self))
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
    fn test_dynamic_data_set_get() {
        let dynamic_type = create_test_type();
        let mut data = DynamicData::new(dynamic_type);

        // Set values
        data.set("id", 42i32).unwrap();
        data.set("message", "Hello, World!").unwrap();

        // Get values
        let id: i32 = data.get("id").unwrap();
        let message: String = data.get("message").unwrap();

        assert_eq!(id, 42);
        assert_eq!(message, "Hello, World!");
    }

    #[test]
    fn test_dynamic_data_field_not_found() {
        let dynamic_type = create_test_type();
        let data = DynamicData::new(dynamic_type);

        let result: Result<i32, _> = data.get("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_dynamic_value_display() {
        assert_eq!(format!("{}", DynamicValue::Int32(42)), "42");
        assert_eq!(format!("{}", DynamicValue::String("hello".to_string())), "\"hello\"");
        assert_eq!(format!("{}", DynamicValue::Boolean(true)), "true");
        assert_eq!(format!("{}", DynamicValue::Null), "null");
    }

    #[test]
    fn test_type_conversion() {
        // Widening conversions
        let val = DynamicValue::Int8(10);
        assert_eq!(i16::from_dynamic(&val).unwrap(), 10i16);
        assert_eq!(i32::from_dynamic(&val).unwrap(), 10i32);
        assert_eq!(i64::from_dynamic(&val).unwrap(), 10i64);

        // Vec conversion
        let seq = DynamicValue::Sequence(vec![
            DynamicValue::Int32(1),
            DynamicValue::Int32(2),
            DynamicValue::Int32(3),
        ]);
        let vec: Vec<i32> = Vec::from_dynamic(&seq).unwrap();
        assert_eq!(vec, vec![1, 2, 3]);
    }
}
