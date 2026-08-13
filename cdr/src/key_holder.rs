//! RTPS KeyHash "KeyHolder" projection (DDSI-RTPS v2.5 §9.6.4.8).
//!
//! The KeyHolder of a type is the derived type that keeps only its `@key`
//! members, with FINAL extensibility, members ordered by ascending member id,
//! applied recursively to nested aggregated members. Its serialized form is
//! PLAIN_CDR2 big-endian with maximum alignment 4 and no member headers.
//!
//! This trait is the static (derive-generated) counterpart of the runtime
//! projection in `int2dds::xtypes::dynamic_serialization`; the two must produce
//! byte-identical KeyHash streams. Derived structs/enums implement it via the
//! proc-macro; primitive and string leaves implement it here.

use crate::cdr::{Xcdr2Deserializer, Xcdr2Serializer, XcdrDeserialize, XcdrResult, XcdrSerialize};
use crate::core::SerializationError;
use crate::WString;

/// Projection of a value into its RTPS KeyHash key-holder form.
pub trait KeyHolder {
    /// See [`XcdrSerialize::IS_PRIMITIVE`]: decides whether an enclosing key-holder
    /// collection frames its elements. Mirrors the classification used for data so
    /// the two streams stay byte-identical in structure.
    const IS_PRIMITIVE: bool = false;

    /// See [`XcdrSerialize::BASE_IS_PRIMITIVE`].
    const BASE_IS_PRIMITIVE: bool = Self::IS_PRIMITIVE;

