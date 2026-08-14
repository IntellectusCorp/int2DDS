mod cdr_input;
pub mod deserializer;
mod prim_bulk;
pub mod serializer;
pub mod xcdr1;
#[cfg(test)]
mod xcdr1_wire_tests;
pub mod xcdr2;
#[cfg(test)]
mod xcdr2_wire_tests;

use crate::core::{SerializationError, SerializationResult};

// Re-export v1 (CDR) types
pub use xcdr1::{CdrDeserializer, CdrSerializer, PlCdrMemberHeader};

// Re-export v2 (XCDR2) types
pub use xcdr2::{Xcdr2Deserializer, Xcdr2Serializer};

// Re-export serializer traits for unified API
pub use serializer::array::ArraySerialize;
pub use serializer::primitive::PrimitiveSerialize;
pub use serializer::sequence::SequenceSerialize;
pub use serializer::string::StringSerialize;
pub use serializer::CdrSerializerCommon;

/// XCDR v2 extensibility kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensibilityKind {
    Final,      // No extensibility, fastest serialization
    Appendable, // Can append new members at the end
    Mutable,    // Full extensibility with member headers
}

/// XCDR encoding kinds - Standard RTPS encapsulation identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingKind {
    // XCDR1 (Legacy CDR)
    CdrBe = 0x0000,   // CDR Big Endian
    CdrLe = 0x0001,   // CDR Little Endian
    PlCdrBe = 0x0002, // PL_CDR Big Endian (MUTABLE v1)
    PlCdrLe = 0x0003, // PL_CDR Little Endian (MUTABLE v1)

    // XCDR2 (Extended CDR version 2)
    PlainCdr2Be = 0x0006, // PLAINCDR2 Big Endian (FINAL v2)
    PlainCdr2Le = 0x0007, // PLAINCDR2 Little Endian (FINAL v2)
    DCdr2Be = 0x0008,     // DELIMITED_CDR Big Endian (APPENDABLE v2)
    DCdr2Le = 0x0009,     // DELIMITED_CDR Little Endian (APPENDABLE v2)
    PlCdr2Be = 0x000A,    // PL_CDR2 Big Endian (MUTABLE v2)
    PlCdr2Le = 0x000B,    // PL_CDR2 Little Endian (MUTABLE v2)
}

/// Member header for mutable extensibility
#[derive(Debug, Clone)]
pub struct MemberHeader {
    pub member_id: u32,
    pub member_length: u32,
    pub must_understand: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LcHint {
    Auto,
    SeqMul4,
    SeqMul8,
    Dheader,
}

impl MemberHeader {
    pub fn new(member_id: u32, length: usize) -> Self {
        Self { member_id, member_length: length as u32, must_understand: false }
    }

    pub fn with_must_understand(member_id: u32, length: usize, must_understand: bool) -> Self {
        Self { member_id, member_length: length as u32, must_understand }
    }

    /// Write EMHEADER1 per DDS-XTypes 7.4.3.4.2.
    pub fn write(
        &self,
        buffer: &mut Vec<u8>,
        endianness: speedy::Endianness,
    ) -> Result<(), SerializationError> {
        use crate::to_bytes_u32;

        if self.member_id > 0x0FFF_FFFF {
            return Err(SerializationError::InvalidMemberId(self.member_id));
        }

        let must_bit = if self.must_understand { 0x8000_0000u32 } else { 0 };
        let (lc_word, nextint) = match self.member_length {
            1 => (0u32 << 28, None),
            2 => (1u32 << 28, None),
            4 => (2u32 << 28, None),
            8 => (3u32 << 28, None),
            n => (4u32 << 28, Some(n)),
        };
        let header = must_bit | lc_word | (self.member_id & 0x0FFF_FFFF);
        buffer.extend_from_slice(&to_bytes_u32(header, endianness));
        if let Some(len) = nextint {
            buffer.extend_from_slice(&to_bytes_u32(len, endianness));
        }
        Ok(())
    }

