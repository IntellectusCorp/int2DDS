use speedy::Endianness;

/// Common conversion from bool to Endianness
/// true = LittleEndian, false = BigEndian
#[inline]
pub(crate) fn endianness_from_bool(little_endian: bool) -> Endianness {
    if little_endian {
        Endianness::LittleEndian
    } else {
        Endianness::BigEndian
    }
}

/// Trait for primitive types that can convert to/from byte arrays with endianness awareness.
pub trait EndianConvertible: Copy {
    type Bytes: Copy;

    fn to_le_bytes(self) -> Self::Bytes;
    fn to_be_bytes(self) -> Self::Bytes;
    fn from_le_bytes(bytes: Self::Bytes) -> Self;
    fn from_be_bytes(bytes: Self::Bytes) -> Self;
}

macro_rules! impl_endian_convertible {
    ($($ty:ty => $len:expr),+ $(,)?) => {
        $(impl EndianConvertible for $ty {
            type Bytes = [u8; $len];

            #[inline]
            fn to_le_bytes(self) -> Self::Bytes {
                self.to_le_bytes()
            }

            #[inline]
            fn to_be_bytes(self) -> Self::Bytes {
                self.to_be_bytes()
            }

            #[inline]
            fn from_le_bytes(bytes: Self::Bytes) -> Self {
                <$ty>::from_le_bytes(bytes)
            }

            #[inline]
            fn from_be_bytes(bytes: Self::Bytes) -> Self {
                <$ty>::from_be_bytes(bytes)
            }
        })+
    };
}

impl_endian_convertible! {
    u16 => 2,
    i16 => 2,
    u32 => 4,
    i32 => 4,
    u64 => 8,
    i64 => 8,
    f32 => 4,
    f64 => 8,
}

#[inline]
pub fn to_bytes<T: EndianConvertible>(value: T, endianness: Endianness) -> T::Bytes {
    match endianness {
        Endianness::LittleEndian => value.to_le_bytes(),
        Endianness::BigEndian => value.to_be_bytes(),
    }
}

#[inline]
pub fn from_bytes<T: EndianConvertible>(bytes: T::Bytes, endianness: Endianness) -> T {
    match endianness {
        Endianness::LittleEndian => T::from_le_bytes(bytes),
        Endianness::BigEndian => T::from_be_bytes(bytes),
    }
}

macro_rules! define_endian_wrappers {
    ($($ty:ty => $to_fn:ident, $from_fn:ident),+ $(,)?) => {
        $(
            #[inline]
            pub fn $to_fn(value: $ty, endianness: Endianness) -> <$ty as EndianConvertible>::Bytes {
                to_bytes(value, endianness)
            }

            #[inline]
            pub fn $from_fn(bytes: <$ty as EndianConvertible>::Bytes, endianness: Endianness) -> $ty {
                from_bytes(bytes, endianness)
            }
        )+
    };
}

define_endian_wrappers! {
    u16 => to_bytes_u16, from_bytes_u16,
    i16 => to_bytes_i16, from_bytes_i16,
    u32 => to_bytes_u32, from_bytes_u32,
    i32 => to_bytes_i32, from_bytes_i32,
    u64 => to_bytes_u64, from_bytes_u64,
    i64 => to_bytes_i64, from_bytes_i64,
    f32 => to_bytes_f32, from_bytes_f32,
    f64 => to_bytes_f64, from_bytes_f64,
}
