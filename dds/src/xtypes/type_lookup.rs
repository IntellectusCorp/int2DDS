//! DDS-XTypes 1.3 TypeLookup service wire types (Section 7.6.3.3).
//!
//! These types carry the request/reply payloads exchanged over the four
//! TypeLookup builtin endpoints so that a participant which only learned a
//! peer's `TypeIdentifier` (via `TypeInformation`) can fetch the missing
//! `TypeObject`s and complete its local `TypeRegistry`.
//!
//! The encoding here is a plain little-endian layout that reuses the existing
//! `TypeIdentifier`/`TypeObject` (de)serializers. Both ends of an int2DDS
//! exchange use it symmetrically; the operation discriminators match the
//! Fast-DDS hash constants to ease future interoperability.

use crate::rtps::common::{guid::Guid, sequence::SequenceNumber};

use super::type_object::{TypeIdentifier, TypeIdentifierWithSize, TypeObject};

/// Operation discriminator for `getTypes` (Fast-DDS `TypeLookup_getTypes_Hash`).
pub const TYPE_LOOKUP_GETTYPES_HASH: i32 = 0x018252d3;
/// Operation discriminator for `getTypeDependencies`.
pub const TYPE_LOOKUP_GETTYPE_DEPENDENCIES_HASH: i32 = 0x05aafb31;

// ---------------------------------------------------------------------------
// Primitive wire helpers (little-endian)
// ---------------------------------------------------------------------------

fn write_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, String> {
    if data.len() < *pos + 4 {
        return Err("Insufficient data for u32".to_string());
    }
    let v = u32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
    *pos += 4;
    Ok(v)
}

fn write_bytes(buffer: &mut Vec<u8>, bytes: &[u8]) {
    write_u32(buffer, bytes.len() as u32);
    buffer.extend_from_slice(bytes);
}

fn read_bytes(data: &[u8], pos: &mut usize) -> Result<Vec<u8>, String> {
    let len = read_u32(data, pos)? as usize;
    if data.len() < *pos + len {
        return Err("Insufficient data for byte sequence".to_string());
    }
    let out = data[*pos..*pos + len].to_vec();
    *pos += len;
    Ok(out)
}

fn write_string(buffer: &mut Vec<u8>, value: &str) {
    write_bytes(buffer, value.as_bytes());
}

fn read_string(data: &[u8], pos: &mut usize) -> Result<String, String> {
    let bytes = read_bytes(data, pos)?;
    String::from_utf8(bytes).map_err(|_| "Invalid UTF-8 in string".to_string())
}

// ---------------------------------------------------------------------------
// SampleIdentity / RPC headers
// ---------------------------------------------------------------------------

/// DDS-RPC `SampleIdentity`: correlates a reply with its originating request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleIdentity {
    pub writer_guid: Guid,
    pub sequence_number: SequenceNumber,
}

impl SampleIdentity {
    pub fn new(writer_guid: Guid, sequence_number: SequenceNumber) -> Self {
        Self { writer_guid, sequence_number }
    }

    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.writer_guid.to_bytes());
        buffer.extend_from_slice(&self.sequence_number.high.to_le_bytes());
        buffer.extend_from_slice(&self.sequence_number.low.to_le_bytes());
    }

    pub fn deserialize(data: &[u8], pos: &mut usize) -> Result<Self, String> {
        if data.len() < *pos + 24 {
            return Err("Insufficient data for SampleIdentity".to_string());
        }
        let mut guid_bytes = [0u8; 16];
        guid_bytes.copy_from_slice(&data[*pos..*pos + 16]);
        *pos += 16;
        let high = i32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
        *pos += 4;
        let low = read_u32(data, pos)?;
        Ok(Self {
            writer_guid: Guid::from_bytes(guid_bytes),
            sequence_number: SequenceNumber::new(high, low),
        })
    }
}

/// DDS-RPC `RequestHeader`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHeader {
    pub request_id: SampleIdentity,
    pub instance_name: String,
}

impl RequestHeader {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.request_id.serialize_into(buffer);
        write_string(buffer, &self.instance_name);
    }

    pub fn deserialize(data: &[u8], pos: &mut usize) -> Result<Self, String> {
        let request_id = SampleIdentity::deserialize(data, pos)?;
        let instance_name = read_string(data, pos)?;
        Ok(Self { request_id, instance_name })
    }
}

