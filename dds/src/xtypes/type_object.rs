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

    /// Serialize as a PID_TYPE_IDV1 (0x0069) parameter payload: a CDR_LE (XCDRv1)
    pub fn serialize_for_parameter_v1(&self) -> Vec<u8> {
        let mut buffer = vec![0x00, 0x01, 0x00, 0x00];
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

            // Maps
            type_kind::TI_PLAIN_MAP_SMALL => {
                if rest.len() < 4 {
                    return Err("Insufficient data for PlainMapSmall".to_string());
                }
                let header = PlainCollectionHeader {
                    equiv_kind: EquivalenceKind::from_u8(rest[0]),
                    element_flags: CollectionElementFlag(rest[1]),
                };
                let bound = rest[2];
                let key_flags = CollectionElementFlag(rest[3]);
                let (key_id, key_len) = TypeIdentifier::deserialize(&rest[4..])?;
                let (element_id, elem_len) = TypeIdentifier::deserialize(&rest[4 + key_len..])?;
                Ok((
                    TypeIdentifier::PlainMapSmall {
                        header,
                        bound,
                        key_flags,
                        key_identifier: Box::new(key_id),
                        element_identifier: Box::new(element_id),
                    },
                    1 + 4 + key_len + elem_len,
                ))
            }
            type_kind::TI_PLAIN_MAP_LARGE => {
                if rest.len() < 7 {
                    return Err("Insufficient data for PlainMapLarge".to_string());
                }
                let header = PlainCollectionHeader {
                    equiv_kind: EquivalenceKind::from_u8(rest[0]),
                    element_flags: CollectionElementFlag(rest[1]),
                };
                let bound = u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]);
                let key_flags = CollectionElementFlag(rest[6]);
                let (key_id, key_len) = TypeIdentifier::deserialize(&rest[7..])?;
                let (element_id, elem_len) = TypeIdentifier::deserialize(&rest[7 + key_len..])?;
                Ok((
                    TypeIdentifier::PlainMapLarge {
                        header,
                        bound,
                        key_flags,
                        key_identifier: Box::new(key_id),
                        element_identifier: Box::new(element_id),
                    },
                    1 + 7 + key_len + elem_len,
                ))
            }

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
    Final = 0,
    #[default]
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
        let name_bytes = self.name.as_bytes();
        buffer.extend_from_slice(&(name_bytes.len() as u32 + 1).to_le_bytes());
        buffer.extend_from_slice(name_bytes);
        buffer.push(0);

        if let Some(ref ann) = self.ann_builtin {
            if !ann.is_empty() {
                buffer.push(1);
                ann.serialize_into(buffer);
            } else {
                buffer.push(0);
            }
        } else {
            buffer.push(0);
        }

        buffer.extend_from_slice(&(self.ann_custom.len() as u32).to_le_bytes());
        for ann in &self.ann_custom {
            ann.serialize_into(buffer);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 4 {
            return Err("Insufficient data for CompleteMemberDetail".to_string());
        }
        let mut pos = 0;

        let name_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        pos += 4;

        if data.len() < pos + name_len {
            return Err("Insufficient data for CompleteMemberDetail name".to_string());
        }

        let name = if name_len > 0 {
            String::from_utf8_lossy(&data[pos..pos + name_len - 1]).to_string()
        } else {
            String::new()
        };
        pos += name_len;

        if data.len() < pos + 1 {
            return Err("Insufficient data for annotations flag".to_string());
        }
        let ann_builtin = if data[pos] != 0 {
            pos += 1;
            let (ann, consumed) = AppliedBuiltinMemberAnnotations::deserialize(&data[pos..])?;
            pos += consumed;
            Some(ann)
        } else {
            pos += 1;
            None
        };

        if data.len() < pos + 4 {
            return Err("Insufficient data for custom annotations count".to_string());
        }
        let custom_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut ann_custom = Vec::with_capacity(custom_count);
        for _ in 0..custom_count {
            let (ann, consumed) = AppliedAnnotation::deserialize(&data[pos..])?;
            pos += consumed;
            ann_custom.push(ann);
        }

        Ok((CompleteMemberDetail { name, ann_builtin, ann_custom }, pos))
    }
}

/// Annotation parameter value (for custom annotations).
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationParameterValue {
    BooleanValue(bool),
    ByteValue(u8),
    Int16Value(i16),
    Uint16Value(u16),
    Int32Value(i32),
    Uint32Value(u32),
    Int64Value(i64),
    Uint64Value(u64),
    Float32Value(f32),
    Float64Value(f64),
    CharValue(u8),
    WcharValue(u16),
    StringValue(String),
    WstringValue(String),
    EnumValue(i32),
}