    /// Write this value's key projection into the (FINAL, big-endian, max-align-4,
    /// headerless) key-holder stream. Aggregated types with `@key` members write
    /// only those members in member-id order, recursing into nested aggregated
    /// members; leaves write themselves.
    fn serialize_key_holder(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()>;

    /// Read side of [`Self::serialize_key_holder`]: reconstruct a value holding
    /// only the projected key members from a key-holder stream.
    fn deserialize_key_holder(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self>
    where
        Self: Sized;

    /// See [`XcdrSerialize::serialize_xcdr_unframed`]: a nested array dimension
    /// contributes its elements without a DHEADER of its own.
    fn serialize_key_holder_unframed(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        self.serialize_key_holder(serializer)
    }

    /// Read side of [`Self::serialize_key_holder_unframed`].
    fn deserialize_key_holder_unframed(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self>
    where
        Self: Sized,
    {
        Self::deserialize_key_holder(deserializer)
    }

    /// Maximum serialized size (max alignment 4) of this value's key-holder form,
    /// or `None` when it has no finite maximum (unbounded string/sequence/map),
    /// in which case the KeyHash is always the MD5 of the actual stream. Used for
    /// the raw(≤16)/MD5 threshold of KeyHash step 5.
    fn key_holder_max_size() -> Option<usize>
    where
        Self: Sized;

    /// Maximum size of [`Self::serialize_key_holder_unframed`], i.e. excluding the
    /// DHEADER an array writes for itself.
    fn key_holder_unframed_max_size() -> Option<usize>
    where
        Self: Sized,
    {
        Self::key_holder_max_size()
    }

    /// CDR alignment of this value as a key-holder member (capped at 4).
    fn key_holder_align() -> usize
    where
        Self: Sized,
    {
        4
    }
}

/// Round `pos` up to a multiple of `align` (used by generated `key_holder_max_size`).
#[inline]
pub fn align_up(pos: usize, align: usize) -> usize {
    if align <= 1 {
        pos
    } else {
        (pos + align - 1) & !(align - 1)
    }
}

/// Autoref-specialization helper letting derive-generated code route each `@key`
/// member through the right projection without knowing at macro-expansion time
/// whether the member's type implements [`KeyHolder`].
///
/// - Member types that implement `KeyHolder` (primitives, strings, keyed nested
///   structs) resolve the inherent methods on `KeyHolderAccessor<T>` (the
///   *specific* path): the member is projected.
/// - Any other serializable member type (a no-key nested aggregate used as a
///   whole key member, e.g. `Guid`) auto-refs to the [`KeyHolderFallback`] trait
///   impl: the member is written/read whole via XCDR and reports no finite
///   maximum size (=> the enclosing key holder always hashes, matching the
///   dynamic path's `type_kind_max_size` for a whole no-key aggregate).
pub struct KeyHolderAccessor<T>(pub core::marker::PhantomData<T>);

/// Fallback for `@key` member types that do NOT implement [`KeyHolder`]: whole
/// XCDR (PLAIN_CDR2) round-trip, no finite maximum size.
pub trait KeyHolderFallback<T> {
    fn kh_serialize(&self, value: &T, serializer: &mut Xcdr2Serializer) -> XcdrResult<()>;
    fn kh_deserialize(&self, deserializer: &mut Xcdr2Deserializer) -> XcdrResult<T>;
    fn kh_max_size(&self) -> Option<usize>;
    fn kh_align(&self) -> usize;
    fn kh_is_primitive(&self) -> bool;
}

impl<T: XcdrSerialize + XcdrDeserialize> KeyHolderFallback<T> for KeyHolderAccessor<T> {
    #[inline]
    fn kh_serialize(&self, value: &T, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        XcdrSerialize::serialize_xcdr(value, serializer)
    }
    #[inline]
    fn kh_deserialize(&self, deserializer: &mut Xcdr2Deserializer) -> XcdrResult<T> {
        XcdrDeserialize::deserialize_xcdr(deserializer)
    }
    #[inline]
    fn kh_max_size(&self) -> Option<usize> {
        None
    }
    #[inline]
    fn kh_align(&self) -> usize {
        4
    }
    #[inline]
    fn kh_is_primitive(&self) -> bool {
        false
    }
}

impl<T: KeyHolder> KeyHolderAccessor<T> {
    #[inline]
    pub fn kh_serialize(&self, value: &T, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        value.serialize_key_holder(serializer)
    }
    #[inline]
    pub fn kh_deserialize(&self, deserializer: &mut Xcdr2Deserializer) -> XcdrResult<T> {
        T::deserialize_key_holder(deserializer)
    }
    #[inline]
    pub fn kh_max_size(&self) -> Option<usize> {
        T::key_holder_max_size()
    }
    #[inline]
    pub fn kh_align(&self) -> usize {
        T::key_holder_align()
    }
    #[inline]
    pub fn kh_is_primitive(&self) -> bool {
        T::IS_PRIMITIVE
    }
}

macro_rules! impl_leaf_key_holder {
    ($($ty:ty => $size:expr),+ $(,)?) => {$(
        impl KeyHolder for $ty {
            const IS_PRIMITIVE: bool = true;
            #[inline]
            fn serialize_key_holder(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
                XcdrSerialize::serialize_xcdr(self, serializer)
            }
            #[inline]
            fn deserialize_key_holder(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self> {
                XcdrDeserialize::deserialize_xcdr(deserializer)
            }
            #[inline]
            fn key_holder_max_size() -> Option<usize> {
                Some($size)
            }
            #[inline]
            fn key_holder_align() -> usize {
                ($size as usize).min(4)
            }
        }
    )+};
}

// Primitive leaves. Alignment is `size.min(4)` per the max-alignment-4 rule;
// `char` is CDR CHAR8 (1 byte). Sizes match `PrimitiveKind::size` in the
// dynamic path so both projections agree.
impl_leaf_key_holder!(
    bool => 1,
    u8 => 1,
    i8 => 1,
    u16 => 2,
    i16 => 2,
    u32 => 4,
    i32 => 4,
    u64 => 8,
    i64 => 8,
    f32 => 4,
    f64 => 8,
    char => 1,
);

/// `String` is a bounded/unbounded CDR string; the bound lives on the `@key`
/// field attribute, not on the type, so a standalone `String` reports no finite
/// maximum. The generated `key_holder_max_size` of the owning struct supplies
/// `4 + bound + 1` for bounded string key fields; an unbounded string key always
/// hashes.
impl KeyHolder for String {
    #[inline]
    fn serialize_key_holder(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        XcdrSerialize::serialize_xcdr(self, serializer)
    }
    #[inline]
    fn deserialize_key_holder(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self> {
        XcdrDeserialize::deserialize_xcdr(deserializer)
    }
    #[inline]
    fn key_holder_max_size() -> Option<usize> {
        None
    }
}

/// `WString` mirrors [`String`]: the bound is a field attribute, so a standalone
/// wstring reports no finite maximum (the owning struct supplies `4 + 2*(bound+1)`
/// for bounded wstring key fields).
impl KeyHolder for WString {
    #[inline]
    fn serialize_key_holder(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        XcdrSerialize::serialize_xcdr(self, serializer)
    }
    #[inline]
    fn deserialize_key_holder(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self> {
        XcdrDeserialize::deserialize_xcdr(deserializer)
    }
    #[inline]
    fn key_holder_max_size() -> Option<usize> {
        None
    }
}

/// A fixed-size array `[T; N]` projects element-wise: each element is written as
/// its own key holder. Per XTypes 7.4.3.5.3 the array carries a DHEADER unless its
/// base element type is primitive, and a multidimensional array is one array, so
/// nested dimensions contribute unframed. Its maximum size is the packed
/// (max-align-4) sum of `N` element key holders plus that DHEADER.
impl<T: KeyHolder, const N: usize> KeyHolder for [T; N] {
    const BASE_IS_PRIMITIVE: bool = T::BASE_IS_PRIMITIVE;

    #[inline]
    fn serialize_key_holder(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        if Self::BASE_IS_PRIMITIVE {
            return self.serialize_key_holder_unframed(serializer);
        }
        let dheader_pos = serializer.reserve_dheader();
        let content_start = serializer.position();
        self.serialize_key_holder_unframed(serializer)?;
        let content_size = (serializer.position() - content_start) as u32;
        serializer.write_dheader_at(dheader_pos, content_size);
        Ok(())
    }
    #[inline]
    fn serialize_key_holder_unframed(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        for element in self {
            element.serialize_key_holder_unframed(serializer)?;
        }
        Ok(())
    }
    #[inline]
    fn deserialize_key_holder(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self> {
        if !Self::BASE_IS_PRIMITIVE {
            let _object_size = deserializer.read_dheader()?;
        }
        Self::deserialize_key_holder_unframed(deserializer)
    }
    #[inline]
    fn deserialize_key_holder_unframed(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self> {
        let mut values = Vec::with_capacity(N);
        for _ in 0..N {
            values.push(T::deserialize_key_holder_unframed(deserializer)?);
        }
        values.try_into().map_err(|_| {
            SerializationError::DeserializationError(
                "array key holder element count mismatch".to_string(),
            )
        })
    }
    #[inline]
    fn key_holder_max_size() -> Option<usize> {
        let body = Self::key_holder_unframed_max_size()?;
        if Self::BASE_IS_PRIMITIVE {
            Some(body)
        } else {
            body.checked_add(4)
        }
    }
    #[inline]
    fn key_holder_unframed_max_size() -> Option<usize> {
        let element = T::key_holder_unframed_max_size()?;
        if N == 0 {
            return Some(0);
        }
        let stride = align_up(element, T::key_holder_align());
        stride.checked_mul(N - 1)?.checked_add(element)
    }
    #[inline]
    fn key_holder_align() -> usize {
        if Self::BASE_IS_PRIMITIVE {
            T::key_holder_align()
        } else {
            4
        }
    }
}

/// A sequence `Vec<T>` writes its `u32` length then each element as its own key
/// holder, preceded by a DHEADER spanning both unless the element type is primitive
/// (XTypes 7.4.3.5.4). It has no finite maximum size, so a sequence key always
/// hashes (MD5).
impl<T: KeyHolder> KeyHolder for Vec<T> {
    #[inline]
    fn serialize_key_holder(&self, serializer: &mut Xcdr2Serializer) -> XcdrResult<()> {
        if T::IS_PRIMITIVE {
            return write_sequence_elements(self, serializer);
        }
        let dheader_pos = serializer.reserve_dheader();
        let content_start = serializer.position();
        write_sequence_elements(self, serializer)?;
        let content_size = (serializer.position() - content_start) as u32;
        serializer.write_dheader_at(dheader_pos, content_size);
        Ok(())
    }
    #[inline]
    fn deserialize_key_holder(deserializer: &mut Xcdr2Deserializer) -> XcdrResult<Self> {
        if !T::IS_PRIMITIVE {
            let _object_size = deserializer.read_dheader()?;
        }
        let len = <u32 as XcdrDeserialize>::deserialize_xcdr(deserializer)? as usize;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(T::deserialize_key_holder(deserializer)?);
        }
        Ok(values)
    }
    #[inline]
    fn key_holder_max_size() -> Option<usize> {
        None
    }
}

#[inline]
fn write_sequence_elements<T: KeyHolder>(
    items: &[T],
    serializer: &mut Xcdr2Serializer,
) -> XcdrResult<()> {
    XcdrSerialize::serialize_xcdr(&(items.len() as u32), serializer)?;
    for element in items {
        element.serialize_key_holder(serializer)?;
    }
    Ok(())
}