/// DDS-RPC `ReplyHeader`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyHeader {
    pub related_request_id: SampleIdentity,
    pub remote_exception_code: i32,
}

impl ReplyHeader {
    pub fn serialize_into(&self, buffer: &mut Vec<u8>) {
        self.related_request_id.serialize_into(buffer);
        buffer.extend_from_slice(&self.remote_exception_code.to_le_bytes());
    }

    pub fn deserialize(data: &[u8], pos: &mut usize) -> Result<Self, String> {
        let related_request_id = SampleIdentity::deserialize(data, pos)?;
        if data.len() < *pos + 4 {
            return Err("Insufficient data for ReplyHeader exception code".to_string());
        }
        let remote_exception_code =
            i32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
        *pos += 4;
        Ok(Self { related_request_id, remote_exception_code })
    }
}

// ---------------------------------------------------------------------------
// Operation payloads
// ---------------------------------------------------------------------------

fn write_type_ids(buffer: &mut Vec<u8>, type_ids: &[TypeIdentifier]) {
    write_u32(buffer, type_ids.len() as u32);
    for type_id in type_ids {
        type_id.serialize_into(buffer);
    }
}

fn read_type_ids(data: &[u8], pos: &mut usize) -> Result<Vec<TypeIdentifier>, String> {
    let count = read_u32(data, pos)? as usize;
    let mut type_ids = Vec::new();
    for _ in 0..count {
        let (type_id, consumed) = TypeIdentifier::deserialize(&data[*pos..])?;
        *pos += consumed;
        type_ids.push(type_id);
    }
    Ok(type_ids)
}

/// `getTypeDependencies` input: the type ids to resolve plus a continuation token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetTypeDependenciesIn {
    pub type_ids: Vec<TypeIdentifier>,
    pub continuation_point: Vec<u8>,
}

impl GetTypeDependenciesIn {
    fn serialize_into(&self, buffer: &mut Vec<u8>) {
        write_type_ids(buffer, &self.type_ids);
        write_bytes(buffer, &self.continuation_point);
    }

    fn deserialize(data: &[u8], pos: &mut usize) -> Result<Self, String> {
        let type_ids = read_type_ids(data, pos)?;
        let continuation_point = read_bytes(data, pos)?;
        Ok(Self { type_ids, continuation_point })
    }
}

/// `getTypeDependencies` output: dependent type ids (with sizes) plus continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetTypeDependenciesOut {
    pub dependent_typeids: Vec<TypeIdentifierWithSize>,
    pub continuation_point: Vec<u8>,
}

impl GetTypeDependenciesOut {
    fn serialize_into(&self, buffer: &mut Vec<u8>) {
        write_u32(buffer, self.dependent_typeids.len() as u32);
        for dep in &self.dependent_typeids {
            dep.serialize_into(buffer);
        }
        write_bytes(buffer, &self.continuation_point);
    }

    fn deserialize(data: &[u8], pos: &mut usize) -> Result<Self, String> {
        let count = read_u32(data, pos)? as usize;
        let mut dependent_typeids = Vec::new();
        for _ in 0..count {
            let (dep, consumed) = TypeIdentifierWithSize::deserialize(&data[*pos..])?;
            *pos += consumed;
            dependent_typeids.push(dep);
        }
        let continuation_point = read_bytes(data, pos)?;
        Ok(Self { dependent_typeids, continuation_point })
    }
}

/// `getTypes` input: the type ids whose TypeObjects are requested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetTypesIn {
    pub type_ids: Vec<TypeIdentifier>,
}

impl GetTypesIn {
    fn serialize_into(&self, buffer: &mut Vec<u8>) {
        write_type_ids(buffer, &self.type_ids);
    }

    fn deserialize(data: &[u8], pos: &mut usize) -> Result<Self, String> {
        Ok(Self { type_ids: read_type_ids(data, pos)? })
    }
}

/// `getTypes` output: each requested type id paired with its TypeObject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetTypesOut {
    pub types: Vec<(TypeIdentifier, TypeObject)>,
}

