mod cdr_input;
mod deserializer;
mod prim_bulk;
pub mod serializer;
pub mod xcdr1;
pub mod xcdr2;

use crate::serialize::core::{SerializationError, SerializationResult};

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

    /// Write EMHEADER1 per DDS-XTypes 7.4.3.4.2.
    pub fn write(
        &self,
        buffer: &mut Vec<u8>,
        endianness: speedy::Endianness,
    ) -> Result<(), SerializationError> {
        use crate::serialize::to_bytes_u32;

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
        use crate::serialize::from_bytes_u32;

        if position > data.len() || data.len() - position < 4 {
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
            if position > buf.len() || buf.len() - position < 8 {
                return Err(SerializationError::InsufficientData);
            }
            let b: [u8; 4] = buf[position + 4..position + 8]
                .try_into()
                .map_err(|_| SerializationError::InsufficientData)?;
            Ok(from_bytes_u32(b, endianness))
        };

        // LC=5/6/7: NEXTINT overlaps with payload's first 4 bytes (DDS-XTypes 7.4.3.4.2).
        // Checked arithmetic: an unchecked wrap turns a huge NEXTINT into a *small*
        // member_length, which suppresses the skip and misparses every later member.
        let scaled_length = |scale: u32| -> Result<u32, SerializationError> {
            read_nextint(data)?
                .checked_mul(scale)
                .and_then(|n| n.checked_add(4))
                .ok_or(SerializationError::InvalidMemberHeader)
        };
        let (member_length, bytes_consumed) = match lc {
            0 => (1u32, 4usize),
            1 => (2u32, 4usize),
            2 => (4u32, 4usize),
            3 => (8u32, 4usize),
            4 => (read_nextint(data)?, 8usize),
            5 => (scaled_length(1)?, 4usize),
            6 => (scaled_length(4)?, 4usize),
            7 => (scaled_length(8)?, 4usize),
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

/// Cap on the bytes a wire-declared element count may reserve up front. A count
/// is validated against remaining input via `checked_capacity`, but one wire byte
/// per element can still authorize `size_of::<T>()` heap bytes per element; the
/// cap bounds that amplification. Larger collections grow past it normally.
pub(crate) const MAX_PREALLOC_BYTES: usize = 64 * 1024;

pub(crate) fn bounded_prealloc_count<T>(count: usize) -> usize {
    count.min(MAX_PREALLOC_BYTES / std::mem::size_of::<T>().max(1))
}

pub(crate) fn try_vec_prealloc<T>(count: usize) -> Result<Vec<T>, SerializationError> {
    let mut vec = Vec::new();
    vec.try_reserve(bounded_prealloc_count::<T>(count))
        .map_err(|_| SerializationError::AllocationFailure)?;
    Ok(vec)
}

/// Build a `[T; N]` element-by-element on the stack (no intermediate heap `Vec`),
/// dropping the already-initialized elements if `next` fails partway through.
fn try_array_from_fn<T, E, const N: usize>(
    mut next: impl FnMut() -> Result<T, E>,
) -> Result<[T; N], E> {
    use std::mem::MaybeUninit;

    struct Partial<T, const N: usize> {
        items: [MaybeUninit<T>; N],
        init: usize,
    }
    impl<T, const N: usize> Drop for Partial<T, N> {
        fn drop(&mut self) {
            for item in &mut self.items[..self.init] {
                // SAFETY: exactly the first `init` slots hold initialized values.
                unsafe { item.assume_init_drop() };
            }
        }
    }

    let mut partial = Partial::<T, N> {
        // SAFETY: an array of `MaybeUninit` is valid without initialization.
        items: unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() },
        init: 0,
    };
    for slot in &mut partial.items {
        *slot = MaybeUninit::new(next()?);
        partial.init += 1;
    }
    partial.init = 0;
    // SAFETY: all `N` slots are initialized and the guard above is disarmed, so each
    // value is read out exactly once. `MaybeUninit<T>` has the same layout as `T`.
    Ok(unsafe { partial.items.as_ptr().cast::<[T; N]>().read() })
}

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
        try_array_from_fn(|| T::deserialize_cdr(deserializer))
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
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()>;
}

