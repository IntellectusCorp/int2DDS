use super::endianness::*;
use speedy::Endianness;

/// Common read functions for deserializers
pub trait DeserializerReader {
    type Error;

    /// Check if enough data is available
    fn check_available(&self, size: usize) -> Result<(), Self::Error>;

    /// Copy out.len() bytes starting at offset into out. May stitch across
    /// fragment chunk boundaries for chained inputs.
    fn copy_bytes_at(&self, offset: usize, out: &mut [u8]);

    /// Get current position
    fn get_position(&self) -> usize;

    /// Set current position
    fn set_position(&mut self, position: usize);

    /// Get endianness
    fn get_endianness(&self) -> Endianness;

    /// Align position to boundary
    fn align(&mut self, alignment: usize);
}

/// Common read implementation for u8
pub fn read_u8<T: DeserializerReader>(reader: &mut T) -> Result<u8, T::Error> {
    reader.check_available(1)?;
    let position = reader.get_position();
    let mut bytes = [0u8; 1];
    reader.copy_bytes_at(position, &mut bytes);
    reader.set_position(position + 1);
    Ok(bytes[0])
}

/// Common read implementation for u16
pub fn read_u16<T: DeserializerReader>(reader: &mut T) -> Result<u16, T::Error> {
    reader.align(2);
    reader.check_available(2)?;
    let position = reader.get_position();

    let mut bytes = [0u8; 2];
    reader.copy_bytes_at(position, &mut bytes);
    let value = from_bytes_u16(bytes, reader.get_endianness());
    reader.set_position(position + 2);
    Ok(value)
}

/// Common read implementation for u32
pub fn read_u32<T: DeserializerReader>(reader: &mut T) -> Result<u32, T::Error> {
    reader.align(4);
    reader.check_available(4)?;
    let position = reader.get_position();

    let mut bytes = [0u8; 4];
    reader.copy_bytes_at(position, &mut bytes);
    let value = from_bytes_u32(bytes, reader.get_endianness());
    reader.set_position(position + 4);
    Ok(value)
}

/// Common read implementation for u64
pub fn read_u64<T: DeserializerReader>(reader: &mut T) -> Result<u64, T::Error> {
    reader.align(8);
    reader.check_available(8)?;
    let position = reader.get_position();

    let mut bytes = [0u8; 8];
    reader.copy_bytes_at(position, &mut bytes);
    let value = from_bytes_u64(bytes, reader.get_endianness());
    reader.set_position(position + 8);
    Ok(value)
}
