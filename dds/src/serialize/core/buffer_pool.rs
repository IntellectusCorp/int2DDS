//! Buffer pool for efficient memory reuse in serialization.
//!
//! This module provides a thread-local buffer pool that reduces allocation overhead
//! by reusing buffers across multiple serialization operations. Buffers are organized
//! into size tiers (Small, Medium, Large) optimized for typical DDS/RTPS message patterns.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};

/// Buffer size tiers optimized for DDS/RTPS serialization patterns
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSize {
    /// Small buffers for parameters and short messages (256 bytes)
    Small,
    /// Medium buffers for typical messages (2 KB)
    Medium,
    /// Large buffers for data samples and long messages (16 KB)
    Large,
}

impl BufferSize {
    const fn capacity(self) -> usize {
        match self {
            BufferSize::Small => 256,
            BufferSize::Medium => 2 * 1024,
            BufferSize::Large => 16 * 1024,
        }
    }

    /// Select appropriate buffer size for the given capacity
    fn for_capacity(capacity: usize) -> Self {
        if capacity <= Self::Small.capacity() {
            BufferSize::Small
        } else if capacity <= Self::Medium.capacity() {
            BufferSize::Medium
        } else {
            BufferSize::Large
        }
    }
}

/// Thread-local buffer pool for high-frequency allocations
struct BufferPool {
    small: Vec<Vec<u8>>,
    medium: Vec<Vec<u8>>,
    large: Vec<Vec<u8>>,
    /// Maximum buffers to keep per size tier
    max_buffers_per_tier: usize,
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new(8)
    }
}

impl BufferPool {
    fn new(max_buffers_per_tier: usize) -> Self {
        Self {
            small: Vec::with_capacity(max_buffers_per_tier),
            medium: Vec::with_capacity(max_buffers_per_tier),
            large: Vec::with_capacity(max_buffers_per_tier),
            max_buffers_per_tier,
        }
    }

    fn acquire(&mut self, size: BufferSize) -> Vec<u8> {
        let pool = match size {
            BufferSize::Small => &mut self.small,
            BufferSize::Medium => &mut self.medium,
            BufferSize::Large => &mut self.large,
        };

        if let Some(mut buffer) = pool.pop() {
            buffer.clear();
            buffer
        } else {
            Vec::with_capacity(size.capacity())
        }
    }

    fn release(&mut self, mut buffer: Vec<u8>, size: BufferSize) {
        let pool = match size {
            BufferSize::Small => &mut self.small,
            BufferSize::Medium => &mut self.medium,
            BufferSize::Large => &mut self.large,
        };

        // Only keep buffer if pool is not full
        if pool.len() < self.max_buffers_per_tier {
            buffer.clear();
            // Shrink if over-allocated (more than 2x tier capacity)
            if buffer.capacity() > size.capacity() * 2 {
                buffer.shrink_to(size.capacity());
            }
            pool.push(buffer);
        }
    }
}

thread_local! {
    static BUFFER_POOL: RefCell<BufferPool> = RefCell::new(BufferPool::default());
}

/// RAII guard that automatically returns buffer to pool on drop
pub struct PooledBuffer {
    buffer: Option<Vec<u8>>,
    size: BufferSize,
}

impl PooledBuffer {
    /// Acquire a buffer from the pool with the specified size tier
    pub fn new(size: BufferSize) -> Self {
        let buffer = BUFFER_POOL.with(|pool| pool.borrow_mut().acquire(size));
        Self { buffer: Some(buffer), size }
    }

    /// Acquire a buffer sized for the given capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(BufferSize::for_capacity(capacity))
    }

    /// Consume the guard and take ownership of the buffer
    /// The buffer will NOT be returned to the pool
    pub fn into_vec(mut self) -> Vec<u8> {
        self.buffer.take().unwrap()
    }

    /// Get a reference to the underlying buffer
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_ref().unwrap()
    }

    /// Get the length of the buffer content
    pub fn len(&self) -> usize {
        self.buffer.as_ref().unwrap().len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.as_ref().unwrap().is_empty()
    }

    /// Clear the buffer content
    pub fn clear(&mut self) {
        self.buffer.as_mut().unwrap().clear();
    }

    /// Reserve additional capacity
    pub fn reserve(&mut self, additional: usize) {
        self.buffer.as_mut().unwrap().reserve(additional);
    }
}

impl Deref for PooledBuffer {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        self.buffer.as_ref().unwrap()
    }
}

impl DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer.as_mut().unwrap()
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            BUFFER_POOL.with(|pool| pool.borrow_mut().release(buffer, self.size));
        }
    }
}
