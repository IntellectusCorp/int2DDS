pub mod cdr;
pub mod core;
pub mod pl_cdr;
pub use core::xcdr;

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
                #[inline]
                fn $serialize_method(&self, serializer: &mut $serializer) -> $result<()> {
                    serializer.$ser_fn(*self)
                }
            }

            impl $deserialize_trait for $ty {
                #[inline]
                fn $deserialize_method(deserializer: &mut $deserializer) -> $result<Self> {
                    deserializer.$de_fn()
                }
            }
        )+
    };
}

pub use crate::infrastructure::qos_policy::DataRepresentationId;
pub use core::{
    // Alignment utilities
    align_buffer,
    align_position_with_header_offset,
    deserialize_array_common,
    // Data payload deserialization
    deserialize_data_payload,
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
    BufferSize,
    DeserializerReader,
    PooledBuffer,
    SerializationError,
    WChar,
    WString,
};
