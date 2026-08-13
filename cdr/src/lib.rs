//! CDR/XCDR serialization kernel.
//!
//! Owns the wire rules — alignment, endianness, encapsulation headers, DHEADER
//! and EMHEADER framing, and the key-holder projection — and knows nothing about
//! DDS entities, the FFI layer or the language bindings. The `int2dds` crate
//! re-exports this whole surface under `int2dds::serialize`, which is the path
//! `#[derive(DdsType)]` writes into generated code.
//!
//! `pl_cdr` (SPDP/SEDP/inline QoS) deliberately stays in `int2dds`: it shares the
//! `PID(2)+length(2)` header shape but obeys the opposite length rule (RTPS 2.5
//! §9.6.2.2.2 counts trailing padding, XTypes PL_CDR does not).

pub mod cdr;
pub mod core;
pub mod key_holder;

pub use self::core::xcdr;
pub use key_holder::{
    align_up as key_holder_align_up, KeyHolder, KeyHolderAccessor, KeyHolderFallback,
};

#[doc(hidden)]
#[macro_export]
macro_rules! impl_primitive_serialization {
    (
        trait Serialize = $serialize_trait:ident::$serialize_method:ident,
        trait Deserialize = $deserialize_trait:ident::$deserialize_method:ident,
        serializer = $serializer:ident,
        deserializer = $deserializer:ident,
        result = $result:ident,
        { $($ty:ty => ($ser_fn:ident, $de_fn:ident)),+ $(,)? }
    ) => {
        $(
            impl $serialize_trait for $ty {
                const IS_PRIMITIVE: bool = true;
                #[inline]
                fn $serialize_method(&self, serializer: &mut $serializer) -> $result<()> {
                    serializer.$ser_fn(*self)
                }
            }

            impl $deserialize_trait for $ty {
                const IS_PRIMITIVE: bool = true;
                #[inline]
                fn $deserialize_method(deserializer: &mut $deserializer) -> $result<Self> {
                    deserializer.$de_fn()
                }
            }
        )+
    };
}

pub use self::core::{
    // Alignment utilities
    align_buffer,
    align_buffer_with_header_offset,
    align_position_with_header_offset,
    deserialize_array_common,
    deserialize_optional_common,
    deserialize_sequence_common,
    // Endianness utilities
    from_bytes_f32,
    from_bytes_f64,
    from_bytes_i16,
    from_bytes_i32,
    from_bytes_i64,
    from_bytes_u16,
    from_bytes_u32,
    from_bytes_u64,
    read_u16,
    read_u32,
    read_u64,
    read_u8,
    // Sequence processing utilities
    serialize_array_common,
    serialize_optional_common,
    serialize_sequence_common,
    to_bytes_f32,
    to_bytes_f64,
    to_bytes_i16,
    to_bytes_i32,
    to_bytes_i64,
    to_bytes_u16,
    to_bytes_u32,
    to_bytes_u64,
    BufferManager,
    DeserializerReader,
    SerializationError,
    WChar,
    WString,
};
