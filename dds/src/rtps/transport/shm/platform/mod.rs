//! Platform-specific Shared Memory Implementation
//!
//! This module provides platform-specific shared memory implementations
//! for Windows and Unix-like systems.

#[cfg(windows)]
pub mod windows;

#[cfg(unix)]
pub mod unix;

use std::io;

/// Platform-independent shared memory segment handle
pub struct SharedMemory {
    #[cfg(windows)]
    inner: windows::WindowsSharedMemory,

    #[cfg(unix)]
    inner: unix::UnixSharedMemory,
}

impl std::fmt::Debug for SharedMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMemory")
            .field("size", &self.size())
            .field("is_creator", &self.is_creator())
            .finish()
    }
}

impl SharedMemory {
    /// Create a new shared memory segment
    ///
    /// # Arguments
    /// * `name` - Name of the shared memory segment
    /// * `size` - Size in bytes
    /// * `create` - If true, create new segment; if false, open existing
    pub fn new(name: &str, size: usize, create: bool) -> io::Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self { inner: windows::WindowsSharedMemory::new(name, size, create)? })
        }

        #[cfg(unix)]
        {
            Ok(Self { inner: unix::UnixSharedMemory::new(name, size, create)? })
        }

        #[cfg(not(any(windows, unix)))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Shared memory not supported on this platform",
            ))
        }
    }

    /// Get a raw pointer to the shared memory
    pub fn as_ptr(&self) -> *mut u8 {
        self.inner.as_ptr()
    }

    /// Get the size of the shared memory segment
    pub fn size(&self) -> usize {
        self.inner.size()
    }

    /// Check if this process created the shared memory
    pub fn is_creator(&self) -> bool {
        self.inner.is_creator()
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        // Platform-specific cleanup is handled by inner Drop
    }
}

/// Generate a shared memory segment name for a given domain
pub fn shm_segment_name(domain_id: u32, segment_type: &str) -> String {
    format!("int2dds_shm_d{}_{}", domain_id, segment_type)
}
