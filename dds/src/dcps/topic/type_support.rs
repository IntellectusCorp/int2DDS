//! Type Support - Type registration and serialization infrastructure for DDS.
//!
//! This module provides the `DdsType` trait and related infrastructure for making Rust types
//! compatible with DDS communication. Types must implement `DdsType` to be used with Topics,
//! DataWriters, and DataReaders.
//!
//! The `DdsType` derive macro (from `int2dds_derive`) automatically implements the required
//! trait methods for structs, handling serialization, key extraction, and type metadata.
//!
//! # Key Concepts
//!
//! - **DdsType Trait**: Marker and functionality trait for DDS-compatible types
//! - **Serialization**: Conversion between Rust types and wire format (CDR, PL_CDR)
//! - **Key Support**: Identification of key fields for instance management
//! - **Type Registration**: Automatic registration of types with DomainParticipant
//!
//! # Example
//!
//! ```no_run
//! use int2dds::topic::type_support::DdsType;
//!
//! #[derive(DdsType)]
//! #[dds_type(crate_path = "int2dds")]
//! struct MyData {
//!     #[dds(key)]
//!     id: u32,
//!     value: String,
//! }
//! ```

use std::{
    any::{Any, TypeId},
    fmt::Debug,
    sync::Arc,
};

use crate::{
    common::instance_handle::InstanceHandle,
    dcps::core::error::{DdsError, DdsResult},
    domain::domain_participant::DomainParticipant,
    rtps::common::types::SerializedData,
    topic::sql::ast::Parameter,
    xtypes::{TypeIdentifier, TypeObject},
};

pub use int2dds_derive::DdsType;

/// Serialization format options for DDS types
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SerializationFormat {
    /// CDR
    #[default]
    Cdr,
    /// XCDR
    Xcdr { extensibility_kind: crate::serialize::xcdr::ExtensibilityKind, use_delimiters: bool },
}

impl SerializationFormat {
    /// The format a writer uses for one representation id and the type's
    /// extensibility. `None` for representations without a wire codec (XML).
    pub fn for_representation(
        representation: crate::infrastructure::qos_policy::DataRepresentationId,
        extensibility: crate::serialize::xcdr::ExtensibilityKind,
    ) -> Option<Self> {
        use crate::infrastructure::qos_policy::DataRepresentationId;
        use crate::serialize::xcdr::ExtensibilityKind;
        match representation {
            DataRepresentationId::XcdrDataRepresentation => Some(Self::Cdr),
            DataRepresentationId::Xcdr2DataRepresentation => Some(Self::Xcdr {
                extensibility_kind: extensibility,
                use_delimiters: matches!(
                    extensibility,
                    ExtensibilityKind::Appendable | ExtensibilityKind::Mutable
                ),
            }),
            DataRepresentationId::XmlDataRepresentation => None,
        }
    }
}

pub trait DdsType: 'static + Send + Sync + Clone + Debug {
    type TypeSupport: TypeSupport + Default;
    type FieldAccessor: FieldAccessor + Default;

    fn get_type_support() -> Arc<Self::TypeSupport> {
        Arc::new(Self::TypeSupport::default())
    }

    fn get_type_name() -> String {
        Self::TypeSupport::default().get_type_name().to_string()
    }

    // Convenient type-safe methods
    fn serialize(&self) -> DdsResult<SerializedData> {
        Self::TypeSupport::default().serialize(self as &dyn Any, None)
    }

    fn deserialize(data: &[u8]) -> DdsResult<Self> {
        let any_box = Self::TypeSupport::default().deserialize(data, None)?;
        any_box
            .downcast::<Self>()
            .map(|boxed| *boxed)
            .map_err(|_| DdsError::Error("Type downcast failed".to_string()))
    }

    fn get_field_value(&self, field_path: &str) -> DdsResult<Parameter> {
        Self::FieldAccessor::default().get_field_value(self as &dyn Any, field_path)
    }

    fn has_field(&self, field_path: &str) -> DdsResult<bool> {
        Ok(Self::FieldAccessor::default().has_field(field_path))
    }
}

pub trait FieldAccessor: Send + Sync + 'static {
    fn get_field_value(&self, data: &dyn Any, field_path: &str) -> DdsResult<Parameter>;
    fn has_field(&self, field_path: &str) -> bool;
}

