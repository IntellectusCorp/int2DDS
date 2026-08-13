//! Serialization facade.
//!
//! The CDR/XCDR wire rules live in the `int2dds-cdr` kernel crate. Everything the
//! kernel owns is re-exported here under the paths it had before the split, which
//! are the paths `#[derive(DdsType)]` writes into generated code; `derive/tests/facade.rs`
//! is the compile gate for that, and `cdr/tests/derive_wire/` exercises these paths
//! from outside the crate. Keep the re-exports enumerated rather than glob:
//! the list is what makes the facade auditable.
//!
//! `pl_cdr` (SPDP/SEDP/inline QoS) stays here. It shares the `PID(2)+length(2)`
//! header shape with XTypes PL_CDR but obeys the opposite length rule — RTPS 2.5
//! §9.6.2.2.2 counts trailing padding, XTypes does not — so the two must not merge.

pub mod pl_cdr;

pub use int2dds_cdr::{cdr, core, key_holder, xcdr};
pub use int2dds_cdr::{key_holder_align_up, KeyHolder, KeyHolderAccessor, KeyHolderFallback};

pub use crate::infrastructure::qos_policy::DataRepresentationId;
pub use int2dds_cdr::{
    // Alignment utilities
    align_buffer,
    align_buffer_with_header_offset,
    align_position_with_header_offset,
    deserialize_array_common,
    deserialize_optional_common,
    deserialize_sequence_common,
    // Endianness utilities
    from_bytes_f32,
    from_bytes_f64,
    from_bytes_i16,
    from_bytes_i32,
    from_bytes_i64,
    from_bytes_u16,
    from_bytes_u32,
    from_bytes_u64,
    read_u16,
    read_u32,
    read_u64,
    read_u8,
    // Sequence processing utilities
    serialize_array_common,
    serialize_optional_common,
    serialize_sequence_common,
    to_bytes_f32,
    to_bytes_f64,
    to_bytes_i16,
    to_bytes_i32,
    to_bytes_i64,
    to_bytes_u16,
    to_bytes_u32,
    to_bytes_u64,
    BufferManager,
    DeserializerReader,
    SerializationError,
    WChar,
    WString,
};

/// Deserialize data payload using derive-based DdsType
pub fn deserialize_data_payload<T>(payload: &[u8]) -> Result<T, String>
where
    T: crate::dcps::topic::type_support::DdsType,
{
    T::deserialize(payload).map_err(|e| format!("Failed to deserialize data: {:?}", e))
}