impl GetTypesOut {
    fn serialize_into(&self, buffer: &mut Vec<u8>) {
        write_u32(buffer, self.types.len() as u32);
        for (type_id, type_object) in &self.types {
            type_id.serialize_into(buffer);
            type_object.serialize_into(buffer);
        }
    }

    fn deserialize(data: &[u8], pos: &mut usize) -> Result<Self, String> {
        let count = read_u32(data, pos)? as usize;
        let mut types = Vec::new();
        for _ in 0..count {
            let (type_id, consumed) = TypeIdentifier::deserialize(&data[*pos..])?;
            *pos += consumed;
            let (type_object, consumed) = TypeObject::deserialize(&data[*pos..])?;
            *pos += consumed;
            types.push((type_id, type_object));
        }
        Ok(Self { types })
    }
}

// ---------------------------------------------------------------------------
// Request / Reply envelopes
// ---------------------------------------------------------------------------

/// Operation call union carried by a [`TypeLookupRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeLookupCall {
    GetTypes(GetTypesIn),
    GetTypeDependencies(GetTypeDependenciesIn),
}

impl TypeLookupCall {
    fn discriminator(&self) -> i32 {
        match self {
            TypeLookupCall::GetTypes(_) => TYPE_LOOKUP_GETTYPES_HASH,
            TypeLookupCall::GetTypeDependencies(_) => TYPE_LOOKUP_GETTYPE_DEPENDENCIES_HASH,
        }
    }

    fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.discriminator().to_le_bytes());
        match self {
            TypeLookupCall::GetTypes(v) => v.serialize_into(buffer),
            TypeLookupCall::GetTypeDependencies(v) => v.serialize_into(buffer),
        }
    }

    fn deserialize(data: &[u8], pos: &mut usize) -> Result<Self, String> {
        let disc = read_discriminator(data, pos)?;
        match disc {
            TYPE_LOOKUP_GETTYPES_HASH => {
                Ok(TypeLookupCall::GetTypes(GetTypesIn::deserialize(data, pos)?))
            }
            TYPE_LOOKUP_GETTYPE_DEPENDENCIES_HASH => Ok(TypeLookupCall::GetTypeDependencies(
                GetTypeDependenciesIn::deserialize(data, pos)?,
            )),
            other => Err(format!("Unknown TypeLookup call discriminator: 0x{:08x}", other)),
        }
    }
}

/// Operation return union carried by a [`TypeLookupReply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeLookupReturn {
    GetTypes(GetTypesOut),
    GetTypeDependencies(GetTypeDependenciesOut),
}

impl TypeLookupReturn {
    fn discriminator(&self) -> i32 {
        match self {
            TypeLookupReturn::GetTypes(_) => TYPE_LOOKUP_GETTYPES_HASH,
            TypeLookupReturn::GetTypeDependencies(_) => TYPE_LOOKUP_GETTYPE_DEPENDENCIES_HASH,
        }
    }

    fn serialize_into(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.discriminator().to_le_bytes());
        match self {
            TypeLookupReturn::GetTypes(v) => v.serialize_into(buffer),
            TypeLookupReturn::GetTypeDependencies(v) => v.serialize_into(buffer),
        }
    }

    fn deserialize(data: &[u8], pos: &mut usize) -> Result<Self, String> {
        let disc = read_discriminator(data, pos)?;
        match disc {
            TYPE_LOOKUP_GETTYPES_HASH => {
                Ok(TypeLookupReturn::GetTypes(GetTypesOut::deserialize(data, pos)?))
            }
            TYPE_LOOKUP_GETTYPE_DEPENDENCIES_HASH => Ok(TypeLookupReturn::GetTypeDependencies(
                GetTypeDependenciesOut::deserialize(data, pos)?,
            )),
            other => Err(format!("Unknown TypeLookup return discriminator: 0x{:08x}", other)),
        }
    }
}

fn read_discriminator(data: &[u8], pos: &mut usize) -> Result<i32, String> {
    if data.len() < *pos + 4 {
        return Err("Insufficient data for discriminator".to_string());
    }
    let v = i32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
    *pos += 4;
    Ok(v)
}

