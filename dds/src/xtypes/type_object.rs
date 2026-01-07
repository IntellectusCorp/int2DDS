//! TypeObject and TypeIdentifier definitions for DDS-XTYPES.
//!
//! Based on OMG DDS-XTYPES 1.3 specification.
//!
//! # Overview
//!
//! The XTypes specification defines a type system for DDS that enables:
//! - Type evolution (adding optional fields to existing types)
//! - Type compatibility checking between publishers and subscribers
//! - Efficient type matching using hash-based identifiers
//!
//! # Key Types
//!
//! - [`TypeIdentifier`] - Compact type reference (discriminator + optional hash)
//! - [`TypeObject`] - Full type description (Complete or Minimal)
//! - [`EquivalenceHash`] - 14-byte MD5 hash for type matching

// Note: HasTypeObject trait does not require DdsType bound
// because primitive types need to implement HasTypeObject
// but don't need full serialization support.

// ============================================================================
// Constants from DDS-XTYPES 1.3 Specification
// ============================================================================

/// TypeIdentifier discriminator values (Table 7.23)
pub mod type_kind {
    // Primitive types
    pub const TK_NONE: u8 = 0x00;
    pub const TK_BOOLEAN: u8 = 0x01;
    pub const TK_BYTE: u8 = 0x02;
    pub const TK_INT16: u8 = 0x03;
    pub const TK_INT32: u8 = 0x04;
    pub const TK_INT64: u8 = 0x05;
    pub const TK_UINT16: u8 = 0x06;
    pub const TK_UINT32: u8 = 0x07;
    pub const TK_UINT64: u8 = 0x08;
    pub const TK_FLOAT32: u8 = 0x09;
    pub const TK_FLOAT64: u8 = 0x0A;
    pub const TK_FLOAT128: u8 = 0x0B;
    pub const TK_INT8: u8 = 0x0C;
    pub const TK_UINT8: u8 = 0x0D;
    pub const TK_CHAR8: u8 = 0x10;
    pub const TK_CHAR16: u8 = 0x11;

    // String types
    pub const TK_STRING8: u8 = 0x20;
    pub const TK_STRING16: u8 = 0x21;

    // Plain collection TypeIdentifiers
    pub const TI_STRING8_SMALL: u8 = 0x70;
    pub const TI_STRING8_LARGE: u8 = 0x71;
    pub const TI_STRING16_SMALL: u8 = 0x72;
    pub const TI_STRING16_LARGE: u8 = 0x73;
    pub const TI_PLAIN_SEQUENCE_SMALL: u8 = 0x80;
    pub const TI_PLAIN_SEQUENCE_LARGE: u8 = 0x81;
    pub const TI_PLAIN_ARRAY_SMALL: u8 = 0x90;
    pub const TI_PLAIN_ARRAY_LARGE: u8 = 0x91;
    pub const TI_PLAIN_MAP_SMALL: u8 = 0xA0;
    pub const TI_PLAIN_MAP_LARGE: u8 = 0xA1;

    // Strongly connected TypeIdentifiers
    pub const TI_STRONGLY_CONNECTED_COMPONENT: u8 = 0xB0;

    // TypeObject equivalence kinds
    pub const EK_COMPLETE: u8 = 0xF1;
    pub const EK_MINIMAL: u8 = 0xF2;
}

/// Member flags bit positions
pub mod member_flag {
    pub const TRY_CONSTRUCT_DISCARD: u16 = 0;
    pub const TRY_CONSTRUCT_USE_DEFAULT: u16 = 1;
    pub const TRY_CONSTRUCT_TRIM: u16 = 2;
    pub const IS_EXTERNAL: u16 = 1 << 2;
    pub const IS_OPTIONAL: u16 = 1 << 3;
    pub const IS_MUST_UNDERSTAND: u16 = 1 << 4;
    pub const IS_KEY: u16 = 1 << 5;
    pub const IS_DEFAULT: u16 = 1 << 6;
}

/// Type flags bit positions
pub mod type_flag {
    pub const IS_FINAL: u16 = 0;
    pub const IS_APPENDABLE: u16 = 1;
    pub const IS_MUTABLE: u16 = 2;
    pub const IS_NESTED: u16 = 1 << 2;
    pub const IS_AUTOID_HASH: u16 = 1 << 3;
}

// ============================================================================
// EquivalenceHash
// ============================================================================

/// 14-byte equivalence hash computed from serialized MinimalTypeObject.
///
/// The hash is computed as: `MD5(XCDR2_serialize(MinimalTypeObject))[0:14]`
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EquivalenceHash(pub [u8; 14]);

impl EquivalenceHash {
    /// Create a new EquivalenceHash from raw bytes.
    pub const fn new(bytes: [u8; 14]) -> Self {
        Self(bytes)
    }

    /// Create an empty/zero hash.
    pub const fn zero() -> Self {
        Self([0u8; 14])
    }

    /// Create from a full MD5 hash (takes first 14 bytes).
    pub fn from_md5(md5: &[u8; 16]) -> Self {
        let mut bytes = [0u8; 14];
        bytes.copy_from_slice(&md5[..14]);
        Self(bytes)
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 14] {
        &self.0
    }

    /// Compute hash from serialized data using MD5.
    pub fn compute(data: &[u8]) -> Self {
        let hash = ::md5::compute(data);
        Self::from_md5(&hash.0)
    }
}

impl Default for EquivalenceHash {
    fn default() -> Self {
        Self::zero()
    }
}

impl std::fmt::Debug for EquivalenceHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EquivalenceHash(")?;
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        write!(f, ")")
    }
}

impl std::fmt::Display for EquivalenceHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

// ============================================================================
// TypeIdentifier
// ============================================================================

/// TypeIdentifier - compact type reference.
///
/// For primitive types, this is just the discriminator byte.
/// For complex types, this includes an EquivalenceHash.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeIdentifier {
    // Primitive types (no additional data)
    None,
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

    // Unbounded strings
    String8,
    String16,

    // Bounded strings (small: bound fits in u8, large: bound needs u32)
    String8Small {
        bound: u8,
    },
    String8Large {
        bound: u32,
    },
    String16Small {
        bound: u8,
    },
    String16Large {
        bound: u32,
    },

    // Sequences
    PlainSequenceSmall {
        header: PlainCollectionHeader,
        bound: u8,
        element_identifier: Box<TypeIdentifier>,
    },
    PlainSequenceLarge {
        header: PlainCollectionHeader,
        bound: u32,
        element_identifier: Box<TypeIdentifier>,
    },

    // Arrays
    PlainArraySmall {
        header: PlainCollectionHeader,
        array_bound_seq: SBoundSeq,
        element_identifier: Box<TypeIdentifier>,
    },
    PlainArrayLarge {
        header: PlainCollectionHeader,
        array_bound_seq: LBoundSeq,
        element_identifier: Box<TypeIdentifier>,
    },

    // Maps
    PlainMapSmall {
        header: PlainCollectionHeader,
        bound: u8,
        key_flags: CollectionElementFlag,
        key_identifier: Box<TypeIdentifier>,
        element_identifier: Box<TypeIdentifier>,
    },
    PlainMapLarge {
        header: PlainCollectionHeader,
        bound: u32,
        key_flags: CollectionElementFlag,
        key_identifier: Box<TypeIdentifier>,
        element_identifier: Box<TypeIdentifier>,
    },

    // Complex types (struct, union, enum, etc.) referenced by hash
    CompleteTypeId(EquivalenceHash),
    MinimalTypeId(EquivalenceHash),
}

/// Small bound sequence (max 255 dimensions, each dimension max 255)
pub type SBoundSeq = Vec<u8>;

/// Large bound sequence (max 2^32-1 dimensions, each dimension max 2^32-1)
pub type LBoundSeq = Vec<u32>;

impl Default for TypeIdentifier {
    fn default() -> Self {
        TypeIdentifier::None
    }
}

