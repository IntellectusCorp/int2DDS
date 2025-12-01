use super::endianness::EndianConvertible;
use super::errors::SerializationError;

/// Trait for writing primitive types to a serializer
pub trait PrimitiveWriter {
    /// Write a primitive value with proper alignment and endianness
    fn write_primitive<T: EndianConvertible>(&mut self, value: T) -> Result<(), SerializationError>
    where
        Self: Sized;
}

/// Trait for reading primitive types from a deserializer
pub trait PrimitiveReader {
    /// Read a primitive value with proper alignment and endianness
    fn read_primitive<T: EndianConvertible>(&mut self) -> Result<T, SerializationError>
    where
        Self: Sized;
}
