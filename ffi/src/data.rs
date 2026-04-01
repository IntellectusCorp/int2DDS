//! # FFI Data Type
//!
//! Minimal type placeholder for DDS generic parameters.
//!
//! `Int2DdsData` is used as the generic type parameter for `DataWriter<Int2DdsData>`
//! and `DataReader<Int2DdsData>` in the FFI layer. Actual serialization/deserialization
//! is handled by the registered `RawTypeSupport`, not by this type.

use std::any::{Any, TypeId};
use std::sync::Arc;

use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::core::error::{DdsError, DdsResult},
    rtps::common::types::SerializedData,
    serialize::cdr::ExtensibilityKind,
    topic::{
        sql::ast::Parameter,
        type_support::{DdsType, FieldAccessor, SerializationFormat, TypeSupport},
    },
};

/// CDR field type identifier for field parsing
#[derive(Debug, Clone)]
pub enum CdrFieldType {
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

/// Descriptor for a single field in a CDR-serialized struct
#[derive(Debug, Clone)]
pub struct CdrFieldDescriptor {
    pub name: std::string::String,
    pub field_type: CdrFieldType,
    pub is_key: bool,
}

/// Minimal data type for FFI generic parameters.
///
/// This type exists solely to satisfy the `DdsType` trait bound on
/// `DataWriter<T>` and `DataReader<T>`. All actual data flows through
/// raw serialized bytes via `int2dds_write_serialized` / `int2dds_take_serialized`.
#[derive(Debug, Clone)]
pub struct Int2DdsData {
    /// Raw CDR bytes stored during deserialize() for compute_key() fallback
    pub(crate) cdr_bytes: Option<Vec<u8>>,
    /// Field descriptors for get_field_value() / has_field() support
    pub(crate) field_descriptors: Option<Arc<Vec<CdrFieldDescriptor>>>,
    /// Extensibility kind for DHEADER skipping
    pub(crate) extensibility: ExtensibilityKind,
}

impl Default for Int2DdsData {
    fn default() -> Self {
        Self { cdr_bytes: None, field_descriptors: None, extensibility: ExtensibilityKind::Final }
    }
}

unsafe impl Send for Int2DdsData {}
unsafe impl Sync for Int2DdsData {}

/// Placeholder TypeSupport for Int2DdsData.
///
/// Returns errors for all operations. The actual TypeSupport used at runtime
/// is `RawTypeSupport`, which is registered with the DomainParticipant
/// via `int2dds_create_topic_raw`.
#[derive(Debug, Clone, Default)]
pub struct Int2DdsDataTypeSupport;

impl FieldAccessor for Int2DdsDataTypeSupport {
    fn get_field_value(&self, _data: &dyn Any, _field_path: &str) -> DdsResult<Parameter> {
        Err(DdsError::Error("Int2DdsDataTypeSupport: Use registered TypeSupport".to_string()))
    }