impl TypeIdentifier {
    /// Get the discriminator byte for this TypeIdentifier.
    pub fn discriminator(&self) -> u8 {
        match self {
            TypeIdentifier::None => type_kind::TK_NONE,
            TypeIdentifier::Boolean => type_kind::TK_BOOLEAN,
            TypeIdentifier::Byte => type_kind::TK_BYTE,
            TypeIdentifier::Int8 => type_kind::TK_INT8,
            TypeIdentifier::Int16 => type_kind::TK_INT16,
            TypeIdentifier::Int32 => type_kind::TK_INT32,
            TypeIdentifier::Int64 => type_kind::TK_INT64,
            TypeIdentifier::Uint8 => type_kind::TK_UINT8,
            TypeIdentifier::Uint16 => type_kind::TK_UINT16,
            TypeIdentifier::Uint32 => type_kind::TK_UINT32,
            TypeIdentifier::Uint64 => type_kind::TK_UINT64,
            TypeIdentifier::Float32 => type_kind::TK_FLOAT32,
            TypeIdentifier::Float64 => type_kind::TK_FLOAT64,
            TypeIdentifier::Float128 => type_kind::TK_FLOAT128,
            TypeIdentifier::Char8 => type_kind::TK_CHAR8,
            TypeIdentifier::Char16 => type_kind::TK_CHAR16,
            TypeIdentifier::String8 => type_kind::TK_STRING8,
            TypeIdentifier::String16 => type_kind::TK_STRING16,
            TypeIdentifier::String8Small { .. } => type_kind::TI_STRING8_SMALL,
            TypeIdentifier::String8Large { .. } => type_kind::TI_STRING8_LARGE,
            TypeIdentifier::String16Small { .. } => type_kind::TI_STRING16_SMALL,
            TypeIdentifier::String16Large { .. } => type_kind::TI_STRING16_LARGE,
            TypeIdentifier::PlainSequenceSmall { .. } => type_kind::TI_PLAIN_SEQUENCE_SMALL,
            TypeIdentifier::PlainSequenceLarge { .. } => type_kind::TI_PLAIN_SEQUENCE_LARGE,
            TypeIdentifier::PlainArraySmall { .. } => type_kind::TI_PLAIN_ARRAY_SMALL,
            TypeIdentifier::PlainArrayLarge { .. } => type_kind::TI_PLAIN_ARRAY_LARGE,
            TypeIdentifier::PlainMapSmall { .. } => type_kind::TI_PLAIN_MAP_SMALL,
            TypeIdentifier::PlainMapLarge { .. } => type_kind::TI_PLAIN_MAP_LARGE,
            TypeIdentifier::CompleteTypeId(_) => type_kind::EK_COMPLETE,
            TypeIdentifier::MinimalTypeId(_) => type_kind::EK_MINIMAL,
        }
    }

    /// Check if this is a primitive type.
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            TypeIdentifier::Boolean
                | TypeIdentifier::Byte
                | TypeIdentifier::Int8
                | TypeIdentifier::Int16
                | TypeIdentifier::Int32
                | TypeIdentifier::Int64
                | TypeIdentifier::Uint8
                | TypeIdentifier::Uint16
                | TypeIdentifier::Uint32
                | TypeIdentifier::Uint64
                | TypeIdentifier::Float32
                | TypeIdentifier::Float64
                | TypeIdentifier::Float128
                | TypeIdentifier::Char8
                | TypeIdentifier::Char16
        )
    }

    /// Check if this is a string type.
    pub fn is_string(&self) -> bool {
        matches!(
            self,
            TypeIdentifier::String8
                | TypeIdentifier::String16
                | TypeIdentifier::String8Small { .. }
                | TypeIdentifier::String8Large { .. }
                | TypeIdentifier::String16Small { .. }
                | TypeIdentifier::String16Large { .. }
        )
    }

    /// Check if this is a collection type.
    pub fn is_collection(&self) -> bool {
        matches!(
            self,
            TypeIdentifier::PlainSequenceSmall { .. }
                | TypeIdentifier::PlainSequenceLarge { .. }
                | TypeIdentifier::PlainArraySmall { .. }
                | TypeIdentifier::PlainArrayLarge { .. }
                | TypeIdentifier::PlainMapSmall { .. }
                | TypeIdentifier::PlainMapLarge { .. }
        )
    }

    /// Check if this is a complex type (struct, union, enum, etc.).
    pub fn is_complex(&self) -> bool {
        matches!(self, TypeIdentifier::CompleteTypeId(_) | TypeIdentifier::MinimalTypeId(_))
    }

    /// Get the equivalence hash if this is a complex type.
    pub fn equivalence_hash(&self) -> Option<&EquivalenceHash> {
        match self {
            TypeIdentifier::CompleteTypeId(hash) | TypeIdentifier::MinimalTypeId(hash) => {
                Some(hash)
            }
            _ => None,
        }
    }

    /// Serialize the TypeIdentifier to bytes (XCDR2 format).
    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    /// Serialize the TypeIdentifier into a buffer.
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        // Write discriminator
        buffer.push(self.discriminator());

        match self {
            // Primitive types - just the discriminator
            TypeIdentifier::None
            | TypeIdentifier::Boolean
            | TypeIdentifier::Byte
            | TypeIdentifier::Int8
            | TypeIdentifier::Int16
            | TypeIdentifier::Int32
            | TypeIdentifier::Int64
            | TypeIdentifier::Uint8
            | TypeIdentifier::Uint16
            | TypeIdentifier::Uint32
            | TypeIdentifier::Uint64
            | TypeIdentifier::Float32
            | TypeIdentifier::Float64
            | TypeIdentifier::Float128
            | TypeIdentifier::Char8
            | TypeIdentifier::Char16
            | TypeIdentifier::String8
            | TypeIdentifier::String16 => {}

            // Bounded strings
            TypeIdentifier::String8Small { bound } | TypeIdentifier::String16Small { bound } => {
                buffer.push(*bound);
            }
            TypeIdentifier::String8Large { bound } | TypeIdentifier::String16Large { bound } => {
                buffer.extend_from_slice(&bound.to_le_bytes());
            }

            // Sequences
            TypeIdentifier::PlainSequenceSmall { header, bound, element_identifier } => {
                header.serialize_into(buffer);
                buffer.push(*bound);
                element_identifier.serialize_into(buffer);
            }
            TypeIdentifier::PlainSequenceLarge { header, bound, element_identifier } => {
                header.serialize_into(buffer);
                buffer.extend_from_slice(&bound.to_le_bytes());
                element_identifier.serialize_into(buffer);
            }

            // Arrays
            TypeIdentifier::PlainArraySmall { header, array_bound_seq, element_identifier } => {
                header.serialize_into(buffer);
                buffer.extend_from_slice(&(array_bound_seq.len() as u32).to_le_bytes());
                buffer.extend_from_slice(array_bound_seq);
                element_identifier.serialize_into(buffer);
            }
            TypeIdentifier::PlainArrayLarge { header, array_bound_seq, element_identifier } => {
                header.serialize_into(buffer);
                buffer.extend_from_slice(&(array_bound_seq.len() as u32).to_le_bytes());
                for bound in array_bound_seq {
                    buffer.extend_from_slice(&bound.to_le_bytes());
                }
                element_identifier.serialize_into(buffer);
            }

            // Maps
            TypeIdentifier::PlainMapSmall {
                header,
                bound,
                key_flags,
                key_identifier,
                element_identifier,
            } => {
                header.serialize_into(buffer);
                buffer.push(*bound);
                key_flags.serialize_into(buffer);
                key_identifier.serialize_into(buffer);
                element_identifier.serialize_into(buffer);
            }
            TypeIdentifier::PlainMapLarge {
                header,
                bound,
                key_flags,
                key_identifier,
                element_identifier,
            } => {
                header.serialize_into(buffer);
                buffer.extend_from_slice(&bound.to_le_bytes());
                key_flags.serialize_into(buffer);
                key_identifier.serialize_into(buffer);
                element_identifier.serialize_into(buffer);
            }

            // Complex types
            TypeIdentifier::CompleteTypeId(hash) | TypeIdentifier::MinimalTypeId(hash) => {
                buffer.extend_from_slice(hash.as_bytes());
            }
        }
    }

    /// Deserialize a TypeIdentifier from bytes (XCDR2 format).
    ///
    /// Returns the TypeIdentifier and the number of bytes consumed.
    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.is_empty() {
            return Err("Empty data for TypeIdentifier".to_string());
        }

        let discriminator = data[0];
        let rest = &data[1..];

        match discriminator {
            // Primitive types - just discriminator (1 byte total)
            type_kind::TK_NONE => Ok((TypeIdentifier::None, 1)),
            type_kind::TK_BOOLEAN => Ok((TypeIdentifier::Boolean, 1)),
            type_kind::TK_BYTE => Ok((TypeIdentifier::Byte, 1)),
            type_kind::TK_INT8 => Ok((TypeIdentifier::Int8, 1)),
            type_kind::TK_INT16 => Ok((TypeIdentifier::Int16, 1)),
            type_kind::TK_INT32 => Ok((TypeIdentifier::Int32, 1)),
            type_kind::TK_INT64 => Ok((TypeIdentifier::Int64, 1)),
            type_kind::TK_UINT8 => Ok((TypeIdentifier::Uint8, 1)),
            type_kind::TK_UINT16 => Ok((TypeIdentifier::Uint16, 1)),
            type_kind::TK_UINT32 => Ok((TypeIdentifier::Uint32, 1)),
            type_kind::TK_UINT64 => Ok((TypeIdentifier::Uint64, 1)),
            type_kind::TK_FLOAT32 => Ok((TypeIdentifier::Float32, 1)),
            type_kind::TK_FLOAT64 => Ok((TypeIdentifier::Float64, 1)),
            type_kind::TK_FLOAT128 => Ok((TypeIdentifier::Float128, 1)),
            type_kind::TK_CHAR8 => Ok((TypeIdentifier::Char8, 1)),
            type_kind::TK_CHAR16 => Ok((TypeIdentifier::Char16, 1)),
            type_kind::TK_STRING8 => Ok((TypeIdentifier::String8, 1)),
            type_kind::TK_STRING16 => Ok((TypeIdentifier::String16, 1)),

            // Bounded strings (small: 1 byte bound)
            type_kind::TI_STRING8_SMALL => {
                if rest.is_empty() {
                    return Err("Missing bound for String8Small".to_string());
                }
                Ok((TypeIdentifier::String8Small { bound: rest[0] }, 2))
            }
            type_kind::TI_STRING16_SMALL => {
                if rest.is_empty() {
                    return Err("Missing bound for String16Small".to_string());
                }
                Ok((TypeIdentifier::String16Small { bound: rest[0] }, 2))
            }

            // Bounded strings (large: 4 byte bound)
            type_kind::TI_STRING8_LARGE => {
                if rest.len() < 4 {
                    return Err("Insufficient data for String8Large bound".to_string());
                }
                let bound = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
                Ok((TypeIdentifier::String8Large { bound }, 5))
            }
            type_kind::TI_STRING16_LARGE => {
                if rest.len() < 4 {
                    return Err("Insufficient data for String16Large bound".to_string());
                }
                let bound = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
                Ok((TypeIdentifier::String16Large { bound }, 5))
            }

            // Complex types with equivalence hash (14 bytes)
            type_kind::EK_MINIMAL => {
                if rest.len() < 14 {
                    return Err("Insufficient data for MinimalTypeId hash".to_string());
                }
                let mut hash_bytes = [0u8; 14];
                hash_bytes.copy_from_slice(&rest[..14]);
                Ok((TypeIdentifier::MinimalTypeId(EquivalenceHash::new(hash_bytes)), 15))
            }
            type_kind::EK_COMPLETE => {
                if rest.len() < 14 {
                    return Err("Insufficient data for CompleteTypeId hash".to_string());
                }
                let mut hash_bytes = [0u8; 14];
                hash_bytes.copy_from_slice(&rest[..14]);
                Ok((TypeIdentifier::CompleteTypeId(EquivalenceHash::new(hash_bytes)), 15))
            }

            // Sequences - basic support
            type_kind::TI_PLAIN_SEQUENCE_SMALL => {
                if rest.len() < 3 {
                    return Err("Insufficient data for PlainSequenceSmall".to_string());
                }
                let header = PlainCollectionHeader {
                    equiv_kind: EquivalenceKind::from_u8(rest[0]),
                    element_flags: CollectionElementFlag(rest[1]),
                };
                let bound = rest[2];
                let (element_id, elem_len) = TypeIdentifier::deserialize(&rest[3..])?;
                Ok((
                    TypeIdentifier::PlainSequenceSmall {
                        header,
                        bound,
                        element_identifier: Box::new(element_id),
                    },
                    1 + 3 + elem_len,
                ))
            }
            type_kind::TI_PLAIN_SEQUENCE_LARGE => {
                if rest.len() < 6 {
                    return Err("Insufficient data for PlainSequenceLarge".to_string());
                }
                let header = PlainCollectionHeader {
                    equiv_kind: EquivalenceKind::from_u8(rest[0]),
                    element_flags: CollectionElementFlag(rest[1]),
                };
                let bound = u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]);
                let (element_id, elem_len) = TypeIdentifier::deserialize(&rest[6..])?;
                Ok((
                    TypeIdentifier::PlainSequenceLarge {
                        header,
                        bound,
                        element_identifier: Box::new(element_id),
                    },
                    1 + 6 + elem_len,
                ))
            }

            // Arrays - basic support
            type_kind::TI_PLAIN_ARRAY_SMALL => {
                if rest.len() < 6 {
                    return Err("Insufficient data for PlainArraySmall".to_string());
                }
                let header = PlainCollectionHeader {
                    equiv_kind: EquivalenceKind::from_u8(rest[0]),
                    element_flags: CollectionElementFlag(rest[1]),
                };
                let bound_count = u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]) as usize;
                if rest.len() < 6 + bound_count {
                    return Err("Insufficient data for PlainArraySmall bounds".to_string());
                }
                let array_bound_seq: Vec<u8> = rest[6..6 + bound_count].to_vec();
                let (element_id, elem_len) = TypeIdentifier::deserialize(&rest[6 + bound_count..])?;
                Ok((
                    TypeIdentifier::PlainArraySmall {
                        header,
                        array_bound_seq,
                        element_identifier: Box::new(element_id),
                    },
                    1 + 6 + bound_count + elem_len,
                ))
            }
            type_kind::TI_PLAIN_ARRAY_LARGE => {
                if rest.len() < 6 {
                    return Err("Insufficient data for PlainArrayLarge".to_string());
                }
                let header = PlainCollectionHeader {
                    equiv_kind: EquivalenceKind::from_u8(rest[0]),
                    element_flags: CollectionElementFlag(rest[1]),
                };
                let bound_count = u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]) as usize;
                if rest.len() < 6 + bound_count * 4 {
                    return Err("Insufficient data for PlainArrayLarge bounds".to_string());
                }
                let mut array_bound_seq = Vec::with_capacity(bound_count);
                for i in 0..bound_count {
                    let offset = 6 + i * 4;
                    let bound = u32::from_le_bytes([
                        rest[offset],
                        rest[offset + 1],
                        rest[offset + 2],
                        rest[offset + 3],
                    ]);
                    array_bound_seq.push(bound);
                }
                let (element_id, elem_len) =
                    TypeIdentifier::deserialize(&rest[6 + bound_count * 4..])?;
                Ok((
                    TypeIdentifier::PlainArrayLarge {
                        header,
                        array_bound_seq,
                        element_identifier: Box::new(element_id),
                    },
                    1 + 6 + bound_count * 4 + elem_len,
                ))
            }

            // Maps - not fully supported yet
            type_kind::TI_PLAIN_MAP_SMALL | type_kind::TI_PLAIN_MAP_LARGE => Err(format!(
                "Map TypeIdentifier deserialization not yet supported: 0x{:02X}",
                discriminator
            )),

            _ => Err(format!("Unsupported TypeIdentifier discriminator: 0x{:02X}", discriminator)),
        }
    }
}