impl AnnotationParameterValue {
    fn discriminator(&self) -> u8 {
        match self {
            Self::BooleanValue(_) => 0x01,
            Self::ByteValue(_) => 0x02,
            Self::Int16Value(_) => 0x03,
            Self::Uint16Value(_) => 0x04,
            Self::Int32Value(_) => 0x05,
            Self::Uint32Value(_) => 0x06,
            Self::Int64Value(_) => 0x07,
            Self::Uint64Value(_) => 0x08,
            Self::Float32Value(_) => 0x09,
            Self::Float64Value(_) => 0x0A,
            Self::CharValue(_) => 0x0B,
            Self::WcharValue(_) => 0x0C,
            Self::StringValue(_) => 0x0D,
            Self::WstringValue(_) => 0x0E,
            Self::EnumValue(_) => 0x0F,
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.discriminator());
        match self {
            Self::BooleanValue(v) => buffer.push(*v as u8),
            Self::ByteValue(v) | Self::CharValue(v) => buffer.push(*v),
            Self::Int16Value(v) => buffer.extend_from_slice(&v.to_le_bytes()),
            Self::Uint16Value(v) | Self::WcharValue(v) => {
                buffer.extend_from_slice(&v.to_le_bytes())
            }
            Self::Int32Value(v) | Self::EnumValue(v) => buffer.extend_from_slice(&v.to_le_bytes()),
            Self::Uint32Value(v) => buffer.extend_from_slice(&v.to_le_bytes()),
            Self::Int64Value(v) => buffer.extend_from_slice(&v.to_le_bytes()),
            Self::Uint64Value(v) => buffer.extend_from_slice(&v.to_le_bytes()),
            Self::Float32Value(v) => buffer.extend_from_slice(&v.to_le_bytes()),
            Self::Float64Value(v) => buffer.extend_from_slice(&v.to_le_bytes()),
            Self::StringValue(s) | Self::WstringValue(s) => {
                let bytes = s.as_bytes();
                buffer.extend_from_slice(&(bytes.len() as u32 + 1).to_le_bytes());
                buffer.extend_from_slice(bytes);
                buffer.push(0);
            }
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.is_empty() {
            return Err("Empty data for AnnotationParameterValue".to_string());
        }
        let disc = data[0];
        let rest = &data[1..];
        match disc {
            0x01 => {
                if rest.is_empty() {
                    return Err("Missing boolean value".to_string());
                }
                Ok((Self::BooleanValue(rest[0] != 0), 2))
            }
            0x02 => {
                if rest.is_empty() {
                    return Err("Missing byte value".to_string());
                }
                Ok((Self::ByteValue(rest[0]), 2))
            }
            0x03 => {
                if rest.len() < 2 {
                    return Err("Missing int16 value".to_string());
                }
                Ok((Self::Int16Value(i16::from_le_bytes([rest[0], rest[1]])), 3))
            }
            0x04 => {
                if rest.len() < 2 {
                    return Err("Missing uint16 value".to_string());
                }
                Ok((Self::Uint16Value(u16::from_le_bytes([rest[0], rest[1]])), 3))
            }
            0x05 => {
                if rest.len() < 4 {
                    return Err("Missing int32 value".to_string());
                }
                Ok((Self::Int32Value(i32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]])), 5))
            }
            0x06 => {
                if rest.len() < 4 {
                    return Err("Missing uint32 value".to_string());
                }
                Ok((Self::Uint32Value(u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]])), 5))
            }
            0x07 => {
                if rest.len() < 8 {
                    return Err("Missing int64 value".to_string());
                }
                Ok((
                    Self::Int64Value(i64::from_le_bytes([
                        rest[0], rest[1], rest[2], rest[3], rest[4], rest[5], rest[6], rest[7],
                    ])),
                    9,
                ))
            }
            0x08 => {
                if rest.len() < 8 {
                    return Err("Missing uint64 value".to_string());
                }
                Ok((
                    Self::Uint64Value(u64::from_le_bytes([
                        rest[0], rest[1], rest[2], rest[3], rest[4], rest[5], rest[6], rest[7],
                    ])),
                    9,
                ))
            }
            0x09 => {
                if rest.len() < 4 {
                    return Err("Missing float32 value".to_string());
                }
                Ok((
                    Self::Float32Value(f32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]])),
                    5,
                ))
            }
            0x0A => {
                if rest.len() < 8 {
                    return Err("Missing float64 value".to_string());
                }
                Ok((
                    Self::Float64Value(f64::from_le_bytes([
                        rest[0], rest[1], rest[2], rest[3], rest[4], rest[5], rest[6], rest[7],
                    ])),
                    9,
                ))
            }
            0x0B => {
                if rest.is_empty() {
                    return Err("Missing char value".to_string());
                }
                Ok((Self::CharValue(rest[0]), 2))
            }
            0x0C => {
                if rest.len() < 2 {
                    return Err("Missing wchar value".to_string());
                }
                Ok((Self::WcharValue(u16::from_le_bytes([rest[0], rest[1]])), 3))
            }
            0x0D | 0x0E => {
                if rest.len() < 4 {
                    return Err("Missing string length".to_string());
                }
                let len = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
                if rest.len() < 4 + len {
                    return Err("Insufficient data for string value".to_string());
                }
                let s = if len > 0 {
                    String::from_utf8_lossy(&rest[4..4 + len - 1]).to_string()
                } else {
                    String::new()
                };
                let val = if disc == 0x0D { Self::StringValue(s) } else { Self::WstringValue(s) };
                Ok((val, 1 + 4 + len))
            }
            0x0F => {
                if rest.len() < 4 {
                    return Err("Missing enum value".to_string());
                }
                Ok((Self::EnumValue(i32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]])), 5))
            }
            _ => Err(format!("Unknown AnnotationParameterValue discriminator: 0x{:02X}", disc)),
        }
    }
}

/// Applied builtin member annotations.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppliedBuiltinMemberAnnotations {
    pub unit: Option<String>,
    pub min: Option<AnnotationParameterValue>,
    pub max: Option<AnnotationParameterValue>,
    pub hash_id: Option<String>,
}

impl Eq for AppliedBuiltinMemberAnnotations {}

impl AppliedBuiltinMemberAnnotations {
    pub fn is_empty(&self) -> bool {
        self.unit.is_none() && self.min.is_none() && self.max.is_none() && self.hash_id.is_none()
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        fn write_opt_string(buffer: &mut Vec<u8>, s: &Option<String>) {
            if let Some(ref val) = s {
                buffer.push(1);
                let bytes = val.as_bytes();
                buffer.extend_from_slice(&(bytes.len() as u32 + 1).to_le_bytes());
                buffer.extend_from_slice(bytes);
                buffer.push(0);
            } else {
                buffer.push(0);
            }
        }

        write_opt_string(buffer, &self.unit);
        if let Some(ref val) = self.min {
            buffer.push(1);
            val.serialize_into(buffer);
        } else {
            buffer.push(0);
        }
        if let Some(ref val) = self.max {
            buffer.push(1);
            val.serialize_into(buffer);
        } else {
            buffer.push(0);
        }
        write_opt_string(buffer, &self.hash_id);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;

        fn read_opt_string(data: &[u8], pos: &mut usize) -> Result<Option<String>, String> {
            if data.len() < *pos + 1 {
                return Err("Insufficient data for optional string flag".to_string());
            }
            let has = data[*pos];
            *pos += 1;
            if has == 0 {
                return Ok(None);
            }
            if data.len() < *pos + 4 {
                return Err("Insufficient data for string length".to_string());
            }
            let len =
                u32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]])
                    as usize;
            *pos += 4;
            if data.len() < *pos + len {
                return Err("Insufficient data for string".to_string());
            }
            let s = if len > 0 {
                String::from_utf8_lossy(&data[*pos..*pos + len - 1]).to_string()
            } else {
                String::new()
            };
            *pos += len;
            Ok(Some(s))
        }

        let unit = read_opt_string(data, &mut pos)?;

        if data.len() < pos + 1 {
            return Err("Insufficient data for min flag".to_string());
        }
        let min = if data[pos] != 0 {
            pos += 1;
            let (val, consumed) = AnnotationParameterValue::deserialize(&data[pos..])?;
            pos += consumed;
            Some(val)
        } else {
            pos += 1;
            None
        };

        if data.len() < pos + 1 {
            return Err("Insufficient data for max flag".to_string());
        }
        let max = if data[pos] != 0 {
            pos += 1;
            let (val, consumed) = AnnotationParameterValue::deserialize(&data[pos..])?;
            pos += consumed;
            Some(val)
        } else {
            pos += 1;
            None
        };

        let hash_id = read_opt_string(data, &mut pos)?;

        Ok((AppliedBuiltinMemberAnnotations { unit, min, max, hash_id }, pos))
    }
}

/// Applied annotation (custom user-defined annotation).
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedAnnotation {
    pub annotation_typeid: TypeIdentifier,
    pub param_seq: Vec<(u32, AnnotationParameterValue)>,
}

impl Eq for AppliedAnnotation {}

