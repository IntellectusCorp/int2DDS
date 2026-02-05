#![allow(clippy::missing_safety_doc)]

//! # Dynamic Data API
//!
//! Runtime data container for C/FFI interoperability.
//!
//! ## Overview
//!
//! Int2DdsData is a dynamic data container that holds field values according to
//! a TypeDescriptor. It enables setting and getting field values by name.
//!
//! ## Example (C)
//!
//! ```c
//! Int2DdsData* data;
//! int2dds_data_create(desc, &data);
//! int2dds_data_set_u32(data, "id", 123);
//! int2dds_data_set_string(data, "message", "Hello!");
//! ```

use std::any::{Any, TypeId};
use std::sync::Arc;

use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::core::error::{DdsError, DdsResult},
    rtps::common::types::SerializedData,
    serialize::cdr::ExtensibilityKind,
    topic::{
        sql::ast::Parameter,
        type_support::{DdsType, SerializationFormat, TypeSupport},
    },
};

use crate::error::*;
use crate::type_descriptor::{FieldTypeInfo, Int2DdsTypeDescriptor};

/// Field value enum - stores actual data values
#[derive(Debug, Clone)]
pub enum FieldValue {
    Bool(bool),
    Int8(i8),
    UInt8(u8),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Float32(f32),
    Float64(f64),
    String(String),
    Bytes(Arc<[u8]>),
    Sequence(Vec<FieldValue>),
    Array(Vec<FieldValue>),
    Struct(Box<Int2DdsData>),
}

impl FieldValue {
    /// Check if this value matches the expected field type
    pub fn matches_type(&self, field_type: &FieldTypeInfo) -> bool {
        match (self, field_type) {
            (FieldValue::Bool(_), FieldTypeInfo::Bool) => true,
            (FieldValue::Int8(_), FieldTypeInfo::Int8) => true,
            (FieldValue::UInt8(_), FieldTypeInfo::UInt8) => true,
            (FieldValue::Int16(_), FieldTypeInfo::Int16) => true,
            (FieldValue::UInt16(_), FieldTypeInfo::UInt16) => true,
            (FieldValue::Int32(_), FieldTypeInfo::Int32) => true,
            (FieldValue::UInt32(_), FieldTypeInfo::UInt32) => true,
            (FieldValue::Int64(_), FieldTypeInfo::Int64) => true,
            (FieldValue::UInt64(_), FieldTypeInfo::UInt64) => true,
            (FieldValue::Float32(_), FieldTypeInfo::Float32) => true,
            (FieldValue::Float64(_), FieldTypeInfo::Float64) => true,
            (FieldValue::String(_), FieldTypeInfo::String { .. }) => true,
            (FieldValue::Bytes(_), FieldTypeInfo::Bytes { .. }) => true,
            (FieldValue::Bytes(_), FieldTypeInfo::Sequence { element_type, .. })
                if matches!(element_type.as_ref(), FieldTypeInfo::UInt8) =>
            {
                true
            }
            (FieldValue::Sequence(_), FieldTypeInfo::Sequence { .. }) => true,
            (FieldValue::Array(_), FieldTypeInfo::Array { .. }) => true,
            (FieldValue::Struct(_), FieldTypeInfo::Struct { .. }) => true,
            _ => false,
        }
    }
}

/// Dynamic data container - holds field values by index
#[derive(Debug, Clone)]
pub struct Int2DdsData {
    /// Reference to the type descriptor
    pub descriptor: Arc<Int2DdsTypeDescriptor>,
    /// Field values by index (aligned with descriptor.fields)
    pub values: Vec<Option<FieldValue>>,
}

impl Int2DdsData {
    /// Create new data instance from type descriptor
    pub fn new(descriptor: Arc<Int2DdsTypeDescriptor>) -> Self {
        let values = vec![None; descriptor.fields.len()];
        Self { descriptor, values }
    }

    /// Clear all values
    pub fn clear(&mut self) {
        for value in &mut self.values {
            *value = None;
        }
    }

    /// Set a field value with type checking
    pub fn set_value(&mut self, field_name: &str, value: FieldValue) -> Result<(), &'static str> {
        let index = self
            .descriptor
            .get_field_index(field_name)
            .ok_or("Field not found in type descriptor")?;
        let field = &self.descriptor.fields[index];