// ============================================================================
// Collection Headers and Flags
// ============================================================================

/// Collection element flags (for sequences, arrays, maps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CollectionElementFlag(pub u8);

impl CollectionElementFlag {
    pub const fn new(try_construct: TryConstructKind, external: bool) -> Self {
        let mut flags = try_construct as u8;
        if external {
            flags |= 0x04;
        }
        Self(flags)
    }

    pub fn try_construct(&self) -> TryConstructKind {
        TryConstructKind::from_u8(self.0 & 0x03)
    }

    pub fn is_external(&self) -> bool {
        (self.0 & 0x04) != 0
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.0);
    }
}

/// Plain collection header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PlainCollectionHeader {
    pub equiv_kind: EquivalenceKind,
    pub element_flags: CollectionElementFlag,
}

impl PlainCollectionHeader {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.equiv_kind as u8);
        self.element_flags.serialize_into(buffer);
    }
}

/// Equivalence kind for collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum EquivalenceKind {
    #[default]
    Minimal = 0xF2,
    Complete = 0xF1,
    Both = 0xF3,
}

impl EquivalenceKind {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0xF1 => EquivalenceKind::Complete,
            0xF2 => EquivalenceKind::Minimal,
            0xF3 => EquivalenceKind::Both,
            _ => EquivalenceKind::Minimal, // Default fallback
        }
    }
}

/// TryConstruct behavior for elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum TryConstructKind {
    #[default]
    Discard = 0,
    UseDefault = 1,
    Trim = 2,
}

impl TryConstructKind {
    pub fn from_u8(value: u8) -> Self {
        match value & 0x03 {
            0 => TryConstructKind::Discard,
            1 => TryConstructKind::UseDefault,
            2 => TryConstructKind::Trim,
            _ => TryConstructKind::Discard,
        }
    }
}

// ============================================================================
// Type Flags
// ============================================================================

/// Extensibility kind for types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ExtensibilityKind {
    #[default]
    Final = 0,
    Appendable = 1,
    Mutable = 2,
}

/// Type flags (16-bit bitmask).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TypeFlag(pub u16);