    /// Read EMHEADER from buffer
    /// Returns (MemberHeader, bytes_consumed) on success
    pub fn read(
        data: &[u8],
        position: usize,
        endianness: speedy::Endianness,
    ) -> Result<(Self, usize), SerializationError> {
        use crate::from_bytes_u32;

        if position + 4 > data.len() {
            return Err(SerializationError::InsufficientData);
        }

        let header_bytes: [u8; 4] = data[position..position + 4]
            .try_into()
            .map_err(|_| SerializationError::InsufficientData)?;
        let header = from_bytes_u32(header_bytes, endianness);

        let must_understand = (header & 0x8000_0000) != 0;
        let lc = ((header >> 28) & 0x07) as u8;
        let member_id = header & 0x0FFF_FFFF;

        let read_nextint = |buf: &[u8]| -> Result<u32, SerializationError> {
            if position + 8 > buf.len() {
                return Err(SerializationError::InsufficientData);
            }
            let b: [u8; 4] = buf[position + 4..position + 8]
                .try_into()
                .map_err(|_| SerializationError::InsufficientData)?;
            Ok(from_bytes_u32(b, endianness))
        };

        // LC=5/6/7: NEXTINT overlaps with payload's first 4 bytes (DDS-XTypes 7.4.3.4.2)
        let (member_length, bytes_consumed) = match lc {
            0 => (1u32, 4usize),
            1 => (2u32, 4usize),
            2 => (4u32, 4usize),
            3 => (8u32, 4usize),
            4 => (read_nextint(data)?, 8usize),
            5 => (4u32 + read_nextint(data)?, 4usize),
            6 => (4u32 + 4 * read_nextint(data)?, 4usize),
            7 => (4u32 + 8 * read_nextint(data)?, 4usize),
            _ => return Err(SerializationError::InvalidMemberHeader),
        };

        Ok((MemberHeader { member_id, member_length, must_understand }, bytes_consumed))
    }
}

/// Legacy CDR error type for backward compatibility
pub type CdrError = SerializationError;

/// Legacy result type for backward compatibility
pub type CdrResult<T> = Result<T, SerializationError>;

/// Legacy XCDR error type for backward compatibility
pub type XcdrError = SerializationError;

/// Legacy XCDR result type for backward compatibility
pub type XcdrResult<T> = SerializationResult<T>;

// Legacy type aliases for backward compatibility
pub type XcdrSerializer = Xcdr2Serializer;
pub type XcdrDeserializer<'a> = Xcdr2Deserializer<'a>;

/// Trait for types that can be serialized using CDR
pub trait CdrSerialize {
    /// Whether this type is a CDR primitive. Unused by classic CDR encoding
    /// (no DHEADER) but defined for symmetry with [`XcdrSerialize`] so the
    /// shared `impl_primitive_serialization!` macro can set it uniformly.
    const IS_PRIMITIVE: bool = false;
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()>;
}

/// Trait for types that can be deserialized using CDR
pub trait CdrDeserialize: Sized {
    /// See [`CdrSerialize::IS_PRIMITIVE`].
    const IS_PRIMITIVE: bool = false;
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self>;
}

crate::impl_primitive_serialization!(
    trait Serialize = CdrSerialize::serialize_cdr,
    trait Deserialize = CdrDeserialize::deserialize_cdr,
    serializer = CdrSerializer,
    deserializer = CdrDeserializer,
    result = CdrResult,
    {
        bool => (serialize_bool, deserialize_bool),
        u8 => (serialize_u8, deserialize_u8),
        i8 => (serialize_i8, deserialize_i8),
        u16 => (serialize_u16, deserialize_u16),
        i16 => (serialize_i16, deserialize_i16),
        u32 => (serialize_u32, deserialize_u32),
        i32 => (serialize_i32, deserialize_i32),
        u64 => (serialize_u64, deserialize_u64),
        i64 => (serialize_i64, deserialize_i64),
        f32 => (serialize_f32, deserialize_f32),
        f64 => (serialize_f64, deserialize_f64),
        char => (serialize_char, deserialize_char)
    }
);

impl CdrSerialize for String {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        serializer.serialize_string(self)
    }
}