        if !value.matches_type(&field.field_type) {
            return Err("Value type does not match field type");
        }

        if index >= self.values.len() {
            return Err("Field index out of bounds");
        }
        self.values[index] = Some(value);
        Ok(())
    }

    /// Get a field value
    pub fn get_value(&self, field_name: &str) -> Option<&FieldValue> {
        let index = self.descriptor.get_field_index(field_name)?;
        self.get_value_by_index(index)
    }

    /// Get a field value by index
    pub fn get_value_by_index(&self, index: usize) -> Option<&FieldValue> {
        self.values.get(index).and_then(|value| value.as_ref())
    }

    /// Check if a field has a value set
    pub fn has_value(&self, field_name: &str) -> bool {
        self.get_value(field_name).is_some()
    }

    /// Get type name
    pub fn type_name(&self) -> &str {
        &self.descriptor.type_name
    }
}

// Safety implementations for FFI
unsafe impl Send for Int2DdsData {}
unsafe impl Sync for Int2DdsData {}

// =============================================================================
// DdsType Implementation
// =============================================================================

/// Default TypeSupport for Int2DdsData.
///
/// This is a placeholder that returns errors for all operations.
/// The actual serialization/deserialization is handled by DynamicTypeSupport
/// which is registered with the DomainParticipant and passed to DataSample.
#[derive(Debug, Clone, Default)]
pub struct DynamicTypeSupportDefault;

impl TypeSupport for DynamicTypeSupportDefault {
    fn type_id(&self) -> TypeId {
        TypeId::of::<Int2DdsData>()
    }

    fn get_type_name(&self) -> &str {
        "Int2DdsData"
    }

    fn get_field_value(&self, _data: &dyn Any, _field_path: &str) -> DdsResult<Parameter> {
        Err(DdsError::Error(
            "DynamicTypeSupportDefault: Use registered TypeSupport for field access".to_string(),
        ))
    }

    fn has_field(&self, _field_path: &str) -> bool {
        false
    }

    fn serialize(
        &self,
        _data: &dyn Any,
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData> {
        Err(DdsError::Error(
            "DynamicTypeSupportDefault: Use registered TypeSupport for serialization".to_string(),
        ))
    }

    fn deserialize(
        &self,
        _data: &[u8],
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        Err(DdsError::Error(
            "DynamicTypeSupportDefault: Use registered TypeSupport for deserialization".to_string(),
        ))
    }

    fn serialize_key(&self, _data: &dyn Any) -> DdsResult<SerializedData> {
        Ok(Arc::from(Vec::new()))
    }

    fn deserialize_key(&self, _serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        Err(DdsError::Error(
            "DynamicTypeSupportDefault: Use registered TypeSupport for key deserialization"
                .to_string(),
        ))
    }

    fn compute_key(&self, _data: &dyn Any) -> InstanceHandle {
        InstanceHandle::NIL
    }

    fn is_compute_key_provided(&self) -> bool {
        false
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        ExtensibilityKind::Final
    }
}

impl DdsType for Int2DdsData {
    type TypeSupport = DynamicTypeSupportDefault;
}

// =============================================================================
// FFI Functions
// =============================================================================

/// Create a new data instance from type descriptor
///
/// # Safety
/// - `desc` must be a valid type descriptor
/// - `out` must be a valid pointer to a null pointer
/// - The returned data must be freed with `int2dds_data_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_create(
    desc: *const Int2DdsTypeDescriptor,
    out: *mut *mut Int2DdsData,
) -> Int2DdsRet {
    check_null!(desc);
    check_null!(out);

    // Clone the descriptor into an Arc
    let desc_ref = &*desc;
    let descriptor = Arc::new(Int2DdsTypeDescriptor {
        type_name: desc_ref.type_name.clone(),
        fields: desc_ref.fields.clone(),
        field_indices: desc_ref.field_indices.clone(),
        extensibility: desc_ref.extensibility,
        next_member_id: desc_ref.fields.len() as u32,
        xcdr_version: desc_ref.xcdr_version,
    });

    let data = Box::new(Int2DdsData::new(descriptor));
    *out = Box::into_raw(data);

    INT2DDS_RET_OK
}

