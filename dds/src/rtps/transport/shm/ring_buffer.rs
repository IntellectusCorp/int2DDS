//! Lock-free Ring Buffer for Shared Memory Transport
//!
//! This module implements a lock-free multi-producer multi-consumer (MPMC)
//! ring buffer designed for shared memory communication.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use log::warn;

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
    /// Write position (atomically updated by multiple writers via CAS)
    pub write_pos: AtomicU64,
    /// Global sequence counter (atomically incremented by all writers)
    pub sequence: AtomicU64,
    /// Maximum message size
    pub max_message_size: u32,
    /// Padding for alignment
    _padding: u32,
}

/// Magic number to identify valid SHM ring buffer
pub const RING_BUFFER_MAGIC: u64 = 0x494E54324444535F; // "INT2DDS_" in hex

/// Current version of ring buffer format
pub const RING_BUFFER_VERSION: u32 = 1;

/// Default ring buffer size (1MB)
pub const DEFAULT_BUFFER_SIZE: usize = 1024 * 1024;

/// Cached SHM buffer size - read once from environment variable
static SHM_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();

/// Get SHM buffer size from environment variable or default
pub fn get_buffer_size() -> usize {
    *SHM_BUFFER_SIZE.get_or_init(|| {
        std::env::var("INT2DDS_SHM_BUFFER_SIZE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(DEFAULT_BUFFER_SIZE)
    })
}

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
        self.sequence = AtomicU64::new(0);
        self.max_message_size = max_message_size;
        self._padding = 0;
    }

    /// Verify this is a valid ring buffer header
    pub fn is_valid(&self) -> bool {
        self.magic == RING_BUFFER_MAGIC && self.version == RING_BUFFER_VERSION
    }
}

/// Ring buffer writer (used by sender)
/// Multiple writers can safely write concurrently using CAS-based space reservation.
pub struct RingBufferWriter {
    /// Pointer to the header in shared memory
    header: *mut RingBufferHeader,
    /// Pointer to the data area in shared memory
    data: *mut u8,
    /// Cached buffer size
    buffer_size: usize,
    /// Cached max message size
    max_message_size: usize,
}

impl std::fmt::Debug for RingBufferWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RingBufferWriter").field("buffer_size", &self.buffer_size).finish()
    }
}

// Safety: RingBufferWriter can be sent between threads
// The underlying shared memory is thread-safe due to atomic operations
unsafe impl Send for RingBufferWriter {}
unsafe impl Sync for RingBufferWriter {}

/// Maximum number of CAS retry attempts before giving up
const MAX_CAS_RETRIES: usize = 100;

impl RingBufferWriter {
    /// Create a new ring buffer writer from raw pointers
    ///
    /// # Safety
    /// - `header` must point to a valid, initialized RingBufferHeader in shared memory
    /// - `data` must point to the data area immediately following the header
    /// - The memory must remain valid for the lifetime of this writer
    pub unsafe fn new(header: *mut RingBufferHeader, data: *mut u8) -> Self {
        let buffer_size = (*header).buffer_size as usize;
        let max_message_size = (*header).max_message_size as usize;
        Self { header, data, buffer_size, max_message_size }
    }

    /// Write a message to the ring buffer using CAS-based space reservation
    ///
    /// Multiple writers can safely call this method concurrently.
    /// Space is reserved atomically using compare-and-swap, then data is written.
    pub fn write(&mut self, data: &[u8]) -> Result<usize, RingBufferError> {
        // Check message size
        if data.len() > self.max_message_size {
            return Err(RingBufferError::MessageTooLarge);
        }

        let total_size = align_up(MessageHeader::SIZE + data.len(), 8);

        // Check if message fits in buffer at all
        if total_size > self.buffer_size {
            return Err(RingBufferError::MessageTooLarge);
        }

        let header = unsafe { &*self.header };

        // CAS loop to reserve space
        let mut retries = 0;
        let reserved_pos = loop {
            if retries >= MAX_CAS_RETRIES {
                return Err(RingBufferError::BufferFull);
            }

            let current_pos = header.write_pos.load(Ordering::SeqCst);
            let write_offset = (current_pos as usize) % self.buffer_size;
            let remaining_space = self.buffer_size - write_offset;

            // Check if we need to wrap
            if total_size > remaining_space {
                // Try to advance write_pos to wrap around
                let wrap_pos = current_pos + remaining_space as u64;

                // Write wrap marker if there's space for header
                if remaining_space >= MessageHeader::SIZE {
                    // Try CAS first to claim the wrap operation
                    if header
                        .write_pos
                        .compare_exchange(current_pos, wrap_pos, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        // We won the CAS, write the wrap marker
                        let wrap_marker = MessageHeader {
                            total_len: remaining_space as u32,
                            data_len: 0, // Indicates wrap marker
                            sequence: 0,
                        };

                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                &wrap_marker as *const MessageHeader as *const u8,
                                self.data.add(write_offset),
                                MessageHeader::SIZE,
                            );
                        }
                    }
                    // If CAS failed, another writer handled the wrap, retry
                }

                retries += 1;
                continue;
            }

