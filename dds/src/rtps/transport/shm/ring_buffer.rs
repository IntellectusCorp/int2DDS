//! Lock-free Ring Buffer for Shared Memory Transport
//!
//! This module implements a lock-free single-producer single-consumer (SPSC)
//! ring buffer designed for shared memory communication.

use std::sync::atomic::{AtomicU64, Ordering};

/// Header structure stored at the beginning of shared memory segment
/// This is placed in shared memory and accessed by multiple processes
#[repr(C)]
pub struct RingBufferHeader {
    /// Magic number to verify valid shared memory segment
    pub magic: u64,
    /// Version for compatibility checking
    pub version: u32,
    /// Total size of the ring buffer data area (excluding header)
    pub buffer_size: u32,
    /// Write position (updated by sender)
    pub write_pos: AtomicU64,
    /// Read position (updated by receiver)
    pub read_pos: AtomicU64,
    /// Maximum message size
    pub max_message_size: u32,
    /// Padding for alignment
    _padding: u32,
}

/// Magic number to identify valid SHM ring buffer
pub const RING_BUFFER_MAGIC: u64 = 0x494E54324444535F; // "INT2DDS_" in hex

/// Current version of ring buffer format
pub const RING_BUFFER_VERSION: u32 = 1;

/// Default ring buffer size (4MB)
pub const DEFAULT_BUFFER_SIZE: usize = 4 * 1024 * 1024;

/// Default maximum message size (64KB)
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Message header prepended to each message in the ring buffer
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessageHeader {
    /// Total length of message including this header
    pub total_len: u32,
    /// Actual data length (excluding header)
    pub data_len: u32,
    /// Sequence number for ordering
    pub sequence: u64,
}

impl MessageHeader {
    pub const SIZE: usize = std::mem::size_of::<MessageHeader>();
}

impl RingBufferHeader {
    pub const SIZE: usize = std::mem::size_of::<RingBufferHeader>();

    /// Initialize a new ring buffer header
    pub fn init(&mut self, buffer_size: u32, max_message_size: u32) {
        self.magic = RING_BUFFER_MAGIC;
        self.version = RING_BUFFER_VERSION;
        self.buffer_size = buffer_size;
        self.write_pos = AtomicU64::new(0);
        self.read_pos = AtomicU64::new(0);
        self.max_message_size = max_message_size;
        self._padding = 0;
    }

    /// Verify this is a valid ring buffer header
    pub fn is_valid(&self) -> bool {
        self.magic == RING_BUFFER_MAGIC && self.version == RING_BUFFER_VERSION
    }
}

/// Ring buffer writer (used by sender)
pub struct RingBufferWriter {
    /// Pointer to the header in shared memory
    header: *mut RingBufferHeader,
    /// Pointer to the data area in shared memory
    data: *mut u8,
    /// Cached buffer size
    buffer_size: usize,
    /// Sequence counter
    sequence: u64,
}

impl std::fmt::Debug for RingBufferWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RingBufferWriter")
            .field("buffer_size", &self.buffer_size)
            .field("sequence", &self.sequence)
            .finish()
    }
}

// Safety: RingBufferWriter can be sent between threads
// The underlying shared memory is thread-safe due to atomic operations
unsafe impl Send for RingBufferWriter {}
unsafe impl Sync for RingBufferWriter {}

impl RingBufferWriter {
    /// Create a new ring buffer writer from raw pointers
    ///
    /// # Safety
    /// - `header` must point to a valid, initialized RingBufferHeader in shared memory
    /// - `data` must point to the data area immediately following the header
    /// - The memory must remain valid for the lifetime of this writer
    pub unsafe fn new(header: *mut RingBufferHeader, data: *mut u8) -> Self {
        let buffer_size = (*header).buffer_size as usize;
        Self { header, data, buffer_size, sequence: 0 }
    }