    fn has_field(&self, _field_path: &str) -> bool {
        false
    }
}

impl TypeSupport for Int2DdsDataTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<Int2DdsData>()
    }

    fn get_type_name(&self) -> &str {
        "Int2DdsData"
    }

    fn serialize(
        &self,
        _data: &dyn Any,
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData> {
        Err(DdsError::Error("Int2DdsDataTypeSupport: Use registered TypeSupport".to_string()))
    }

    fn deserialize(
        &self,
        _data: &[u8],
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        Err(DdsError::Error("Int2DdsDataTypeSupport: Use registered TypeSupport".to_string()))
    }

    fn serialize_key(&self, _data: &dyn Any) -> DdsResult<SerializedData> {
        Ok(Arc::from(Vec::new()))
    }

    fn deserialize_key(&self, _serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        Err(DdsError::Error("Int2DdsDataTypeSupport: Use registered TypeSupport".to_string()))
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
    type TypeSupport = Int2DdsDataTypeSupport;
    type FieldAccessor = Int2DdsDataTypeSupport;

    fn has_field(&self, field_path: &str) -> DdsResult<bool> {
        if let Some(fields) = &self.field_descriptors {
            Ok(fields.iter().any(|f| f.name == field_path))
        } else {
            Ok(false)
        }
    }

    fn get_field_value(&self, field_path: &str) -> DdsResult<Parameter> {
        let cdr_bytes = self
            .cdr_bytes
            .as_ref()
            .ok_or_else(|| DdsError::Error("No CDR bytes available".to_string()))?;
        let fields = self
            .field_descriptors
            .as_ref()
            .ok_or_else(|| DdsError::Error("No field descriptors available".to_string()))?;

        cdr_parse_field_value(cdr_bytes, fields, &self.extensibility, field_path)
    }
}

// ============================================================================
// CDR Field Parsing for get_field_value() support
// ============================================================================

/// Align position to N-byte boundary
fn cdr_align(pos: usize, alignment: usize) -> usize {
    (pos + alignment - 1) & !(alignment - 1)
}

/// Skip a CDR field and return the new position
fn cdr_skip_field(data: &[u8], pos: usize, field_type: &CdrFieldType) -> Option<usize> {
    match field_type {
        CdrFieldType::String => {
            let aligned = cdr_align(pos, 4);
            if aligned + 4 > data.len() {
                return None;
            }
            let str_len = u32::from_le_bytes([
                data[aligned],
                data[aligned + 1],
                data[aligned + 2],
                data[aligned + 3],
            ]) as usize;
            let end = aligned + 4 + str_len;
            if end > data.len() {
                return None;
            }
            Some(end)
        }
        CdrFieldType::Int8 | CdrFieldType::UInt8 | CdrFieldType::Bool => {
            if pos >= data.len() {
                return None;
            }
            Some(pos + 1)
        }
        CdrFieldType::Int16 | CdrFieldType::UInt16 => {
            let aligned = cdr_align(pos, 2);
            if aligned + 2 > data.len() {
                return None;
            }
            Some(aligned + 2)
        }
        CdrFieldType::Int32 | CdrFieldType::UInt32 => {
            let aligned = cdr_align(pos, 4);
            if aligned + 4 > data.len() {
                return None;
            }
            Some(aligned + 4)
        }
        CdrFieldType::Int64 | CdrFieldType::UInt64 => {
            let aligned = cdr_align(pos, 8);
            if aligned + 8 > data.len() {
                return None;
            }
            Some(aligned + 8)
        }
    }
}

/// Read a CDR field value at the given position and return it as a Parameter
fn cdr_read_field(
    data: &[u8],
    pos: usize,
    field_type: &CdrFieldType,
) -> Option<(Parameter, usize)> {
    match field_type {
        CdrFieldType::String => {
            let aligned = cdr_align(pos, 4);
            if aligned + 4 > data.len() {
                return None;
            }
            let str_len = u32::from_le_bytes([
                data[aligned],
                data[aligned + 1],
                data[aligned + 2],
                data[aligned + 3],
            ]) as usize;
            let str_start = aligned + 4;
            if str_start + str_len > data.len() {
                return None;
            }
            let actual_len = if str_len > 0 && data[str_start + str_len - 1] == 0 {
                str_len - 1
            } else {
                str_len
            };
            let s = std::str::from_utf8(&data[str_start..str_start + actual_len]).ok()?.to_string();
            Some((Parameter::String(s), str_start + str_len))
        }
        CdrFieldType::Int32 => {
            let aligned = cdr_align(pos, 4);
            if aligned + 4 > data.len() {
                return None;
            }
            let val = i32::from_le_bytes([
                data[aligned],
                data[aligned + 1],
                data[aligned + 2],
                data[aligned + 3],
            ]);
            Some((Parameter::IntegerValue(val as i32), aligned + 4))
        }
        CdrFieldType::UInt32 => {
            let aligned = cdr_align(pos, 4);
            if aligned + 4 > data.len() {
                return None;
            }
            let val = u32::from_le_bytes([
                data[aligned],
                data[aligned + 1],
                data[aligned + 2],
                data[aligned + 3],
            ]);
            Some((Parameter::IntegerValue(val as i32), aligned + 4))
        }
        CdrFieldType::Int16 => {
            let aligned = cdr_align(pos, 2);
            if aligned + 2 > data.len() {
                return None;
            }
            let val = i16::from_le_bytes([data[aligned], data[aligned + 1]]);
            Some((Parameter::IntegerValue(val as i32), aligned + 2))
        }
        CdrFieldType::UInt16 => {
            let aligned = cdr_align(pos, 2);
            if aligned + 2 > data.len() {
                return None;
            }
            let val = u16::from_le_bytes([data[aligned], data[aligned + 1]]);
            Some((Parameter::IntegerValue(val as i32), aligned + 2))
        }
        CdrFieldType::Int64 => {
            let aligned = cdr_align(pos, 8);
            if aligned + 8 > data.len() {
                return None;
            }
            let val = i64::from_le_bytes([
                data[aligned],
                data[aligned + 1],
                data[aligned + 2],
                data[aligned + 3],
                data[aligned + 4],
                data[aligned + 5],
                data[aligned + 6],
                data[aligned + 7],
            ]);
            Some((Parameter::IntegerValue(val as i32), aligned + 8))
        }
        CdrFieldType::UInt64 => {
            let aligned = cdr_align(pos, 8);
            if aligned + 8 > data.len() {
                return None;
            }
            let val = u64::from_le_bytes([
                data[aligned],
                data[aligned + 1],
                data[aligned + 2],
                data[aligned + 3],
                data[aligned + 4],
                data[aligned + 5],
                data[aligned + 6],
                data[aligned + 7],
            ]);
            Some((Parameter::IntegerValue(val as i32), aligned + 8))
        }
        CdrFieldType::Int8 => {
            if pos >= data.len() {
                return None;
            }
            Some((Parameter::IntegerValue(data[pos] as i8 as i32), pos + 1))
        }
        CdrFieldType::UInt8 => {
            if pos >= data.len() {
                return None;
            }
            Some((Parameter::IntegerValue(data[pos] as i32), pos + 1))
        }
        CdrFieldType::Bool => {
            if pos >= data.len() {
                return None;
            }
            Some((Parameter::IntegerValue(data[pos] as i32), pos + 1))
        }
    }
}

/// Parse a field value from CDR bytes by field name
fn cdr_parse_field_value(
    cdr_bytes: &[u8],
    fields: &[CdrFieldDescriptor],
    extensibility: &ExtensibilityKind,
    field_name: &str,
) -> DdsResult<Parameter> {
    if cdr_bytes.len() < 4 {
        return Err(DdsError::Error("CDR data too short".to_string()));
    }

    // Skip encapsulation header (4 bytes)
    let encoding_id = u16::from_be_bytes([cdr_bytes[0], cdr_bytes[1]]);
    let is_xcdr2 = matches!(encoding_id, 0x0006 | 0x0007 | 0x0008 | 0x0009 | 0x000A | 0x000B);
    let mut pos = 4;

    // Skip DHEADER for Appendable/Mutable XCDR2
    if is_xcdr2
        && matches!(extensibility, ExtensibilityKind::Appendable | ExtensibilityKind::Mutable)
    {
        if pos + 4 <= cdr_bytes.len() {
            pos += 4;
        }
    }

    // Parse fields sequentially until we find the target
    for field in fields {
        if field.name == field_name {
            return cdr_read_field(cdr_bytes, pos, &field.field_type)
                .map(|(param, _)| param)
                .ok_or_else(|| {
                    DdsError::Error(format!("Failed to parse field '{}' from CDR data", field_name))
                });
        }
        // Not the target - skip this field
        pos = cdr_skip_field(cdr_bytes, pos, &field.field_type).ok_or_else(|| {
            DdsError::Error(format!("Failed to skip field '{}' in CDR data", field.name))
        })?;
    }

    Err(DdsError::Error(format!("Field '{}' not found", field_name)))
}
