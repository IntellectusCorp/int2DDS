//! Lightweight TypeSupport for the raw bytes FFI path.
//!
//! `RawTypeSupport` implements the `TypeSupport` trait with stub serialize/deserialize
//! methods. It is used when C users handle serialization themselves via
//! `int2dds_write_serialized()` / `int2dds_take_serialized()`.
//!
//! This allows topic creation and DDS discovery without the full dynamic type system.

use std::any::{Any, TypeId};
use std::sync::Arc;

use int2dds::{
    common::instance_handle::InstanceHandle,
    core::error::{DdsError, DdsResult},
    rtps::common::types::SerializedData,
    serialize::cdr::ExtensibilityKind,
    topic::sql::ast::Parameter,
    topic::type_support::{FieldAccessor, SerializationFormat, TypeSupport},
    xtypes::{TypeIdentifier, TypeObject},
};

/// CDR field type for key extraction
#[derive(Clone, Debug)]
pub enum KeyFieldType {
    String,
    Int32,
    UInt32,
    Int16,
    UInt16,
    Int64,
    UInt64,
    Int8,
    UInt8,
    Bool,
}

/// Key field descriptor
#[derive(Clone, Debug)]
pub struct KeyFieldInfo {
    pub field_index: usize,
    pub field_type: KeyFieldType,
}

/// Lightweight TypeSupport for raw bytes FFI path.
///
/// Serialize/deserialize methods return errors since they are never called
/// when using `write_serialized()` / `take_serialized()`.
pub struct RawTypeSupport {
    type_name: String,
    extensibility: ExtensibilityKind,
    has_key: bool,
    type_identifier: Option<TypeIdentifier>,
    type_object: Option<TypeObject>,
    key_fields: Vec<KeyFieldInfo>,
    all_fields: Option<Arc<Vec<crate::data::CdrFieldDescriptor>>>,
}

impl RawTypeSupport {
    pub fn new(type_name: String, extensibility: ExtensibilityKind) -> Self {
        Self {
            type_name,
            extensibility,
            has_key: false,
            type_identifier: None,
            type_object: None,
            key_fields: Vec::new(),
            all_fields: None,
        }
    }

    pub fn new_with_key(
        type_name: String,
        extensibility: ExtensibilityKind,
        has_key: bool,
    ) -> Self {
        Self {
            type_name,
            extensibility,
            has_key,
            type_identifier: None,
            type_object: None,
            key_fields: Vec::new(),
            all_fields: None,
        }
    }

    /// Create a RawTypeSupport with pre-built TypeIdentifier and TypeObject.
    ///
    /// This enables DDS-XTypes discovery parameters (0x0069, 0x0072) to be sent
    /// during endpoint matching.
    pub fn with_type_info(
        type_name: String,
        extensibility: ExtensibilityKind,
        has_key: bool,
        type_identifier: TypeIdentifier,
        type_object: TypeObject,
    ) -> Self {
        Self {
            type_name,
            extensibility,
            has_key,
            type_identifier: Some(type_identifier),
            type_object: Some(type_object),
            key_fields: Vec::new(),
            all_fields: None,
        }
    }

    /// Set key field metadata for compute_key() support.
    /// Called from FFI when Python binding provides key field info.
    pub fn set_key_fields(&mut self, fields: Vec<KeyFieldInfo>) {
        self.key_fields = fields;
    }

    /// Set all field descriptors for get_field_value() / has_field() support.
    pub fn set_all_fields(&mut self, fields: Vec<crate::data::CdrFieldDescriptor>) {
        self.all_fields = Some(Arc::new(fields));
    }