impl AppliedAnnotation {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.annotation_typeid.serialize_into(buffer);
        buffer.extend_from_slice(&(self.param_seq.len() as u32).to_le_bytes());
        for (name_hash, value) in &self.param_seq {
            buffer.extend_from_slice(&name_hash.to_le_bytes());
            value.serialize_into(buffer);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (annotation_typeid, consumed) = TypeIdentifier::deserialize(data)?;
        pos += consumed;

        if data.len() < pos + 4 {
            return Err("Insufficient data for param_seq length".to_string());
        }
        let param_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut param_seq = Vec::with_capacity(param_count);
        for _ in 0..param_count {
            if data.len() < pos + 4 {
                return Err("Insufficient data for param name_hash".to_string());
            }
            let name_hash =
                u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            pos += 4;
            let (value, consumed) = AnnotationParameterValue::deserialize(&data[pos..])?;
            pos += consumed;
            param_seq.push((name_hash, value));
        }

        Ok((AppliedAnnotation { annotation_typeid, param_seq }, pos))
    }
}

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
        let name_bytes = self.type_name.as_bytes();
        buffer.extend_from_slice(&(name_bytes.len() as u32 + 1).to_le_bytes());
        buffer.extend_from_slice(name_bytes);
        buffer.push(0);

        if let Some(ref ann) = self.ann_builtin {
            if !ann.is_empty() {
                buffer.push(1);
                ann.serialize_into(buffer);
            } else {
                buffer.push(0);
            }
        } else {
            buffer.push(0);
        }

        buffer.extend_from_slice(&(self.ann_custom.len() as u32).to_le_bytes());
        for ann in &self.ann_custom {
            ann.serialize_into(buffer);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 4 {
            return Err("Insufficient data for CompleteTypeDetail".to_string());
        }
        let mut pos = 0;

        let name_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        pos += 4;

        if data.len() < pos + name_len {
            return Err("Insufficient data for CompleteTypeDetail type_name".to_string());
        }

        let type_name = if name_len > 0 {
            String::from_utf8_lossy(&data[pos..pos + name_len - 1]).to_string()
        } else {
            String::new()
        };
        pos += name_len;

        if data.len() < pos + 1 {
            return Err("Insufficient data for annotations flag".to_string());
        }
        let ann_builtin = if data[pos] != 0 {
            pos += 1;
            let (ann, consumed) = AppliedBuiltinTypeAnnotations::deserialize(&data[pos..])?;
            pos += consumed;
            Some(ann)
        } else {
            pos += 1;
            None
        };

        if data.len() < pos + 4 {
            return Err("Insufficient data for custom annotations count".to_string());
        }
        let custom_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut ann_custom = Vec::with_capacity(custom_count);
        for _ in 0..custom_count {
            let (ann, consumed) = AppliedAnnotation::deserialize(&data[pos..])?;
            pos += consumed;
            ann_custom.push(ann);
        }

        Ok((CompleteTypeDetail { type_name, ann_builtin, ann_custom }, pos))
    }
}

/// Applied builtin type annotations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppliedBuiltinTypeAnnotations {
    pub verbatim: Option<bool>,
    pub nested: Option<bool>,
    pub data_representation: Option<u16>,
}