pub trait TypeSupport: Send + Sync + 'static {
    fn type_id(&self) -> TypeId;
    fn get_type_name(&self) -> &str;

    // Serialization with optional format override.
    // When format is None, the implementation uses its own default behavior.
    fn serialize(
        &self,
        data: &dyn Any,
        format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData>;
    fn deserialize(
        &self,
        data: &[u8],
        format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>>;

    // Deserialize from non-contiguous fragment chunks (scatter-gather receive
    // path). Default materializes the chunks into a contiguous buffer and
    // delegates to `deserialize`; the derive macro overrides this to read classic
    // CDR directly across chunks. Slice order is fragment order.
    fn deserialize_chained(
        &self,
        chunks: &[bytes::Bytes],
        format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        let mut buf = Vec::with_capacity(total);
        for c in chunks {
            buf.extend_from_slice(c);
        }
        self.deserialize(&buf, format)
    }

    /// Serialize into an existing buffer, reusing its capacity.
    /// The buffer is cleared and filled with serialized data (including encapsulation header).
    /// Default implementation delegates to `serialize()` (no buffer reuse).
    /// Derive macro generates an optimized version that reuses the buffer's capacity.
    fn serialize_into(
        &self,
        data: &dyn Any,
        buffer: &mut Vec<u8>,
        format: Option<&SerializationFormat>,
    ) -> DdsResult<()> {
        let serialized = self.serialize(data, format)?;
        buffer.clear();
        buffer.extend_from_slice(&serialized);
        Ok(())
    }

    // Key handling
    fn serialize_key(&self, data: &dyn Any) -> DdsResult<SerializedData>;
    fn deserialize_key(&self, serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>>;

    // Decode a wire serializedKey (K-flag SerializedPayload, with encapsulation header)
    // into the key value. Default strips the 4-byte header and decodes big-endian;
    // generated impls override to read endianness from the header.
    fn deserialize_key_payload(&self, payload: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        let body = if payload.len() >= 4 { &payload[4..] } else { payload };
        self.deserialize_key(body)
    }

    // Encode the key as a wire serializedKey (K-flag SerializedPayload, with a
    // 4-byte encapsulation header) in the given representation. Default emits XCDR1
    // CDR_BE; generated impls override to honor XCDR2.
    fn serialize_key_payload(
        &self,
        data: &dyn Any,
        _format: &SerializationFormat,
    ) -> DdsResult<SerializedData> {
        let key = self.serialize_key(data)?;
        let mut payload = Vec::with_capacity(key.len() + 4);
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // CDR_BE, no options
        payload.extend_from_slice(&key);
        Ok(std::sync::Arc::from(payload))
    }
    fn compute_key(&self, data: &dyn Any) -> InstanceHandle;
    fn is_compute_key_provided(&self) -> bool;

    /// Canonical serialized key CDR and InstanceHandle of a full serialized sample.
    /// Default materializes the sample and asks the two key entry points; an
    /// implementation that reads the wire directly overrides this to project once.
    fn key_info_from_bytes(&self, sample: &[u8]) -> DdsResult<(SerializedData, InstanceHandle)> {
        let boxed = self.deserialize(sample, None)?;
        Ok((self.serialize_key(&*boxed)?, self.compute_key(&*boxed)))
    }

    fn get_extensibility_kind(&self) -> crate::serialize::xcdr::ExtensibilityKind;

    /// Whether `field_path` names a member a filter expression can reference,
    /// answered from the same metadata the runtime filter evaluation uses.
    /// `None` means no metadata is available and expression validation is skipped.
    fn filter_has_field(&self, _field_path: &str) -> Option<bool> {
        None
    }

    fn serialize_key_and_non_key(
        &self,
        data: &dyn Any,
    ) -> DdsResult<(SerializedData, SerializedData)> {
        let key_data = self.serialize_key(data)?;
        let full_data = self.serialize(data, None)?;
        Ok((key_data, full_data))
    }

    /// Get serialized size estimate for the type
    // Todo:()
    fn get_serialized_size_bound(&self) -> Option<usize> {
        None
    }

    /// Get the TypeIdentifier for this type (DDS-XTypes).
    /// Returns None if the type does not support XTypes.
    fn get_type_identifier(&self) -> Option<TypeIdentifier> {
        None
    }

    /// Get the TypeObject for this type (DDS-XTypes).
    /// Returns None if the type does not support XTypes.
    fn get_type_object(&self) -> Option<TypeObject> {
        None
    }

    /// Get this type's `TypeObject` plus the transitive
    /// closure of every nested composite it references.
    fn get_type_object_closure(&self) -> Vec<(TypeIdentifier, TypeObject)> {
        match (self.get_type_identifier(), self.get_type_object()) {
            (Some(id), Some(obj)) => vec![(id, obj)],
            _ => Vec::new(),
        }
    }

    fn register_type(
        self: Arc<Self>,
        participant: &mut DomainParticipant,
        type_name: &str,
    ) -> DdsResult<()>
    where
        Self: Sized,
    {
        let type_support: Arc<dyn TypeSupport> = self;
        participant.register_type(type_support, type_name)
    }
}

/// Autoref specialization helper for dot-notation nested field access.
///
/// Allows derive macro generated code to delegate field access into nested struct
/// fields without knowing at macro expansion time whether a field type implements
/// `DdsType`. Uses the autoref specialization pattern:
/// - If `T: DdsType`, inherent methods on `NestedAccessor<T>` are resolved first.
/// - Otherwise, auto-ref finds the fallback trait impl on `&NestedAccessor<T>`.
pub mod nested_access {
    use super::*;

    pub struct NestedAccessor<T>(pub core::marker::PhantomData<T>);

    /// Fallback for types that do NOT implement DdsType.
    pub trait NestedAccessFallback {
        fn nested_has_field(&self, _rest: &str) -> bool {
            false
        }
        fn nested_get_field_value(&self, _data: &dyn Any, rest: &str) -> DdsResult<Parameter> {
            Err(DdsError::Error(format!("Type has no nested field '{}'", rest)))
        }
    }

    impl<T> NestedAccessFallback for NestedAccessor<T> {}

    /// Preferred path for types that DO implement DdsType.
    impl<T: DdsType> NestedAccessor<T> {
        pub fn nested_has_field(&self, rest: &str) -> bool {
            T::FieldAccessor::default().has_field(rest)
        }

        pub fn nested_get_field_value(&self, data: &dyn Any, rest: &str) -> DdsResult<Parameter> {
            T::FieldAccessor::default().get_field_value(data, rest)
        }
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod keyhash_tests {
    use super::*;
    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct SingleU32Key {
        #[dds(key)]
        pub id: u32,
        pub data: f64,
    }

    #[test]
    fn test_keyhash_u32_big_endian() {
        use crate::dcps::topic::type_support::TypeSupport;

        let value = SingleU32Key { id: 42, data: 1.0 };
        let type_support = SingleU32Key::get_type_support();
        let key_bytes = type_support.serialize_key(&value).unwrap();

        assert_eq!(&*key_bytes, &[0x00, 0x00, 0x00, 0x2A]);

        let instance_handle = type_support.compute_key(&value);
        let handle_bytes = instance_handle.value();
        let mut expected = [0u8; 16];
        expected[..4].copy_from_slice(&[0x00, 0x00, 0x00, 0x2A]);
        assert_eq!(handle_bytes, &expected);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct MultiKeyStruct {
        #[dds(key)]
        pub a: u16,
        #[dds(key)]
        pub b: u32,
        pub c: f64,
    }

    #[test]
    fn test_keyhash_multi_key_big_endian_order() {
        use crate::dcps::topic::type_support::TypeSupport;

        let value = MultiKeyStruct { a: 1, b: 2, c: 99.0 };
        let type_support = MultiKeyStruct::get_type_support();
        let key_bytes = type_support.serialize_key(&value).unwrap();

        assert_eq!(&*key_bytes, &[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,]);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct LargeKeyStruct {
        #[dds(key)]
        pub a: u64,
        #[dds(key)]
        pub b: u64,
        #[dds(key)]
        pub c: u64,
    }

    #[test]
    fn test_keyhash_large_key_uses_md5() {
        use crate::dcps::topic::type_support::TypeSupport;

        let value = LargeKeyStruct { a: 1, b: 2, c: 3 };
        let type_support = LargeKeyStruct::get_type_support();
        let key_bytes = type_support.serialize_key(&value).unwrap();

        assert_eq!(key_bytes.len(), 24);

        let instance_handle = type_support.compute_key(&value);
        let expected_md5 = md5::compute(&*key_bytes);
        assert_eq!(instance_handle.value(), &expected_md5.0);
    }

    // ============================================================================
    // LC 6/7 EMHEADER Optimization Tests
    // ============================================================================
}

#[cfg(test)]
#[allow(unused_imports)]
mod key_payload_tests {
    // Dispose/unregister samples carry the key as a K-flag SerializedPayload (CDR
    // with a 4-byte encapsulation header), not the headerless KeyHash bytes.
    // deserialize_key_payload reads representation/endianness from the wire header.
    use super::*;
    use std::any::Any;

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct KeyedShape {
        #[dds(key)]
        color: String,
        x: i32,
    }

    #[test]
    fn deserialize_key_payload_roundtrip_big_endian() {
        let ts = KeyedShape::get_type_support();
        let shape = KeyedShape { color: "BLUE".to_string(), x: 7 };

        // serialize_key is headerless big-endian; wrap it as a wire serializedKey.
        let key = ts.serialize_key(&shape as &dyn Any).unwrap();
        let mut payload = vec![0x00, 0x00, 0x00, 0x00]; // CDR_BE encapsulation header
        payload.extend_from_slice(&key);

        let decoded = ts.deserialize_key_payload(&payload).unwrap();
        let decoded = decoded.downcast_ref::<KeyedShape>().unwrap();
        assert_eq!(decoded.color, "BLUE");
    }

    #[test]
    fn deserialize_key_payload_xcdr2_delimited_le_wire() {
        // CoreDX-style dispose serializedKey for "BLUE" under XCDR2: DELIMITED_CDR2_LE
        // header (0x0009), single struct DHEADER, then the key members.
        let ts = KeyedShape::get_type_support();
        let payload: &[u8] = &[
            0x00, 0x09, 0x00, 0x00, // DELIMITED_CDR2_LE encapsulation header
            0x09, 0x00, 0x00, 0x00, // DHEADER: object size = 9 bytes
            0x05, 0x00, 0x00, 0x00, // string length 5 (little-endian)
            0x42, 0x4c, 0x55, 0x45, 0x00, // "BLUE\0"
        ];

        let decoded = ts.deserialize_key_payload(payload).unwrap();
        let decoded = decoded.downcast_ref::<KeyedShape>().unwrap();
        assert_eq!(decoded.color, "BLUE");
    }

    #[test]
    fn deserialize_key_payload_little_endian_wire() {
        // Cyclone/OpenDDS XCDR1 dispose serializedKey for "BLUE": CDR_LE header,
        // little-endian string length 5, then "BLUE\0".
        let ts = KeyedShape::get_type_support();
        let payload: &[u8] = &[
            0x00, 0x01, 0x00, 0x03, // CDR_LE encapsulation header
            0x05, 0x00, 0x00, 0x00, // string length 5 (little-endian)
            0x42, 0x4c, 0x55, 0x45, 0x00, // "BLUE\0"
        ];

        let decoded = ts.deserialize_key_payload(payload).unwrap();
        let decoded = decoded.downcast_ref::<KeyedShape>().unwrap();
        assert_eq!(decoded.color, "BLUE");
    }

    #[test]
    fn serialize_key_payload_xcdr2_appendable_roundtrips() {
        use crate::serialize::xcdr::ExtensibilityKind;
        let ts = KeyedShape::get_type_support();
        let shape = KeyedShape { color: "BLUE".to_string(), x: 7 };
        let format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Appendable,
            use_delimiters: true,
        };

        let payload = ts.serialize_key_payload(&shape as &dyn Any, &format).unwrap();
        // Appendable XCDR2 -> DELIMITED_CDR2_LE encapsulation id (0x0009).
        assert_eq!(payload[1], 0x09);

        let decoded = ts.deserialize_key_payload(&payload).unwrap();
        assert_eq!(decoded.downcast_ref::<KeyedShape>().unwrap().color, "BLUE");
    }

    #[test]
    fn raw_key_transcode_matches_typed_xcdr2_wire() {
        // The raw-bytes dispose path has no typed value, so it transcodes the stored
        // big-endian key: deserialize_key -> serialize_key_payload. The result must be
        // byte-identical to the typed path that serializes the value directly.
        use crate::serialize::xcdr::ExtensibilityKind;
        let ts = KeyedShape::get_type_support();
        let shape = KeyedShape { color: "BLUE".to_string(), x: 7 };
        let format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Appendable,
            use_delimiters: true,
        };

        let typed_wire = ts.serialize_key_payload(&shape as &dyn Any, &format).unwrap();

        let be_key = ts.serialize_key(&shape as &dyn Any).unwrap();
        let value = ts.deserialize_key(&be_key).unwrap();
        let transcoded_wire = ts.serialize_key_payload(&*value, &format).unwrap();

        assert_eq!(&*typed_wire, &*transcoded_wire);
    }

    #[test]
    fn serialize_key_payload_xcdr1_is_cdr_be() {
        let ts = KeyedShape::get_type_support();
        let shape = KeyedShape { color: "BLUE".to_string(), x: 7 };

        let payload =
            ts.serialize_key_payload(&shape as &dyn Any, &SerializationFormat::Cdr).unwrap();
        // XCDR1 -> CDR_BE encapsulation header (0x0000).
        assert_eq!(&payload[..2], &[0x00, 0x00]);

        let decoded = ts.deserialize_key_payload(&payload).unwrap();
        assert_eq!(decoded.downcast_ref::<KeyedShape>().unwrap().color, "BLUE");
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct MixedAlignKey {
        #[dds(key)]
        a: u32,
        #[dds(key)]
        b: u64,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct MixedAlignKeyFinal {
        #[dds(key)]
        a: u32,
        #[dds(key)]
        b: u64,
    }

    // The three key serialization profiles below are deliberately different, and a
    // refactor that merges any two of them is silently wrong on the wire. `MixedAlignKey`
    // puts a u64 at a 4-but-not-8 offset so every difference shows up in these bytes:
    // endianness, maximum alignment, encapsulation header and DHEADER all differ.
    // See `serialize_key` / `serialize_key_payload` in derive's `key_methods.rs`.

    /// `serialize_key` — RTPS 9.6.4.8 step 4 KeyHash: PLAIN_CDR2 big-endian, maximum
    /// alignment 4, no encapsulation header.
    const KEY_HASH_BE_ALIGN4: &[u8] = &[
        0x00, 0x00, 0x00, 0x01, // a = 1, big-endian
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, // b = 2 at body offset 4
    ];

    /// `serialize_key_payload(Cdr)` — CDR_BE header plus classic CDR body, maximum
    /// alignment 8, so `b` moves to body offset 8.
    const KEY_PAYLOAD_CDR_BE_ALIGN8: &[u8] = &[
        0x00, 0x00, 0x00, 0x00, // CDR_BE encapsulation header
        0x00, 0x00, 0x00, 0x01, // a = 1, big-endian
        0x00, 0x00, 0x00, 0x00, // padding to 8-byte alignment
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, // b = 2 at body offset 8
    ];

    /// `serialize_key_payload(Xcdr)` for an Appendable type — DELIMITED_CDR2_LE header,
    /// struct DHEADER, little-endian body, maximum alignment 4.
    const KEY_PAYLOAD_XCDR2_APPENDABLE_LE: &[u8] = &[
        0x00, 0x09, 0x00, 0x00, // DELIMITED_CDR2_LE encapsulation header
        0x0c, 0x00, 0x00, 0x00, // DHEADER: object size = 12 bytes
        0x01, 0x00, 0x00, 0x00, // a = 1, little-endian
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // b = 2 at body offset 4
    ];

    /// `serialize_key_payload(Xcdr)` for a Final type — PLAIN_CDR2_LE header and no
    /// DHEADER, which is a separate branch from the Appendable case above.
    const KEY_PAYLOAD_XCDR2_FINAL_LE: &[u8] = &[
        0x00, 0x07, 0x00, 0x00, // PLAIN_CDR2_LE encapsulation header
        0x01, 0x00, 0x00, 0x00, // a = 1, little-endian
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // b = 2 at body offset 4
    ];

    fn xcdr_format(
        extensibility_kind: crate::serialize::xcdr::ExtensibilityKind,
        use_delimiters: bool,
    ) -> SerializationFormat {
        SerializationFormat::Xcdr { extensibility_kind, use_delimiters }
    }

    #[test]
    fn key_hash_profile_is_headerless_big_endian_align4() {
        let ts = MixedAlignKey::get_type_support();
        let key = MixedAlignKey { a: 1, b: 2 };

        let bytes = ts.serialize_key(&key as &dyn Any).unwrap();
        assert_eq!(&*bytes, KEY_HASH_BE_ALIGN4);
    }

    #[test]
    fn key_payload_xcdr2_appendable_profile_is_little_endian_align4() {
        let ts = MixedAlignKey::get_type_support();
        let key = MixedAlignKey { a: 1, b: 2 };

        let payload = ts
            .serialize_key_payload(
                &key as &dyn Any,
                &xcdr_format(crate::serialize::xcdr::ExtensibilityKind::Appendable, true),
            )
            .unwrap();
        assert_eq!(&*payload, KEY_PAYLOAD_XCDR2_APPENDABLE_LE);
    }

    #[test]
    fn key_payload_xcdr2_final_profile_omits_dheader() {
        let ts = MixedAlignKeyFinal::get_type_support();
        let key = MixedAlignKeyFinal { a: 1, b: 2 };

        let payload = ts
            .serialize_key_payload(
                &key as &dyn Any,
                &xcdr_format(crate::serialize::xcdr::ExtensibilityKind::Final, false),
            )
            .unwrap();
        assert_eq!(&*payload, KEY_PAYLOAD_XCDR2_FINAL_LE);

        let decoded = ts.deserialize_key_payload(&payload).unwrap();
        let decoded = decoded.downcast_ref::<MixedAlignKeyFinal>().unwrap();
        assert_eq!((decoded.a, decoded.b), (1, 2));
    }

    #[test]
    fn three_key_profiles_stay_distinct() {
        // Guards the refactor that says "these are duplicates, merge them". Round-trip
        // tests survive such a merge because each profile stays self-consistent; only
        // comparing the profiles against each other catches it.
        let ts = MixedAlignKey::get_type_support();
        let key = MixedAlignKey { a: 1, b: 2 };

        let key_hash = ts.serialize_key(&key as &dyn Any).unwrap();
        let cdr = ts.serialize_key_payload(&key as &dyn Any, &SerializationFormat::Cdr).unwrap();
        let xcdr = ts
            .serialize_key_payload(
                &key as &dyn Any,
                &xcdr_format(crate::serialize::xcdr::ExtensibilityKind::Appendable, true),
            )
            .unwrap();

        assert_ne!(&cdr[4..], &*key_hash, "CDR body must not reuse the max-align-4 KeyHash body");
        assert_ne!(&xcdr[4..], &*key_hash, "XCDR2 payload is little-endian, KeyHash is big-endian");
        assert_ne!(&*cdr, &*xcdr, "CDR and XCDR2 key payloads must differ");
    }

    #[test]
    fn serialize_key_payload_xcdr1_reencodes_8byte_alignment() {
        // A u32 followed by a u64: XCDR1 (8-byte max alignment) and the KeyHash body
        // (max-align-4) place `b` at different offsets. The XCDR1 wire serializedKey must
        // re-encode with 8-byte alignment so the CDR_BE header agrees with the body;
        // otherwise a header-honoring reader (this crate's own deserialize_key_payload, and
        // any spec-compliant peer) misparses `b`. Round-tripping proves header/body agree.
        let ts = MixedAlignKey::get_type_support();
        let key = MixedAlignKey { a: 1, b: 2 };

        let payload =
            ts.serialize_key_payload(&key as &dyn Any, &SerializationFormat::Cdr).unwrap();
        // CDR_BE header (4) + u32 a @0 (4) + 4 pad + u64 b @8 (8) = 20 bytes.
        assert_eq!(&*payload, KEY_PAYLOAD_CDR_BE_ALIGN8);

        let decoded = ts.deserialize_key_payload(&payload).unwrap();
        let decoded = decoded.downcast_ref::<MixedAlignKey>().unwrap();
        assert_eq!((decoded.a, decoded.b), (1, 2));
    }
}