/// Trait for types that can be deserialized using XCDR
pub trait XcdrDeserialize: Sized {
    /// See [`XcdrSerialize::IS_PRIMITIVE`].
    const IS_PRIMITIVE: bool = false;
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
        serializer.serialize_u32(self.len() as u32)?;
        if T::IS_PRIMITIVE {
            serializer.buffer_mut().reserve(self.len() * std::mem::size_of::<T>());
        }
        for item in self {
            item.serialize_xcdr(serializer)?;
        }
        Ok(())
    }
}

impl<T: XcdrDeserialize> XcdrDeserialize for Vec<T> {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        let length = deserializer.deserialize_u32()? as usize;
        let length = deserializer.checked_capacity(length, 1)?;
        let mut result = try_vec_prealloc::<T>(length)?;
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

impl<T: XcdrSerialize, const N: usize> XcdrSerialize for [T; N] {
    fn serialize_xcdr(&self, serializer: &mut XcdrSerializer) -> XcdrResult<()> {
        for item in self.iter() {
            item.serialize_xcdr(serializer)?;
        }
        Ok(())
    }
}

impl<T: XcdrDeserialize, const N: usize> XcdrDeserialize for [T; N] {
    fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
        try_array_from_fn(|| T::deserialize_xcdr(deserializer))
    }
}

// HashMap and BTreeMap support: one macro emits all four codec impls per map type.
// Only the key bounds and the prealloc strategy differ (BTreeMap has no capacity
// concept, so it skips the reserve).
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

fn hashmap_with_prealloc<K: Eq + Hash, V>(len: usize) -> Result<HashMap<K, V>, SerializationError> {
    let mut map = HashMap::new();
    map.try_reserve(bounded_prealloc_count::<(K, V)>(len))
        .map_err(|_| SerializationError::AllocationFailure)?;
    Ok(map)
}

fn btreemap_without_prealloc<K: Ord, V>(_len: usize) -> Result<BTreeMap<K, V>, SerializationError> {
    Ok(BTreeMap::new())
}

macro_rules! impl_map_serialization {
    ($map:ident, [$($bound:tt)+], $new:ident) => {
        impl<K, V> CdrSerialize for $map<K, V>
        where
            K: CdrSerialize + $($bound)+,
            V: CdrSerialize,
        {
            fn serialize_cdr(&self, serializer: &mut CdrSerializer) -> CdrResult<()> {
                serializer.serialize_u32(self.len() as u32)?;
                for (key, value) in self {
                    key.serialize_cdr(serializer)?;
                    value.serialize_cdr(serializer)?;
                }
                Ok(())
            }
        }

        impl<K, V> CdrDeserialize for $map<K, V>
        where
            K: CdrDeserialize + $($bound)+,
            V: CdrDeserialize,
        {
            fn deserialize_cdr(deserializer: &mut CdrDeserializer) -> CdrResult<Self> {
                let len = deserializer.deserialize_u32()? as usize;
                let len = deserializer.checked_capacity(len, 2)?;
                let mut map = $new::<K, V>(len)?;
                for _ in 0..len {
                    let key = K::deserialize_cdr(deserializer)?;
                    let value = V::deserialize_cdr(deserializer)?;
                    map.insert(key, value);
                }
                Ok(map)
            }
        }

        impl<K, V> XcdrSerialize for $map<K, V>
        where
            K: XcdrSerialize + $($bound)+,
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

        impl<K, V> XcdrDeserialize for $map<K, V>
        where
            K: XcdrDeserialize + $($bound)+,
            V: XcdrDeserialize,
        {
            fn deserialize_xcdr(deserializer: &mut XcdrDeserializer) -> XcdrResult<Self> {
                if !(K::IS_PRIMITIVE && V::IS_PRIMITIVE) {
                    let _dheader = deserializer.read_dheader()?;
                }
                let len = deserializer.deserialize_u32()? as usize;
                let len = deserializer.checked_capacity(len, 2)?;
                let mut map = $new::<K, V>(len)?;
                for _ in 0..len {
                    let key = K::deserialize_xcdr(deserializer)?;
                    let value = V::deserialize_xcdr(deserializer)?;
                    map.insert(key, value);
                }
                Ok(map)
            }
        }
    };
}

impl_map_serialization!(HashMap, [Eq + Hash], hashmap_with_prealloc);
impl_map_serialization!(BTreeMap, [Ord], btreemap_without_prealloc);

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

