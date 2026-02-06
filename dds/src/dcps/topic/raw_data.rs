//! Raw Data Type Support for FFI
//!
//! This module provides a type-erased data type for use with FFI bindings.
//! The RawData type wraps raw bytes that have been pre-serialized by the C application.

use std::any::{Any, TypeId};
use std::sync::Arc;

use crate::{
    common::instance_handle::InstanceHandle,
    dcps::core::error::{DdsError, DdsResult},
    rtps::common::types::SerializedData,
    serialize::xcdr::ExtensibilityKind,
    topic::sql::ast::Parameter,
};

use super::type_support::{DdsType, SerializationFormat, TypeSupport};

/// Raw data wrapper for FFI
///
/// This type wraps pre-serialized bytes from C applications.
/// The bytes are passed through without additional serialization.
/// Uses Arc<[u8]> internally to avoid copies during serialization.
#[derive(Clone, Debug)]
pub struct RawData {
    /// The raw serialized bytes (Arc for zero-copy serialization)
    pub data: Arc<[u8]>,
    /// Optional key bytes for keyed types
    pub key: Option<Arc<[u8]>>,
}

/// Static empty data for key-only operations (avoids allocation)
static EMPTY_DATA: &[u8] = &[];

impl RawData {
    /// Create new RawData from bytes
    pub fn new(data: Vec<u8>) -> Self {
        Self { data: data.into(), key: None }
    }

    /// Create new RawData from a slice (zero-copy to Arc)
    ///
    /// This is more efficient than `new()` as it avoids intermediate Vec allocation.
    /// Arc allocates exactly the required size without Vec's capacity overhead.
    #[inline]
    pub fn from_slice(data: &[u8]) -> Self {
        Self { data: Arc::from(data), key: None }
    }

    /// Create new RawData with key
    pub fn with_key(data: Vec<u8>, key: Vec<u8>) -> Self {
        Self { data: data.into(), key: Some(key.into()) }
    }

    /// Create new RawData with key from slices (zero-copy to Arc)
    ///
    /// This is more efficient than `with_key()` as it avoids intermediate Vec allocations.
    #[inline]
    pub fn from_slices(data: &[u8], key: &[u8]) -> Self {
        Self { data: Arc::from(data), key: Some(Arc::from(key)) }
    }

    /// Create RawData with key only (for register/unregister/dispose operations)
    /// This avoids allocating empty data vector
    pub fn key_only(key: Vec<u8>) -> Self {
        Self { data: Arc::from(EMPTY_DATA), key: Some(key.into()) }
    }

    /// Create RawData with key only from slice (zero-copy to Arc)
    ///
    /// More efficient than `key_only()` as it avoids intermediate Vec allocation.
    #[inline]
    pub fn key_only_from_slice(key: &[u8]) -> Self {
        Self { data: Arc::from(EMPTY_DATA), key: Some(Arc::from(key)) }
    }

    /// Create empty RawData (for operations that don't need data or key)
    pub fn empty() -> Self {
        Self { data: Arc::from(EMPTY_DATA), key: None }
    }

    /// Get the raw bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Get the key bytes if available
    pub fn key_bytes(&self) -> Option<&[u8]> {
        self.key.as_deref()
    }
}

impl DdsType for RawData {
    type TypeSupport = RawDataTypeSupport;

    fn get_type_name() -> String {
        "RawData".to_string()
    }
}

/// Type support for RawData
///
/// This implementation passes through pre-serialized bytes without modification.
/// The C application is responsible for serialization/deserialization.
#[derive(Clone, Debug)]
pub struct RawDataTypeSupport {
    type_name: String,
}

impl Default for RawDataTypeSupport {
    fn default() -> Self {
        Self { type_name: "RawData".to_string() }
    }
}

impl RawDataTypeSupport {
    /// Create with custom type name
    pub fn with_type_name(type_name: &str) -> Self {
        Self { type_name: type_name.to_string() }
    }
}

impl TypeSupport for RawDataTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<RawData>()
    }

    fn get_type_name(&self) -> &str {
        &self.type_name
    }

    fn get_field_value(&self, _data: &dyn Any, _field_path: &str) -> DdsResult<Parameter> {
        // RawData doesn't support field access
        Err(DdsError::Unsupported)
    }

    fn has_field(&self, _field_path: &str) -> bool {
        false
    }

    fn serialize(
        &self,
        data: &dyn Any,
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData> {
        // Ignore format - data is already serialized
        let raw_data = data
            .downcast_ref::<RawData>()
            .ok_or_else(|| DdsError::Error("Expected RawData type".to_string()))?;

        // Return the pre-serialized bytes directly (Arc clone is cheap - just ref count increment)
        Ok(raw_data.data.clone())
    }

    fn deserialize(
        &self,
        data: &[u8],
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        // Ignore format - pass raw bytes
        Ok(Box::new(RawData::new(data.to_vec())))
    }

    fn serialize_key(&self, data: &dyn Any) -> DdsResult<SerializedData> {
        let raw_data = data
            .downcast_ref::<RawData>()
            .ok_or_else(|| DdsError::Error("Expected RawData type".to_string()))?;

        // Return key bytes if available, otherwise empty (Arc clone is cheap)
        match &raw_data.key {
            Some(key) => Ok(key.clone()),
            None => Ok(Arc::from(Vec::new())),
        }
    }

    fn deserialize_key(&self, serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        // Create RawData with key only
        Ok(Box::new(RawData {
            data: Arc::from(Vec::<u8>::new()),
            key: Some(Arc::from(serialized_key.to_vec())),
        }))
    }

    fn compute_key(&self, data: &dyn Any) -> InstanceHandle {
        let raw_data = match data.downcast_ref::<RawData>() {
            Some(d) => d,
            None => return InstanceHandle::NIL,
        };

        // Compute hash from key bytes if available
        match &raw_data.key {
            Some(key) if !key.is_empty() => {
                // Simple hash computation
                let mut hash = [0u8; 16];
                for (i, byte) in key.iter().enumerate() {
                    hash[i % 16] ^= byte;
                }
                InstanceHandle::new(hash)
            }
            _ => InstanceHandle::NIL,
        }
    }

    fn is_compute_key_provided(&self) -> bool {
        // RawData only supports keys when explicitly provided via with_key()
        // Return false to skip unnecessary key computation for keyless data
        false
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        ExtensibilityKind::Final
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_data_creation() {
        let data = vec![1, 2, 3, 4];
        let raw = RawData::new(data.clone());
        assert_eq!(raw.as_bytes(), &data);
        assert!(raw.key_bytes().is_none());
    }

    #[test]
    fn test_raw_data_with_key() {
        let data = vec![1, 2, 3, 4];
        let key = vec![5, 6, 7, 8];
        let raw = RawData::with_key(data.clone(), key.clone());
        assert_eq!(raw.as_bytes(), &data);
        assert_eq!(raw.key_bytes(), Some(key.as_slice()));
    }

    #[test]
    fn test_type_support_serialize() {
        let ts = RawDataTypeSupport::default();
        let data = vec![1, 2, 3, 4];
        let raw = RawData::new(data.clone());

        let serialized = ts.serialize(&raw as &dyn Any, None).unwrap();
        assert_eq!(serialized.as_ref(), &data);
    }

    #[test]
    fn test_type_support_deserialize() {
        let ts = RawDataTypeSupport::default();
        let data = vec![1, 2, 3, 4];

        let deserialized = ts.deserialize(&data, None).unwrap();
        let raw = deserialized.downcast_ref::<RawData>().unwrap();
        assert_eq!(raw.as_bytes(), &data);
    }
}
