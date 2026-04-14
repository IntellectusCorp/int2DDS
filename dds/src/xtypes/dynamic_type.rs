//! DynamicType - Runtime type descriptor from TypeObject.
//!
//! This module provides runtime type information extracted from CompleteTypeObject,
//! enabling dynamic data access without compile-time type knowledge.

use std::collections::HashMap;
use std::sync::Arc;

use crate::serialize::xcdr::ExtensibilityKind;
use crate::xtypes::{
    CompleteEnumeratedType, CompleteStructType, CompleteTypeObject, TypeIdentifier,
};

/// Error type for DynamicType operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicTypeError {
    /// Type conversion failed
    ConversionError(String),
    /// Field not found
    FieldNotFound(String),
    /// Invalid type operation
    InvalidOperation(String),
    /// Unsupported type
    UnsupportedType(String),
}

impl std::fmt::Display for DynamicTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynamicTypeError::ConversionError(msg) => write!(f, "Conversion error: {}", msg),
            DynamicTypeError::FieldNotFound(field) => write!(f, "Field not found: {}", field),
            DynamicTypeError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
            DynamicTypeError::UnsupportedType(msg) => write!(f, "Unsupported type: {}", msg),
        }
    }
}

impl std::error::Error for DynamicTypeError {}

/// Primitive type kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveKind {
    Boolean,
    Byte,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    Float128,
    Char8,
    Char16,
}

impl PrimitiveKind {
    /// Get the size in bytes for this primitive type.
    pub fn size(&self) -> usize {
        match self {
            PrimitiveKind::Boolean
            | PrimitiveKind::Byte
            | PrimitiveKind::Int8
            | PrimitiveKind::Uint8
            | PrimitiveKind::Char8 => 1,
            PrimitiveKind::Int16 | PrimitiveKind::Uint16 | PrimitiveKind::Char16 => 2,
            PrimitiveKind::Int32 | PrimitiveKind::Uint32 | PrimitiveKind::Float32 => 4,
            PrimitiveKind::Int64 | PrimitiveKind::Uint64 | PrimitiveKind::Float64 => 8,
            PrimitiveKind::Float128 => 16,
        }
    }
}

/// Runtime type descriptor - describes the structure of a type at runtime.
#[derive(Debug, Clone)]
pub struct DynamicType {
    /// The type name (e.g., "HelloWorld")
    type_name: String,
    /// The kind of type
    kind: DynamicTypeKind,
    /// Extensibility kind (Final, Appendable, Mutable)
    extensibility: ExtensibilityKind,
    /// TypeIdentifier for this type
    type_identifier: TypeIdentifier,
    /// Original CompleteTypeObject (for serialization context)
    type_object: Arc<CompleteTypeObject>,
}

impl DynamicType {
    /// Create a DynamicType from a CompleteTypeObject.
    pub fn from_type_object(
        type_object: CompleteTypeObject,
        type_identifier: TypeIdentifier,
    ) -> Result<Self, DynamicTypeError> {
        let type_object = Arc::new(type_object);
        Self::from_type_object_arc(type_object, type_identifier)
    }

    /// Create a DynamicType from an Arc<CompleteTypeObject>.
    pub fn from_type_object_arc(
        type_object: Arc<CompleteTypeObject>,
        type_identifier: TypeIdentifier,
    ) -> Result<Self, DynamicTypeError> {
        match type_object.as_ref() {
            CompleteTypeObject::Struct(struct_type) => {
                Self::from_struct_type(struct_type, type_identifier, type_object.clone())
            }
            CompleteTypeObject::Enum(enum_type) => {
                Self::from_enum_type(enum_type, type_identifier, type_object.clone())
            }
            _ => Err(DynamicTypeError::UnsupportedType(format!(
                "TypeObject kind 0x{:02X} not yet supported for DynamicType",
                type_object.discriminator()
            ))),
        }
    }

