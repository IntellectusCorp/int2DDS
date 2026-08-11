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
/// raw serialized bytes via `int2dds_datawriter_write_serialized` /
/// `int2dds_datareader_take_serialized`.
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
        ExtensibilityKind::Appendable
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
pub(crate) fn cdr_align(pos: usize, alignment: usize) -> usize {
    (pos + alignment - 1) & !(alignment - 1)
}

/// Align a stream position whose CDR alignment origin is `base` (the first byte
/// after the 4-byte encapsulation header), per the RTPS/CDR alignment rule.
pub(crate) fn align_body(pos: usize, base: usize, alignment: usize) -> usize {
    base + cdr_align(pos - base, alignment)
}

/// Maximum alignment permitted by the encoding: XCDR2 caps it at 4, XCDR1 allows 8.
pub(crate) fn max_alignment(is_xcdr2: bool) -> usize {
    if is_xcdr2 {
        4
    } else {
        8
    }
}

/// Skip a CDR field and return the new position
fn cdr_skip_field(
    data: &[u8],
    pos: usize,
    base: usize,
    src_le: bool,
    max_align: usize,
    field_type: &CdrFieldType,
) -> Option<usize> {
    match field_type {
        CdrFieldType::String => {
            let aligned = align_body(pos, base, 4.min(max_align));
            if aligned + 4 > data.len() {
                return None;
            }
            let raw = [data[aligned], data[aligned + 1], data[aligned + 2], data[aligned + 3]];
            let str_len =
                if src_le { u32::from_le_bytes(raw) } else { u32::from_be_bytes(raw) } as usize;
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
            let aligned = align_body(pos, base, 2.min(max_align));
            if aligned + 2 > data.len() {
                return None;
            }
            Some(aligned + 2)
        }
        CdrFieldType::Int32 | CdrFieldType::UInt32 => {
            let aligned = align_body(pos, base, 4.min(max_align));
            if aligned + 4 > data.len() {
                return None;
            }
            Some(aligned + 4)
        }
        CdrFieldType::Int64 | CdrFieldType::UInt64 => {
            let aligned = align_body(pos, base, 8.min(max_align));
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
    base: usize,
    src_le: bool,
    max_align: usize,
    field_type: &CdrFieldType,
) -> Option<(Parameter, usize)> {
    match field_type {
        CdrFieldType::String => {
            let aligned = align_body(pos, base, 4.min(max_align));
            if aligned + 4 > data.len() {
                return None;
            }
            let raw = [data[aligned], data[aligned + 1], data[aligned + 2], data[aligned + 3]];
            let str_len =
                if src_le { u32::from_le_bytes(raw) } else { u32::from_be_bytes(raw) } as usize;
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
            let aligned = align_body(pos, base, 4.min(max_align));
            if aligned + 4 > data.len() {
                return None;
            }
            let raw = [data[aligned], data[aligned + 1], data[aligned + 2], data[aligned + 3]];
            let val = if src_le { i32::from_le_bytes(raw) } else { i32::from_be_bytes(raw) };
            Some((Parameter::IntegerValue(val as i128), aligned + 4))
        }
        CdrFieldType::UInt32 => {
            let aligned = align_body(pos, base, 4.min(max_align));
            if aligned + 4 > data.len() {
                return None;
            }
            let raw = [data[aligned], data[aligned + 1], data[aligned + 2], data[aligned + 3]];
            let val = if src_le { u32::from_le_bytes(raw) } else { u32::from_be_bytes(raw) };
            Some((Parameter::IntegerValue(val as i128), aligned + 4))
        }
        CdrFieldType::Int16 => {
            let aligned = align_body(pos, base, 2.min(max_align));
            if aligned + 2 > data.len() {
                return None;
            }
            let raw = [data[aligned], data[aligned + 1]];
            let val = if src_le { i16::from_le_bytes(raw) } else { i16::from_be_bytes(raw) };
            Some((Parameter::IntegerValue(val as i128), aligned + 2))
        }
        CdrFieldType::UInt16 => {
            let aligned = align_body(pos, base, 2.min(max_align));
            if aligned + 2 > data.len() {
                return None;
            }
            let raw = [data[aligned], data[aligned + 1]];
            let val = if src_le { u16::from_le_bytes(raw) } else { u16::from_be_bytes(raw) };
            Some((Parameter::IntegerValue(val as i128), aligned + 2))
        }
        CdrFieldType::Int64 => {
            let aligned = align_body(pos, base, 8.min(max_align));
            if aligned + 8 > data.len() {
                return None;
            }
            let raw = [
                data[aligned],
                data[aligned + 1],
                data[aligned + 2],
                data[aligned + 3],
                data[aligned + 4],
                data[aligned + 5],
                data[aligned + 6],
                data[aligned + 7],
            ];
            let val = if src_le { i64::from_le_bytes(raw) } else { i64::from_be_bytes(raw) };
            Some((Parameter::IntegerValue(val as i128), aligned + 8))
        }
        CdrFieldType::UInt64 => {
            let aligned = align_body(pos, base, 8.min(max_align));
            if aligned + 8 > data.len() {
                return None;
            }
            let raw = [
                data[aligned],
                data[aligned + 1],
                data[aligned + 2],
                data[aligned + 3],
                data[aligned + 4],
                data[aligned + 5],
                data[aligned + 6],
                data[aligned + 7],
            ];
            let val = if src_le { u64::from_le_bytes(raw) } else { u64::from_be_bytes(raw) };
            Some((Parameter::IntegerValue(val as i128), aligned + 8))
        }
        CdrFieldType::Int8 => {
            if pos >= data.len() {
                return None;
            }
            Some((Parameter::IntegerValue(data[pos] as i8 as i128), pos + 1))
        }
        CdrFieldType::UInt8 => {
            if pos >= data.len() {
                return None;
            }
            Some((Parameter::IntegerValue(data[pos] as i128), pos + 1))
        }
        CdrFieldType::Bool => {
            if pos >= data.len() {
                return None;
            }
            Some((Parameter::IntegerValue(data[pos] as i128), pos + 1))
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
    let src_le = (encoding_id & 1) == 1;
    let max_align = max_alignment(is_xcdr2);
    let base = 4;
    let mut pos = base;

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
            return cdr_read_field(cdr_bytes, pos, base, src_le, max_align, &field.field_type)
                .map(|(param, _)| param)
                .ok_or_else(|| {
                    DdsError::Error(format!("Failed to parse field '{}' from CDR data", field_name))
                });
        }
        // Not the target - skip this field
        pos = cdr_skip_field(cdr_bytes, pos, base, src_le, max_align, &field.field_type)
            .ok_or_else(|| {
                DdsError::Error(format!("Failed to skip field '{}' in CDR data", field.name))
            })?;
    }

    Err(DdsError::Error(format!("Field '{}' not found", field_name)))
}

#[cfg(test)]
mod alignment_tests {
    use super::*;

    const A: i32 = 0x1111_1111;
    const B: i64 = 0x2222_2222_3333_3333;
    const C: i32 = 0x4444_4444;
    const PAD: u8 = 0xAA;

    fn field(name: &str, field_type: CdrFieldType) -> CdrFieldDescriptor {
        CdrFieldDescriptor { name: name.to_string(), field_type, is_key: false }
    }

    fn i32_i64() -> Vec<CdrFieldDescriptor> {
        vec![field("a", CdrFieldType::Int32), field("b", CdrFieldType::Int64)]
    }

    fn read(
        bytes: &[u8],
        fields: &[CdrFieldDescriptor],
        ext: ExtensibilityKind,
        name: &str,
    ) -> i128 {
        match cdr_parse_field_value(bytes, fields, &ext, name).expect("field must parse") {
            Parameter::IntegerValue(v) => v,
            other => panic!("expected IntegerValue, got {:?}", other),
        }
    }

    /// XCDR1 aligns i64 to 8, so `b` sits at body offset 8 (stream offset 12).
    #[test]
    fn xcdr1_keeps_8_byte_alignment() {
        let mut bytes = vec![0x00, 0x01, 0x00, 0x00];
        bytes.extend_from_slice(&A.to_le_bytes());
        bytes.extend_from_slice(&[PAD; 4]);
        bytes.extend_from_slice(&B.to_le_bytes());

        assert_eq!(read(&bytes, &i32_i64(), ExtensibilityKind::Final, "b"), B as i128);
    }

    /// XCDR2 caps alignment at 4, so `b` follows `a` with no padding.
    #[test]
    fn xcdr2_caps_alignment_at_4() {
        let mut bytes = vec![0x00, 0x07, 0x00, 0x00];
        bytes.extend_from_slice(&A.to_le_bytes());
        bytes.extend_from_slice(&B.to_le_bytes());

        assert_eq!(read(&bytes, &i32_i64(), ExtensibilityKind::Final, "b"), B as i128);
    }

    /// The cap must apply while skipping fields, not only while reading them.
    #[test]
    fn xcdr2_caps_alignment_when_skipping() {
        let mut bytes = vec![0x00, 0x07, 0x00, 0x00];
        bytes.extend_from_slice(&A.to_le_bytes());
        bytes.extend_from_slice(&B.to_le_bytes());
        bytes.extend_from_slice(&C.to_le_bytes());

        let fields = vec![
            field("a", CdrFieldType::Int32),
            field("b", CdrFieldType::Int64),
            field("c", CdrFieldType::Int32),
        ];
        assert_eq!(read(&bytes, &fields, ExtensibilityKind::Final, "c"), C as i128);
    }

    /// DHEADER shifts the fields but the alignment origin stays at the body start.
    /// Two leading i32 put `b` at body offset 12, which is 4- but not 8-aligned.
    #[test]
    fn xcdr2_appendable_skips_dheader() {
        let mut bytes = vec![0x00, 0x07, 0x00, 0x00];
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&A.to_le_bytes());
        bytes.extend_from_slice(&C.to_le_bytes());
        bytes.extend_from_slice(&B.to_le_bytes());

        let fields = vec![
            field("a", CdrFieldType::Int32),
            field("c", CdrFieldType::Int32),
            field("b", CdrFieldType::Int64),
        ];
        assert_eq!(read(&bytes, &fields, ExtensibilityKind::Appendable, "b"), B as i128);
    }

    #[test]
    fn xcdr2_big_endian_caps_alignment() {
        let mut bytes = vec![0x00, 0x06, 0x00, 0x00];
        bytes.extend_from_slice(&A.to_be_bytes());
        bytes.extend_from_slice(&B.to_be_bytes());

        assert_eq!(read(&bytes, &i32_i64(), ExtensibilityKind::Final, "b"), B as i128);
    }

    /// A string reached by skipping a misaligned i64.
    #[test]
    fn xcdr2_string_after_int64() {
        let mut bytes = vec![0x00, 0x07, 0x00, 0x00];
        bytes.extend_from_slice(&A.to_le_bytes());
        bytes.extend_from_slice(&B.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(b"abc\0");

        let fields = vec![
            field("a", CdrFieldType::Int32),
            field("b", CdrFieldType::Int64),
            field("s", CdrFieldType::String),
        ];
        let parsed = cdr_parse_field_value(&bytes, &fields, &ExtensibilityKind::Final, "s")
            .expect("field must parse");
        assert_eq!(parsed, Parameter::String("abc".to_string()));
    }
}
