//! Generic key-value entry type.
//!
//! This module provides a simple `Entry` structure for storing key-value pairs,
//! used in various internal data structures throughout int2dds.

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry<K, V> {
    pub key: K,
    pub value: V,
}

impl<K, V> Entry<K, V> {
    pub fn new(key: K, value: V) -> Self {
        Self { key, value }
    }
}