impl CdrDeserialize for String {
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
        deserializer.deserialize_string()
    }
}

impl<T: CdrSerialize> CdrSerialize for Vec<T> {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        // Write sequence length
        serializer.serialize_u32(self.len() as u32)?;

        // Write sequence elements
        for item in self {
            item.serialize_cdr(serializer)?;
        }
        Ok(())
    }
}

impl<T: CdrDeserialize> CdrDeserialize for Vec<T> {
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
        deserializer.deserialize_sequence(|d| T::deserialize_cdr(d))
    }
}

impl<T: CdrSerialize, const N: usize> CdrSerialize for [T; N] {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        // Arrays are fixed size, no need to write length
        for item in self.iter() {
            item.serialize_cdr(serializer)?;
        }
        Ok(())
    }
}

impl<T: CdrDeserialize, const N: usize> CdrDeserialize for [T; N] {
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
        // Collect into Vec and try to convert to array
        let mut vec = Vec::with_capacity(N);
        for _ in 0..N {
            vec.push(T::deserialize_cdr(deserializer)?);
        }
        vec.try_into().map_err(|_| {
            SerializationError::DeserializationError("Failed to convert Vec to array".to_string())
        })
    }
}

pub trait XcdrSerializeMembers {
    fn serialize_xcdr_members(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()>;
}

pub trait XcdrDeserializeMembers: Sized {
    fn deserialize_xcdr_members(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self>;
}

/// Trait for types that can be serialized using XCDR
pub trait XcdrSerialize {
    /// Whether this type is a CDR primitive (no DHEADER required when used as
    /// the element type of a sequence/array per DDS-XTypes 7.4.3.5.3-4).
    /// Defaults to `false`; primitive impls override to `true`.
    const IS_PRIMITIVE: bool = false;
    /// Primitiveness of the base element once nested arrays are flattened. Only
    /// `[T; N]` overrides it; it decides whether an array frames itself.
    const BASE_IS_PRIMITIVE: bool = Self::IS_PRIMITIVE;
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()>;
    /// Write this value as an element of an enclosing array. Multidimensional IDL
    /// arrays serialize as one flat array (DDS-XTypes 7.4.3.4), so a nested array
    /// contributes its elements without a DHEADER of its own.
    fn serialize_xcdr_unframed(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        self.serialize_xcdr(serializer)
    }
}

/// Trait for types that can be deserialized using XCDR
pub trait XcdrDeserialize: Sized {
    /// See [`XcdrSerialize::IS_PRIMITIVE`].
    const IS_PRIMITIVE: bool = false;
    /// See [`XcdrSerialize::BASE_IS_PRIMITIVE`].
    const BASE_IS_PRIMITIVE: bool = Self::IS_PRIMITIVE;
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self>;
    /// See [`XcdrSerialize::serialize_xcdr_unframed`].
    fn deserialize_xcdr_unframed(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        Self::deserialize_xcdr(deserializer)
    }
}

crate::impl_primitive_serialization!(
    trait Serialize = XcdrSerialize::serialize_xcdr,
    trait Deserialize = XcdrDeserialize::deserialize_xcdr,
    serializer = XcdrSerializer,
    deserializer = XcdrDeserializer,
    result = XcdrResult,
    {
        bool => (serialize_bool, deserialize_bool),
        u8 => (serialize_u8, deserialize_u8),
        i8 => (serialize_i8, deserialize_i8),
        u16 => (serialize_u16, deserialize_u16),
        i16 => (serialize_i16, deserialize_i16),
        u32 => (serialize_u32, deserialize_u32),
        i32 => (serialize_i32, deserialize_i32),
        u64 => (serialize_u64, deserialize_u64),
        i64 => (serialize_i64, deserialize_i64),
        f32 => (serialize_f32, deserialize_f32),
        f64 => (serialize_f64, deserialize_f64),
        char => (serialize_char, deserialize_char)
    }
);

impl XcdrSerialize for String {
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        serializer.serialize_string(self)
    }
}

impl XcdrDeserialize for String {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        deserializer.deserialize_string()
    }
}

