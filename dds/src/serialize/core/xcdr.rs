//! XCDR (Extended CDR) v2 module
//!
//! This module re-exports XCDR2 types from the cdr module for backward compatibility.

// Re-export v2 types
pub use crate::serialize::cdr::{
    EncodingKind, ExtensibilityKind, MemberHeader, Xcdr2Deserializer, Xcdr2Serializer,
    XcdrDeserialize, XcdrDeserializeMembers, XcdrDeserializer, XcdrError, XcdrResult,
    XcdrSerialize, XcdrSerializeMembers, XcdrSerializer,
};