    fn from_struct_type(
        struct_type: &CompleteStructType,
        type_identifier: TypeIdentifier,
        type_object: Arc<CompleteTypeObject>,
    ) -> Result<Self, DynamicTypeError> {
        let type_name = struct_type.header.detail.type_name.clone();
        let extensibility = Self::convert_extensibility(struct_type.struct_flags.extensibility());

        // Build struct descriptor
        let mut members = Vec::with_capacity(struct_type.member_seq.len());
        let mut member_by_name = HashMap::new();
        let mut member_by_id = HashMap::new();

        for (index, member) in struct_type.member_seq.iter().enumerate() {
            let member_type = Self::type_from_identifier(&member.common.member_type_id)?;
            let descriptor = MemberDescriptor {
                name: Arc::from(member.detail.name.as_str()),
                member_id: member.common.member_id,
                member_type,
                is_key: member.common.member_flags.is_key(),
                is_optional: member.common.member_flags.is_optional(),
                is_must_understand: member.common.member_flags.is_must_understand(),
                index,
            };

            member_by_name.insert(descriptor.name.clone(), index);
            member_by_id.insert(descriptor.member_id, index);
            members.push(descriptor);
        }

        let struct_desc = StructDescriptor { members, member_by_name, member_by_id };

        Ok(Self {
            type_name,
            kind: DynamicTypeKind::Struct(struct_desc),
            extensibility,
            type_identifier,
            type_object,
        })
    }

    fn from_enum_type(
        enum_type: &CompleteEnumeratedType,
        type_identifier: TypeIdentifier,
        type_object: Arc<CompleteTypeObject>,
    ) -> Result<Self, DynamicTypeError> {
        let type_name = enum_type.header.detail.type_name.clone();
        let extensibility = Self::convert_extensibility(enum_type.enum_flags.extensibility());

        // Build enum descriptor
        let mut literals = Vec::with_capacity(enum_type.literal_seq.len());
        let mut literal_by_name = HashMap::new();
        let mut literal_by_value = HashMap::new();

        for (index, literal) in enum_type.literal_seq.iter().enumerate() {
            let descriptor = EnumLiteralDescriptor {
                name: literal.detail.name.clone(),
                value: literal.common.value,
                is_default: literal.common.flags.is_default(),
                index,
            };

            literal_by_name.insert(descriptor.name.clone(), index);
            literal_by_value.insert(descriptor.value, index);
            literals.push(descriptor);
        }

        let bit_bound = enum_type.header.common.bit_bound;
        let enum_desc = EnumDescriptor { literals, literal_by_name, literal_by_value, bit_bound };

        Ok(Self {
            type_name,
            kind: DynamicTypeKind::Enum(enum_desc),
            extensibility,
            type_identifier,
            type_object,
        })
    }

    fn convert_extensibility(ext: crate::xtypes::ExtensibilityKind) -> ExtensibilityKind {
        match ext {
            crate::xtypes::ExtensibilityKind::Final => ExtensibilityKind::Final,
            crate::xtypes::ExtensibilityKind::Appendable => ExtensibilityKind::Appendable,
            crate::xtypes::ExtensibilityKind::Mutable => ExtensibilityKind::Mutable,
        }
    }