impl<T: XcdrSerialize> XcdrSerialize for Vec<T> {
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        // DDS-XTypes 7.4.3.5.4: a sequence of non-primitive elements is preceded by a
        // DHEADER carrying the byte size of length + elements. Primitives omit it.
        if T::IS_PRIMITIVE {
            serializer.serialize_u32(self.len() as u32)?;
            serializer.buffer_mut().reserve(self.len() * std::mem::size_of::<T>());
            for item in self {
                item.serialize_xcdr(serializer)?;
            }
            Ok(())
        } else {
            let dheader_pos = serializer.reserve_dheader();
            let content_start = serializer.position();
            serializer.serialize_u32(self.len() as u32)?;
            for item in self {
                item.serialize_xcdr(serializer)?;
            }
            let content_size = (serializer.position() - content_start) as u32;
            serializer.write_dheader_at(dheader_pos, content_size);
            Ok(())
        }
    }
}

impl<T: XcdrDeserialize> XcdrDeserialize for Vec<T> {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        if !T::IS_PRIMITIVE {
            let _object_size = deserializer.read_dheader()?;
        }
        let length = deserializer.deserialize_u32()? as usize;
        let capacity = deserializer.checked_capacity(length, 1)?;
        let mut result = Vec::with_capacity(capacity);
        for _ in 0..length {
            result.push(T::deserialize_xcdr(deserializer)?);
        }
        Ok(result)
    }
}

impl<T: XcdrSerialize> XcdrSerialize for Option<T> {
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        match self {
            Some(value) => {
                serializer.serialize_bool(true)?;
                value.serialize_xcdr(serializer)?;
            }
            None => {
                serializer.serialize_bool(false)?;
            }
        }
        Ok(())
    }
}

impl<T: XcdrDeserialize> XcdrDeserialize for Option<T> {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        deserializer.deserialize_optional(|d| T::deserialize_xcdr(d))
    }
}

// A multidimensional IDL array (`long a[2][3]`) is one array of the base type
// (DDS-XTypes 7.4.3.4), so the nested `[[T; N]; M]` that represents it carries a single
// DHEADER over all elements: the outer array frames, inner arrays contribute unframed.
// An array is never itself a primitive type, so an enclosing sequence/map always frames.
impl<T: XcdrSerialize, const N: usize> XcdrSerialize for [T; N] {
    const BASE_IS_PRIMITIVE: bool = T::BASE_IS_PRIMITIVE;

    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        // DDS-XTypes 7.4.3.5.3: arrays of non-primitive elements carry a DHEADER of the
        // element payload byte size (and no element count); primitive arrays do not.
        if Self::BASE_IS_PRIMITIVE {
            self.serialize_xcdr_unframed(serializer)
        } else {
            let dheader_pos = serializer.reserve_dheader();
            let content_start = serializer.position();
            self.serialize_xcdr_unframed(serializer)?;
            let content_size = (serializer.position() - content_start) as u32;
            serializer.write_dheader_at(dheader_pos, content_size);
            Ok(())
        }
    }

    fn serialize_xcdr_unframed(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        for item in self.iter() {
            item.serialize_xcdr_unframed(serializer)?;
        }
        Ok(())
    }
}

impl<T: XcdrDeserialize, const N: usize> XcdrDeserialize for [T; N] {
    const BASE_IS_PRIMITIVE: bool = T::BASE_IS_PRIMITIVE;

    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        if !Self::BASE_IS_PRIMITIVE {
            let _object_size = deserializer.read_dheader()?;
        }
        Self::deserialize_xcdr_unframed(deserializer)
    }

    fn deserialize_xcdr_unframed(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        let mut vec = Vec::with_capacity(N);
        for _ in 0..N {
            vec.push(T::deserialize_xcdr_unframed(deserializer)?);
        }
        vec.try_into().map_err(|_| {
            SerializationError::DeserializationError("Failed to convert Vec to array".to_string())
        })
    }
}