impl TypeFlag {
    pub const fn new(
        extensibility: ExtensibilityKind,
        is_nested: bool,
        is_autoid_hash: bool,
    ) -> Self {
        let mut flags = extensibility as u16;
        if is_nested {
            flags |= type_flag::IS_NESTED;
        }
        if is_autoid_hash {
            flags |= type_flag::IS_AUTOID_HASH;
        }
        Self(flags)
    }

    pub fn extensibility(&self) -> ExtensibilityKind {
        match self.0 & 0x03 {
            0 => ExtensibilityKind::Final,
            1 => ExtensibilityKind::Appendable,
            2 => ExtensibilityKind::Mutable,
            _ => ExtensibilityKind::Final,
        }
    }

    pub fn is_nested(&self) -> bool {
        (self.0 & type_flag::IS_NESTED) != 0
    }

    pub fn is_autoid_hash(&self) -> bool {
        (self.0 & type_flag::IS_AUTOID_HASH) != 0
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.0.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 2 {
            return Err("Insufficient data for TypeFlag".to_string());
        }
        let flags = u16::from_le_bytes([data[0], data[1]]);
        Ok((TypeFlag(flags), 2))
    }
}

/// Member flags (16-bit bitmask).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MemberFlag(pub u16);

impl MemberFlag {
    pub const fn new(
        try_construct: TryConstructKind,
        is_external: bool,
        is_optional: bool,
        is_must_understand: bool,
        is_key: bool,
        is_default: bool,
    ) -> Self {
        let mut flags = try_construct as u16;
        if is_external {
            flags |= member_flag::IS_EXTERNAL;
        }
        if is_optional {
            flags |= member_flag::IS_OPTIONAL;
        }
        if is_must_understand {
            flags |= member_flag::IS_MUST_UNDERSTAND;
        }
        if is_key {
            flags |= member_flag::IS_KEY;
        }
        if is_default {
            flags |= member_flag::IS_DEFAULT;
        }
        Self(flags)
    }

    pub fn try_construct(&self) -> TryConstructKind {
        TryConstructKind::from_u8((self.0 & 0x03) as u8)
    }

    pub fn is_external(&self) -> bool {
        (self.0 & member_flag::IS_EXTERNAL) != 0
    }

    pub fn is_optional(&self) -> bool {
        (self.0 & member_flag::IS_OPTIONAL) != 0
    }

    pub fn is_must_understand(&self) -> bool {
        (self.0 & member_flag::IS_MUST_UNDERSTAND) != 0
    }

    pub fn is_key(&self) -> bool {
        (self.0 & member_flag::IS_KEY) != 0
    }

    pub fn is_default(&self) -> bool {
        (self.0 & member_flag::IS_DEFAULT) != 0
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.0.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 2 {
            return Err("Insufficient data for MemberFlag".to_string());
        }
        let flags = u16::from_le_bytes([data[0], data[1]]);
        Ok((MemberFlag(flags), 2))
    }
}

// ============================================================================
// Name Hash
// ============================================================================

/// Compute name hash from a string.
/// Returns first 4 bytes of MD5(name) masked with 0x0FFFFFFF.
pub fn compute_name_hash(name: &str) -> u32 {
    let hash = ::md5::compute(name.as_bytes());
    let bytes = [hash[0], hash[1], hash[2], hash[3]];
    u32::from_be_bytes(bytes) & 0x0FFFFFFF
}

// ============================================================================
// Struct Member Types
// ============================================================================

/// Minimal struct member for hash computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimalStructMember {
    pub common: CommonStructMember,
    pub name_hash: u32,
}

impl MinimalStructMember {
    pub fn new(member_id: u32, flags: MemberFlag, type_id: TypeIdentifier, name: &str) -> Self {
        Self {
            common: CommonStructMember { member_id, member_flags: flags, member_type_id: type_id },
            name_hash: compute_name_hash(name),
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        buffer.extend_from_slice(&self.name_hash.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        // common
        let (common, consumed) = CommonStructMember::deserialize(data)?;
        pos += consumed;

        // name_hash (4 bytes)
        if data.len() < pos + 4 {
            return Err("Insufficient data for MinimalStructMember name_hash".to_string());
        }
        let name_hash =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        Ok((MinimalStructMember { common, name_hash }, pos))
    }
}

/// Complete struct member with full details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteStructMember {
    pub common: CommonStructMember,
    pub detail: CompleteMemberDetail,
}

impl CompleteStructMember {
    pub fn new(member_id: u32, flags: MemberFlag, type_id: TypeIdentifier, name: String) -> Self {
        Self {
            common: CommonStructMember { member_id, member_flags: flags, member_type_id: type_id },
            detail: CompleteMemberDetail { name, ann_builtin: None, ann_custom: Vec::new() },
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        self.detail.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        // common
        let (common, consumed) = CommonStructMember::deserialize(data)?;
        pos += consumed;

        // detail
        let (detail, consumed) = CompleteMemberDetail::deserialize(&data[pos..])?;
        pos += consumed;

        Ok((CompleteStructMember { common, detail }, pos))
    }
}

/// Common struct member data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonStructMember {
    pub member_id: u32,
    pub member_flags: MemberFlag,
    pub member_type_id: TypeIdentifier,
}

impl CommonStructMember {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.member_id.to_le_bytes());
        self.member_flags.serialize_into(buffer);
        self.member_type_id.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 6 {
            return Err("Insufficient data for CommonStructMember".to_string());
        }
        let mut pos = 0;

        // member_id (4 bytes)
        let member_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        pos += 4;

        // member_flags (2 bytes)
        let (member_flags, consumed) = MemberFlag::deserialize(&data[pos..])?;
        pos += consumed;

        // member_type_id (variable)
        let (member_type_id, consumed) = TypeIdentifier::deserialize(&data[pos..])?;
        pos += consumed;

        Ok((CommonStructMember { member_id, member_flags, member_type_id }, pos))
    }
}

/// Complete member detail.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteMemberDetail {
    pub name: String,
    pub ann_builtin: Option<AppliedBuiltinMemberAnnotations>,
    pub ann_custom: Vec<AppliedAnnotation>,
}

impl CompleteMemberDetail {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        // Serialize name as length-prefixed string
        let name_bytes = self.name.as_bytes();
        buffer.extend_from_slice(&(name_bytes.len() as u32 + 1).to_le_bytes());
        buffer.extend_from_slice(name_bytes);
        buffer.push(0); // null terminator

        // Serialize optional annotations (simplified - just write empty for now)
        buffer.push(0); // no builtin annotations
        buffer.extend_from_slice(&0u32.to_le_bytes()); // empty custom annotations
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 4 {
            return Err("Insufficient data for CompleteMemberDetail".to_string());
        }
        let mut pos = 0;

        // Read name length (includes null terminator)
        let name_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        pos += 4;

        if data.len() < pos + name_len {
            return Err("Insufficient data for CompleteMemberDetail name".to_string());
        }

        // Read name (excluding null terminator)
        let name = if name_len > 0 {
            String::from_utf8_lossy(&data[pos..pos + name_len - 1]).to_string()
        } else {
            String::new()
        };
        pos += name_len;

        // Skip annotations (simplified - just read the flags)
        if data.len() < pos + 1 {
            return Err("Insufficient data for annotations flag".to_string());
        }
        let _has_builtin = data[pos];
        pos += 1;

        if data.len() < pos + 4 {
            return Err("Insufficient data for custom annotations count".to_string());
        }
        let _custom_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        Ok((CompleteMemberDetail { name, ann_builtin: None, ann_custom: Vec::new() }, pos))
    }
}

/// Applied builtin member annotations (placeholder).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppliedBuiltinMemberAnnotations;

/// Applied annotation (placeholder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedAnnotation;

// ============================================================================
// Struct Type Definitions
// ============================================================================

/// Minimal struct type for hash computation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinimalStructType {
    pub struct_flags: TypeFlag,
    pub header: MinimalStructHeader,
    pub member_seq: Vec<MinimalStructMember>,
}

impl MinimalStructType {
    pub fn new(flags: TypeFlag, base_type: Option<TypeIdentifier>) -> Self {
        Self {
            struct_flags: flags,
            header: MinimalStructHeader { base_type },
            member_seq: Vec::new(),
        }
    }

    pub fn add_member(&mut self, member: MinimalStructMember) {
        self.member_seq.push(member);
    }

    /// Serialize to bytes for hash computation.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.struct_flags.serialize_into(buffer);
        self.header.serialize_into(buffer);

