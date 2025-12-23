//! # Type Descriptor API
//!
//! Runtime type definition for C/FFI interoperability.
//!
//! ## Overview
//!
//! TypeDescriptor allows C applications to define DDS data types at runtime,
//! enabling automatic CDR serialization/deserialization without manual encoding.
//!
//! ## Example (C)
//!
//! ```c
//! Int2DdsTypeDescriptor* desc;
//! int2dds_type_descriptor_create("HelloWorld", &desc);
//! int2dds_type_descriptor_add_u32(desc, "id", true);  // key field
//! int2dds_type_descriptor_add_string(desc, "message", 256, false);
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use int2dds::serialize::xcdr::ExtensibilityKind;

use crate::error::*;

/// Field type enumeration for C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Int2DdsFieldType {
    Bool = 0,
    Int8 = 1,
    UInt8 = 2,
    Int16 = 3,
    UInt16 = 4,
    Int32 = 5,
    UInt32 = 6,
    Int64 = 7,
    UInt64 = 8,
    Float32 = 9,
    Float64 = 10,
    String = 11,
    Sequence = 12,
    Array = 13,
    Struct = 14,
    /// Optimized byte sequence - zero-copy for large payloads (1KB-1MB)
    Bytes = 15,
}

/// Extensibility kind for XCDR encoding
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Int2DdsExtensibilityKind {
    /// No extensibility - fastest encoding (XCDR2 PlainCDR2)
    Final = 0,
    /// Can append new members at end (XCDR2 Delimited)
    Appendable = 1,
    /// Full extensibility with member headers (XCDR2 PL_CDR2)
    Mutable = 2,
}

/// XCDR version for serialization
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Int2DdsXcdrVersion {
    /// XCDR v1 (CDR) - compatible with legacy DDS implementations
    /// Encapsulation IDs: 0x0000 (CDR_BE), 0x0001 (CDR_LE)
    #[default]
    Xcdr1 = 0,
    /// XCDR v2 - extended CDR with better extensibility support
    /// Encapsulation IDs: 0x0006 (CDR2_BE), 0x0007 (CDR2_LE), etc.
    Xcdr2 = 1,
}

impl From<Int2DdsExtensibilityKind> for ExtensibilityKind {
    fn from(kind: Int2DdsExtensibilityKind) -> Self {
        match kind {
            Int2DdsExtensibilityKind::Final => ExtensibilityKind::Final,
            Int2DdsExtensibilityKind::Appendable => ExtensibilityKind::Appendable,
            Int2DdsExtensibilityKind::Mutable => ExtensibilityKind::Mutable,
        }
    }
}

impl From<ExtensibilityKind> for Int2DdsExtensibilityKind {
    fn from(kind: ExtensibilityKind) -> Self {
        match kind {
            ExtensibilityKind::Final => Int2DdsExtensibilityKind::Final,
            ExtensibilityKind::Appendable => Int2DdsExtensibilityKind::Appendable,
            ExtensibilityKind::Mutable => Int2DdsExtensibilityKind::Mutable,
        }
    }
}

/// Internal field type representation with full type information
#[derive(Debug, Clone)]
pub enum FieldTypeInfo {
    Bool,
    Int8,
    UInt8,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float32,
    Float64,
    String {
        max_length: u32,
    },
    Sequence {
        element_type: Box<FieldTypeInfo>,
        max_length: u32,
    },
    Array {
        element_type: Box<FieldTypeInfo>,
        length: u32,
    },
    Struct {
        descriptor: Arc<Int2DdsTypeDescriptor>,
    },
    /// Optimized byte sequence - stored as Arc<[u8]> for zero-copy
    /// Serialized as CDR octet sequence for wire compatibility
    Bytes {
        max_length: u32,
    },
}

impl FieldTypeInfo {
    /// Get the base field type for C API
    pub fn base_type(&self) -> Int2DdsFieldType {
        match self {
            FieldTypeInfo::Bool => Int2DdsFieldType::Bool,
            FieldTypeInfo::Int8 => Int2DdsFieldType::Int8,
            FieldTypeInfo::UInt8 => Int2DdsFieldType::UInt8,
            FieldTypeInfo::Int16 => Int2DdsFieldType::Int16,
            FieldTypeInfo::UInt16 => Int2DdsFieldType::UInt16,
            FieldTypeInfo::Int32 => Int2DdsFieldType::Int32,
            FieldTypeInfo::UInt32 => Int2DdsFieldType::UInt32,
            FieldTypeInfo::Int64 => Int2DdsFieldType::Int64,
            FieldTypeInfo::UInt64 => Int2DdsFieldType::UInt64,
            FieldTypeInfo::Float32 => Int2DdsFieldType::Float32,
            FieldTypeInfo::Float64 => Int2DdsFieldType::Float64,
            FieldTypeInfo::String { .. } => Int2DdsFieldType::String,
            FieldTypeInfo::Sequence { .. } => Int2DdsFieldType::Sequence,
            FieldTypeInfo::Array { .. } => Int2DdsFieldType::Array,
            FieldTypeInfo::Struct { .. } => Int2DdsFieldType::Struct,
            FieldTypeInfo::Bytes { .. } => Int2DdsFieldType::Bytes,
        }
    }
}