    /// Create a DynamicType for a primitive TypeIdentifier.
    fn type_from_identifier(type_id: &TypeIdentifier) -> Result<DynamicTypeKind, DynamicTypeError> {
        match type_id {
            TypeIdentifier::Boolean => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Boolean)),
            TypeIdentifier::Byte => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Byte)),
            TypeIdentifier::Int8 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Int8)),
            TypeIdentifier::Int16 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Int16)),
            TypeIdentifier::Int32 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Int32)),
            TypeIdentifier::Int64 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Int64)),
            TypeIdentifier::Uint8 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Uint8)),
            TypeIdentifier::Uint16 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Uint16)),
            TypeIdentifier::Uint32 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Uint32)),
            TypeIdentifier::Uint64 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Uint64)),
            TypeIdentifier::Float32 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Float32)),
            TypeIdentifier::Float64 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Float64)),
            TypeIdentifier::Float128 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Float128)),
            TypeIdentifier::Char8 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Char8)),
            TypeIdentifier::Char16 => Ok(DynamicTypeKind::Primitive(PrimitiveKind::Char16)),
            TypeIdentifier::String8 => Ok(DynamicTypeKind::String { bound: None }),
            TypeIdentifier::String16 => Ok(DynamicTypeKind::WString { bound: None }),
            TypeIdentifier::String8Small { bound } => {
                Ok(DynamicTypeKind::String { bound: Some(*bound as u32) })
            }
            TypeIdentifier::String8Large { bound } => {
                Ok(DynamicTypeKind::String { bound: Some(*bound) })
            }
            TypeIdentifier::String16Small { bound } => {
                Ok(DynamicTypeKind::WString { bound: Some(*bound as u32) })
            }
            TypeIdentifier::String16Large { bound } => {
                Ok(DynamicTypeKind::WString { bound: Some(*bound) })
            }
            TypeIdentifier::PlainSequenceSmall { element_identifier, bound, .. } => {
                let element_type = Self::type_from_identifier(element_identifier)?;
                Ok(DynamicTypeKind::Sequence {
                    element_type: Box::new(element_type),
                    bound: if *bound == 0 { None } else { Some(*bound as u32) },
                })
            }
            TypeIdentifier::PlainSequenceLarge { element_identifier, bound, .. } => {
                let element_type = Self::type_from_identifier(element_identifier)?;
                Ok(DynamicTypeKind::Sequence {
                    element_type: Box::new(element_type),
                    bound: if *bound == 0 { None } else { Some(*bound) },
                })
            }
            TypeIdentifier::PlainArraySmall { element_identifier, array_bound_seq, .. } => {
                let element_type = Self::type_from_identifier(element_identifier)?;
                Ok(DynamicTypeKind::Array {
                    element_type: Box::new(element_type),
                    dimensions: array_bound_seq.iter().map(|&d| d as u32).collect(),
                })
            }
            TypeIdentifier::PlainArrayLarge { element_identifier, array_bound_seq, .. } => {
                let element_type = Self::type_from_identifier(element_identifier)?;
                Ok(DynamicTypeKind::Array {
                    element_type: Box::new(element_type),
                    dimensions: array_bound_seq.clone(),
                })
            }
            TypeIdentifier::CompleteTypeId(_) => {
                // This references another type by hash - return as external reference
                Ok(DynamicTypeKind::ExternalType { type_identifier: type_id.clone() })
            }
            TypeIdentifier::MinimalTypeId(_) => {
                Ok(DynamicTypeKind::ExternalType { type_identifier: type_id.clone() })
            }
            TypeIdentifier::None => {
                Err(DynamicTypeError::UnsupportedType("None type identifier".to_string()))
            }
            _ => Err(DynamicTypeError::UnsupportedType(format!(
                "Unsupported TypeIdentifier: {:?}",
                type_id
            ))),
        }
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Get the type name.
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// Get the type kind.
    pub fn kind(&self) -> &DynamicTypeKind {
        &self.kind
    }

    /// Get the extensibility kind.
    pub fn extensibility(&self) -> ExtensibilityKind {
        self.extensibility
    }

    /// Get the TypeIdentifier.
    pub fn type_identifier(&self) -> &TypeIdentifier {
        &self.type_identifier
    }

    /// Get the original CompleteTypeObject.
    pub fn type_object(&self) -> &Arc<CompleteTypeObject> {
        &self.type_object
    }

    /// Check if this is a struct type.
    pub fn is_struct(&self) -> bool {
        matches!(self.kind, DynamicTypeKind::Struct(_))
    }

    /// Check if this is an enum type.
    pub fn is_enum(&self) -> bool {
        matches!(self.kind, DynamicTypeKind::Enum(_))
    }

    /// Check if this is a primitive type.
    pub fn is_primitive(&self) -> bool {
        matches!(self.kind, DynamicTypeKind::Primitive(_))
    }

    /// Get struct descriptor (if this is a struct type).
    pub fn as_struct(&self) -> Option<&StructDescriptor> {
        match &self.kind {
            DynamicTypeKind::Struct(desc) => Some(desc),
            _ => None,
        }
    }

    /// Get enum descriptor (if this is an enum type).
    pub fn as_enum(&self) -> Option<&EnumDescriptor> {
        match &self.kind {
            DynamicTypeKind::Enum(desc) => Some(desc),
            _ => None,
        }
    }

    /// Get a member by name (for struct types).
    pub fn get_member(&self, name: &str) -> Option<&MemberDescriptor> {
        self.as_struct().and_then(|s| s.get_member(name))
    }

    /// Get a member by ID (for struct types).
    pub fn get_member_by_id(&self, member_id: u32) -> Option<&MemberDescriptor> {
        self.as_struct().and_then(|s| s.get_member_by_id(member_id))
    }

    /// Get all members (for struct types).
    pub fn members(&self) -> Option<&[MemberDescriptor]> {
        self.as_struct().map(|s| s.members())
    }

    /// Get key members (for struct types).
    pub fn key_members(&self) -> Vec<&MemberDescriptor> {
        self.as_struct().map(|s| s.key_members()).unwrap_or_default()
    }

    /// Get an enum literal by name.
    pub fn get_literal(&self, name: &str) -> Option<&EnumLiteralDescriptor> {
        self.as_enum().and_then(|e| e.get_literal(name))
    }

    /// Get an enum literal by value.
    pub fn get_literal_by_value(&self, value: i32) -> Option<&EnumLiteralDescriptor> {
        self.as_enum().and_then(|e| e.get_literal_by_value(value))
    }
}