        // Serialize member sequence
        buffer.extend_from_slice(&(self.member_seq.len() as u32).to_le_bytes());
        for member in &self.member_seq {
            member.serialize_into(buffer);
        }
    }

    /// Compute the equivalence hash for this type.
    pub fn compute_hash(&self) -> EquivalenceHash {
        let serialized = self.serialize();
        EquivalenceHash::compute(&serialized)
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        // struct_flags (2 bytes)
        let (struct_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;

        // header
        let (header, consumed) = MinimalStructHeader::deserialize(&data[pos..])?;
        pos += consumed;

        // member_seq length (4 bytes)
        if data.len() < pos + 4 {
            return Err("Insufficient data for member_seq length".to_string());
        }
        let member_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        // members
        let mut member_seq = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            let (member, consumed) = MinimalStructMember::deserialize(&data[pos..])?;
            pos += consumed;
            member_seq.push(member);
        }

        Ok((MinimalStructType { struct_flags, header, member_seq }, pos))
    }
}

/// Minimal struct header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinimalStructHeader {
    pub base_type: Option<TypeIdentifier>,
}

impl MinimalStructHeader {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        if let Some(ref base) = self.base_type {
            buffer.push(1); // has base type
            base.serialize_into(buffer);
        } else {
            buffer.push(0); // no base type
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.is_empty() {
            return Err("Insufficient data for MinimalStructHeader".to_string());
        }
        let mut pos = 0;

        let has_base = data[0];
        pos += 1;

        let base_type = if has_base != 0 {
            let (type_id, consumed) = TypeIdentifier::deserialize(&data[pos..])?;
            pos += consumed;
            Some(type_id)
        } else {
            None
        };

        Ok((MinimalStructHeader { base_type }, pos))
    }
}

/// Complete struct type with full details.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteStructType {
    pub struct_flags: TypeFlag,
    pub header: CompleteStructHeader,
    pub member_seq: Vec<CompleteStructMember>,
}

impl CompleteStructType {
    pub fn new(flags: TypeFlag, type_name: String, base_type: Option<TypeIdentifier>) -> Self {
        Self {
            struct_flags: flags,
            header: CompleteStructHeader {
                base_type,
                detail: CompleteTypeDetail { type_name, ann_builtin: None, ann_custom: Vec::new() },
            },
            member_seq: Vec::new(),
        }
    }

    pub fn add_member(&mut self, member: CompleteStructMember) {
        self.member_seq.push(member);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.struct_flags.serialize_into(buffer);
        self.header.serialize_into(buffer);

        // Serialize member sequence
        buffer.extend_from_slice(&(self.member_seq.len() as u32).to_le_bytes());
        for member in &self.member_seq {
            member.serialize_into(buffer);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        // struct_flags (2 bytes)
        let (struct_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;

        // header
        let (header, consumed) = CompleteStructHeader::deserialize(&data[pos..])?;
        pos += consumed;

        // member_seq length (4 bytes)
        if data.len() < pos + 4 {
            return Err("Insufficient data for member_seq length".to_string());
        }
        let member_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        // members
        let mut member_seq = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            let (member, consumed) = CompleteStructMember::deserialize(&data[pos..])?;
            pos += consumed;
            member_seq.push(member);
        }

        Ok((CompleteStructType { struct_flags, header, member_seq }, pos))
    }
}

/// Complete struct header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteStructHeader {
    pub base_type: Option<TypeIdentifier>,
    pub detail: CompleteTypeDetail,
}

impl CompleteStructHeader {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        if let Some(ref base) = self.base_type {
            buffer.push(1);
            base.serialize_into(buffer);
        } else {
            buffer.push(0);
        }
        self.detail.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.is_empty() {
            return Err("Insufficient data for CompleteStructHeader".to_string());
        }
        let mut pos = 0;

        let has_base = data[0];
        pos += 1;

        let base_type = if has_base != 0 {
            let (type_id, consumed) = TypeIdentifier::deserialize(&data[pos..])?;
            pos += consumed;
            Some(type_id)
        } else {
            None
        };

        let (detail, consumed) = CompleteTypeDetail::deserialize(&data[pos..])?;
        pos += consumed;

        Ok((CompleteStructHeader { base_type, detail }, pos))
    }
}

/// Complete type detail.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteTypeDetail {
    pub type_name: String,
    pub ann_builtin: Option<AppliedBuiltinTypeAnnotations>,
    pub ann_custom: Vec<AppliedAnnotation>,
}

impl CompleteTypeDetail {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        // Serialize name
        let name_bytes = self.type_name.as_bytes();
        buffer.extend_from_slice(&(name_bytes.len() as u32 + 1).to_le_bytes());
        buffer.extend_from_slice(name_bytes);
        buffer.push(0);

        // Annotations (simplified)
        buffer.push(0);
        buffer.extend_from_slice(&0u32.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 4 {
            return Err("Insufficient data for CompleteTypeDetail".to_string());
        }
        let mut pos = 0;

        // Read name length (includes null terminator)
        let name_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        pos += 4;

        if data.len() < pos + name_len {
            return Err("Insufficient data for CompleteTypeDetail type_name".to_string());
        }

        // Read name (excluding null terminator)
        let type_name = if name_len > 0 {
            String::from_utf8_lossy(&data[pos..pos + name_len - 1]).to_string()
        } else {
            String::new()
        };
        pos += name_len;

        // Skip annotations
        if data.len() < pos + 1 {
            return Err("Insufficient data for annotations flag".to_string());
        }
        let _has_builtin = data[pos];
        pos += 1;

        if data.len() < pos + 4 {
            return Err("Insufficient data for custom annotations count".to_string());
        }
        let _custom_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        Ok((CompleteTypeDetail { type_name, ann_builtin: None, ann_custom: Vec::new() }, pos))
    }
}

/// Applied builtin type annotations (placeholder).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppliedBuiltinTypeAnnotations;

// ============================================================================
// Enumerated Type Definitions
// ============================================================================

/// Minimal enumerated literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimalEnumeratedLiteral {
    pub common: CommonEnumeratedLiteral,
    pub name_hash: u32,
}

impl MinimalEnumeratedLiteral {
    pub fn new(value: i32, flags: EnumeratedLiteralFlag, name: &str) -> Self {
        Self {
            common: CommonEnumeratedLiteral { value, flags },
            name_hash: compute_name_hash(name),
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        buffer.extend_from_slice(&self.name_hash.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        let (common, consumed) = CommonEnumeratedLiteral::deserialize(data)?;
        pos += consumed;

        if data.len() < pos + 4 {
            return Err("Insufficient data for MinimalEnumeratedLiteral name_hash".to_string());
        }
        let name_hash =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        Ok((MinimalEnumeratedLiteral { common, name_hash }, pos))
    }
}

/// Complete enumerated literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteEnumeratedLiteral {
    pub common: CommonEnumeratedLiteral,
    pub detail: CompleteMemberDetail,
}

impl CompleteEnumeratedLiteral {
    pub fn new(value: i32, flags: EnumeratedLiteralFlag, name: String) -> Self {
        Self {
            common: CommonEnumeratedLiteral { value, flags },
            detail: CompleteMemberDetail { name, ann_builtin: None, ann_custom: Vec::new() },
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        self.detail.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        let (common, consumed) = CommonEnumeratedLiteral::deserialize(data)?;
        pos += consumed;

        let (detail, consumed) = CompleteMemberDetail::deserialize(&data[pos..])?;
        pos += consumed;

        Ok((CompleteEnumeratedLiteral { common, detail }, pos))
    }
}

/// Common enumerated literal data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonEnumeratedLiteral {
    pub value: i32,
    pub flags: EnumeratedLiteralFlag,
}

impl CommonEnumeratedLiteral {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.value.to_le_bytes());
        self.flags.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 5 {
            return Err("Insufficient data for CommonEnumeratedLiteral".to_string());
        }
        let mut pos = 0;

        let value = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        pos += 4;

        let (flags, consumed) = EnumeratedLiteralFlag::deserialize(&data[pos..])?;
        pos += consumed;

        Ok((CommonEnumeratedLiteral { value, flags }, pos))
    }
}

/// Enumerated literal flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EnumeratedLiteralFlag(pub u8);

impl EnumeratedLiteralFlag {
    pub const DEFAULT: Self = Self(0x01);

    pub fn is_default(&self) -> bool {
        (self.0 & 0x01) != 0
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.0);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.is_empty() {
            return Err("Insufficient data for EnumeratedLiteralFlag".to_string());
        }
        Ok((EnumeratedLiteralFlag(data[0]), 1))
    }
}

/// Minimal enumerated type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinimalEnumeratedType {
    pub enum_flags: TypeFlag,
    pub header: MinimalEnumeratedHeader,
    pub literal_seq: Vec<MinimalEnumeratedLiteral>,
}

impl MinimalEnumeratedType {
    pub fn new(flags: TypeFlag, bit_bound: u16) -> Self {
        Self {
            enum_flags: flags,
            header: MinimalEnumeratedHeader { common: CommonEnumeratedHeader { bit_bound } },
            literal_seq: Vec::new(),
        }
    }

