pub mod deserializer;
pub mod serializer;
pub mod xcdr1;
pub mod xcdr2;

use crate::serialize::core::{SerializationError, SerializationResult};

// Re-export v1 (CDR) types
pub use xcdr1::{CdrDeserializer, CdrSerializer};

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

/// PL_CDR2 sentinel member_id indicating end of mutable struct members
pub const MEMBER_ID_SENTINEL: u32 = 0x3F02;

/// Check if a member_id is the list terminator sentinel
#[inline]
pub fn is_sentinel_member_id(member_id: u32) -> bool {
    member_id == MEMBER_ID_SENTINEL
}

/// EMHEADER Length Code values (LC field in bits 30-28)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthCode {
    /// LC 0-3: Length is directly in lower 16 bits (0-65535 bytes)
    Direct = 0,
    /// LC 4: Next 4 bytes contain the actual length
    NextInt = 4,
    /// LC 5: Length is (next 4 bytes) * 4
    NextIntMul4 = 5,
    /// LC 6: Length is (next 4 bytes) * 8
    NextIntMul8 = 6,
    /// LC 7: Nested length (complex case)
    Nested = 7,
}

impl MemberHeader {
    pub fn new(member_id: u32, length: usize) -> Self {
        Self { member_id, member_length: length as u32, must_understand: false }
    }

    pub fn with_must_understand(member_id: u32, length: usize, must_understand: bool) -> Self {
        Self { member_id, member_length: length as u32, must_understand }
    }