            // Try to reserve space for our message
            let new_pos = current_pos + total_size as u64;
            match header.write_pos.compare_exchange(
                current_pos,
                new_pos,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break current_pos, // Successfully reserved space
                Err(_) => {
                    // Another writer beat us, retry
                    retries += 1;
                    continue;
                }
            }
        };

        // Get sequence number atomically
        let sequence = header.sequence.fetch_add(1, Ordering::SeqCst);

        // Now we have exclusive access to the reserved space, write the data
        let write_offset = (reserved_pos as usize) % self.buffer_size;

        let msg_header = MessageHeader {
            total_len: (MessageHeader::SIZE + data.len()) as u32,
            data_len: data.len() as u32,
            sequence,
        };

        unsafe {
            // Write message header
            std::ptr::copy_nonoverlapping(
                &msg_header as *const MessageHeader as *const u8,
                self.data.add(write_offset),
                MessageHeader::SIZE,
            );

            // Write message data
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                self.data.add(write_offset + MessageHeader::SIZE),
                data.len(),
            );
        }

        // Memory fence to ensure all data writes are visible
        std::sync::atomic::fence(Ordering::SeqCst);

        Ok(data.len())
    }

    /// Get current write position (for diagnostics)
    pub fn write_position(&self) -> u64 {
        let header = unsafe { &*self.header };
        header.write_pos.load(Ordering::Acquire)
    }

    /// Get current global sequence number (for diagnostics)
    pub fn sequence(&self) -> u64 {
        let header = unsafe { &*self.header };
        header.sequence.load(Ordering::Acquire)
    }
}

/// Ring buffer reader (used by receiver in MPMC pattern)
///
/// Each reader maintains its own local read position, allowing multiple
/// readers to consume from the same ring buffer independently.
pub struct RingBufferReader {
    /// Pointer to the header in shared memory (read-only access to write_pos)
    header: *const RingBufferHeader,
    /// Pointer to the data area in shared memory
    data: *const u8,
    /// Cached buffer size
    buffer_size: usize,
    /// Local read position (independent per reader, not shared)
    local_read_pos: u64,
    /// Last seen sequence number for detecting gaps
    last_sequence: u64,
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
    pub unsafe fn new(header: *const RingBufferHeader, data: *const u8) -> Self {
        let buffer_size = (*header).buffer_size as usize;
        // Initialize local_read_pos to current write_pos to only receive new messages
        let local_read_pos = (*header).write_pos.load(Ordering::SeqCst);
        Self { header, data, buffer_size, local_read_pos, last_sequence: 0 }
    }

    /// Read a message from the ring buffer
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<Option<(usize, u64)>, RingBufferError> {
        let header = unsafe { &*self.header };

        // Use SeqCst for strongest cross-process memory ordering
        let write_pos = header.write_pos.load(Ordering::SeqCst);
        let read_pos = self.local_read_pos;

        // Check if buffer is empty (no new data since our last read)
        if write_pos == read_pos {
            return Ok(None);
        }

        // Check if we're too far behind (data was overwritten)
        let distance = write_pos.saturating_sub(read_pos);

        if distance > self.buffer_size as u64 {
            let skip_to = write_pos - self.buffer_size as u64;
            warn!(
                "[RingBuffer] Reader too far behind, skipping from {} to {} ({} bytes lost)",
                read_pos,
                skip_to,
                skip_to - read_pos
            );
            self.local_read_pos = skip_to;
            // Try reading again from new position
            return self.read(buffer);
        }

        // Memory fence to ensure we see the latest data written by the writer
        std::sync::atomic::fence(Ordering::SeqCst);

        let read_offset = (read_pos as usize) % self.buffer_size;

        // Read message header
        let msg_header: MessageHeader =
            unsafe { std::ptr::read(self.data.add(read_offset) as *const MessageHeader) };

        // Validate message header to detect corrupted data
        let total_len = msg_header.total_len as usize;
        let data_len = msg_header.data_len as usize;

        // Sanity check: total_len should be reasonable
        if total_len == 0 || total_len > self.buffer_size {
            // Corrupted header, skip to current write position
            warn!(
                "[RingBuffer] Corrupted header detected (total_len={}), resetting reader",
                total_len
            );
            self.local_read_pos = write_pos;
            return Ok(None);
        }

        // Check for wrap marker
        if data_len == 0 {
            // Skip wrap marker and continue from beginning
            let new_read_pos = read_pos + total_len as u64;
            self.local_read_pos = new_read_pos;
            return self.read(buffer);
        }

        // Validate data_len is within total_len
        if data_len > total_len.saturating_sub(MessageHeader::SIZE) {
            warn!(
                "[RingBuffer] Invalid data_len {} for total_len {}, skipping message",
                data_len, total_len
            );
            let total_size = align_up(total_len, 8);
            self.local_read_pos = read_pos + total_size as u64;
            return self.read(buffer);
        }

        // Check if output buffer is large enough
        if buffer.len() < data_len {
            return Err(RingBufferError::BufferTooSmall);
        }

        // Copy message data
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.data.add(read_offset + MessageHeader::SIZE),
                buffer.as_mut_ptr(),
                data_len,
            );
        }

        // Calculate lost messages via sequence gap
        let expected_sequence = self.last_sequence + 1;
        let lost = if self.last_sequence == 0 {
            0 // First message, no loss
        } else if msg_header.sequence >= expected_sequence {
            msg_header.sequence - expected_sequence
        } else {
            0 // Sequence wrapped or reset
        };

        self.last_sequence = msg_header.sequence;

        // Update local read position only (no shared state update)
        let total_size = align_up(total_len, 8);
        self.local_read_pos = read_pos + total_size as u64;

        Ok(Some((data_len, lost)))
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
        write_pos.saturating_sub(self.local_read_pos) as usize
    }

    /// Get current local read position (for diagnostics)
    pub fn read_position(&self) -> u64 {
        self.local_read_pos
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
    debug_assert!(alignment.is_power_of_two(), "alignment must be a power of two");
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
