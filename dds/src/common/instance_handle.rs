//! Instance handle - Unique identifier for DDS data instances.
//!
//! An `InstanceHandle` uniquely identifies a data instance within a topic. It is derived
//! from the key fields of the data type and is used in operations like `register_instance()`,
//! `write()`, and `dispose()`.

use std::ops::{Index, IndexMut};

use crate::rtps::common::{entity_id::EntityId, guid::Guid, parameters::KeyHash};

#[derive(Default, Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstanceHandle {
    value: KeyHash,
    is_defined: bool,
}
impl std::fmt::Display for InstanceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.is_defined {
            return write!(f, "<undefined>");
        }
        for b in &self.value {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl InstanceHandle {
    pub const NIL: Self = Self { value: [0; 16], is_defined: false };

    pub fn new(value: KeyHash) -> Self {
        Self { value, is_defined: true }
    }

    pub fn from_key_cdr(key_cdr: &[u8]) -> Self {
        let mut value = [0u8; 16];
        if key_cdr.len() <= 16 {
            value[..key_cdr.len()].copy_from_slice(key_cdr);
        } else {
            value = md5::compute(key_cdr).0;
        }
        Self::new(value)
    }

    pub fn from_key_cdr_hashed(key_cdr: &[u8]) -> Self {
        Self::new(md5::compute(key_cdr).0)
    }

    pub fn value(&self) -> &[u8; 16] {
        &self.value
    }

    pub(crate) fn from_guid(guid: &Guid) -> Self {
        let mut v = [0u8; 16];
        v[..12].copy_from_slice(&guid.prefix());
        v[12..].copy_from_slice(&guid.entity_id().to_bytes());
        Self { value: v, is_defined: true }
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_guid(&self) -> Guid {
        let mut prefix = [0u8; 12];
        prefix.copy_from_slice(&self.value[..12]);

        let mut entity_id_bytes = [0u8; 4];
        entity_id_bytes.copy_from_slice(&self.value[12..16]);
        let entity_id = EntityId::from_bytes(entity_id_bytes);

        // Combine GuidPrefix and EntityId to create Guid
        Guid::new(prefix, entity_id)
    }

    pub(crate) fn is_nil(&self) -> bool {
        *self == Self::NIL
    }
}

// Implement indexing for read access
impl Index<usize> for InstanceHandle {
    type Output = u8;

    fn index(&self, idx: usize) -> &Self::Output {
        &self.value[idx]
    }
}

// Implement indexing for write access
impl IndexMut<usize> for InstanceHandle {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.value[idx]
    }
}

// Implement range indexing
impl Index<std::ops::Range<usize>> for InstanceHandle {
    type Output = [u8];

    fn index(&self, range: std::ops::Range<usize>) -> &Self::Output {
        &self.value[range]
    }
}

impl IndexMut<std::ops::Range<usize>> for InstanceHandle {
    fn index_mut(&mut self, range: std::ops::Range<usize>) -> &mut Self::Output {
        &mut self.value[range]
    }
}

#[cfg(test)]
mod tests {

    use crate::rtps::common::entity_id::EntityId;

    use super::*;
    #[test]
    fn test_instance_handle_from_guid() {
        let prefix = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let entity_id = EntityId::UNKNOWN;
        let guid = Guid::new(prefix, entity_id);

        let handle = InstanceHandle::from_guid(&guid);

        let expected = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 0, 0, 0, 0];
        assert_eq!(*handle.value(), expected);
    }

    #[test]
    fn test_nil_instance() {
        let nil_handle = InstanceHandle::NIL;
        let expected = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(*nil_handle.value(), expected);
    }
}