    /// Write a message to the ring buffer
    ///
    /// Returns the number of bytes written, or an error if the buffer is full
    pub fn write(&mut self, data: &[u8]) -> Result<usize, RingBufferError> {
        let header = unsafe { &*self.header };

        // Check message size
        if data.len() > header.max_message_size as usize {
            return Err(RingBufferError::MessageTooLarge);
        }

        let msg_header = MessageHeader {
            total_len: (MessageHeader::SIZE + data.len()) as u32,
            data_len: data.len() as u32,
            sequence: self.sequence,
        };

        let total_size = align_up(msg_header.total_len as usize, 8);

        // Load current positions with SeqCst for cross-process visibility
        let write_pos = header.write_pos.load(Ordering::SeqCst);
        let read_pos = header.read_pos.load(Ordering::SeqCst);

        // Calculate available space
        let available = if write_pos >= read_pos {
            self.buffer_size - (write_pos - read_pos) as usize
        } else {
            (read_pos - write_pos) as usize
        };

        // Need space for message + possible wrap marker
        if available < total_size + MessageHeader::SIZE {
            return Err(RingBufferError::BufferFull);
        }

        let write_offset = (write_pos as usize) % self.buffer_size;

        // Check if we need to wrap
        if write_offset + total_size > self.buffer_size {
            // Write wrap marker (message with zero data_len)
            let wrap_marker = MessageHeader {
                total_len: (self.buffer_size - write_offset) as u32,
                data_len: 0, // Indicates wrap marker
                sequence: 0,
            };

            unsafe {
                // Write wrap marker using volatile writes
                let marker_bytes = std::slice::from_raw_parts(
                    &wrap_marker as *const MessageHeader as *const u8,
                    MessageHeader::SIZE,
                );
                for (i, byte) in marker_bytes.iter().enumerate() {
                    std::ptr::write_volatile(self.data.add(write_offset + i), *byte);
                }
            }

            // Update write position to start of buffer
            let new_write_pos = write_pos + (self.buffer_size - write_offset) as u64;
            header.write_pos.store(new_write_pos, Ordering::SeqCst);

            // Recursively write at the beginning
            return self.write(data);
        }

        // Write message header and data using volatile writes for cross-process visibility
        unsafe {
            // Write message header byte by byte using volatile
            let header_bytes = std::slice::from_raw_parts(
                &msg_header as *const MessageHeader as *const u8,
                MessageHeader::SIZE,
            );
            for (i, byte) in header_bytes.iter().enumerate() {
                std::ptr::write_volatile(self.data.add(write_offset + i), *byte);
            }

            // Write message data byte by byte using volatile
            for (i, byte) in data.iter().enumerate() {
                std::ptr::write_volatile(
                    self.data.add(write_offset + MessageHeader::SIZE + i),
                    *byte,
                );
            }
        }

        // Memory fence to ensure all data writes are visible before updating write_pos
        std::sync::atomic::fence(Ordering::SeqCst);

        // Update write position with strongest ordering for cross-process visibility
        let new_write_pos = write_pos + total_size as u64;
        header.write_pos.store(new_write_pos, Ordering::SeqCst);

        self.sequence += 1;

        Ok(data.len())
    }

    /// Get the number of bytes available for writing
    pub fn available_space(&self) -> usize {
        let header = unsafe { &*self.header };
        let write_pos = header.write_pos.load(Ordering::Acquire);
        let read_pos = header.read_pos.load(Ordering::Acquire);

        if write_pos >= read_pos {
            self.buffer_size - (write_pos - read_pos) as usize
        } else {
            (read_pos - write_pos) as usize
        }
    }
}

/// Ring buffer reader (used by receiver)
/// Each reader maintains its own local read position to allow multiple readers
pub struct RingBufferReader {
    /// Pointer to the header in shared memory
    header: *mut RingBufferHeader,
    /// Pointer to the data area in shared memory
    data: *const u8,
    /// Cached buffer size
    buffer_size: usize,
    /// Local read position (each reader tracks its own position)
    local_read_pos: u64,
}

impl std::fmt::Debug for RingBufferReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RingBufferReader").field("buffer_size", &self.buffer_size).finish()
    }
}

// Safety: RingBufferReader can be sent between threads
unsafe impl Send for RingBufferReader {}
unsafe impl Sync for RingBufferReader {}

impl RingBufferReader {
    /// Create a new ring buffer reader from raw pointers
    ///
    /// # Safety
    /// - `header` must point to a valid, initialized RingBufferHeader in shared memory
    /// - `data` must point to the data area immediately following the header
    /// - The memory must remain valid for the lifetime of this reader
    pub unsafe fn new(header: *mut RingBufferHeader, data: *const u8) -> Self {
        let buffer_size = (*header).buffer_size as usize;
        // Initialize local_read_pos to current write_pos to only receive new messages
        let local_read_pos = (*header).write_pos.load(Ordering::SeqCst);
        Self { header, data, buffer_size, local_read_pos }
    }

