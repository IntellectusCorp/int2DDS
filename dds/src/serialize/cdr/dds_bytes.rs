//! DdsBytes - Zero-copy byte sequence type for DDS.
//!
//! When a CacheChange holds `DataPayload::Shared(Arc<Vec<u8>>)`, DdsBytes can
//! reference a sub-slice of that shared buffer without copying. For non-shared
//! payloads, DdsBytes falls back to an owned `Vec<u8>`.
//!
//! Users opt-in by declaring `data: DdsBytes` instead of `data: Vec<u8>` in
//! their DDS type structs.

use std::{fmt, ops::Deref, sync::Arc};

/// A byte sequence that can be either owned or a zero-copy sub-slice of a shared buffer.
#[derive(Clone)]
pub enum DdsBytes {
    Owned(Vec<u8>),
    Shared { backing: Arc<Vec<u8>>, offset: usize, len: usize },
}

impl DdsBytes {
    pub fn owned(data: Vec<u8>) -> Self {
        DdsBytes::Owned(data)
    }

    pub fn shared(backing: Arc<Vec<u8>>, offset: usize, len: usize) -> Self {
        DdsBytes::Shared { backing, offset, len }
    }

    pub fn len(&self) -> usize {
        match self {
            DdsBytes::Owned(v) => v.len(),
            DdsBytes::Shared { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Deref for DdsBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            DdsBytes::Owned(v) => v,
            DdsBytes::Shared { backing, offset, len } => &backing[*offset..*offset + *len],
        }
    }
}

impl AsRef<[u8]> for DdsBytes {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl Default for DdsBytes {
    fn default() -> Self {
        DdsBytes::Owned(Vec::new())
    }
}

impl fmt::Debug for DdsBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DdsBytes::Owned(v) => write!(f, "DdsBytes::Owned([{} bytes])", v.len()),
            DdsBytes::Shared { len, .. } => write!(f, "DdsBytes::Shared([{} bytes])", len),
        }
    }
}

impl PartialEq for DdsBytes {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

impl Eq for DdsBytes {}

impl From<Vec<u8>> for DdsBytes {
    fn from(v: Vec<u8>) -> Self {
        DdsBytes::Owned(v)
    }
}

impl From<DdsBytes> for Vec<u8> {
    fn from(b: DdsBytes) -> Vec<u8> {
        match b {
            DdsBytes::Owned(v) => v,
            DdsBytes::Shared { backing, offset, len } => backing[offset..offset + len].to_vec(),
        }
    }
}