// HashMap and BTreeMap support
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

impl<K, V> CdrSerialize for HashMap<K, V>
where
    K: CdrSerialize + Eq + Hash,
    V: CdrSerialize,
{
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        // Write map length
        serializer.serialize_u32(self.len() as u32)?;

        // Write key-value pairs
        for (key, value) in self {
            key.serialize_cdr(serializer)?;
            value.serialize_cdr(serializer)?;
        }
        Ok(())
    }
}

impl<K, V> CdrDeserialize for HashMap<K, V>
where
    K: CdrDeserialize + Eq + Hash,
    V: CdrDeserialize,
{
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
        // Read map length
        let len = deserializer.deserialize_u32()? as usize;

        // Read key-value pairs
        let mut map = HashMap::new();
        for _ in 0..len {
            let key = K::deserialize_cdr(deserializer)?;
            let value = V::deserialize_cdr(deserializer)?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

impl<K, V> XcdrSerialize for HashMap<K, V>
where
    K: XcdrSerialize + Eq + Hash,
    V: XcdrSerialize,
{
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        if K::IS_PRIMITIVE && V::IS_PRIMITIVE {
            serializer.serialize_u32(self.len() as u32)?;
            for (key, value) in self {
                key.serialize_xcdr(serializer)?;
                value.serialize_xcdr(serializer)?;
            }
            Ok(())
        } else {
            let dh = serializer.reserve_dheader();
            let start = serializer.position();
            serializer.serialize_u32(self.len() as u32)?;
            for (key, value) in self {
                key.serialize_xcdr(serializer)?;
                value.serialize_xcdr(serializer)?;
            }
            let size = (serializer.position() - start) as u32;
            serializer.write_dheader_at(dh, size);
            Ok(())
        }
    }
}

impl<K, V> XcdrDeserialize for HashMap<K, V>
where
    K: XcdrDeserialize + Eq + Hash,
    V: XcdrDeserialize,
{
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        if K::IS_PRIMITIVE && V::IS_PRIMITIVE {
            let len = deserializer.deserialize_u32()? as usize;
            let mut map = HashMap::new();
            for _ in 0..len {
                let key = K::deserialize_xcdr(deserializer)?;
                let value = V::deserialize_xcdr(deserializer)?;
                map.insert(key, value);
            }
            Ok(map)
        } else {
            let _dheader = deserializer.read_dheader()?;
            let len = deserializer.deserialize_u32()? as usize;
            let mut map = HashMap::new();
            for _ in 0..len {
                let key = K::deserialize_xcdr(deserializer)?;
                let value = V::deserialize_xcdr(deserializer)?;
                map.insert(key, value);
            }
            Ok(map)
        }
    }
}

impl<K, V> CdrSerialize for BTreeMap<K, V>
where
    K: CdrSerialize + Ord,
    V: CdrSerialize,
{
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        // Write map length
        serializer.serialize_u32(self.len() as u32)?;

        // Write key-value pairs
        for (key, value) in self {
            key.serialize_cdr(serializer)?;
            value.serialize_cdr(serializer)?;
        }
        Ok(())
    }
}