    /// Read a message from the ring buffer
    ///
    /// Returns the message data, or None if the buffer is empty
    /// Each reader maintains its own local read position, allowing multiple readers
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, RingBufferError> {
        let header = unsafe { &*self.header };

        // Use SeqCst for strongest cross-process memory ordering
        let write_pos = header.write_pos.load(Ordering::SeqCst);
        let read_pos = self.local_read_pos; // Use local read position

        // Check if buffer is empty (no new data since our last read)
        if write_pos == read_pos {
            return Ok(None);
        }

        // Check if we're too far behind (data was overwritten)
        // This happens when writer has wrapped around and overwritten our data
        let distance = if write_pos >= read_pos {
            write_pos - read_pos
        } else {
            // Handle wrap-around of write_pos counter
            u64::MAX - read_pos + write_pos + 1
        };

        if distance > self.buffer_size as u64 {
            // We're too far behind, skip to current write position
            log::warn!(
                "[RingBuffer] Reader too far behind, skipping {} bytes",
                distance - self.buffer_size as u64
            );
            self.local_read_pos = write_pos;
            return Ok(None);
        }

        // Memory fence to ensure we see the latest data written by the writer
        std::sync::atomic::fence(Ordering::SeqCst);

        let read_offset = (read_pos as usize) % self.buffer_size;

        // Read message header using volatile reads for cross-process visibility
        let msg_header: MessageHeader = unsafe {
            let mut header_bytes = [0u8; MessageHeader::SIZE];
            for (i, byte) in header_bytes.iter_mut().enumerate() {
                *byte = std::ptr::read_volatile(self.data.add(read_offset + i));
            }
            std::ptr::read(header_bytes.as_ptr() as *const MessageHeader)
        };

        // Check for wrap marker
        if msg_header.data_len == 0 {
            // Skip wrap marker and continue from beginning
            let new_read_pos = read_pos + msg_header.total_len as u64;
            self.local_read_pos = new_read_pos;
            return self.read(buffer);
        }

        let data_len = msg_header.data_len as usize;

        // Check if output buffer is large enough
        if buffer.len() < data_len {
            return Err(RingBufferError::BufferTooSmall);
        }

        // Copy message data using volatile reads
        unsafe {
            for (i, byte) in buffer[..data_len].iter_mut().enumerate() {
                *byte =
                    std::ptr::read_volatile(self.data.add(read_offset + MessageHeader::SIZE + i));
            }
        }

        // Update both local and shared read position
        let total_size = align_up(msg_header.total_len as usize, 8);
        let new_read_pos = read_pos + total_size as u64;
        self.local_read_pos = new_read_pos;

        // Update shared read_pos so writer knows we've consumed the data
        let header = unsafe { &*self.header };
        header.read_pos.store(new_read_pos, Ordering::SeqCst);

        Ok(Some(data_len))
    }

    /// Check if there are messages available to read
    pub fn has_data(&self) -> bool {
        let header = unsafe { &*self.header };
        let write_pos = header.write_pos.load(Ordering::Acquire);
        write_pos != self.local_read_pos
    }

    /// Get the number of bytes available for reading
    pub fn available_data(&self) -> usize {
        let header = unsafe { &*self.header };
        let write_pos = header.write_pos.load(Ordering::Acquire);
        let read_pos = self.local_read_pos;

        if write_pos >= read_pos {
            (write_pos - read_pos) as usize
        } else {
            self.buffer_size - (read_pos - write_pos) as usize
        }
    }
}

/// Errors that can occur during ring buffer operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingBufferError {
    /// The ring buffer is full
    BufferFull,
    /// The message is too large for the ring buffer
    MessageTooLarge,
    /// The output buffer is too small to hold the message
    BufferTooSmall,
    /// The ring buffer header is invalid
    InvalidHeader,
}

impl std::fmt::Display for RingBufferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RingBufferError::BufferFull => write!(f, "Ring buffer is full"),
            RingBufferError::MessageTooLarge => write!(f, "Message too large for ring buffer"),
            RingBufferError::BufferTooSmall => write!(f, "Output buffer too small"),
            RingBufferError::InvalidHeader => write!(f, "Invalid ring buffer header"),
        }
    }
}

impl std::error::Error for RingBufferError {}

/// Align a value up to the specified alignment
#[inline]
fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0, 8), 0);
        assert_eq!(align_up(1, 8), 8);
        assert_eq!(align_up(7, 8), 8);
        assert_eq!(align_up(8, 8), 8);
        assert_eq!(align_up(9, 8), 16);
    }

    #[test]
    fn test_ring_buffer_header_size() {
        // Ensure header size is aligned
        assert_eq!(RingBufferHeader::SIZE % 8, 0);
    }

    #[test]
    fn test_message_header_size() {
        assert_eq!(MessageHeader::SIZE, 16);
    }
}