/// The kind of a DynamicType.
#[derive(Debug, Clone)]
pub enum DynamicTypeKind {
    /// Primitive type (boolean, integers, floats, char)
    Primitive(PrimitiveKind),
    /// String type (UTF-8)
    String { bound: Option<u32> },
    /// Wide string type (UTF-16)
    WString { bound: Option<u32> },
    /// Struct type with member descriptors
    Struct(StructDescriptor),
    /// Enum type with literal descriptors
    Enum(EnumDescriptor),
    /// Sequence (dynamic array)
    Sequence { element_type: Box<DynamicTypeKind>, bound: Option<u32> },
    /// Array (fixed-size)
    Array { element_type: Box<DynamicTypeKind>, dimensions: Vec<u32> },
    /// Reference to an external type by TypeIdentifier (for nested structs)
    ExternalType { type_identifier: TypeIdentifier },
}

/// Descriptor for struct type members.
#[derive(Debug, Clone)]
pub struct StructDescriptor {
    /// Members in declaration order
    members: Vec<MemberDescriptor>,
    /// Map from member name to index
    member_by_name: HashMap<Arc<str>, usize>,
    /// Map from member ID to index
    member_by_id: HashMap<u32, usize>,
}

impl StructDescriptor {
    /// Get a member by name.
    pub fn get_member(&self, name: &str) -> Option<&MemberDescriptor> {
        self.member_by_name.get(name).map(|&idx| &self.members[idx])
    }

    /// Get a member by ID.
    pub fn get_member_by_id(&self, member_id: u32) -> Option<&MemberDescriptor> {
        self.member_by_id.get(&member_id).map(|&idx| &self.members[idx])
    }

    /// Get all members in declaration order.
    pub fn members(&self) -> &[MemberDescriptor] {
        &self.members
    }

    /// Get key members.
    pub fn key_members(&self) -> Vec<&MemberDescriptor> {
        self.members.iter().filter(|m| m.is_key).collect()
    }

    /// Get the number of members.
    pub fn member_count(&self) -> usize {
        self.members.len()
    }
}

/// Descriptor for a single struct member.
#[derive(Debug, Clone)]
pub struct MemberDescriptor {
    /// Member name (shared via Arc so cloning is a refcount bump)
    pub name: Arc<str>,
    /// Member ID (used in MUTABLE types)
    pub member_id: u32,
    /// Member type kind
    pub member_type: DynamicTypeKind,
    /// Whether this is a key field
    pub is_key: bool,
    /// Whether this field is optional
    pub is_optional: bool,
    /// Whether this field must be understood
    pub is_must_understand: bool,
    /// Index in the member sequence
    pub index: usize,
}