    /// Write EMHEADER to buffer
    /// Supports extended length encoding (LC) for lengths > 65535 bytes
    pub fn write(
        &self,
        buffer: &mut Vec<u8>,
        endianness: speedy::Endianness,
    ) -> Result<(), SerializationError> {
        use crate::serialize::to_bytes_u32;

        let must_understand_bit = if self.must_understand { 0x8000_0000u32 } else { 0 };

        if self.member_length <= 0xFFFF {
            // Short encoding: LC=0, length directly in lower 16 bits
            // Format: [M][LC=0][member_id (12 bits)][length (16 bits)]
            // Simplified: (member_id << 16) | length
            let header =
                must_understand_bit | (self.member_id << 16) | (self.member_length & 0xFFFF);
            let bytes = to_bytes_u32(header, endianness);
            buffer.extend_from_slice(&bytes);
        } else {
            // Extended encoding with LC=4: length follows in next 4 bytes
            // Format: [M][LC=4][member_id (12 bits)][0000]
            let lc = LengthCode::NextInt as u32;
            let header = must_understand_bit | (lc << 28) | (self.member_id << 16);
            let bytes = to_bytes_u32(header, endianness);
            buffer.extend_from_slice(&bytes);
            // Write actual length as next 4 bytes
            let len_bytes = to_bytes_u32(self.member_length, endianness);
            buffer.extend_from_slice(&len_bytes);
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
        use crate::serialize::from_bytes_u32;

        if position + 4 > data.len() {
            return Err(SerializationError::InsufficientData);
        }

        let header_bytes: [u8; 4] = data[position..position + 4]
            .try_into()
            .map_err(|_| SerializationError::InsufficientData)?;
        let header = from_bytes_u32(header_bytes, endianness);

        // Parse EMHEADER fields
        let must_understand = (header & 0x8000_0000) != 0;
        let lc = ((header >> 28) & 0x07) as u8; // LC is bits 30-28
        let member_id = (header >> 16) & 0x0FFF; // member_id is bits 27-16 (12 bits)
        let length_or_flags = header & 0xFFFF; // lower 16 bits

        let (member_length, bytes_consumed) = match lc {
            0 | 1 | 2 | 3 => {
                // LC 0-3: Direct length encoding
                // For LC 0-3, length is directly in lower 16 bits
                (length_or_flags, 4)
            }
            4 => {
                // LC 4: Next 4 bytes contain the actual length
                if position + 8 > data.len() {
                    return Err(SerializationError::InsufficientData);
                }
                let ext_bytes: [u8; 4] = data[position + 4..position + 8]
                    .try_into()
                    .map_err(|_| SerializationError::InsufficientData)?;
                let ext_len = from_bytes_u32(ext_bytes, endianness);
                (ext_len, 8)
            }
            5 => {
                // LC 5: Length is (next 4 bytes) * 4
                if position + 8 > data.len() {
                    return Err(SerializationError::InsufficientData);
                }
                let ext_bytes: [u8; 4] = data[position + 4..position + 8]
                    .try_into()
                    .map_err(|_| SerializationError::InsufficientData)?;
                let ext_len = from_bytes_u32(ext_bytes, endianness);
                (ext_len * 4, 8)
            }
            6 => {
                // LC 6: Length is (next 4 bytes) * 8
                if position + 8 > data.len() {
                    return Err(SerializationError::InsufficientData);
                }
                let ext_bytes: [u8; 4] = data[position + 4..position + 8]
                    .try_into()
                    .map_err(|_| SerializationError::InsufficientData)?;
                let ext_len = from_bytes_u32(ext_bytes, endianness);
                (ext_len * 8, 8)
            }
            7 => {
                // LC 7: Nested length - use lower 16 bits as length
                (length_or_flags, 4)
            }
            _ => {
                // Should not happen with 3-bit LC
                (length_or_flags, 4)
            }
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
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()>;
}

/// Trait for types that can be deserialized using CDR
pub trait CdrDeserialize: Sized {
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

impl<T: CdrSerialize> CdrSerialize for Option<T> {
    fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
        match self {
            Some(value) => {
                serializer.serialize_bool(true)?;
                value.serialize_cdr(serializer)?;
            }
            None => {
                serializer.serialize_bool(false)?;
            }
        }
        Ok(())
    }
}

impl<T: CdrDeserialize> CdrDeserialize for Option<T> {
    fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
        deserializer.deserialize_optional(|d| T::deserialize_cdr(d))
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

/// Trait for types that can be serialized using XCDR
pub trait XcdrSerialize {
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()>;
}

/// Trait for types that can be deserialized using XCDR
pub trait XcdrDeserialize: Sized {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self>;
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
        // Write sequence length
        serializer.serialize_u32(self.len() as u32)?;

        // Write sequence elements
        for item in self {
            item.serialize_xcdr(serializer)?;
        }
        Ok(())
    }
}

impl<T: XcdrDeserialize> XcdrDeserialize for Vec<T> {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        deserializer.deserialize_sequence(|d| T::deserialize_xcdr(d))
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

impl<T: XcdrSerialize, const N: usize> XcdrSerialize for [T; N] {
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        // Arrays are fixed size, no need to write length
        for item in self.iter() {
            item.serialize_xcdr(serializer)?;
        }
        Ok(())
    }
}

impl<T: XcdrDeserialize, const N: usize> XcdrDeserialize for [T; N] {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        // Collect into Vec and try to convert to array
        let mut vec = Vec::with_capacity(N);
        for _ in 0..N {
            vec.push(T::deserialize_xcdr(deserializer)?);
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
        let mut map = HashMap::with_capacity(len);
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
        // Write map length
        serializer.serialize_u32(self.len() as u32)?;

        // Write key-value pairs
        for (key, value) in self {
            key.serialize_xcdr(serializer)?;
            value.serialize_xcdr(serializer)?;
        }
        Ok(())
    }
}

impl<K, V> XcdrDeserialize for HashMap<K, V>
where
    K: XcdrDeserialize + Eq + Hash,
    V: XcdrDeserialize,
{
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        // Read map length
        let len = deserializer.deserialize_u32()? as usize;

        // Read key-value pairs
        let mut map = HashMap::with_capacity(len);
        for _ in 0..len {
            let key = K::deserialize_xcdr(deserializer)?;
            let value = V::deserialize_xcdr(deserializer)?;
            map.insert(key, value);
        }
        Ok(map)
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
        // Write map length
        serializer.serialize_u32(self.len() as u32)?;

        // Write key-value pairs
        for (key, value) in self {
            key.serialize_xcdr(serializer)?;
            value.serialize_xcdr(serializer)?;
        }
        Ok(())
    }
}

impl<K, V> XcdrDeserialize for BTreeMap<K, V>
where
    K: XcdrDeserialize + Ord,
    V: XcdrDeserialize,
{
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        // Read map length
        let len = deserializer.deserialize_u32()? as usize;

        // Read key-value pairs
        let mut map = BTreeMap::new();
        for _ in 0..len {
            let key = K::deserialize_xcdr(deserializer)?;
            let value = V::deserialize_xcdr(deserializer)?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

// Wstring support
use crate::serialize::core::WString;

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
use crate::serialize::core::WChar;

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
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        serializer.serialize_wchar16(self.as_char())
    }
}

impl XcdrDeserialize for WChar {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        let c = deserializer.deserialize_wchar16()?;
        Ok(WChar::from(c))
    }
}
