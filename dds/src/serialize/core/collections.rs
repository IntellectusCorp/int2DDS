use super::alignment::align_buffer;
use super::endianness::{from_bytes, to_bytes, EndianConvertible};
use super::errors::SerializationError;
use speedy::Endianness;

pub fn serialize_array_common<T, E, F>(items: &[T], serialize_fn: F) -> Result<(), E>
where
    F: Fn(&T) -> Result<(), E>,
{
    for item in items {
        serialize_fn(item)?;
    }
    Ok(())
}

/// Generic helper for serializing aligned arrays of EndianConvertible types
pub fn serialize_aligned_array<T>(
    buffer: &mut Vec<u8>,
    data: &[T],
    alignment: usize,
    endianness: Endianness,
) -> Result<(), SerializationError>
where
    T: EndianConvertible,
    T::Bytes: AsRef<[u8]>,
{
    align_buffer(buffer, alignment);
    for &value in data {
        let bytes = to_bytes(value, endianness);
        buffer.extend_from_slice(bytes.as_ref());
    }
    Ok(())
}

/// Generic helper for serializing aligned sequences of EndianConvertible types (with length prefix)
pub fn serialize_aligned_sequence<T>(
    buffer: &mut Vec<u8>,
    data: &[T],
    alignment: usize,
    endianness: Endianness,
    write_length: impl FnOnce(&mut Vec<u8>, u32, Endianness) -> Result<(), SerializationError>,
) -> Result<(), SerializationError>
where
    T: EndianConvertible,
    T::Bytes: AsRef<[u8]>,
{
    write_length(buffer, data.len() as u32, endianness)?;
    serialize_aligned_array(buffer, data, alignment, endianness)
}

pub fn serialize_sequence_common<T, E, F>(
    items: &[T],
    mut write_length_fn: impl FnMut(u32) -> Result<(), E>,
    serialize_fn: F,
) -> Result<(), E>
where
    F: Fn(&T) -> Result<(), E>,
{
    write_length_fn(items.len() as u32)?;

    for item in items {
        serialize_fn(item)?;
    }
    Ok(())
}

pub fn serialize_optional_common<T, E, F>(
    value: &Option<T>,
    mut write_bool_fn: impl FnMut(bool) -> Result<(), E>,
    serialize_fn: F,
) -> Result<(), E>
where
    F: Fn(&T) -> Result<(), E>,
{
    match value {
        Some(val) => {
            write_bool_fn(true)?;
            serialize_fn(val)?;
        }
        None => {
            write_bool_fn(false)?;
        }
    }
    Ok(())
}

pub fn deserialize_array_common<T, E, F>(size: usize, mut deserialize_fn: F) -> Result<Vec<T>, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut result = Vec::with_capacity(size);
    for _ in 0..size {
        result.push(deserialize_fn()?);
    }
    Ok(result)
}

pub fn deserialize_sequence_common<T, E, F>(
    mut read_length_fn: impl FnMut() -> Result<u32, E>,
    mut deserialize_fn: F,
) -> Result<Vec<T>, E>
where
    F: FnMut() -> Result<T, E>,
{
    let length = read_length_fn()? as usize;

    let mut result = Vec::with_capacity(length);
    for _ in 0..length {
        result.push(deserialize_fn()?);
    }
    Ok(result)
}

pub fn deserialize_optional_common<T, E, F>(
    mut read_bool_fn: impl FnMut() -> Result<bool, E>,
    mut deserialize_fn: F,
) -> Result<Option<T>, E>
where
    F: FnMut() -> Result<T, E>,
{
    let has_value = read_bool_fn()?;
    if has_value {
        Ok(Some(deserialize_fn()?))
    } else {
        Ok(None)
    }
}

/// Generic helper for deserializing aligned arrays of EndianConvertible types
pub fn deserialize_aligned_array<T>(
    data: &[u8],
    position: &mut usize,
    count: usize,
    alignment: usize,
    endianness: Endianness,
    check_available: impl Fn(usize) -> Result<(), SerializationError>,
) -> Result<Vec<T>, SerializationError>
where
    T: EndianConvertible,
{
    // Align position
    let element_size = std::mem::size_of::<T>();
    *position = (*position + alignment - 1) & !(alignment - 1);

    check_available(count * element_size)?;

    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let mut bytes = std::mem::MaybeUninit::<T::Bytes>::uninit();
        let bytes_slice =
            unsafe { std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut u8, element_size) };
        bytes_slice.copy_from_slice(&data[*position..*position + element_size]);

        let value = unsafe { from_bytes(bytes.assume_init(), endianness) };
        result.push(value);
        *position += element_size;
    }
    Ok(result)
}

/// Generic helper for deserializing aligned sequences of EndianConvertible types (with length prefix)
pub fn deserialize_aligned_sequence<T>(
    data: &[u8],
    position: &mut usize,
    alignment: usize,
    endianness: Endianness,
    read_length: impl FnOnce(&[u8], &mut usize, Endianness) -> Result<u32, SerializationError>,
    check_available: impl Fn(usize) -> Result<(), SerializationError>,
) -> Result<Vec<T>, SerializationError>
where
    T: EndianConvertible,
{
    let length = read_length(data, position, endianness)? as usize;
    deserialize_aligned_array(data, position, length, alignment, endianness, check_available)
}