/// Field descriptor - describes a single field in a type
#[derive(Debug, Clone)]
pub struct FieldDescriptor {
    /// Field name
    pub name: String,
    /// Field type with full type information
    pub field_type: FieldTypeInfo,
    /// Whether this field is a key field
    pub is_key: bool,
    /// Member ID for XCDR2 mutable types (auto-assigned if not set)
    pub member_id: u32,
    /// Whether this field is optional (for Mutable types)
    pub is_optional: bool,
}

/// Type descriptor - describes a complete DDS type
#[derive(Debug)]
pub struct Int2DdsTypeDescriptor {
    /// Type name (e.g., "HelloWorld")
    pub type_name: String,
    /// List of fields in order
    pub fields: Vec<FieldDescriptor>,
    /// Field name to index lookup
    pub field_indices: HashMap<String, usize>,
    /// Extensibility kind for XCDR encoding
    pub extensibility: ExtensibilityKind,
    /// Next member ID for auto-assignment
    pub next_member_id: u32,
    /// XCDR version for serialization (default: XCDR1 for compatibility)
    pub xcdr_version: Int2DdsXcdrVersion,
}

impl Int2DdsTypeDescriptor {
    /// Create a new type descriptor
    pub fn new(type_name: &str) -> Self {
        Self {
            type_name: type_name.to_string(),
            fields: Vec::new(),
            field_indices: HashMap::new(),
            extensibility: ExtensibilityKind::Final,
            next_member_id: 0,
            xcdr_version: Int2DdsXcdrVersion::Xcdr1, // Default to XCDR1 for compatibility
        }
    }

    /// Add a field to the type
    pub fn add_field(&mut self, name: &str, field_type: FieldTypeInfo, is_key: bool) {
        self.add_field_with_options(name, field_type, is_key, false);
    }

    /// Add an optional field to the type (for Mutable types)
    pub fn add_optional_field(&mut self, name: &str, field_type: FieldTypeInfo, is_key: bool) {
        self.add_field_with_options(name, field_type, is_key, true);
    }

    /// Add a field with all options
    pub fn add_field_with_options(
        &mut self,
        name: &str,
        field_type: FieldTypeInfo,
        is_key: bool,
        is_optional: bool,
    ) {
        let member_id = self.next_member_id;
        self.next_member_id += 1;

        let index = self.fields.len();
        self.field_indices.insert(name.to_string(), index);
        self.fields.push(FieldDescriptor {
            name: name.to_string(),
            field_type,
            is_key,
            member_id,
            is_optional,
        });
    }

    /// Get field by name
    pub fn get_field(&self, name: &str) -> Option<&FieldDescriptor> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get field index by name
    pub fn get_field_index(&self, name: &str) -> Option<usize> {
        self.field_indices.get(name).copied()
    }

    /// Get key fields
    pub fn key_fields(&self) -> Vec<&FieldDescriptor> {
        self.fields.iter().filter(|f| f.is_key).collect()
    }

    /// Check if type has key fields
    pub fn has_key(&self) -> bool {
        self.fields.iter().any(|f| f.is_key)
    }
}

// Safety implementations for FFI
unsafe impl Send for Int2DdsTypeDescriptor {}
unsafe impl Sync for Int2DdsTypeDescriptor {}

// =============================================================================
// FFI Functions
// =============================================================================

/// Create a new type descriptor
///
/// # Safety
/// - `type_name` must be a valid null-terminated C string
/// - `out` must be a valid pointer to a null pointer
/// - The returned descriptor must be freed with `int2dds_type_descriptor_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_create(
    type_name: *const std::ffi::c_char,
    out: *mut *mut Int2DdsTypeDescriptor,
) -> Int2DdsRet {
    check_null!(type_name);
    check_null!(out);

    let type_name = match std::ffi::CStr::from_ptr(type_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let descriptor = Box::new(Int2DdsTypeDescriptor::new(type_name));
    *out = Box::into_raw(descriptor);

    INT2DDS_RET_OK
}

/// Delete a type descriptor
///
/// # Safety
/// - `desc` must be a valid type descriptor or null
/// - `desc` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_delete(
    desc: *mut Int2DdsTypeDescriptor,
) -> Int2DdsRet {
    if desc.is_null() {
        return INT2DDS_RET_OK;
    }

    drop(Box::from_raw(desc));
    INT2DDS_RET_OK
}