/// A TypeLookup request sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeLookupRequest {
    pub header: RequestHeader,
    pub data: TypeLookupCall,
}

impl TypeLookupRequest {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.header.serialize_into(&mut buffer);
        self.data.serialize_into(&mut buffer);
        buffer
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        let mut pos = 0;
        let header = RequestHeader::deserialize(data, &mut pos)?;
        let call = TypeLookupCall::deserialize(data, &mut pos)?;
        Ok(Self { header, data: call })
    }
}

/// A TypeLookup reply sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeLookupReply {
    pub header: ReplyHeader,
    pub data: TypeLookupReturn,
}

impl TypeLookupReply {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.header.serialize_into(&mut buffer);
        self.data.serialize_into(&mut buffer);
        buffer
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        let mut pos = 0;
        let header = ReplyHeader::deserialize(data, &mut pos)?;
        let ret = TypeLookupReturn::deserialize(data, &mut pos)?;
        Ok(Self { header, data: ret })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::entity_id::EntityId;
    use crate::xtypes::type_object::{
        CompleteStructType, CompleteTypeObject, EquivalenceHash, ExtensibilityKind, TypeFlag,
    };

    fn sample_identity() -> SampleIdentity {
        SampleIdentity::new(
            Guid::new([1u8; 12], EntityId::TYPE_LOOKUP_REQUEST_WRITER),
            SequenceNumber::new(0, 7),
        )
    }

    fn sample_type_object() -> TypeObject {
        let struct_type = CompleteStructType::new(
            TypeFlag::new(ExtensibilityKind::Final, false, false),
            "LookupProbe".to_string(),
            None,
        );
        TypeObject::Complete(CompleteTypeObject::Struct(struct_type))
    }

    #[test]
    fn request_get_type_dependencies_roundtrip() {
        let request = TypeLookupRequest {
            header: RequestHeader {
                request_id: sample_identity(),
                instance_name: "probe".to_string(),
            },
            data: TypeLookupCall::GetTypeDependencies(GetTypeDependenciesIn {
                type_ids: vec![TypeIdentifier::CompleteTypeId(EquivalenceHash::compute(b"a"))],
                continuation_point: vec![1, 2, 3],
            }),
        };
        let bytes = request.serialize();
        assert_eq!(TypeLookupRequest::deserialize(&bytes).unwrap(), request);
    }

    #[test]
    fn request_get_types_roundtrip() {
        let request = TypeLookupRequest {
            header: RequestHeader { request_id: sample_identity(), instance_name: String::new() },
            data: TypeLookupCall::GetTypes(GetTypesIn {
                type_ids: vec![
                    TypeIdentifier::CompleteTypeId(EquivalenceHash::compute(b"x")),
                    TypeIdentifier::CompleteTypeId(EquivalenceHash::compute(b"y")),
                ],
            }),
        };
        let bytes = request.serialize();
        assert_eq!(TypeLookupRequest::deserialize(&bytes).unwrap(), request);
    }

    #[test]
    fn reply_get_type_dependencies_roundtrip() {
        let reply = TypeLookupReply {
            header: ReplyHeader { related_request_id: sample_identity(), remote_exception_code: 0 },
            data: TypeLookupReturn::GetTypeDependencies(GetTypeDependenciesOut {
                dependent_typeids: vec![TypeIdentifierWithSize::new(
                    TypeIdentifier::CompleteTypeId(EquivalenceHash::compute(b"dep")),
                    42,
                )],
                continuation_point: vec![9],
            }),
        };
        let bytes = reply.serialize();
        assert_eq!(TypeLookupReply::deserialize(&bytes).unwrap(), reply);
    }

    #[test]
    fn reply_get_types_roundtrip() {
        let type_object = sample_type_object();
        let hash = EquivalenceHash::compute(&type_object.serialize());
        let reply = TypeLookupReply {
            header: ReplyHeader { related_request_id: sample_identity(), remote_exception_code: 0 },
            data: TypeLookupReturn::GetTypes(GetTypesOut {
                types: vec![(TypeIdentifier::CompleteTypeId(hash), type_object)],
            }),
        };
        let bytes = reply.serialize();
        assert_eq!(TypeLookupReply::deserialize(&bytes).unwrap(), reply);
    }
}