    pub fn add_literal(&mut self, literal: MinimalEnumeratedLiteral) {
        self.literal_seq.push(literal);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.enum_flags.serialize_into(buffer);
        self.header.serialize_into(buffer);

        buffer.extend_from_slice(&(self.literal_seq.len() as u32).to_le_bytes());
        for literal in &self.literal_seq {
            literal.serialize_into(buffer);
        }
    }

    pub fn compute_hash(&self) -> EquivalenceHash {
        let serialized = self.serialize();
        EquivalenceHash::compute(&serialized)
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        // enum_flags (2 bytes)
        let (enum_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;

        // header
        let (header, consumed) = MinimalEnumeratedHeader::deserialize(&data[pos..])?;
        pos += consumed;

        // literal_seq length (4 bytes)
        if data.len() < pos + 4 {
            return Err("Insufficient data for literal_seq length".to_string());
        }
        let literal_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        // literals
        let mut literal_seq = Vec::with_capacity(literal_count);
        for _ in 0..literal_count {
            let (literal, consumed) = MinimalEnumeratedLiteral::deserialize(&data[pos..])?;
            pos += consumed;
            literal_seq.push(literal);
        }

        Ok((MinimalEnumeratedType { enum_flags, header, literal_seq }, pos))
    }
}

/// Minimal enumerated header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinimalEnumeratedHeader {
    pub common: CommonEnumeratedHeader,
}

impl MinimalEnumeratedHeader {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (common, consumed) = CommonEnumeratedHeader::deserialize(data)?;
        Ok((MinimalEnumeratedHeader { common }, consumed))
    }
}

/// Common enumerated header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommonEnumeratedHeader {
    pub bit_bound: u16,
}

impl CommonEnumeratedHeader {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.bit_bound.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 2 {
            return Err("Insufficient data for CommonEnumeratedHeader".to_string());
        }
        let bit_bound = u16::from_le_bytes([data[0], data[1]]);
        Ok((CommonEnumeratedHeader { bit_bound }, 2))
    }
}

/// Complete enumerated type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteEnumeratedType {
    pub enum_flags: TypeFlag,
    pub header: CompleteEnumeratedHeader,
    pub literal_seq: Vec<CompleteEnumeratedLiteral>,
}

impl CompleteEnumeratedType {
    pub fn new(flags: TypeFlag, type_name: String, bit_bound: u16) -> Self {
        Self {
            enum_flags: flags,
            header: CompleteEnumeratedHeader {
                common: CommonEnumeratedHeader { bit_bound },
                detail: CompleteTypeDetail { type_name, ann_builtin: None, ann_custom: Vec::new() },
            },
            literal_seq: Vec::new(),
        }
    }

    pub fn add_literal(&mut self, literal: CompleteEnumeratedLiteral) {
        self.literal_seq.push(literal);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.enum_flags.serialize_into(buffer);
        self.header.serialize_into(buffer);

        buffer.extend_from_slice(&(self.literal_seq.len() as u32).to_le_bytes());
        for literal in &self.literal_seq {
            literal.serialize_into(buffer);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        // enum_flags (2 bytes)
        let (enum_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;

        // header
        let (header, consumed) = CompleteEnumeratedHeader::deserialize(&data[pos..])?;
        pos += consumed;

        // literal_seq length (4 bytes)
        if data.len() < pos + 4 {
            return Err("Insufficient data for literal_seq length".to_string());
        }
        let literal_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        // literals
        let mut literal_seq = Vec::with_capacity(literal_count);
        for _ in 0..literal_count {
            let (literal, consumed) = CompleteEnumeratedLiteral::deserialize(&data[pos..])?;
            pos += consumed;
            literal_seq.push(literal);
        }

        Ok((CompleteEnumeratedType { enum_flags, header, literal_seq }, pos))
    }
}

/// Complete enumerated header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteEnumeratedHeader {
    pub common: CommonEnumeratedHeader,
    pub detail: CompleteTypeDetail,
}

impl CompleteEnumeratedHeader {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        self.detail.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        let (common, consumed) = CommonEnumeratedHeader::deserialize(data)?;
        pos += consumed;

        let (detail, consumed) = CompleteTypeDetail::deserialize(&data[pos..])?;
        pos += consumed;

        Ok((CompleteEnumeratedHeader { common, detail }, pos))
    }
}

// ============================================================================
// TypeObject - Top Level
// ============================================================================

/// Discriminator values for TypeObject kinds.
pub mod type_object_kind {
    pub const TK_NONE: u8 = 0x00;
    pub const TK_STRUCT: u8 = 0x51;
    pub const TK_ENUM: u8 = 0x54;
    pub const TK_UNION: u8 = 0x52;
    pub const TK_ALIAS: u8 = 0x53;
    pub const TK_BITSET: u8 = 0x55;
    pub const TK_SEQUENCE: u8 = 0x56;
    pub const TK_ARRAY: u8 = 0x57;
    pub const TK_MAP: u8 = 0x58;
    pub const TK_BITMASK: u8 = 0x59;
}

/// Minimal TypeObject - used for hash computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MinimalTypeObject {
    Struct(MinimalStructType),
    Enum(MinimalEnumeratedType),
}

impl Default for MinimalTypeObject {
    fn default() -> Self {
        MinimalTypeObject::Struct(MinimalStructType::default())
    }
}

impl MinimalTypeObject {
    pub fn discriminator(&self) -> u8 {
        match self {
            MinimalTypeObject::Struct(_) => type_object_kind::TK_STRUCT,
            MinimalTypeObject::Enum(_) => type_object_kind::TK_ENUM,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.discriminator());
        match self {
            MinimalTypeObject::Struct(s) => s.serialize_into(buffer),
            MinimalTypeObject::Enum(e) => e.serialize_into(buffer),
        }
    }

    pub fn compute_hash(&self) -> EquivalenceHash {
        let serialized = self.serialize();
        EquivalenceHash::compute(&serialized)
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.is_empty() {
            return Err("Empty data for MinimalTypeObject".to_string());
        }

        let discriminator = data[0];
        match discriminator {
            type_object_kind::TK_STRUCT => MinimalStructType::deserialize(&data[1..])
                .map(|(s, consumed)| (MinimalTypeObject::Struct(s), 1 + consumed)),
            type_object_kind::TK_ENUM => MinimalEnumeratedType::deserialize(&data[1..])
                .map(|(e, consumed)| (MinimalTypeObject::Enum(e), 1 + consumed)),
            _ => Err(format!("Unsupported MinimalTypeObject kind: 0x{:02X}", discriminator)),
        }
    }
}

/// Complete TypeObject - full type description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteTypeObject {
    Struct(CompleteStructType),
    Enum(CompleteEnumeratedType),
}

impl Default for CompleteTypeObject {
    fn default() -> Self {
        CompleteTypeObject::Struct(CompleteStructType::default())
    }
}

impl CompleteTypeObject {
    pub fn discriminator(&self) -> u8 {
        match self {
            CompleteTypeObject::Struct(_) => type_object_kind::TK_STRUCT,
            CompleteTypeObject::Enum(_) => type_object_kind::TK_ENUM,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.discriminator());
        match self {
            CompleteTypeObject::Struct(s) => s.serialize_into(buffer),
            CompleteTypeObject::Enum(e) => e.serialize_into(buffer),
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.is_empty() {
            return Err("Empty data for CompleteTypeObject".to_string());
        }

        let discriminator = data[0];
        match discriminator {
            type_object_kind::TK_STRUCT => CompleteStructType::deserialize(&data[1..])
                .map(|(s, consumed)| (CompleteTypeObject::Struct(s), 1 + consumed)),
            type_object_kind::TK_ENUM => CompleteEnumeratedType::deserialize(&data[1..])
                .map(|(e, consumed)| (CompleteTypeObject::Enum(e), 1 + consumed)),
            _ => Err(format!("Unsupported CompleteTypeObject kind: 0x{:02X}", discriminator)),
        }
    }
}

/// Combined TypeObject for both Complete and Minimal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeObject {
    Complete(CompleteTypeObject),
    Minimal(MinimalTypeObject),
}

impl TypeObject {
    pub fn compute_hash(&self) -> EquivalenceHash {
        match self {
            TypeObject::Complete(c) => EquivalenceHash::compute(&c.serialize()),
            TypeObject::Minimal(m) => m.compute_hash(),
        }
    }