/// Set extensibility kind for the type
///
/// # Safety
/// - `desc` must be a valid type descriptor
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_set_extensibility(
    desc: *mut Int2DdsTypeDescriptor,
    kind: Int2DdsExtensibilityKind,
) -> Int2DdsRet {
    check_null!(desc);

    let desc = &mut *desc;
    desc.extensibility = kind.into();

    INT2DDS_RET_OK
}

/// Set XCDR version for serialization
///
/// Controls the CDR encoding version used for serialization:
/// - `Xcdr1` (0): XCDR v1 (CDR) - compatible with legacy DDS implementations
/// - `Xcdr2` (1): XCDR v2 - extended CDR with better extensibility support
///
/// Default is XCDR v1 for maximum compatibility.
///
/// # Safety
/// - `desc` must be a valid type descriptor
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_set_xcdr_version(
    desc: *mut Int2DdsTypeDescriptor,
    version: Int2DdsXcdrVersion,
) -> Int2DdsRet {
    check_null!(desc);

    let desc = &mut *desc;
    desc.xcdr_version = version;

    INT2DDS_RET_OK
}

/// Get XCDR version for serialization
///
/// # Safety
/// - `desc` must be a valid type descriptor
/// - `version` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_get_xcdr_version(
    desc: *const Int2DdsTypeDescriptor,
    version: *mut Int2DdsXcdrVersion,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(version);

    let desc = &*desc;
    *version = desc.xcdr_version;

    INT2DDS_RET_OK
}

/// Get the type name
///
/// # Safety
/// - `desc` must be a valid type descriptor
/// - `buf` must point to a buffer of at least `buf_size` bytes
/// - `out_len` will be set to the actual length of the type name
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_get_name(
    desc: *const Int2DdsTypeDescriptor,
    buf: *mut std::ffi::c_char,
    buf_size: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(buf);
    check_null!(out_len);

    let desc = &*desc;
    let name_bytes = desc.type_name.as_bytes();
    *out_len = name_bytes.len();

    if name_bytes.len() + 1 > buf_size {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), buf as *mut u8, name_bytes.len());
    *buf.add(name_bytes.len()) = 0; // null terminator

    INT2DDS_RET_OK
}

/// Get number of fields
///
/// # Safety
/// - `desc` must be a valid type descriptor
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_get_field_count(
    desc: *const Int2DdsTypeDescriptor,
    count: *mut usize,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(count);

    let desc = &*desc;
    *count = desc.fields.len();

    INT2DDS_RET_OK
}

// -----------------------------------------------------------------------------
// Field addition functions
// -----------------------------------------------------------------------------

/// Add a bool field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_bool(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::Bool, is_key);

    INT2DDS_RET_OK
}

/// Add an i8 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_i8(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::Int8, is_key);

    INT2DDS_RET_OK
}

/// Add a u8 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_u8(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::UInt8, is_key);

    INT2DDS_RET_OK
}

/// Add an i16 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_i16(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::Int16, is_key);

    INT2DDS_RET_OK
}

/// Add a u16 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_u16(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::UInt16, is_key);

    INT2DDS_RET_OK
}

/// Add an i32 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_i32(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::Int32, is_key);

    INT2DDS_RET_OK
}

/// Add a u32 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_u32(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::UInt32, is_key);

    INT2DDS_RET_OK
}

/// Add an i64 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_i64(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::Int64, is_key);

    INT2DDS_RET_OK
}

/// Add a u64 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_u64(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::UInt64, is_key);

    INT2DDS_RET_OK
}

/// Add an f32 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_f32(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::Float32, is_key);

    INT2DDS_RET_OK
}

/// Add an f64 field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_f64(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::Float64, is_key);

    INT2DDS_RET_OK
}

/// Add a string field with max length
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_string(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    max_length: u32,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::String { max_length }, is_key);

    INT2DDS_RET_OK
}

/// Add a sequence field with element type and max length
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_sequence(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    element_type: Int2DdsFieldType,
    max_length: u32,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let element_type_info = match element_type {
        Int2DdsFieldType::Bool => FieldTypeInfo::Bool,
        Int2DdsFieldType::Int8 => FieldTypeInfo::Int8,
        Int2DdsFieldType::UInt8 => FieldTypeInfo::UInt8,
        Int2DdsFieldType::Int16 => FieldTypeInfo::Int16,
        Int2DdsFieldType::UInt16 => FieldTypeInfo::UInt16,
        Int2DdsFieldType::Int32 => FieldTypeInfo::Int32,
        Int2DdsFieldType::UInt32 => FieldTypeInfo::UInt32,
        Int2DdsFieldType::Int64 => FieldTypeInfo::Int64,
        Int2DdsFieldType::UInt64 => FieldTypeInfo::UInt64,
        Int2DdsFieldType::Float32 => FieldTypeInfo::Float32,
        Int2DdsFieldType::Float64 => FieldTypeInfo::Float64,
        Int2DdsFieldType::String => FieldTypeInfo::String { max_length: 256 },
        _ => return INT2DDS_RET_INVALID_ARGUMENT, // Nested sequences/arrays not supported
    };

    let desc = &mut *desc;
    desc.add_field(
        name,
        FieldTypeInfo::Sequence { element_type: Box::new(element_type_info), max_length },
        is_key,
    );

    INT2DDS_RET_OK
}