/// Delete a data instance
///
/// # Safety
/// - `data` must be a valid data instance or null
/// - `data` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_delete(data: *mut Int2DdsData) -> Int2DdsRet {
    if data.is_null() {
        return INT2DDS_RET_OK;
    }

    drop(Box::from_raw(data));
    INT2DDS_RET_OK
}

/// Clear all values in the data instance
///
/// # Safety
/// - `data` must be a valid data instance
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_clear(data: *mut Int2DdsData) -> Int2DdsRet {
    check_null!(data);

    let data = &mut *data;
    data.clear();

    INT2DDS_RET_OK
}

// -----------------------------------------------------------------------------
// Value setters
// -----------------------------------------------------------------------------

/// Set a bool field value
#[allow(clippy::missing_safety_doc)]
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_bool(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: bool,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::Bool(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set an i8 field value
#[allow(clippy::missing_safety_doc)]
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_i8(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: i8,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::Int8(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set a u8 field value
#[allow(clippy::missing_safety_doc)]
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_u8(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: u8,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::UInt8(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set an i16 field value
#[allow(clippy::missing_safety_doc)]
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_i16(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: i16,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::Int16(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set a u16 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_u16(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: u16,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::UInt16(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set an i32 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_i32(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: i32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::Int32(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set a u32 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_u32(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: u32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::UInt32(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set an i64 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_i64(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: i64,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::Int64(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set a u64 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_u64(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: u64,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::UInt64(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set an f32 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_f32(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: f32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::Float32(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set an f64 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_f64(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: f64,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::Float64(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Set a string field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_string(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    value: *const std::ffi::c_char,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(value);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let value_str = match std::ffi::CStr::from_ptr(value).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::String(value_str.to_string())) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

// -----------------------------------------------------------------------------
// Value getters
// -----------------------------------------------------------------------------

/// Get a bool field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_bool(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut bool,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::Bool(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR, // Wrong type
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get an i8 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_i8(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut i8,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::Int8(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get a u8 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_u8(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut u8,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::UInt8(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get an i16 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_i16(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut i16,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::Int16(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get a u16 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_u16(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut u16,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::UInt16(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get an i32 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_i32(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut i32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::Int32(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get a u32 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_u32(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut u32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::UInt32(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get an i64 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_i64(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut i64,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::Int64(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get a u64 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_u64(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut u64,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::UInt64(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get an f32 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_f32(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut f32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::Float32(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get an f64 field value
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_f64(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    out: *mut f64,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(out);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::Float64(v)) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR,
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Set a byte sequence field value (sequence of u8)
///
/// Uses optimized FieldValue::Bytes for zero-copy storage.
/// For 1MB data, this avoids creating 1 million individual FieldValue::UInt8 instances.
///
/// # Safety
/// - `data` must be a valid data instance
/// - `field` must be a valid null-terminated C string
/// - `bytes` must point to a buffer of at least `len` bytes
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_set_bytes(
    data: *mut Int2DdsData,
    field: *const std::ffi::c_char,
    bytes: *const u8,
    len: u32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    if len > 0 {
        check_null!(bytes);
    }

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    // Use optimized FieldValue::Bytes - single allocation instead of N allocations
    let byte_arc: Arc<[u8]> = if len > 0 {
        Arc::from(std::slice::from_raw_parts(bytes, len as usize))
    } else {
        Arc::from([].as_slice())
    };

    let data = &mut *data;
    match data.set_value(field_name, FieldValue::Bytes(byte_arc)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_ERROR,
    }
}

/// Get a byte sequence field value (sequence of u8)
///
/// Handles both optimized FieldValue::Bytes and legacy FieldValue::Sequence<UInt8>.
///
/// # Safety
/// - `data` must be a valid data instance
/// - `field` must be a valid null-terminated C string
/// - `buf` must point to a buffer of at least `buf_size` bytes
/// - `out_len` will be set to the actual length of the byte sequence
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_bytes(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    buf: *mut u8,
    buf_size: u32,
    out_len: *mut u32,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(buf);
    check_null!(out_len);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        // Optimized path: FieldValue::Bytes - direct memcpy
        Some(FieldValue::Bytes(bytes)) => {
            *out_len = bytes.len() as u32;

            if bytes.len() > buf_size as usize {
                return INT2DDS_RET_ERROR; // Buffer too small
            }

            // Direct copy - no per-element overhead
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
            INT2DDS_RET_OK
        }
        // Legacy path: FieldValue::Sequence<UInt8> - for backward compatibility
        Some(FieldValue::Sequence(values)) => {
            *out_len = values.len() as u32;

            if values.len() > buf_size as usize {
                return INT2DDS_RET_ERROR; // Buffer too small
            }

            // Extract u8 values from the sequence
            for (i, val) in values.iter().enumerate() {
                match val {
                    FieldValue::UInt8(b) => *buf.add(i) = *b,
                    _ => return INT2DDS_RET_ERROR, // Not a byte sequence
                }
            }

            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR, // Wrong type
        None => INT2DDS_RET_NO_DATA,
    }
}

/// Get a string field value
///
/// # Safety
/// - `buf` must point to a buffer of at least `buf_size` bytes
/// - `out_len` will be set to the actual length of the string
#[no_mangle]
pub unsafe extern "C" fn int2dds_data_get_string(
    data: *const Int2DdsData,
    field: *const std::ffi::c_char,
    buf: *mut std::ffi::c_char,
    buf_size: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(buf);
    check_null!(out_len);

    let field_name = match std::ffi::CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let data = &*data;
    match data.get_value(field_name) {
        Some(FieldValue::String(s)) => {
            let bytes = s.as_bytes();
            *out_len = bytes.len();

            if bytes.len() + 1 > buf_size {
                return INT2DDS_RET_ERROR; // Buffer too small
            }

            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, bytes.len());
            *buf.add(bytes.len()) = 0; // null terminator

            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_ERROR, // Wrong type
        None => INT2DDS_RET_NO_DATA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_descriptor::FieldTypeInfo;

    fn create_test_descriptor() -> Arc<Int2DdsTypeDescriptor> {
        let mut desc = Int2DdsTypeDescriptor::new("TestType");
        desc.add_field("id", FieldTypeInfo::UInt32, true);
        desc.add_field("message", FieldTypeInfo::String { max_length: 256 }, false);
        desc.add_field("value", FieldTypeInfo::Float64, false);
        Arc::new(desc)
    }

    #[test]
    fn test_data_creation() {
        let desc = create_test_descriptor();
        let data = Int2DdsData::new(desc);
        assert_eq!(data.type_name(), "TestType");
        assert!(!data.has_value("id"));
    }

    #[test]
    fn test_set_and_get_values() {
        let desc = create_test_descriptor();
        let mut data = Int2DdsData::new(desc);

        // Set values
        assert!(data.set_value("id", FieldValue::UInt32(123)).is_ok());
        assert!(data.set_value("message", FieldValue::String("hello".to_string())).is_ok());
        assert!(data.set_value("value", FieldValue::Float64(3.14)).is_ok());

        // Get values
        assert!(matches!(data.get_value("id"), Some(FieldValue::UInt32(123))));
        assert!(matches!(data.get_value("message"), Some(FieldValue::String(s)) if s == "hello"));
        assert!(
            matches!(data.get_value("value"), Some(FieldValue::Float64(v)) if (*v - 3.14).abs() < 0.001)
        );
    }

    #[test]
    fn test_type_mismatch() {
        let desc = create_test_descriptor();
        let mut data = Int2DdsData::new(desc);

        // Try to set wrong type
        assert!(data.set_value("id", FieldValue::String("wrong".to_string())).is_err());
    }

    #[test]
    fn test_nonexistent_field() {
        let desc = create_test_descriptor();
        let mut data = Int2DdsData::new(desc);

        assert!(data.set_value("nonexistent", FieldValue::UInt32(123)).is_err());
        assert!(data.get_value("nonexistent").is_none());
    }

    #[test]
    fn test_clear() {
        let desc = create_test_descriptor();
        let mut data = Int2DdsData::new(desc);

        data.set_value("id", FieldValue::UInt32(123)).unwrap();
        assert!(data.has_value("id"));

        data.clear();
        assert!(!data.has_value("id"));
    }
}
