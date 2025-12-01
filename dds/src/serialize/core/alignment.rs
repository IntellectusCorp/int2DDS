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