    /// Extract key bytes from CDR-serialized data using key field metadata.
    /// Parses CDR fields sequentially, collecting only key field values.
    fn extract_key_from_cdr(&self, cdr_bytes: &[u8]) -> Vec<u8> {
        if cdr_bytes.len() < 4 {
            return Vec::new();
        }

        // Skip 4-byte CDR encapsulation header
        let encoding_id = u16::from_be_bytes([cdr_bytes[0], cdr_bytes[1]]);
        let is_xcdr2 = matches!(encoding_id, 0x0006 | 0x0007 | 0x0008 | 0x0009 | 0x000A | 0x000B);
        let mut pos = 4;

        // Skip DHEADER (4 bytes) for Appendable/Mutable XCDR2
        if is_xcdr2
            && matches!(
                self.extensibility,
                ExtensibilityKind::Appendable | ExtensibilityKind::Mutable
            )
        {
            if pos + 4 <= cdr_bytes.len() {
                pos += 4;
            }
        }

        let mut key_bytes = Vec::new();
        let data = cdr_bytes;

        // Parse fields sequentially, collecting key field values
        for (idx, field_info) in self.key_fields.iter().enumerate() {
            // Skip non-key fields up to this field index
            // For now, we only support the first field being a key (common case: color)
            // A full implementation would need all field types to skip correctly
            if field_info.field_index == 0 && idx == 0 {
                match &field_info.field_type {
                    KeyFieldType::String => {
                        if pos + 4 <= data.len() {
                            let str_len = u32::from_le_bytes([
                                data[pos],
                                data[pos + 1],
                                data[pos + 2],
                                data[pos + 3],
                            ]) as usize;
                            pos += 4;
                            if pos + str_len <= data.len() {
                                // Include length + string bytes (with null terminator) for key hash
                                key_bytes.extend_from_slice(&(str_len as u32).to_be_bytes());
                                key_bytes.extend_from_slice(&data[pos..pos + str_len]);
                            }
                        }
                    }
                    KeyFieldType::Int32 | KeyFieldType::UInt32 => {
                        if pos + 4 <= data.len() {
                            key_bytes.extend_from_slice(&data[pos..pos + 4]);
                        }
                    }
                    _ => {}
                }
            }
        }

        key_bytes
    }
}

impl FieldAccessor for RawTypeSupport {
    fn get_field_value(&self, _data: &dyn Any, _field_path: &str) -> DdsResult<Parameter> {
        Err(DdsError::Error(
            "RawTypeSupport: field access not supported in raw bytes mode".to_string(),
        ))
    }

    fn has_field(&self, _field_path: &str) -> bool {
        false
    }
}

impl TypeSupport for RawTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<crate::data::Int2DdsData>()
    }

    fn get_type_name(&self) -> &str {
        &self.type_name
    }

    fn serialize(
        &self,
        _data: &dyn Any,
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData> {
        Err(DdsError::Error("RawTypeSupport: use write_serialized() instead".to_string()))
    }

    fn deserialize(
        &self,
        data: &[u8],
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        // Store CDR bytes and field metadata when configured (Python binding).
        // Otherwise return empty Int2DdsData (existing behavior for C/C# bindings).
        let need_bytes = !self.key_fields.is_empty() || self.all_fields.is_some();
        Ok(Box::new(crate::data::Int2DdsData {
            cdr_bytes: if need_bytes { Some(data.to_vec()) } else { None },
            field_descriptors: self.all_fields.clone(),
            extensibility: self.extensibility,
        }))
    }

    fn serialize_key(&self, _data: &dyn Any) -> DdsResult<SerializedData> {
        Ok(Arc::from(Vec::new()))
    }

    fn deserialize_key(&self, _serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        Err(DdsError::Error("RawTypeSupport: use take_serialized() for key access".to_string()))
    }

    fn compute_key(&self, data: &dyn Any) -> InstanceHandle {
        // No key fields configured — preserve existing behavior (C/C# bindings)
        if self.key_fields.is_empty() {
            return InstanceHandle::NIL;
        }

        let int2dds_data = match data.downcast_ref::<crate::data::Int2DdsData>() {
            Some(d) => d,
            None => return InstanceHandle::NIL,
        };

        let cdr_bytes = match &int2dds_data.cdr_bytes {
            Some(b) => b,
            None => return InstanceHandle::NIL,
        };

        let key_bytes = self.extract_key_from_cdr(cdr_bytes);
        if key_bytes.is_empty() {
            return InstanceHandle::NIL;
        }

        let hash = md5::compute(&key_bytes);
        InstanceHandle::new(hash.0)
    }

    fn is_compute_key_provided(&self) -> bool {
        self.has_key
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        self.extensibility
    }

    fn get_type_identifier(&self) -> Option<TypeIdentifier> {
        self.type_identifier.clone()
    }

    fn get_type_object(&self) -> Option<TypeObject> {
        self.type_object.clone()
    }
}
