//! DynamicType - Runtime type descriptor from TypeObject.
//!
//! This module provides runtime type information extracted from CompleteTypeObject,
//! enabling dynamic data access without compile-time type knowledge.

use std::collections::HashMap;
use std::sync::Arc;

use crate::serialize::xcdr::ExtensibilityKind;
use crate::xtypes::type_object::EquivalenceHash;
use crate::xtypes::type_registry::TypeRegistry;
use crate::xtypes::{
    CompleteBitmaskType, CompleteBitsetType, CompleteEnumeratedType, CompleteStructType,
    CompleteTypeObject, CompleteUnionType, TypeIdentifier,
};

/// Maximum nested type-resolution depth. A remote peer controls the
/// type-dependency chain via TypeLookup; a long linear (acyclic) chain would
/// otherwise recurse until the stack overflows. Beyond this depth the nested
/// type is left unresolved instead of recursing further.
const MAX_BUILD_DEPTH: usize = 64;

struct BuildCtx<'a> {
    registry: &'a TypeRegistry,
    memo: HashMap<EquivalenceHash, Arc<DynamicType>>,
    stack: Vec<EquivalenceHash>,
}

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
        Self::build_from_object(type_object, type_identifier, None)
    }

    pub fn from_type_object_with_registry(
        type_object: Arc<CompleteTypeObject>,
        type_identifier: TypeIdentifier,
        registry: &TypeRegistry,
    ) -> Result<Self, DynamicTypeError> {
        let mut ctx = BuildCtx { registry, memo: HashMap::new(), stack: Vec::new() };
        Self::build_from_object(type_object, type_identifier, Some(&mut ctx))
    }

    fn build_from_object(
        type_object: Arc<CompleteTypeObject>,
        type_identifier: TypeIdentifier,
        ctx: Option<&mut BuildCtx>,
    ) -> Result<Self, DynamicTypeError> {
        match type_object.as_ref() {
            CompleteTypeObject::Struct(struct_type) => {
                Self::from_struct_type(struct_type, type_identifier, type_object.clone(), ctx)
            }
            CompleteTypeObject::Enum(enum_type) => {
                Self::from_enum_type(enum_type, type_identifier, type_object.clone())
            }
            CompleteTypeObject::Union(union_type) => {
                Self::from_union_type(union_type, type_identifier, type_object.clone(), ctx)
            }
            CompleteTypeObject::Bitmask(bitmask_type) => {
                Self::from_bitmask_type(bitmask_type, type_identifier, type_object.clone())
            }
            CompleteTypeObject::Bitset(bitset_type) => {
                Self::from_bitset_type(bitset_type, type_identifier, type_object.clone())
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
        mut ctx: Option<&mut BuildCtx>,
    ) -> Result<Self, DynamicTypeError> {
        let type_name = struct_type.header.detail.type_name.clone();
        let extensibility = Self::convert_extensibility(struct_type.struct_flags.extensibility());

        // Build struct descriptor
        let mut members = Vec::with_capacity(struct_type.member_seq.len());
        let mut member_by_name = HashMap::new();
        let mut member_by_id = HashMap::new();

        // Inherited base members come first (XTypes: parent fields precede child).
        // Skip when a member already references the base type — derive embeds the
        // parent as a named member, so flattening there would duplicate fields.
        if let Some(base_id) = &struct_type.header.base_type {
            let already_embedded =
                struct_type.member_seq.iter().any(|m| &m.common.member_type_id == base_id);
            if !already_embedded {
                let base_kind = Self::type_from_identifier(base_id, ctx.as_deref_mut())?;
                if let Some(base_members) = Self::struct_members_of(&base_kind) {
                    for bm in base_members {
                        let index = members.len();
                        member_by_name.insert(bm.name.clone(), index);
                        member_by_id.insert(bm.member_id, index);
                        members.push(MemberDescriptor { index, ..bm.clone() });
                    }
                }
            }
        }

        for member in struct_type.member_seq.iter() {
            let member_type =
                Self::type_from_identifier(&member.common.member_type_id, ctx.as_deref_mut())?;
            let index = members.len();
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

    fn from_union_type(
        union_type: &CompleteUnionType,
        type_identifier: TypeIdentifier,
        type_object: Arc<CompleteTypeObject>,
        mut ctx: Option<&mut BuildCtx>,
    ) -> Result<Self, DynamicTypeError> {
        let type_name = union_type.header.type_name.clone();
        let extensibility = Self::convert_extensibility(union_type.union_flags.extensibility());

        let discriminator_type = Box::new(Self::type_from_identifier(
            &union_type.discriminator.type_id,
            ctx.as_deref_mut(),
        )?);

        let mut members = Vec::with_capacity(union_type.member_seq.len());
        let mut member_by_id = HashMap::new();
        for (index, member) in union_type.member_seq.iter().enumerate() {
            let member_type =
                Self::type_from_identifier(&member.common.member_type_id, ctx.as_deref_mut())?;
            let descriptor = UnionMemberDescriptor {
                name: Arc::from(member.detail.name.as_str()),
                member_id: member.common.member_id,
                member_type,
                labels: member.common.label_seq.clone(),
                is_default: member.common.member_flags.is_default(),
                is_must_understand: member.common.member_flags.is_must_understand(),
                index,
            };
            member_by_id.insert(descriptor.member_id, index);
            members.push(descriptor);
        }

        let union_desc = UnionDescriptor { discriminator_type, members, member_by_id };

        Ok(Self {
            type_name,
            kind: DynamicTypeKind::Union(union_desc),
            extensibility,
            type_identifier,
            type_object,
        })
    }

    fn from_bitmask_type(
        bitmask_type: &CompleteBitmaskType,
        type_identifier: TypeIdentifier,
        type_object: Arc<CompleteTypeObject>,
    ) -> Result<Self, DynamicTypeError> {
        let type_name = bitmask_type.header.detail.type_name.clone();
        let extensibility = Self::convert_extensibility(bitmask_type.bitmask_flags.extensibility());
        let bit_bound = bitmask_type.header.common.bit_bound;

        let flags = bitmask_type
            .flag_seq
            .iter()
            .map(|flag| BitflagDescriptor {
                name: Arc::from(flag.detail.name.as_str()),
                position: flag.common.position,
            })
            .collect();

        Ok(Self {
            type_name,
            kind: DynamicTypeKind::Bitmask(BitmaskDescriptor { bit_bound, flags }),
            extensibility,
            type_identifier,
            type_object,
        })
    }

    fn from_bitset_type(
        bitset_type: &CompleteBitsetType,
        type_identifier: TypeIdentifier,
        type_object: Arc<CompleteTypeObject>,
    ) -> Result<Self, DynamicTypeError> {
        let type_name = bitset_type.header.type_name.clone();
        let extensibility = Self::convert_extensibility(bitset_type.bitset_flags.extensibility());

        let mut fields = Vec::with_capacity(bitset_type.field_seq.len());
        let mut total_bits = 0u16;
        for field in &bitset_type.field_seq {
            total_bits = total_bits.max(field.common.position + field.common.bitcount as u16);
            fields.push(BitfieldDescriptor {
                name: Arc::from(field.detail.name.as_str()),
                position: field.common.position,
                bitcount: field.common.bitcount,
            });
        }

        Ok(Self {
            type_name,
            kind: DynamicTypeKind::Bitset(BitsetDescriptor { fields, total_bits }),
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
    fn type_from_identifier(
        type_id: &TypeIdentifier,
        mut ctx: Option<&mut BuildCtx>,
    ) -> Result<DynamicTypeKind, DynamicTypeError> {
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
                let element_type = Self::type_from_identifier(element_identifier, ctx)?;
                Ok(DynamicTypeKind::Sequence {
                    element_type: Box::new(element_type),
                    bound: if *bound == 0 { None } else { Some(*bound as u32) },
                })
            }
            TypeIdentifier::PlainSequenceLarge { element_identifier, bound, .. } => {
                let element_type = Self::type_from_identifier(element_identifier, ctx)?;
                Ok(DynamicTypeKind::Sequence {
                    element_type: Box::new(element_type),
                    bound: if *bound == 0 { None } else { Some(*bound) },
                })
            }
            TypeIdentifier::PlainArraySmall { element_identifier, array_bound_seq, .. } => {
                let element_type = Self::type_from_identifier(element_identifier, ctx)?;
                Ok(DynamicTypeKind::Array {
                    element_type: Box::new(element_type),
                    dimensions: array_bound_seq.iter().map(|&d| d as u32).collect(),
                })
            }
            TypeIdentifier::PlainArrayLarge { element_identifier, array_bound_seq, .. } => {
                let element_type = Self::type_from_identifier(element_identifier, ctx)?;
                Ok(DynamicTypeKind::Array {
                    element_type: Box::new(element_type),
                    dimensions: array_bound_seq.clone(),
                })
            }
            TypeIdentifier::PlainMapSmall { key_identifier, element_identifier, bound, .. } => {
                let key_type = Self::type_from_identifier(key_identifier, ctx.as_deref_mut())?;
                let value_type = Self::type_from_identifier(element_identifier, ctx)?;
                Ok(DynamicTypeKind::Map {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                    bound: if *bound == 0 { None } else { Some(*bound as u32) },
                })
            }
            TypeIdentifier::PlainMapLarge { key_identifier, element_identifier, bound, .. } => {
                let key_type = Self::type_from_identifier(key_identifier, ctx.as_deref_mut())?;
                let value_type = Self::type_from_identifier(element_identifier, ctx)?;
                Ok(DynamicTypeKind::Map {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                    bound: if *bound == 0 { None } else { Some(*bound) },
                })
            }
            TypeIdentifier::CompleteTypeId(hash) | TypeIdentifier::MinimalTypeId(hash) => {
                Self::resolve_nested(type_id, hash, ctx.as_deref_mut())
            }
            TypeIdentifier::None => {
                Err(DynamicTypeError::UnsupportedType("None type identifier".to_string()))
            }
        }
    }

    fn resolve_nested(
        type_id: &TypeIdentifier,
        hash: &EquivalenceHash,
        ctx: Option<&mut BuildCtx>,
    ) -> Result<DynamicTypeKind, DynamicTypeError> {
        let ctx = match ctx {
            Some(ctx) => ctx,
            None => return Ok(DynamicTypeKind::ExternalType { type_identifier: type_id.clone() }),
        };

        if let Some(existing) = ctx.memo.get(hash) {
            return Ok(DynamicTypeKind::TypeRef(existing.clone()));
        }
        if ctx.stack.contains(hash) {
            // Cyclic back-edge: keep unresolved to terminate recursion.
            return Ok(DynamicTypeKind::ExternalType { type_identifier: type_id.clone() });
        }
        if ctx.stack.len() >= MAX_BUILD_DEPTH {
            return Ok(DynamicTypeKind::ExternalType { type_identifier: type_id.clone() });
        }

        let object = match ctx.registry.lookup_complete(hash) {
            Some(object) => Arc::new(object.clone()),
            None => return Ok(DynamicTypeKind::ExternalType { type_identifier: type_id.clone() }),
        };

        ctx.stack.push(*hash);
        let nested = Self::build_from_object(object, type_id.clone(), Some(&mut *ctx));
        ctx.stack.pop();
        let nested = Arc::new(nested?);
        ctx.memo.insert(*hash, nested.clone());
        Ok(DynamicTypeKind::TypeRef(nested))
    }

    /// Resolve a base type's struct members (following a `TypeRef`), for
    /// flattening inherited members. Returns None when the base could not be
    /// resolved to a struct.
    fn struct_members_of(kind: &DynamicTypeKind) -> Option<&[MemberDescriptor]> {
        let resolved = match kind {
            DynamicTypeKind::TypeRef(arc) => arc.kind(),
            other => other,
        };
        match resolved {
            DynamicTypeKind::Struct(desc) => Some(desc.members()),
            _ => None,
        }
    }

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

    /// Get union descriptor (if this is a union type).
    pub fn as_union(&self) -> Option<&UnionDescriptor> {
        match &self.kind {
            DynamicTypeKind::Union(desc) => Some(desc),
            _ => None,
        }
    }

    /// Get bitmask descriptor (if this is a bitmask type).
    pub fn as_bitmask(&self) -> Option<&BitmaskDescriptor> {
        match &self.kind {
            DynamicTypeKind::Bitmask(desc) => Some(desc),
            _ => None,
        }
    }

    /// Get bitset descriptor (if this is a bitset type).
    pub fn as_bitset(&self) -> Option<&BitsetDescriptor> {
        match &self.kind {
            DynamicTypeKind::Bitset(desc) => Some(desc),
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
    /// Union type with a discriminator and case members
    Union(UnionDescriptor),
    /// Bitmask type (serialized as a packed unsigned integer)
    Bitmask(BitmaskDescriptor),
    /// Bitset type (named bitfields packed into an unsigned integer)
    Bitset(BitsetDescriptor),
    /// Sequence (dynamic array)
    Sequence { element_type: Box<DynamicTypeKind>, bound: Option<u32> },
    /// Array (fixed-size)
    Array { element_type: Box<DynamicTypeKind>, dimensions: Vec<u32> },
    /// Map (key-value collection)
    Map { key_type: Box<DynamicTypeKind>, value_type: Box<DynamicTypeKind>, bound: Option<u32> },
    /// Reference to an external type by TypeIdentifier (for nested structs)
    ExternalType { type_identifier: TypeIdentifier },
    /// Resolved nested composite type holding full metadata
    TypeRef(Arc<DynamicType>),
}

impl DynamicTypeKind {
    /// Get the resolved nested type (if this is a `TypeRef`).
    pub fn as_type_ref(&self) -> Option<&Arc<DynamicType>> {
        match self {
            DynamicTypeKind::TypeRef(t) => Some(t),
            _ => None,
        }
    }
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

/// Descriptor for union types.
#[derive(Debug, Clone)]
pub struct UnionDescriptor {
    /// Type of the discriminator (primitive or enum)
    discriminator_type: Box<DynamicTypeKind>,
    /// Case members in declaration order
    members: Vec<UnionMemberDescriptor>,
    /// Map from member ID to index
    member_by_id: HashMap<u32, usize>,
}

impl UnionDescriptor {
    /// Get the discriminator type kind.
    pub fn discriminator_type(&self) -> &DynamicTypeKind {
        &self.discriminator_type
    }

    /// Get all case members in declaration order.
    pub fn members(&self) -> &[UnionMemberDescriptor] {
        &self.members
    }

    /// Get a case member by ID.
    pub fn get_member_by_id(&self, member_id: u32) -> Option<&UnionMemberDescriptor> {
        self.member_by_id.get(&member_id).map(|&idx| &self.members[idx])
    }

    /// Resolve the selected case member for a discriminator value: the member
    /// whose label set contains `discriminator`, else the default member.
    pub fn select_member(&self, discriminator: i64) -> Option<&UnionMemberDescriptor> {
        self.members
            .iter()
            .find(|m| m.labels.iter().any(|&l| i64::from(l) == discriminator))
            .or_else(|| self.members.iter().find(|m| m.is_default))
    }
}

/// Descriptor for a single union case member.
#[derive(Debug, Clone)]
pub struct UnionMemberDescriptor {
    /// Member name
    pub name: Arc<str>,
    /// Member ID
    pub member_id: u32,
    /// Member type kind
    pub member_type: DynamicTypeKind,
    /// Case labels selecting this member
    pub labels: Vec<i32>,
    /// Whether this is the default case
    pub is_default: bool,
    /// Whether this member must be understood
    pub is_must_understand: bool,
    /// Index in the member sequence (branch id is `index + 1`)
    pub index: usize,
}

/// Descriptor for bitmask types.
#[derive(Debug, Clone)]
pub struct BitmaskDescriptor {
    /// Bit bound determining the wire width (1/2/4/8 bytes)
    pub bit_bound: u16,
    /// Named flags (bit positions); not needed for the wire, kept for introspection
    flags: Vec<BitflagDescriptor>,
}

impl BitmaskDescriptor {
    /// Get all named flags.
    pub fn flags(&self) -> &[BitflagDescriptor] {
        &self.flags
    }
}

/// Descriptor for a single bitmask flag.
#[derive(Debug, Clone)]
pub struct BitflagDescriptor {
    /// Flag name
    pub name: Arc<str>,
    /// Bit position
    pub position: u16,
}

/// Descriptor for bitset types.
#[derive(Debug, Clone)]
pub struct BitsetDescriptor {
    fields: Vec<BitfieldDescriptor>,
    total_bits: u16,
}

impl BitsetDescriptor {
    pub fn fields(&self) -> &[BitfieldDescriptor] {
        &self.fields
    }

    pub fn total_bits(&self) -> u16 {
        self.total_bits
    }
}

/// Descriptor for a single bitset field.
#[derive(Debug, Clone)]
pub struct BitfieldDescriptor {
    /// Field name
    pub name: Arc<str>,
    /// Bit offset of this field
    pub position: u16,
    /// Number of bits in this field
    pub bitcount: u8,
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

#[cfg(test)]
#[allow(unused_imports)]
mod resolution_tests {
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
    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct Leaf {
        a: i32,
        b: i64,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds")]
    #[repr(i32)]
    enum Color {
        Red = 0,
        Green = 1,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct Branch {
        tag: i32,
        leaf: Leaf,
        color: Color,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct Tree {
        id: i32,
        direct: Branch,
    }

    /// Build the closure exactly the way a writer/reader populates the registry:
    /// the top type under its own identifier, then nested members under their
    /// name-based ids.
    fn closure_of<T: HasTypeObject>() -> Vec<(TypeIdentifier, TypeObject)> {
        let mut out = vec![(T::type_identifier(), TypeObject::Complete(T::complete_type_object()))];
        T::collect_nested_type_objects(&mut out);
        out
    }

    fn registry_with<T: HasTypeObject>() -> TypeRegistry {
        let mut registry = TypeRegistry::new();
        for (id, obj) in closure_of::<T>() {
            registry.register_type_object_with_id(&id, obj);
        }
        registry
    }

    fn member<'a>(dt: &'a DynamicType, name: &str) -> &'a DynamicTypeKind {
        let desc = dt.as_struct().expect("struct type");
        &desc.members().iter().find(|m| &*m.name == name).expect("member present").member_type
    }

    #[test]
    fn closure_includes_transitive_nested_objects() {
        let closure = closure_of::<Tree>();
        // Tree (under content id + its own name id), Branch, Leaf.
        let names: Vec<String> = closure
            .iter()
            .map(|(_, o)| match o {
                TypeObject::Complete(CompleteTypeObject::Struct(s)) => {
                    s.header.detail.type_name.clone()
                }
                _ => String::new(),
            })
            .collect();
        assert!(names.iter().any(|n| n == "Branch"), "closure must carry Branch: {:?}", names);
        assert!(names.iter().any(|n| n == "Leaf"), "closure must carry Leaf: {:?}", names);
    }

    #[test]
    fn nested_members_resolve_to_typeref() {
        let registry = registry_with::<Tree>();
        let dt = DynamicType::from_type_object_with_registry(
            Arc::new(Tree::complete_type_object()),
            Tree::type_identifier(),
            &registry,
        )
        .expect("build Tree dynamic type");

        // Direct nested struct member resolves, not left external.
        match member(&dt, "direct") {
            DynamicTypeKind::TypeRef(branch) => {
                // The grandchild struct (Leaf inside Branch) resolves too.
                assert!(
                    matches!(member(branch, "leaf"), DynamicTypeKind::TypeRef(_)),
                    "Branch.leaf must resolve to TypeRef, got {:?}",
                    member(branch, "leaf")
                );
                // And a nested enum member resolves as well.
                assert!(
                    matches!(member(branch, "color"), DynamicTypeKind::TypeRef(_)),
                    "Branch.color must resolve to TypeRef, got {:?}",
                    member(branch, "color")
                );
            }
            other => panic!("Tree.direct must resolve to TypeRef, got {:?}", other),
        }
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct Collections {
        leaves: Vec<Leaf>,
        fixed: [Leaf; 3],
        boxed: Box<Leaf>,
    }

    #[test]
    fn composite_collection_elements_resolve_to_typeref() {
        let registry = registry_with::<Collections>();
        let dt = DynamicType::from_type_object_with_registry(
            Arc::new(Collections::complete_type_object()),
            Collections::type_identifier(),
            &registry,
        )
        .expect("build Collections dynamic type");

        match member(&dt, "leaves") {
            DynamicTypeKind::Sequence { element_type, .. } => assert!(
                matches!(element_type.as_ref(), DynamicTypeKind::TypeRef(_)),
                "Vec<Leaf> element must resolve to TypeRef, got {:?}",
                element_type
            ),
            other => panic!("leaves must be a Sequence, got {:?}", other),
        }
        match member(&dt, "fixed") {
            DynamicTypeKind::Array { element_type, .. } => assert!(
                matches!(element_type.as_ref(), DynamicTypeKind::TypeRef(_)),
                "[Leaf; 3] element must resolve to TypeRef, got {:?}",
                element_type
            ),
            other => panic!("fixed must be an Array, got {:?}", other),
        }
        assert!(
            matches!(member(&dt, "boxed"), DynamicTypeKind::TypeRef(_)),
            "Box<Leaf> must resolve to TypeRef, got {:?}",
            member(&dt, "boxed")
        );
    }

    #[test]
    fn unregistered_nested_stays_external() {
        // With an empty registry the same build leaves nested members external,
        // proving the closure registration is what makes resolution succeed.
        let empty = TypeRegistry::new();
        let dt = DynamicType::from_type_object_with_registry(
            Arc::new(Tree::complete_type_object()),
            Tree::type_identifier(),
            &empty,
        )
        .expect("build Tree dynamic type");
        assert!(
            matches!(member(&dt, "direct"), DynamicTypeKind::ExternalType { .. }),
            "without registration the nested member must stay ExternalType"
        );
    }

    // ===========================================================================
    // Byte-for-byte equivalence: dynamic serialization vs derive codegen.
    // ===========================================================================
}