#[cfg(test)]
mod member_header_tests {
    use super::*;
    use speedy::Endianness;

    fn wire(lc: u32, member_id: u32, nextint: u32) -> Vec<u8> {
        let header = (lc << 28) | (member_id & 0x0FFF_FFFF);
        let mut v = header.to_le_bytes().to_vec();
        v.extend_from_slice(&nextint.to_le_bytes());
        v
    }

    fn read(lc: u32, nextint: u32) -> Result<(MemberHeader, usize), SerializationError> {
        MemberHeader::read(&wire(lc, 1, nextint), 0, Endianness::LittleEndian)
    }

    #[test]
    fn lc5_length_overflow_is_rejected() {
        assert!(matches!(read(5, u32::MAX), Err(SerializationError::InvalidMemberHeader)));
    }

    #[test]
    fn lc6_length_overflow_is_rejected() {
        assert!(matches!(read(6, 0x4000_0000), Err(SerializationError::InvalidMemberHeader)));
    }

    #[test]
    fn lc7_length_overflow_is_rejected() {
        assert!(matches!(read(7, 0x2000_0000), Err(SerializationError::InvalidMemberHeader)));
    }

    #[test]
    fn lc5_6_7_lengths_are_unchanged_for_valid_nextint() {
        assert_eq!(read(5, 5).unwrap().0.member_length, 9);
        assert_eq!(read(6, 3).unwrap().0.member_length, 16);
        assert_eq!(read(7, 2).unwrap().0.member_length, 20);
    }

    #[test]
    fn lc0_to_4_are_unchanged() {
        assert_eq!(read(0, 0).unwrap().0.member_length, 1);
        assert_eq!(read(1, 0).unwrap().0.member_length, 2);
        assert_eq!(read(2, 0).unwrap().0.member_length, 4);
        assert_eq!(read(3, 0).unwrap().0.member_length, 8);
        let (h, consumed) = read(4, 0xDEAD).unwrap();
        assert_eq!(h.member_length, 0xDEAD);
        assert_eq!(consumed, 8);
    }
}

#[cfg(test)]
mod prealloc_tests {
    use super::*;

    #[test]
    fn initial_reserve_is_capped_by_bytes_not_count() {
        assert_eq!(bounded_prealloc_count::<[u8; 4096]>(1_000_000), 16);
        assert_eq!(bounded_prealloc_count::<u64>(1_000_000), MAX_PREALLOC_BYTES / 8);
        assert_eq!(bounded_prealloc_count::<u8>(1024), 1024);
    }

    #[test]
    fn try_vec_prealloc_reserves_at_most_the_cap() {
        let v = try_vec_prealloc::<[u8; 4096]>(1_000_000).unwrap();
        // 16 requested; the allocator may round up, but nowhere near the 4 GiB
        // that length * size_of would have reserved.
        assert!(v.capacity() <= 32);
        let v = try_vec_prealloc::<u8>(100).unwrap();
        assert!(v.capacity() >= 100);
    }
}

#[cfg(test)]
mod try_array_from_fn_tests {
    use super::try_array_from_fn;
    use std::cell::Cell;
    use std::rc::Rc;

    struct DropCounter(Rc<Cell<usize>>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn partial_failure_drops_only_initialized_elements() {
        let drops = Rc::new(Cell::new(0));
        let mut produced = 0;
        let result: Result<[DropCounter; 8], ()> = try_array_from_fn(|| {
            if produced == 5 {
                return Err(());
            }
            produced += 1;
            Ok(DropCounter(drops.clone()))
        });
        assert!(result.is_err());
        assert_eq!(drops.get(), 5);
    }

    #[test]
    fn success_yields_each_element_exactly_once() {
        let drops = Rc::new(Cell::new(0));
        let arr: [DropCounter; 4] =
            try_array_from_fn(|| Ok::<_, ()>(DropCounter(drops.clone()))).unwrap();
        assert_eq!(drops.get(), 0);
        drop(arr);
        assert_eq!(drops.get(), 4);
    }

    #[test]
    fn values_arrive_in_call_order() {
        let mut n = 0u32;
        let arr: [u32; 5] = try_array_from_fn(|| {
            n += 1;
            Ok::<_, ()>(n)
        })
        .unwrap();
        assert_eq!(arr, [1, 2, 3, 4, 5]);
    }
}
