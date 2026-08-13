/// Calculate alignment padding for serializer buffers
#[inline]
pub fn align_buffer(buffer: &mut Vec<u8>, alignment: usize) {
    debug_assert!(alignment.is_power_of_two(), "Alignment must be a power of 2");
    let current_len = buffer.len();
    let aligned_len = (current_len + alignment - 1) & !(alignment - 1);
    if aligned_len > current_len {
        buffer.resize(aligned_len, 0);
    }
}

/// Calculate alignment padding for serializer buffers with header offset consideration
/// CDR standard requires alignment to be calculated relative to the data stream start
/// (after the encapsulation header), not from the beginning of the buffer.
#[inline]
pub fn align_buffer_with_header_offset(buffer: &mut Vec<u8>, alignment: usize, header_size: usize) {
    debug_assert!(alignment.is_power_of_two(), "Alignment must be a power of 2");
    // Calculate stream position (position within data, excluding header)
    let stream_position = buffer.len() - header_size;
    // Calculate aligned stream position
    let aligned_stream_position = (stream_position + alignment - 1) & !(alignment - 1);
    // Calculate padding needed
    let padding = aligned_stream_position - stream_position;
    if padding > 0 {
        buffer.resize(buffer.len() + padding, 0);
    }
}

#[inline]
pub fn align_position_with_header_offset(
    position: &mut usize,
    alignment: usize,
    header_size: usize,
) {
    debug_assert!(alignment.is_power_of_two(), "Alignment must be a power of 2");
    let stream_position = *position + header_size;
    let aligned_position = (stream_position + alignment - 1) & !(alignment - 1);
    *position = aligned_position - header_size;
}