/// Descriptor for enum types.
#[derive(Debug, Clone)]
pub struct EnumDescriptor {
    /// Literals in declaration order
    literals: Vec<EnumLiteralDescriptor>,
    /// Map from literal name to index
    literal_by_name: HashMap<String, usize>,
    /// Map from literal value to index
    literal_by_value: HashMap<i32, usize>,
    /// Bit bound for the enum discriminant
    bit_bound: u16,
}

impl EnumDescriptor {
    /// Get a literal by name.
    pub fn get_literal(&self, name: &str) -> Option<&EnumLiteralDescriptor> {
        self.literal_by_name.get(name).map(|&idx| &self.literals[idx])
    }

    /// Get a literal by value.
    pub fn get_literal_by_value(&self, value: i32) -> Option<&EnumLiteralDescriptor> {
        self.literal_by_value.get(&value).map(|&idx| &self.literals[idx])
    }

    /// Get all literals in declaration order.
    pub fn literals(&self) -> &[EnumLiteralDescriptor] {
        &self.literals
    }

    /// Get the default literal (if any).
    pub fn default_literal(&self) -> Option<&EnumLiteralDescriptor> {
        self.literals.iter().find(|l| l.is_default)
    }

    /// Get the bit bound for the discriminant.
    pub fn bit_bound(&self) -> u16 {
        self.bit_bound
    }

    /// Get the number of literals.
    pub fn literal_count(&self) -> usize {
        self.literals.len()
    }
}

/// Descriptor for an enum literal.
#[derive(Debug, Clone)]
pub struct EnumLiteralDescriptor {
    /// Literal name
    pub name: String,
    /// Literal value
    pub value: i32,
    /// Whether this is the default value
    pub is_default: bool,
    /// Index in the literal sequence
    pub index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xtypes::{CompleteStructMember, MemberFlag, TypeFlag};

    #[test]
    fn test_dynamic_type_from_struct() {
        // Create a simple struct type
        let mut struct_type = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Final, false, false),
            "TestStruct".to_string(),
            None,
        );

        // Add members
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
        let type_id = TypeIdentifier::Int32; // Placeholder

        let dynamic_type = DynamicType::from_type_object(type_object, type_id).unwrap();

        assert_eq!(dynamic_type.type_name(), "TestStruct");
        assert!(dynamic_type.is_struct());
        assert_eq!(dynamic_type.extensibility(), ExtensibilityKind::Final);

        let members = dynamic_type.members().unwrap();
        assert_eq!(members.len(), 2);

        let id_member = dynamic_type.get_member("id").unwrap();
        assert_eq!(&*id_member.name, "id");
        assert!(id_member.is_key);
        assert!(matches!(id_member.member_type, DynamicTypeKind::Primitive(PrimitiveKind::Int32)));

        let msg_member = dynamic_type.get_member("message").unwrap();
        assert_eq!(&*msg_member.name, "message");
        assert!(!msg_member.is_key);
        assert!(matches!(msg_member.member_type, DynamicTypeKind::String { bound: None }));
    }

    #[test]
    fn test_dynamic_type_key_members() {
        let mut struct_type = CompleteStructType::new(
            TypeFlag::new(crate::xtypes::ExtensibilityKind::Mutable, false, false),
            "KeyedStruct".to_string(),
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
            "key1".to_string(),
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
            TypeIdentifier::Float64,
            "value".to_string(),
        ));
        struct_type.add_member(CompleteStructMember::new(
            2,
            MemberFlag::new(
                crate::xtypes::TryConstructKind::Discard,
                false,
                false,
                false,
                true,
                false,
            ),
            TypeIdentifier::String8,
            "key2".to_string(),
        ));

        let type_object = CompleteTypeObject::Struct(struct_type);
        let dynamic_type =
            DynamicType::from_type_object(type_object, TypeIdentifier::None).unwrap();

        let key_members = dynamic_type.key_members();
        assert_eq!(key_members.len(), 2);
        assert_eq!(&*key_members[0].name, "key1");
        assert_eq!(&*key_members[1].name, "key2");
    }
}