/// Add an array field with element type and fixed length
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_array(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    element_type: Int2DdsFieldType,
    length: u32,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let element_type_info = match element_type {
        Int2DdsFieldType::Bool => FieldTypeInfo::Bool,
        Int2DdsFieldType::Int8 => FieldTypeInfo::Int8,
        Int2DdsFieldType::UInt8 => FieldTypeInfo::UInt8,
        Int2DdsFieldType::Int16 => FieldTypeInfo::Int16,
        Int2DdsFieldType::UInt16 => FieldTypeInfo::UInt16,
        Int2DdsFieldType::Int32 => FieldTypeInfo::Int32,
        Int2DdsFieldType::UInt32 => FieldTypeInfo::UInt32,
        Int2DdsFieldType::Int64 => FieldTypeInfo::Int64,
        Int2DdsFieldType::UInt64 => FieldTypeInfo::UInt64,
        Int2DdsFieldType::Float32 => FieldTypeInfo::Float32,
        Int2DdsFieldType::Float64 => FieldTypeInfo::Float64,
        Int2DdsFieldType::String => FieldTypeInfo::String { max_length: 256 },
        _ => return INT2DDS_RET_INVALID_ARGUMENT, // Nested arrays not supported
    };

    let desc = &mut *desc;
    desc.add_field(
        name,
        FieldTypeInfo::Array { element_type: Box::new(element_type_info), length },
        is_key,
    );

    INT2DDS_RET_OK
}

/// Add a nested struct field
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_struct(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    nested_desc: *const Int2DdsTypeDescriptor,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);
    check_null!(nested_desc);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    // Clone the nested descriptor
    let nested = &*nested_desc;
    let nested_clone = Int2DdsTypeDescriptor {
        type_name: nested.type_name.clone(),
        fields: nested.fields.clone(),
        field_indices: nested.field_indices.clone(),
        extensibility: nested.extensibility,
        next_member_id: nested.next_member_id,
        xcdr_version: nested.xcdr_version,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::Struct { descriptor: Arc::new(nested_clone) }, is_key);

    INT2DDS_RET_OK
}

/// Add a bytes field (optimized byte sequence for large payloads)
///
/// This creates a field that stores byte data efficiently using Arc<[u8]>,
/// avoiding the overhead of individual FieldValue::UInt8 allocations.
/// Ideal for payloads 1KB-1MB where performance is critical.
///
/// # Safety
/// - `desc` must be a valid type descriptor
/// - `name` must be a valid null-terminated C string
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_descriptor_add_bytes(
    desc: *mut Int2DdsTypeDescriptor,
    name: *const std::ffi::c_char,
    max_length: u32,
    is_key: bool,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(name);

    let name = match std::ffi::CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let desc = &mut *desc;
    desc.add_field(name, FieldTypeInfo::Bytes { max_length }, is_key);

    INT2DDS_RET_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_descriptor_creation() {
        let mut desc = Int2DdsTypeDescriptor::new("TestType");
        assert_eq!(desc.type_name, "TestType");
        assert!(desc.fields.is_empty());

        desc.add_field("id", FieldTypeInfo::UInt32, true);
        desc.add_field("message", FieldTypeInfo::String { max_length: 256 }, false);

        assert_eq!(desc.fields.len(), 2);
        assert!(desc.has_key());

        let key_fields = desc.key_fields();
        assert_eq!(key_fields.len(), 1);
        assert_eq!(key_fields[0].name, "id");
    }

    #[test]
    fn test_field_lookup() {
        let mut desc = Int2DdsTypeDescriptor::new("TestType");
        desc.add_field("id", FieldTypeInfo::UInt32, true);
        desc.add_field("value", FieldTypeInfo::Float64, false);

        let field = desc.get_field("id");
        assert!(field.is_some());
        assert_eq!(field.unwrap().member_id, 0);

        let field = desc.get_field("value");
        assert!(field.is_some());
        assert_eq!(field.unwrap().member_id, 1);

        assert!(desc.get_field("nonexistent").is_none());
    }
}