    /// Serialize TypeObject to bytes.
    /// Format: [EK_MINIMAL/EK_COMPLETE] [TypeObject data...]
    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    /// Serialize TypeObject into a buffer.
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        match self {
            TypeObject::Minimal(m) => {
                buffer.push(type_kind::EK_MINIMAL);
                m.serialize_into(buffer);
            }
            TypeObject::Complete(c) => {
                buffer.push(type_kind::EK_COMPLETE);
                c.serialize_into(buffer);
            }
        }
    }

    /// Deserialize TypeObject from bytes.
    /// Returns the deserialized TypeObject and number of bytes consumed.
    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.is_empty() {
            return Err("Empty data for TypeObject".to_string());
        }

        let kind = data[0];
        match kind {
            type_kind::EK_MINIMAL => MinimalTypeObject::deserialize(&data[1..])
                .map(|(obj, consumed)| (TypeObject::Minimal(obj), 1 + consumed)),
            type_kind::EK_COMPLETE => CompleteTypeObject::deserialize(&data[1..])
                .map(|(obj, consumed)| (TypeObject::Complete(obj), 1 + consumed)),
            _ => Err(format!("Unknown TypeObject kind: 0x{:02X}", kind)),
        }
    }
}

// ============================================================================
// HasTypeObject Trait
// ============================================================================

/// Trait for types that have TypeObject representation.
///
/// This trait provides type metadata for DDS-XTypes support.
/// It is automatically implemented by the `#[derive(DdsType)]` macro.
pub trait HasTypeObject {
    /// Get the TypeIdentifier for this type.
    fn type_identifier() -> TypeIdentifier;

    /// Get the MinimalTypeObject for this type.
    fn minimal_type_object() -> MinimalTypeObject;

    /// Get the CompleteTypeObject for this type.
    fn complete_type_object() -> CompleteTypeObject;

    /// Get the type name as used in IDL/DDS.
    fn dds_type_name() -> &'static str;
}

// ============================================================================
// Primitive Type Implementations
// ============================================================================

macro_rules! impl_primitive_has_type_object {
    ($($rust_type:ty => $identifier:ident, $name:literal),* $(,)?) => {
        $(
            impl HasTypeObject for $rust_type {
                fn type_identifier() -> TypeIdentifier {
                    TypeIdentifier::$identifier
                }

                fn minimal_type_object() -> MinimalTypeObject {
                    MinimalTypeObject::Struct(MinimalStructType::default())
                }

                fn complete_type_object() -> CompleteTypeObject {
                    CompleteTypeObject::Struct(CompleteStructType::default())
                }

                fn dds_type_name() -> &'static str {
                    $name
                }
            }
        )*
    };
}

impl_primitive_has_type_object! {
    bool => Boolean, "boolean",
    u8 => Byte, "octet",
    i8 => Int8, "int8",
    i16 => Int16, "int16",
    i32 => Int32, "int32",
    i64 => Int64, "int64",
    u16 => Uint16, "uint16",
    u32 => Uint32, "uint32",
    u64 => Uint64, "uint64",
    f32 => Float32, "float32",
    f64 => Float64, "float64",
    char => Char8, "char",
}

impl HasTypeObject for String {
    fn type_identifier() -> TypeIdentifier {
        TypeIdentifier::String8
    }

    fn minimal_type_object() -> MinimalTypeObject {
        MinimalTypeObject::Struct(MinimalStructType::default())
    }

    fn complete_type_object() -> CompleteTypeObject {
        CompleteTypeObject::Struct(CompleteStructType::default())
    }

    fn dds_type_name() -> &'static str {
        "string"
    }
}

impl<T: HasTypeObject> HasTypeObject for Vec<T> {
    fn type_identifier() -> TypeIdentifier {
        TypeIdentifier::PlainSequenceLarge {
            header: PlainCollectionHeader::default(),
            bound: 0, // unbounded
            element_identifier: Box::new(T::type_identifier()),
        }
    }

    fn minimal_type_object() -> MinimalTypeObject {
        MinimalTypeObject::Struct(MinimalStructType::default())
    }

    fn complete_type_object() -> CompleteTypeObject {
        CompleteTypeObject::Struct(CompleteStructType::default())
    }

    fn dds_type_name() -> &'static str {
        "sequence"
    }
}

impl<T: HasTypeObject> HasTypeObject for Option<T> {
    fn type_identifier() -> TypeIdentifier {
        T::type_identifier()
    }

    fn minimal_type_object() -> MinimalTypeObject {
        T::minimal_type_object()
    }

    fn complete_type_object() -> CompleteTypeObject {
        T::complete_type_object()
    }

    fn dds_type_name() -> &'static str {
        T::dds_type_name()
    }
}

// Implement for common array sizes
macro_rules! impl_array_has_type_object {
    ($($n:expr),* $(,)?) => {
        $(
            impl<T: HasTypeObject> HasTypeObject for [T; $n] {
                fn type_identifier() -> TypeIdentifier {
                    TypeIdentifier::PlainArrayLarge {
                        header: PlainCollectionHeader::default(),
                        array_bound_seq: vec![$n],
                        element_identifier: Box::new(T::type_identifier()),
                    }
                }

                fn minimal_type_object() -> MinimalTypeObject {
                    MinimalTypeObject::Struct(MinimalStructType::default())
                }

                fn complete_type_object() -> CompleteTypeObject {
                    CompleteTypeObject::Struct(CompleteStructType::default())
                }

                fn dds_type_name() -> &'static str {
                    "array"
                }
            }
        )*
    };
}

impl_array_has_type_object! {
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    20, 24, 32, 48, 64, 128, 256, 512, 1024
}

// ============================================================================
// CDR/XCDR Serialization Traits for TypeIdentifier
// ============================================================================

use crate::serialize::cdr::{
    CdrDeserialize, CdrResult, CdrSerialize, CdrSerializer, CdrSerializerCommon,
    PrimitiveSerialize, Xcdr2Deserializer, Xcdr2Serializer, XcdrDeserialize, XcdrResult,
    XcdrSerialize,
};

impl CdrSerialize for TypeIdentifier {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        // Serialize as length-prefixed byte array
        let bytes = self.serialize();
        let len = bytes.len() as u32;
        serializer.serialize_u32(len)?;
        serializer.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }
}

impl CdrDeserialize for TypeIdentifier {
    fn deserialize_cdr(
        deserializer: &mut crate::serialize::cdr::CdrDeserializer,
    ) -> CdrResult<Self> {
        let len = deserializer.deserialize_u32()? as usize;
        let bytes = deserializer.deserialize_byte_array(len)?;
        TypeIdentifier::deserialize(&bytes)
            .map(|(id, _)| id)
            .map_err(|e| crate::serialize::cdr::CdrError::DeserializationError(e))
    }
}

impl XcdrSerialize for TypeIdentifier {
    fn serialize_xcdr(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        // Serialize as length-prefixed byte array
        let bytes = self.serialize();
        let len = bytes.len() as u32;
        serializer.serialize_u32(len)?;
        serializer.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }
}

impl XcdrDeserialize for TypeIdentifier {
    fn deserialize_xcdr(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self> {
        let len = deserializer.deserialize_u32()? as usize;
        let bytes = deserializer.deserialize_byte_array(len)?;
        TypeIdentifier::deserialize(&bytes)
            .map(|(id, _)| id)
            .map_err(|e| crate::serialize::cdr::XcdrError::DeserializationError(e))
    }
}

// CDR/XCDR traits for TypeObject
impl CdrSerialize for TypeObject {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        let bytes = self.serialize();
        let len = bytes.len() as u32;
        serializer.serialize_u32(len)?;
        serializer.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }
}

impl CdrDeserialize for TypeObject {
    fn deserialize_cdr(
        deserializer: &mut crate::serialize::cdr::CdrDeserializer,
    ) -> CdrResult<Self> {
        let len = deserializer.deserialize_u32()? as usize;
        let bytes = deserializer.deserialize_byte_array(len)?;
        TypeObject::deserialize(&bytes)
            .map(|(obj, _)| obj)
            .map_err(|e| crate::serialize::cdr::CdrError::DeserializationError(e))
    }
}

impl XcdrSerialize for TypeObject {
    fn serialize_xcdr(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        let bytes = self.serialize();
        let len = bytes.len() as u32;
        serializer.serialize_u32(len)?;
        serializer.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }
}

impl XcdrDeserialize for TypeObject {
    fn deserialize_xcdr(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self> {
        let len = deserializer.deserialize_u32()? as usize;
        let bytes = deserializer.deserialize_byte_array(len)?;
        TypeObject::deserialize(&bytes)
            .map(|(obj, _)| obj)
            .map_err(|e| crate::serialize::cdr::XcdrError::DeserializationError(e))
    }
}

// Speedy traits for TypeIdentifier
use speedy::{Context, Readable, Reader, Writable, Writer};

impl<C: Context> Writable<C> for TypeIdentifier {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        let bytes = self.serialize();
        let len = bytes.len() as u32;
        writer.write_value(&len)?;
        writer.write_bytes(&bytes)?;
        Ok(())
    }
}

impl<'a, C: Context> Readable<'a, C> for TypeIdentifier {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let len: u32 = reader.read_value()?;
        let bytes = reader.read_vec(len as usize)?;
        TypeIdentifier::deserialize(&bytes)
            .map(|(id, _)| id)
            .map_err(|_| speedy::Error::custom("Failed to deserialize TypeIdentifier").into())
    }
}