impl<K, V> CdrDeserialize for BTreeMap<K, V>
where
    K: CdrDeserialize + Ord,
    V: CdrDeserialize,
{
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
        // Read map length
        let len = deserializer.deserialize_u32()? as usize;

        // Read key-value pairs
        let mut map = BTreeMap::new();
        for _ in 0..len {
            let key = K::deserialize_cdr(deserializer)?;
            let value = V::deserialize_cdr(deserializer)?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

impl<K, V> XcdrSerialize for BTreeMap<K, V>
where
    K: XcdrSerialize + Ord,
    V: XcdrSerialize,
{
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        if K::IS_PRIMITIVE && V::IS_PRIMITIVE {
            serializer.serialize_u32(self.len() as u32)?;
            for (key, value) in self {
                key.serialize_xcdr(serializer)?;
                value.serialize_xcdr(serializer)?;
            }
            Ok(())
        } else {
            let dh = serializer.reserve_dheader();
            let start = serializer.position();
            serializer.serialize_u32(self.len() as u32)?;
            for (key, value) in self {
                key.serialize_xcdr(serializer)?;
                value.serialize_xcdr(serializer)?;
            }
            let size = (serializer.position() - start) as u32;
            serializer.write_dheader_at(dh, size);
            Ok(())
        }
    }
}

impl<K, V> XcdrDeserialize for BTreeMap<K, V>
where
    K: XcdrDeserialize + Ord,
    V: XcdrDeserialize,
{
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        if K::IS_PRIMITIVE && V::IS_PRIMITIVE {
            let len = deserializer.deserialize_u32()? as usize;
            let mut map = BTreeMap::new();
            for _ in 0..len {
                let key = K::deserialize_xcdr(deserializer)?;
                let value = V::deserialize_xcdr(deserializer)?;
                map.insert(key, value);
            }
            Ok(map)
        } else {
            let _dheader = deserializer.read_dheader()?;
            let len = deserializer.deserialize_u32()? as usize;
            let mut map = BTreeMap::new();
            for _ in 0..len {
                let key = K::deserialize_xcdr(deserializer)?;
                let value = V::deserialize_xcdr(deserializer)?;
                map.insert(key, value);
            }
            Ok(map)
        }
    }
}

// Box<T> support for @external
impl<T: CdrSerialize> CdrSerialize for Box<T> {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        (**self).serialize_cdr(serializer)
    }
}

impl<T: CdrDeserialize> CdrDeserialize for Box<T> {
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
        Ok(Box::new(T::deserialize_cdr(deserializer)?))
    }
}

impl<T: XcdrSerialize> XcdrSerialize for Box<T> {
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        (**self).serialize_xcdr(serializer)
    }
}

impl<T: XcdrDeserialize> XcdrDeserialize for Box<T> {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        Ok(Box::new(T::deserialize_xcdr(deserializer)?))
    }
}

impl<T: XcdrSerializeMembers> XcdrSerializeMembers for Box<T> {
    fn serialize_xcdr_members(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        (**self).serialize_xcdr_members(serializer)
    }
}

impl<T: XcdrDeserializeMembers> XcdrDeserializeMembers for Box<T> {
    fn deserialize_xcdr_members(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        Ok(Box::new(T::deserialize_xcdr_members(deserializer)?))
    }
}

// Wstring support
use crate::core::WString;

impl CdrSerialize for WString {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        serializer.serialize_wstring16(self.as_str())
    }
}

impl CdrDeserialize for WString {
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
        let s = deserializer.deserialize_wstring16()?;
        Ok(WString::from(s))
    }
}

impl XcdrSerialize for WString {
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        serializer.serialize_wstring16(self.as_str())
    }
}

impl XcdrDeserialize for WString {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        let s = deserializer.deserialize_wstring16()?;
        Ok(WString::from(s))
    }
}

// WChar support
use crate::core::WChar;

impl CdrSerialize for WChar {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        serializer.serialize_wchar16(self.as_char())
    }
}

impl CdrDeserialize for WChar {
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
        let c = deserializer.deserialize_wchar16()?;
        Ok(WChar::from(c))
    }
}

impl XcdrSerialize for WChar {
    const IS_PRIMITIVE: bool = true;
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        serializer.serialize_wchar16(self.as_char())
    }
}

impl XcdrDeserialize for WChar {
    const IS_PRIMITIVE: bool = true;
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        let c = deserializer.deserialize_wchar16()?;
        Ok(WChar::from(c))
    }
}