impl AppliedBuiltinTypeAnnotations {
    pub fn is_empty(&self) -> bool {
        self.verbatim.is_none() && self.nested.is_none() && self.data_representation.is_none()
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        if let Some(v) = self.verbatim {
            buffer.push(1);
            buffer.push(v as u8);
        } else {
            buffer.push(0);
        }
        if let Some(v) = self.nested {
            buffer.push(1);
            buffer.push(v as u8);
        } else {
            buffer.push(0);
        }
        if let Some(v) = self.data_representation {
            buffer.push(1);
            buffer.extend_from_slice(&v.to_le_bytes());
        } else {
            buffer.push(0);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let mut result = Self::default();

        if data.len() < pos + 1 {
            return Err("Insufficient data for verbatim flag".to_string());
        }
        if data[pos] != 0 {
            pos += 1;
            if data.len() < pos + 1 {
                return Err("Missing verbatim value".to_string());
            }
            result.verbatim = Some(data[pos] != 0);
            pos += 1;
        } else {
            pos += 1;
        }

        if data.len() < pos + 1 {
            return Err("Insufficient data for nested flag".to_string());
        }
        if data[pos] != 0 {
            pos += 1;
            if data.len() < pos + 1 {
                return Err("Missing nested value".to_string());
            }
            result.nested = Some(data[pos] != 0);
            pos += 1;
        } else {
            pos += 1;
        }

        if data.len() < pos + 1 {
            return Err("Insufficient data for data_representation flag".to_string());
        }
        if data[pos] != 0 {
            pos += 1;
            if data.len() < pos + 2 {
                return Err("Missing data_representation value".to_string());
            }
            result.data_representation = Some(u16::from_le_bytes([data[pos], data[pos + 1]]));
            pos += 2;
        } else {
            pos += 1;
        }

        Ok((result, pos))
    }
}

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
// Union Type Definitions
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonUnionMember {
    pub member_id: u32,
    pub member_flags: MemberFlag,
    pub member_type_id: TypeIdentifier,
    pub label_seq: Vec<i32>,
}

impl CommonUnionMember {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.member_id.to_le_bytes());
        self.member_flags.serialize_into(buffer);
        self.member_type_id.serialize_into(buffer);
        buffer.extend_from_slice(&(self.label_seq.len() as u32).to_le_bytes());
        for label in &self.label_seq {
            buffer.extend_from_slice(&label.to_le_bytes());
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 6 {
            return Err("Insufficient data for CommonUnionMember".to_string());
        }
        let mut pos = 0;

        let member_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        pos += 4;

        let (member_flags, consumed) = MemberFlag::deserialize(&data[pos..])?;
        pos += consumed;

        let (member_type_id, consumed) = TypeIdentifier::deserialize(&data[pos..])?;
        pos += consumed;

        if data.len() < pos + 4 {
            return Err("Insufficient data for label_seq length".to_string());
        }
        let label_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        if data.len() < pos + label_count * 4 {
            return Err("Insufficient data for label_seq".to_string());
        }
        let mut label_seq = Vec::with_capacity(label_count);
        for _ in 0..label_count {
            let label =
                i32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            pos += 4;
            label_seq.push(label);
        }

        Ok((CommonUnionMember { member_id, member_flags, member_type_id, label_seq }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimalUnionMember {
    pub common: CommonUnionMember,
    pub name_hash: u32,
}

impl MinimalUnionMember {
    pub fn new(
        member_id: u32,
        flags: MemberFlag,
        type_id: TypeIdentifier,
        labels: Vec<i32>,
        name: &str,
    ) -> Self {
        Self {
            common: CommonUnionMember {
                member_id,
                member_flags: flags,
                member_type_id: type_id,
                label_seq: labels,
            },
            name_hash: compute_name_hash(name),
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        buffer.extend_from_slice(&self.name_hash.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (common, mut pos) = CommonUnionMember::deserialize(data)?;
        if data.len() < pos + 4 {
            return Err("Insufficient data for MinimalUnionMember name_hash".to_string());
        }
        let name_hash =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;
        Ok((MinimalUnionMember { common, name_hash }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteUnionMember {
    pub common: CommonUnionMember,
    pub detail: CompleteMemberDetail,
}

impl CompleteUnionMember {
    pub fn new(
        member_id: u32,
        flags: MemberFlag,
        type_id: TypeIdentifier,
        labels: Vec<i32>,
        name: String,
    ) -> Self {
        Self {
            common: CommonUnionMember {
                member_id,
                member_flags: flags,
                member_type_id: type_id,
                label_seq: labels,
            },
            detail: CompleteMemberDetail { name, ann_builtin: None, ann_custom: Vec::new() },
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        self.detail.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (common, mut pos) = CommonUnionMember::deserialize(data)?;
        let (detail, consumed) = CompleteMemberDetail::deserialize(&data[pos..])?;
        pos += consumed;
        Ok((CompleteUnionMember { common, detail }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonDiscriminatorMember {
    pub member_flags: MemberFlag,
    pub type_id: TypeIdentifier,
}

impl CommonDiscriminatorMember {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.member_flags.serialize_into(buffer);
        self.type_id.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (member_flags, consumed) = MemberFlag::deserialize(data)?;
        pos += consumed;
        let (type_id, consumed) = TypeIdentifier::deserialize(&data[pos..])?;
        pos += consumed;
        Ok((CommonDiscriminatorMember { member_flags, type_id }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinimalUnionType {
    pub union_flags: TypeFlag,
    pub discriminator: CommonDiscriminatorMember,
    pub member_seq: Vec<MinimalUnionMember>,
}

impl Default for CommonDiscriminatorMember {
    fn default() -> Self {
        Self { member_flags: MemberFlag::default(), type_id: TypeIdentifier::Int32 }
    }
}

impl MinimalUnionType {
    pub fn new(flags: TypeFlag, disc_flags: MemberFlag, disc_type: TypeIdentifier) -> Self {
        Self {
            union_flags: flags,
            discriminator: CommonDiscriminatorMember {
                member_flags: disc_flags,
                type_id: disc_type,
            },
            member_seq: Vec::new(),
        }
    }

    pub fn add_member(&mut self, member: MinimalUnionMember) {
        self.member_seq.push(member);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.union_flags.serialize_into(buffer);
        self.discriminator.serialize_into(buffer);
        buffer.extend_from_slice(&(self.member_seq.len() as u32).to_le_bytes());
        for member in &self.member_seq {
            member.serialize_into(buffer);
        }
    }

    pub fn compute_hash(&self) -> EquivalenceHash {
        EquivalenceHash::compute(&self.serialize())
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (union_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;
        let (discriminator, consumed) = CommonDiscriminatorMember::deserialize(&data[pos..])?;
        pos += consumed;

        if data.len() < pos + 4 {
            return Err("Insufficient data for union member_seq length".to_string());
        }
        let member_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut member_seq = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            let (member, consumed) = MinimalUnionMember::deserialize(&data[pos..])?;
            pos += consumed;
            member_seq.push(member);
        }

        Ok((MinimalUnionType { union_flags, discriminator, member_seq }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteUnionType {
    pub union_flags: TypeFlag,
    pub discriminator: CommonDiscriminatorMember,
    pub header: CompleteTypeDetail,
    pub member_seq: Vec<CompleteUnionMember>,
}

impl CompleteUnionType {
    pub fn new(
        flags: TypeFlag,
        disc_flags: MemberFlag,
        disc_type: TypeIdentifier,
        type_name: String,
    ) -> Self {
        Self {
            union_flags: flags,
            discriminator: CommonDiscriminatorMember {
                member_flags: disc_flags,
                type_id: disc_type,
            },
            header: CompleteTypeDetail { type_name, ann_builtin: None, ann_custom: Vec::new() },
            member_seq: Vec::new(),
        }
    }

    pub fn add_member(&mut self, member: CompleteUnionMember) {
        self.member_seq.push(member);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.union_flags.serialize_into(buffer);
        self.discriminator.serialize_into(buffer);
        self.header.serialize_into(buffer);
        buffer.extend_from_slice(&(self.member_seq.len() as u32).to_le_bytes());
        for member in &self.member_seq {
            member.serialize_into(buffer);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (union_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;
        let (discriminator, consumed) = CommonDiscriminatorMember::deserialize(&data[pos..])?;
        pos += consumed;
        let (header, consumed) = CompleteTypeDetail::deserialize(&data[pos..])?;
        pos += consumed;

        if data.len() < pos + 4 {
            return Err("Insufficient data for union member_seq length".to_string());
        }
        let member_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut member_seq = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            let (member, consumed) = CompleteUnionMember::deserialize(&data[pos..])?;
            pos += consumed;
            member_seq.push(member);
        }

        Ok((CompleteUnionType { union_flags, discriminator, header, member_seq }, pos))
    }
}

// ============================================================================
// Alias Type Definitions
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonAliasBody {
    pub related_flags: MemberFlag,
    pub related_type: TypeIdentifier,
}

impl CommonAliasBody {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.related_flags.serialize_into(buffer);
        self.related_type.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (related_flags, consumed) = MemberFlag::deserialize(data)?;
        pos += consumed;
        let (related_type, consumed) = TypeIdentifier::deserialize(&data[pos..])?;
        pos += consumed;
        Ok((CommonAliasBody { related_flags, related_type }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinimalAliasType {
    pub alias_flags: TypeFlag,
    pub body: CommonAliasBody,
}

impl Default for CommonAliasBody {
    fn default() -> Self {
        Self { related_flags: MemberFlag::default(), related_type: TypeIdentifier::None }
    }
}

impl MinimalAliasType {
    pub fn new(flags: TypeFlag, related_flags: MemberFlag, related_type: TypeIdentifier) -> Self {
        Self { alias_flags: flags, body: CommonAliasBody { related_flags, related_type } }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.alias_flags.serialize_into(buffer);
        self.body.serialize_into(buffer);
    }

    pub fn compute_hash(&self) -> EquivalenceHash {
        EquivalenceHash::compute(&self.serialize())
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (alias_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;
        let (body, consumed) = CommonAliasBody::deserialize(&data[pos..])?;
        pos += consumed;
        Ok((MinimalAliasType { alias_flags, body }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteAliasType {
    pub alias_flags: TypeFlag,
    pub header: CompleteTypeDetail,
    pub body: CommonAliasBody,
}

impl CompleteAliasType {
    pub fn new(
        flags: TypeFlag,
        type_name: String,
        related_flags: MemberFlag,
        related_type: TypeIdentifier,
    ) -> Self {
        Self {
            alias_flags: flags,
            header: CompleteTypeDetail { type_name, ann_builtin: None, ann_custom: Vec::new() },
            body: CommonAliasBody { related_flags, related_type },
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.alias_flags.serialize_into(buffer);
        self.header.serialize_into(buffer);
        self.body.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (alias_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;
        let (header, consumed) = CompleteTypeDetail::deserialize(&data[pos..])?;
        pos += consumed;
        let (body, consumed) = CommonAliasBody::deserialize(&data[pos..])?;
        pos += consumed;
        Ok((CompleteAliasType { alias_flags, header, body }, pos))
    }
}

// ============================================================================
// Bitmask Type Definitions
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonBitflag {
    pub position: u16,
    pub flags: MemberFlag,
}

impl CommonBitflag {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.position.to_le_bytes());
        self.flags.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 4 {
            return Err("Insufficient data for CommonBitflag".to_string());
        }
        let position = u16::from_le_bytes([data[0], data[1]]);
        let (flags, consumed) = MemberFlag::deserialize(&data[2..])?;
        Ok((CommonBitflag { position, flags }, 2 + consumed))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimalBitflag {
    pub common: CommonBitflag,
    pub name_hash: u32,
}

impl MinimalBitflag {
    pub fn new(position: u16, flags: MemberFlag, name: &str) -> Self {
        Self { common: CommonBitflag { position, flags }, name_hash: compute_name_hash(name) }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        buffer.extend_from_slice(&self.name_hash.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (common, mut pos) = CommonBitflag::deserialize(data)?;
        if data.len() < pos + 4 {
            return Err("Insufficient data for MinimalBitflag name_hash".to_string());
        }
        let name_hash =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;
        Ok((MinimalBitflag { common, name_hash }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteBitflag {
    pub common: CommonBitflag,
    pub detail: CompleteMemberDetail,
}

impl CompleteBitflag {
    pub fn new(position: u16, flags: MemberFlag, name: String) -> Self {
        Self {
            common: CommonBitflag { position, flags },
            detail: CompleteMemberDetail { name, ann_builtin: None, ann_custom: Vec::new() },
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        self.detail.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (common, mut pos) = CommonBitflag::deserialize(data)?;
        let (detail, consumed) = CompleteMemberDetail::deserialize(&data[pos..])?;
        pos += consumed;
        Ok((CompleteBitflag { common, detail }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinimalBitmaskType {
    pub bitmask_flags: TypeFlag,
    pub header: CommonEnumeratedHeader,
    pub flag_seq: Vec<MinimalBitflag>,
}

impl MinimalBitmaskType {
    pub fn new(flags: TypeFlag, bit_bound: u16) -> Self {
        Self {
            bitmask_flags: flags,
            header: CommonEnumeratedHeader { bit_bound },
            flag_seq: Vec::new(),
        }
    }

    pub fn add_flag(&mut self, flag: MinimalBitflag) {
        self.flag_seq.push(flag);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.bitmask_flags.serialize_into(buffer);
        self.header.serialize_into(buffer);
        buffer.extend_from_slice(&(self.flag_seq.len() as u32).to_le_bytes());
        for flag in &self.flag_seq {
            flag.serialize_into(buffer);
        }
    }

    pub fn compute_hash(&self) -> EquivalenceHash {
        EquivalenceHash::compute(&self.serialize())
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (bitmask_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;
        let (header, consumed) = CommonEnumeratedHeader::deserialize(&data[pos..])?;
        pos += consumed;

        if data.len() < pos + 4 {
            return Err("Insufficient data for flag_seq length".to_string());
        }
        let flag_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut flag_seq = Vec::with_capacity(flag_count);
        for _ in 0..flag_count {
            let (flag, consumed) = MinimalBitflag::deserialize(&data[pos..])?;
            pos += consumed;
            flag_seq.push(flag);
        }

        Ok((MinimalBitmaskType { bitmask_flags, header, flag_seq }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteBitmaskType {
    pub bitmask_flags: TypeFlag,
    pub header: CompleteEnumeratedHeader,
    pub flag_seq: Vec<CompleteBitflag>,
}

impl CompleteBitmaskType {
    pub fn new(flags: TypeFlag, type_name: String, bit_bound: u16) -> Self {
        Self {
            bitmask_flags: flags,
            header: CompleteEnumeratedHeader {
                common: CommonEnumeratedHeader { bit_bound },
                detail: CompleteTypeDetail { type_name, ann_builtin: None, ann_custom: Vec::new() },
            },
            flag_seq: Vec::new(),
        }
    }

    pub fn add_flag(&mut self, flag: CompleteBitflag) {
        self.flag_seq.push(flag);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.bitmask_flags.serialize_into(buffer);
        self.header.serialize_into(buffer);
        buffer.extend_from_slice(&(self.flag_seq.len() as u32).to_le_bytes());
        for flag in &self.flag_seq {
            flag.serialize_into(buffer);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (bitmask_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;
        let (header, consumed) = CompleteEnumeratedHeader::deserialize(&data[pos..])?;
        pos += consumed;

        if data.len() < pos + 4 {
            return Err("Insufficient data for flag_seq length".to_string());
        }
        let flag_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut flag_seq = Vec::with_capacity(flag_count);
        for _ in 0..flag_count {
            let (flag, consumed) = CompleteBitflag::deserialize(&data[pos..])?;
            pos += consumed;
            flag_seq.push(flag);
        }

        Ok((CompleteBitmaskType { bitmask_flags, header, flag_seq }, pos))
    }
}

// ============================================================================
// Bitset Type Definitions
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonBitfield {
    pub position: u16,
    pub flags: MemberFlag,
    pub bitcount: u8,
    pub holder_type: TypeIdentifier,
}

impl CommonBitfield {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.position.to_le_bytes());
        self.flags.serialize_into(buffer);
        buffer.push(self.bitcount);
        self.holder_type.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 5 {
            return Err("Insufficient data for CommonBitfield".to_string());
        }
        let mut pos = 0;
        let position = u16::from_le_bytes([data[0], data[1]]);
        pos += 2;
        let (flags, consumed) = MemberFlag::deserialize(&data[pos..])?;
        pos += consumed;
        let bitcount = data[pos];
        pos += 1;
        let (holder_type, consumed) = TypeIdentifier::deserialize(&data[pos..])?;
        pos += consumed;
        Ok((CommonBitfield { position, flags, bitcount, holder_type }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimalBitfield {
    pub common: CommonBitfield,
    pub name_hash: u32,
}

impl MinimalBitfield {
    pub fn new(
        position: u16,
        flags: MemberFlag,
        bitcount: u8,
        holder_type: TypeIdentifier,
        name: &str,
    ) -> Self {
        Self {
            common: CommonBitfield { position, flags, bitcount, holder_type },
            name_hash: compute_name_hash(name),
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        buffer.extend_from_slice(&self.name_hash.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (common, mut pos) = CommonBitfield::deserialize(data)?;
        if data.len() < pos + 4 {
            return Err("Insufficient data for MinimalBitfield name_hash".to_string());
        }
        let name_hash =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;
        Ok((MinimalBitfield { common, name_hash }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteBitfield {
    pub common: CommonBitfield,
    pub detail: CompleteMemberDetail,
}

impl CompleteBitfield {
    pub fn new(
        position: u16,
        flags: MemberFlag,
        bitcount: u8,
        holder_type: TypeIdentifier,
        name: String,
    ) -> Self {
        Self {
            common: CommonBitfield { position, flags, bitcount, holder_type },
            detail: CompleteMemberDetail { name, ann_builtin: None, ann_custom: Vec::new() },
        }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.common.serialize_into(buffer);
        self.detail.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (common, mut pos) = CommonBitfield::deserialize(data)?;
        let (detail, consumed) = CompleteMemberDetail::deserialize(&data[pos..])?;
        pos += consumed;
        Ok((CompleteBitfield { common, detail }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinimalBitsetType {
    pub bitset_flags: TypeFlag,
    pub field_seq: Vec<MinimalBitfield>,
}

impl MinimalBitsetType {
    pub fn new(flags: TypeFlag) -> Self {
        Self { bitset_flags: flags, field_seq: Vec::new() }
    }

    pub fn add_field(&mut self, field: MinimalBitfield) {
        self.field_seq.push(field);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.bitset_flags.serialize_into(buffer);
        buffer.extend_from_slice(&(self.field_seq.len() as u32).to_le_bytes());
        for field in &self.field_seq {
            field.serialize_into(buffer);
        }
    }

    pub fn compute_hash(&self) -> EquivalenceHash {
        EquivalenceHash::compute(&self.serialize())
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (bitset_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;

        if data.len() < pos + 4 {
            return Err("Insufficient data for field_seq length".to_string());
        }
        let field_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut field_seq = Vec::with_capacity(field_count);
        for _ in 0..field_count {
            let (field, consumed) = MinimalBitfield::deserialize(&data[pos..])?;
            pos += consumed;
            field_seq.push(field);
        }

        Ok((MinimalBitsetType { bitset_flags, field_seq }, pos))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteBitsetType {
    pub bitset_flags: TypeFlag,
    pub header: CompleteTypeDetail,
    pub field_seq: Vec<CompleteBitfield>,
}

impl CompleteBitsetType {
    pub fn new(flags: TypeFlag, type_name: String) -> Self {
        Self {
            bitset_flags: flags,
            header: CompleteTypeDetail { type_name, ann_builtin: None, ann_custom: Vec::new() },
            field_seq: Vec::new(),
        }
    }

    pub fn add_field(&mut self, field: CompleteBitfield) {
        self.field_seq.push(field);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.bitset_flags.serialize_into(buffer);
        self.header.serialize_into(buffer);
        buffer.extend_from_slice(&(self.field_seq.len() as u32).to_le_bytes());
        for field in &self.field_seq {
            field.serialize_into(buffer);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let mut pos = 0;
        let (bitset_flags, consumed) = TypeFlag::deserialize(data)?;
        pos += consumed;
        let (header, consumed) = CompleteTypeDetail::deserialize(&data[pos..])?;
        pos += consumed;

        if data.len() < pos + 4 {
            return Err("Insufficient data for field_seq length".to_string());
        }
        let field_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut field_seq = Vec::with_capacity(field_count);
        for _ in 0..field_count {
            let (field, consumed) = CompleteBitfield::deserialize(&data[pos..])?;
            pos += consumed;
            field_seq.push(field);
        }

        Ok((CompleteBitsetType { bitset_flags, header, field_seq }, pos))
    }
}

// ============================================================================
// TypeInformation (DDS-XTypes 1.3 Section 7.6.3.3)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeIdentifierWithSize {
    pub type_id: TypeIdentifier,
    pub typeobject_serialized_size: u32,
}

impl TypeIdentifierWithSize {
    pub fn new(type_id: TypeIdentifier, size: u32) -> Self {
        Self { type_id, typeobject_serialized_size: size }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.type_id.serialize_into(buffer);
        buffer.extend_from_slice(&self.typeobject_serialized_size.to_le_bytes());
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (type_id, mut pos) = TypeIdentifier::deserialize(data)?;
        if data.len() < pos + 4 {
            return Err("Insufficient data for typeobject_serialized_size".to_string());
        }
        let typeobject_serialized_size =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;
        Ok((TypeIdentifierWithSize { type_id, typeobject_serialized_size }, pos))
    }

    /// Write as an APPENDABLE (DELIMIT_CDR2) struct: DHEADER + type_id union + u32 size.
    fn write_xcdr2(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        let pos = s.begin_struct()?;
        s.buffer_mut().extend_from_slice(&self.type_id.serialize());
        s.serialize_u32(self.typeobject_serialized_size)?;
        s.end_struct(pos)
    }

    /// Read an APPENDABLE (DELIMIT_CDR2) TypeIdentifierWithSize.
    fn read_xcdr2(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let (size, start) = d.begin_struct()?;
        let (type_id, consumed) = TypeIdentifier::deserialize(&d.get_data()[d.get_position()..])
            .map_err(|_| CdrError::DeserializationError("TypeIdentifier".to_string()))?;
        d.set_position(d.get_position() + consumed);
        let typeobject_serialized_size = d.deserialize_u32()?;
        d.end_struct(size, start)?;
        Ok(TypeIdentifierWithSize { type_id, typeobject_serialized_size })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeIdentifierWithDependencies {
    pub typeid_with_size: TypeIdentifierWithSize,
    pub dependent_typeids: Vec<TypeIdentifierWithSize>,
}

impl TypeIdentifierWithDependencies {
    pub fn new(typeid_with_size: TypeIdentifierWithSize) -> Self {
        Self { typeid_with_size, dependent_typeids: Vec::new() }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.typeid_with_size.serialize_into(buffer);
        buffer.extend_from_slice(&(self.dependent_typeids.len() as i32).to_le_bytes());
        for dep in &self.dependent_typeids {
            dep.serialize_into(buffer);
        }
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (typeid_with_size, mut pos) = TypeIdentifierWithSize::deserialize(data)?;

        if data.len() < pos + 4 {
            return Err("Insufficient data for dependent_typeid_count".to_string());
        }
        let dep_count =
            i32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        let count = if dep_count < 0 { 0 } else { dep_count as usize };
        // Grow on demand instead of pre-allocating `count` elements: a corrupt
        // count from the wire must not drive an unbounded allocation.
        let mut dependent_typeids = Vec::new();
        for _ in 0..count {
            let (dep, consumed) = TypeIdentifierWithSize::deserialize(&data[pos..])?;
            pos += consumed;
            dependent_typeids.push(dep);
        }

        Ok((TypeIdentifierWithDependencies { typeid_with_size, dependent_typeids }, pos))
    }

    /// Write as an APPENDABLE (DELIMIT_CDR2) struct: DHEADER + typeid_with_size +
    /// i32 dependent_typeid_count + dependent_typeids sequence.
    fn write_xcdr2(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        let pos = s.begin_struct()?;
        self.typeid_with_size.write_xcdr2(s)?;
        s.serialize_i32(self.dependent_typeids.len() as i32)?;
        // sequence<TypeIdentifierWithSize>: XCDR2 prefixes non-primitive collections
        // with a DHEADER, then the element count, then the elements.
        let seq_pos = s.reserve_dheader();
        let seq_start = s.position();
        s.serialize_i32(self.dependent_typeids.len() as i32)?;
        for dep in &self.dependent_typeids {
            dep.write_xcdr2(s)?;
        }
        let seq_len = (s.position() - seq_start) as u32;
        s.write_dheader_at(seq_pos, seq_len);
        s.end_struct(pos)
    }

    /// Read an APPENDABLE (DELIMIT_CDR2) TypeIdentifierWithDependencies.
    fn read_xcdr2(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let (size, start) = d.begin_struct()?;
        let typeid_with_size = TypeIdentifierWithSize::read_xcdr2(d)?;
        let _dependent_typeid_count = d.deserialize_i32()?;
        let _seq = d.begin_struct()?; // sequence DHEADER
        let count = d.deserialize_i32()?;
        let n = if count < 0 { 0 } else { count as usize };
        // Grow on demand; never pre-allocate `n` from an untrusted wire count.
        let mut dependent_typeids = Vec::new();
        for _ in 0..n {
            dependent_typeids.push(TypeIdentifierWithSize::read_xcdr2(d)?);
        }
        d.end_struct(size, start)?;
        Ok(TypeIdentifierWithDependencies { typeid_with_size, dependent_typeids })
    }
}

/// If `data` begins with a recognized CDR/XCDR encapsulation header (2-byte
/// big-endian encoding id), return the body after it plus the indicated
/// endianness. Otherwise return `data` unchanged and assume little-endian.
/// Used to tolerate legacy headerless-vs-headered TypeInformation payloads.
fn strip_optional_encapsulation(data: &[u8]) -> (&[u8], bool) {
    if data.len() >= 4 {
        match u16::from_be_bytes([data[0], data[1]]) {
            0x0000 | 0x0002 | 0x0006 | 0x0008 | 0x000A => return (&data[4..], false),
            0x0001 | 0x0003 | 0x0007 | 0x0009 | 0x000B => return (&data[4..], true),
            _ => {}
        }
    }
    (data, true)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInformation {
    pub minimal: TypeIdentifierWithDependencies,
    pub complete: TypeIdentifierWithDependencies,
}

impl TypeInformation {
    pub fn new(
        minimal: TypeIdentifierWithDependencies,
        complete: TypeIdentifierWithDependencies,
    ) -> Self {
        Self { minimal, complete }
    }

    pub fn from_type_identifier(type_id: TypeIdentifier) -> Self {
        let tws = TypeIdentifierWithSize::new(type_id.clone(), 0);
        Self {
            minimal: TypeIdentifierWithDependencies::new(TypeIdentifierWithSize::new(type_id, 0)),
            complete: TypeIdentifierWithDependencies::new(tws),
        }
    }

    pub fn minimal_type_id(&self) -> &TypeIdentifier {
        &self.minimal.typeid_with_size.type_id
    }

    pub fn complete_type_id(&self) -> &TypeIdentifier {
        &self.complete.typeid_with_size.type_id
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer);
        buffer
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.minimal.serialize_into(buffer);
        self.complete.serialize_into(buffer);
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (minimal, mut pos) = TypeIdentifierWithDependencies::deserialize(data)?;
        let (complete, consumed) = TypeIdentifierWithDependencies::deserialize(&data[pos..])?;
        pos += consumed;
        Ok((TypeInformation { minimal, complete }, pos))
    }

    /// Serialize as a PID_TYPE_INFORMATION (0x0075) parameter payload. Per Fast-DDS
    /// (QosPoliciesSerializer<TypeInformationParameter>) this is a *headerless* XCDR2
    /// PL_CDR2 (little-endian) body: a top-level DHEADER, then members @id 0x1001
    /// (minimal) and @id 0x1002 (complete). Unlike 0x0069/0x0072, there is NO
    /// encapsulation header — the wire bytes begin directly with the DHEADER.
    pub fn serialize_for_parameter(&self) -> Vec<u8> {
        let mut s = Xcdr2Serializer::new(true, crate::serialize::cdr::ExtensibilityKind::Mutable);
        let build = |s: &mut Xcdr2Serializer| -> Result<(), CdrError> {
            let top = s.begin_struct()?;
            // minimal/complete are APPENDABLE (each starts with a DHEADER); emit LC=5
            // so that DHEADER serves as the EMHEADER NEXTINT, matching Fast-CDR.
            s.write_member_with_lc(0x1001, false, LcHint::Dheader, |ser| {
                self.minimal.write_xcdr2(ser)
            })?;
            s.write_member_with_lc(0x1002, false, LcHint::Dheader, |ser| {
                self.complete.write_xcdr2(ser)
            })?;
            s.end_struct(top)
        };
        let _ = build(&mut s);
        s.into_buffer()
    }

    /// Parse a PID_TYPE_INFORMATION (0x0075) parameter payload.
    ///
    /// The standard (Fast-DDS) layout is headerless little-endian PL_CDR2. A legacy
    /// int2DDS payload that still carries a PL_CDR2 encapsulation header is tolerated
    /// by stripping it first.
    pub fn deserialize_for_parameter(data: &[u8]) -> Result<Self, String> {
        let (body, little_endian) = strip_optional_encapsulation(data);
        let mut d = Xcdr2Deserializer::new_without_header(body, little_endian);
        let (top_size, top_start) = d.begin_struct().map_err(|e| format!("{:?}", e))?;
        let top_end = top_start + top_size as usize;

        let mut minimal: Option<TypeIdentifierWithDependencies> = None;
        let mut complete: Option<TypeIdentifierWithDependencies> = None;
        while d.get_position() < top_end {
            let (mid, mlen, _mu) = d.read_member_header_full().map_err(|e| format!("{:?}", e))?;
            let member_start = d.get_position();
            match mid {
                0x1001 => {
                    minimal = Some(
                        TypeIdentifierWithDependencies::read_xcdr2(&mut d)
                            .map_err(|e| format!("{:?}", e))?,
                    )
                }
                0x1002 => {
                    complete = Some(
                        TypeIdentifierWithDependencies::read_xcdr2(&mut d)
                            .map_err(|e| format!("{:?}", e))?,
                    )
                }
                _ => {}
            }
            // Re-sync to the member boundary declared by the EMHEADER regardless of
            // how much the body parser consumed (forward compatibility, unknown members).
            d.set_position(member_start + mlen as usize);
        }

        let empty = || {
            TypeIdentifierWithDependencies::new(TypeIdentifierWithSize::new(
                TypeIdentifier::None,
                0,
            ))
        };
        Ok(TypeInformation {
            minimal: minimal.unwrap_or_else(empty),
            complete: complete.unwrap_or_else(empty),
        })
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
    Union(MinimalUnionType),
    Alias(MinimalAliasType),
    Bitmask(MinimalBitmaskType),
    Bitset(MinimalBitsetType),
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
            MinimalTypeObject::Union(_) => type_object_kind::TK_UNION,
            MinimalTypeObject::Alias(_) => type_object_kind::TK_ALIAS,
            MinimalTypeObject::Bitmask(_) => type_object_kind::TK_BITMASK,
            MinimalTypeObject::Bitset(_) => type_object_kind::TK_BITSET,
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
            MinimalTypeObject::Union(u) => u.serialize_into(buffer),
            MinimalTypeObject::Alias(a) => a.serialize_into(buffer),
            MinimalTypeObject::Bitmask(b) => b.serialize_into(buffer),
            MinimalTypeObject::Bitset(b) => b.serialize_into(buffer),
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
            type_object_kind::TK_UNION => MinimalUnionType::deserialize(&data[1..])
                .map(|(u, consumed)| (MinimalTypeObject::Union(u), 1 + consumed)),
            type_object_kind::TK_ALIAS => MinimalAliasType::deserialize(&data[1..])
                .map(|(a, consumed)| (MinimalTypeObject::Alias(a), 1 + consumed)),
            type_object_kind::TK_BITMASK => MinimalBitmaskType::deserialize(&data[1..])
                .map(|(b, consumed)| (MinimalTypeObject::Bitmask(b), 1 + consumed)),
            type_object_kind::TK_BITSET => MinimalBitsetType::deserialize(&data[1..])
                .map(|(b, consumed)| (MinimalTypeObject::Bitset(b), 1 + consumed)),
            _ => Err(format!("Unsupported MinimalTypeObject kind: 0x{:02X}", discriminator)),
        }
    }
}

/// Complete TypeObject - full type description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteTypeObject {
    Struct(CompleteStructType),
    Enum(CompleteEnumeratedType),
    Union(CompleteUnionType),
    Alias(CompleteAliasType),
    Bitmask(CompleteBitmaskType),
    Bitset(CompleteBitsetType),
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
            CompleteTypeObject::Union(_) => type_object_kind::TK_UNION,
            CompleteTypeObject::Alias(_) => type_object_kind::TK_ALIAS,
            CompleteTypeObject::Bitmask(_) => type_object_kind::TK_BITMASK,
            CompleteTypeObject::Bitset(_) => type_object_kind::TK_BITSET,
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
            CompleteTypeObject::Union(u) => u.serialize_into(buffer),
            CompleteTypeObject::Alias(a) => a.serialize_into(buffer),
            CompleteTypeObject::Bitmask(b) => b.serialize_into(buffer),
            CompleteTypeObject::Bitset(b) => b.serialize_into(buffer),
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
            type_object_kind::TK_UNION => CompleteUnionType::deserialize(&data[1..])
                .map(|(u, consumed)| (CompleteTypeObject::Union(u), 1 + consumed)),
            type_object_kind::TK_ALIAS => CompleteAliasType::deserialize(&data[1..])
                .map(|(a, consumed)| (CompleteTypeObject::Alias(a), 1 + consumed)),
            type_object_kind::TK_BITMASK => CompleteBitmaskType::deserialize(&data[1..])
                .map(|(b, consumed)| (CompleteTypeObject::Bitmask(b), 1 + consumed)),
            type_object_kind::TK_BITSET => CompleteBitsetType::deserialize(&data[1..])
                .map(|(b, consumed)| (CompleteTypeObject::Bitset(b), 1 + consumed)),
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

    /// Serialize as a PID_TYPE_OBJECTV1 (0x0072) parameter payload: a CDR_LE (XCDRv1)
    /// encapsulation header followed by the legacy TypeObject body.
    pub fn serialize_for_parameter(&self) -> Vec<u8> {
        let mut buffer = vec![0x00, 0x01, 0x00, 0x00];
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

    fn collect_nested_type_objects(_out: &mut Vec<(TypeIdentifier, TypeObject)>) {}
}

pub mod nested_closure {
    use super::{HasTypeObject, TypeIdentifier, TypeObject};

    pub struct Probe<T>(pub core::marker::PhantomData<T>);

    /// Fallback for field types that do NOT implement `HasTypeObject`.
    pub trait CollectFallback {
        fn collect_nested(&self, _out: &mut Vec<(TypeIdentifier, TypeObject)>) {}
    }
    impl<T> CollectFallback for Probe<T> {}

    /// Preferred path for field types that implement `HasTypeObject`.
    impl<T: HasTypeObject> Probe<T> {
        pub fn collect_nested(&self, out: &mut Vec<(TypeIdentifier, TypeObject)>) {
            T::collect_nested_type_objects(out);
        }
    }
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

    fn collect_nested_type_objects(out: &mut Vec<(TypeIdentifier, TypeObject)>) {
        T::collect_nested_type_objects(out);
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

    fn collect_nested_type_objects(out: &mut Vec<(TypeIdentifier, TypeObject)>) {
        T::collect_nested_type_objects(out);
    }
}

impl<T: HasTypeObject> HasTypeObject for Box<T> {
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

    fn collect_nested_type_objects(out: &mut Vec<(TypeIdentifier, TypeObject)>) {
        T::collect_nested_type_objects(out);
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

                fn collect_nested_type_objects(out: &mut Vec<(TypeIdentifier, TypeObject)>) {
                    T::collect_nested_type_objects(out);
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
    CdrDeserialize, CdrError, CdrResult, CdrSerialize, CdrSerializer, CdrSerializerCommon, LcHint,
    PrimitiveSerialize, Xcdr2Deserializer, Xcdr2Serializer, XcdrDeserialize, XcdrResult,
    XcdrSerialize,
};
use crate::serialize::DeserializerReader;

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

// Speedy traits for TypeInformation
impl<C: Context> Writable<C> for TypeInformation {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        let bytes = self.serialize();
        let len = bytes.len() as u32;
        writer.write_value(&len)?;
        writer.write_bytes(&bytes)?;
        Ok(())
    }
}

impl<'a, C: Context> Readable<'a, C> for TypeInformation {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let len: u32 = reader.read_value()?;
        let bytes = reader.read_vec(len as usize)?;
        TypeInformation::deserialize(&bytes)
            .map(|(obj, _)| obj)
            .map_err(|_| speedy::Error::custom("Failed to deserialize TypeInformation").into())
    }
}

// CDR/XCDR traits for TypeInformation
impl CdrSerialize for TypeInformation {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        let bytes = self.serialize();
        let len = bytes.len() as u32;
        serializer.serialize_u32(len)?;
        serializer.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }
}

impl CdrDeserialize for TypeInformation {
    fn deserialize_cdr(
        deserializer: &mut crate::serialize::cdr::CdrDeserializer,
    ) -> CdrResult<Self> {
        let len = deserializer.deserialize_u32()? as usize;
        let bytes = deserializer.deserialize_byte_array(len)?;
        TypeInformation::deserialize(&bytes)
            .map(|(obj, _)| obj)
            .map_err(|e| crate::serialize::cdr::CdrError::DeserializationError(e))
    }
}

impl XcdrSerialize for TypeInformation {
    fn serialize_xcdr(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        let bytes = self.serialize();
        let len = bytes.len() as u32;
        serializer.serialize_u32(len)?;
        serializer.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }
}

impl XcdrDeserialize for TypeInformation {
    fn deserialize_xcdr(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self> {
        let len = deserializer.deserialize_u32()? as usize;
        let bytes = deserializer.deserialize_byte_array(len)?;
        TypeInformation::deserialize(&bytes)
            .map(|(obj, _)| obj)
            .map_err(|e| crate::serialize::cdr::XcdrError::DeserializationError(e))
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