// Speedy traits for TypeObject
impl<C: Context> Writable<C> for TypeObject {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        let bytes = self.serialize();
        let len = bytes.len() as u32;
        writer.write_value(&len)?;
        writer.write_bytes(&bytes)?;
        Ok(())
    }
}

impl<'a, C: Context> Readable<'a, C> for TypeObject {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let len: u32 = reader.read_value()?;
        let bytes = reader.read_vec(len as usize)?;
        TypeObject::deserialize(&bytes)
            .map(|(obj, _)| obj)
            .map_err(|_| speedy::Error::custom("Failed to deserialize TypeObject").into())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equivalence_hash_compute() {
        let data = b"test data";
        let hash = EquivalenceHash::compute(data);
        assert_eq!(hash.as_bytes().len(), 14);

        // Same data should produce same hash
        let hash2 = EquivalenceHash::compute(data);
        assert_eq!(hash, hash2);

        // Different data should produce different hash
        let hash3 = EquivalenceHash::compute(b"other data");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_name_hash() {
        let hash1 = compute_name_hash("field1");
        let hash2 = compute_name_hash("field1");
        assert_eq!(hash1, hash2);

        let hash3 = compute_name_hash("field2");
        assert_ne!(hash1, hash3);

        // Verify mask is applied
        assert_eq!(hash1 & 0xF0000000, 0);
    }

    #[test]
    fn test_type_identifier_serialize_primitive() {
        let id = TypeIdentifier::Int32;
        let bytes = id.serialize();
        assert_eq!(bytes, vec![type_kind::TK_INT32]);
    }

    #[test]
    fn test_type_identifier_serialize_complex() {
        let hash = EquivalenceHash::new([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
        let id = TypeIdentifier::MinimalTypeId(hash);
        let bytes = id.serialize();
        assert_eq!(bytes.len(), 15); // 1 discriminator + 14 hash bytes
        assert_eq!(bytes[0], type_kind::EK_MINIMAL);
    }

    #[test]
    fn test_minimal_struct_type_hash() {
        let mut struct_type =
            MinimalStructType::new(TypeFlag::new(ExtensibilityKind::Final, false, false), None);

        struct_type.add_member(MinimalStructMember::new(
            0,
            MemberFlag::new(TryConstructKind::Discard, false, false, false, true, false),
            TypeIdentifier::Int32,
            "id",
        ));

        struct_type.add_member(MinimalStructMember::new(
            1,
            MemberFlag::default(),
            TypeIdentifier::String8,
            "name",
        ));

        let hash = struct_type.compute_hash();
        assert_ne!(hash, EquivalenceHash::zero());

        // Same structure should produce same hash
        let mut struct_type2 =
            MinimalStructType::new(TypeFlag::new(ExtensibilityKind::Final, false, false), None);
        struct_type2.add_member(MinimalStructMember::new(
            0,
            MemberFlag::new(TryConstructKind::Discard, false, false, false, true, false),
            TypeIdentifier::Int32,
            "id",
        ));
        struct_type2.add_member(MinimalStructMember::new(
            1,
            MemberFlag::default(),
            TypeIdentifier::String8,
            "name",
        ));

        assert_eq!(struct_type.compute_hash(), struct_type2.compute_hash());
    }

    #[test]
    fn test_primitive_type_identifiers() {
        assert_eq!(bool::type_identifier(), TypeIdentifier::Boolean);
        assert_eq!(i32::type_identifier(), TypeIdentifier::Int32);
        assert_eq!(String::type_identifier(), TypeIdentifier::String8);
    }

    #[test]
    fn test_sequence_type_identifier() {
        let vec_id = Vec::<i32>::type_identifier();
        if let TypeIdentifier::PlainSequenceLarge { element_identifier, bound, .. } = vec_id {
            assert_eq!(*element_identifier, TypeIdentifier::Int32);
            assert_eq!(bound, 0); // unbounded
        } else {
            panic!("Expected PlainSequenceLarge");
        }
    }

    #[test]
    fn test_type_flag_extensibility() {
        let final_flag = TypeFlag::new(ExtensibilityKind::Final, false, false);
        assert_eq!(final_flag.extensibility(), ExtensibilityKind::Final);
        assert!(!final_flag.is_nested());
        assert!(!final_flag.is_autoid_hash());

        let appendable_flag = TypeFlag::new(ExtensibilityKind::Appendable, true, false);
        assert_eq!(appendable_flag.extensibility(), ExtensibilityKind::Appendable);
        assert!(appendable_flag.is_nested());

        let mutable_flag = TypeFlag::new(ExtensibilityKind::Mutable, false, true);
        assert_eq!(mutable_flag.extensibility(), ExtensibilityKind::Mutable);
        assert!(mutable_flag.is_autoid_hash());
    }

    #[test]
    fn test_member_flag() {
        let flag = MemberFlag::new(
            TryConstructKind::UseDefault,
            false,
            true,  // optional
            true,  // must_understand
            true,  // key
            false, // default
        );

        assert_eq!(flag.try_construct(), TryConstructKind::UseDefault);
        assert!(!flag.is_external());
        assert!(flag.is_optional());
        assert!(flag.is_must_understand());
        assert!(flag.is_key());
        assert!(!flag.is_default());
    }

    #[test]
    fn test_minimal_enumerated_type() {
        let mut enum_type = MinimalEnumeratedType::new(TypeFlag::default(), 32);

        enum_type.add_literal(MinimalEnumeratedLiteral::new(
            0,
            EnumeratedLiteralFlag::default(),
            "RED",
        ));
        enum_type.add_literal(MinimalEnumeratedLiteral::new(
            1,
            EnumeratedLiteralFlag::default(),
            "GREEN",
        ));
        enum_type.add_literal(MinimalEnumeratedLiteral::new(
            2,
            EnumeratedLiteralFlag::default(),
            "BLUE",
        ));

        let hash = enum_type.compute_hash();
        assert_ne!(hash, EquivalenceHash::zero());

        // Verify serialization produces consistent output
        let serialized = enum_type.serialize();
        assert!(!serialized.is_empty());
    }

    #[test]
    fn test_complete_struct_type() {
        let mut struct_type = CompleteStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            "TestStruct".to_string(),
            None,
        );

        struct_type.add_member(CompleteStructMember::new(
            0,
            MemberFlag::new(TryConstructKind::Discard, false, false, false, true, false),
            TypeIdentifier::Int32,
            "id".to_string(),
        ));

        struct_type.add_member(CompleteStructMember::new(
            1,
            MemberFlag::default(),
            TypeIdentifier::String8,
            "name".to_string(),
        ));

        let serialized = struct_type.serialize();
        assert!(!serialized.is_empty());

        // Verify type name is in the complete object
        assert_eq!(struct_type.header.detail.type_name, "TestStruct");
    }

    #[test]
    fn test_array_type_identifier() {
        let arr_id = <[i32; 10]>::type_identifier();
        if let TypeIdentifier::PlainArrayLarge { array_bound_seq, element_identifier, .. } = arr_id
        {
            assert_eq!(*element_identifier, TypeIdentifier::Int32);
            assert_eq!(array_bound_seq, vec![10]);
        } else {
            panic!("Expected PlainArrayLarge");
        }
    }

    #[test]
    fn test_type_identifier_discriminator() {
        assert_eq!(TypeIdentifier::Boolean.discriminator(), type_kind::TK_BOOLEAN);
        assert_eq!(TypeIdentifier::Int32.discriminator(), type_kind::TK_INT32);
        assert_eq!(TypeIdentifier::String8.discriminator(), type_kind::TK_STRING8);

        let hash = EquivalenceHash::zero();
        assert_eq!(TypeIdentifier::MinimalTypeId(hash).discriminator(), type_kind::EK_MINIMAL);
        assert_eq!(TypeIdentifier::CompleteTypeId(hash).discriminator(), type_kind::EK_COMPLETE);
    }

    #[test]
    fn test_type_identifier_is_methods() {
        assert!(TypeIdentifier::Int32.is_primitive());
        assert!(!TypeIdentifier::String8.is_primitive());
        assert!(TypeIdentifier::String8.is_string());
        assert!(!TypeIdentifier::Int32.is_string());

        let seq_id = TypeIdentifier::PlainSequenceLarge {
            header: PlainCollectionHeader::default(),
            bound: 0,
            element_identifier: Box::new(TypeIdentifier::Int32),
        };
        assert!(seq_id.is_collection());
        assert!(!seq_id.is_complex());

        let complex_id = TypeIdentifier::MinimalTypeId(EquivalenceHash::zero());
        assert!(complex_id.is_complex());
        assert!(!complex_id.is_collection());
    }
}
