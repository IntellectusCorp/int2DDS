//! DDS-XTypes 1.3 TypeLookup service wire types (Section 7.6.3.3).
//!
//! These types carry the request/reply payloads exchanged over the four
//! TypeLookup builtin endpoints so that a participant which only learned a
//! peer's `TypeIdentifier` (via `TypeInformation`) can fetch the missing
//! `TypeObject`s and complete its local `TypeRegistry`.
//!
//! `TypeLookupTypes.idl`: a 4-byte PLAIN_CDR2 encapsulation header, `@final`
//! `TypeLookup_Request`/`TypeLookup_Reply` carrying `@final` RPC headers, the
//! operation-hash `TypeLookup_Call`/`TypeLookup_Return` unions (the reply
//! nests a `*_Result` union with a `RETCODE_OK` discriminator), and `@mutable`
//! `*_In`/`*_Out` structs whose `@hashid` members use EMHEADER encoding. The
//! leaf `TypeIdentifier`/`TypeObject`/`TypeIdentifierWithSize` payloads reuse
//! their existing serializers, matching how SEDP already places them on the
//! wire.

use crate::rtps::common::{guid::Guid, sequence::SequenceNumber};
use crate::serialize::cdr::{
    CdrError, CdrSerializerCommon, ExtensibilityKind, LcHint, PrimitiveSerialize, StringSerialize,
    Xcdr2Deserializer, Xcdr2Serializer,
};
use crate::serialize::DeserializerReader;

use super::type_object::{TypeIdentifier, TypeIdentifierWithSize, TypeObject};

/// Operation discriminator for `getTypes`.
pub const TYPE_LOOKUP_GETTYPES_HASH: i32 = 0x018252d3;
/// Operation discriminator for `getTypeDependencies`.
pub const TYPE_LOOKUP_GETTYPE_DEPENDENCIES_HASH: i32 = 0x05aafb31;

const RETCODE_OK: i32 = 0;

pub const MAX_DEPENDENCIES_PER_REPLY: usize = 75;

const CONTINUATION_POINT_LEN: usize = 32;

fn hashid(name: &str) -> u32 {
    let digest = md5::compute(name.as_bytes());
    u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]) & 0x0FFF_FFFF
}

/// Decode a continuation point (big-endian counter) into a chunk index.
pub fn continuation_point_index(continuation_point: &[u8]) -> usize {
    continuation_point.iter().fold(0usize, |acc, &b| (acc << 8) | b as usize)
}

pub fn continuation_point_for(index: usize) -> Vec<u8> {
    let mut cp = vec![0u8; CONTINUATION_POINT_LEN];
    let bytes = index.to_be_bytes();
    cp[CONTINUATION_POINT_LEN - bytes.len()..].copy_from_slice(&bytes);
    cp
}

pub fn chunk_dependencies(
    all: Vec<TypeIdentifierWithSize>,
    continuation_point: &[u8],
) -> GetTypeDependenciesOut {
    if all.len() < MAX_DEPENDENCIES_PER_REPLY {
        return GetTypeDependenciesOut { dependent_typeids: all, continuation_point: Vec::new() };
    }
    let index = continuation_point_index(continuation_point);
    let start = index * MAX_DEPENDENCIES_PER_REPLY;
    let end = (start + MAX_DEPENDENCIES_PER_REPLY).min(all.len());
    let dependent_typeids = all.get(start..end).map(<[_]>::to_vec).unwrap_or_default();
    let next = if start + MAX_DEPENDENCIES_PER_REPLY > all.len() {
        Vec::new()
    } else {
        continuation_point_for(index + 1)
    };
    GetTypeDependenciesOut { dependent_typeids, continuation_point: next }
}

fn write_nonprimitive_seq<F>(
    s: &mut Xcdr2Serializer,
    count: usize,
    write_elems: F,
) -> Result<(), CdrError>
where
    F: FnOnce(&mut Xcdr2Serializer) -> Result<(), CdrError>,
{
    let dheader_pos = s.reserve_dheader();
    let start = s.position();
    s.serialize_u32(count as u32)?;
    write_elems(s)?;
    let len = (s.position() - start) as u32;
    s.write_dheader_at(dheader_pos, len);
    Ok(())
}

/// Consume a non-primitive sequence DHEADER and return the element count.
fn read_nonprimitive_seq_count(d: &mut Xcdr2Deserializer) -> Result<usize, CdrError> {
    d.begin_struct()?; // sequence DHEADER
    Ok(d.deserialize_u32()? as usize)
}

fn write_type_ids(s: &mut Xcdr2Serializer, type_ids: &[TypeIdentifier]) -> Result<(), CdrError> {
    write_nonprimitive_seq(s, type_ids.len(), |s| {
        for type_id in type_ids {
            type_id.serialize_into(s.buffer_mut());
        }
        Ok(())
    })
}

fn read_type_ids(d: &mut Xcdr2Deserializer) -> Result<Vec<TypeIdentifier>, CdrError> {
    let count = read_nonprimitive_seq_count(d)?;
    // Grow on demand; never pre-allocate from an untrusted wire count.
    let mut type_ids = Vec::new();
    for _ in 0..count {
        let pos = d.get_position();
        let (type_id, consumed) = TypeIdentifier::deserialize(&d.get_data()[pos..])
            .map_err(CdrError::DeserializationError)?;
        d.set_position(pos + consumed);
        type_ids.push(type_id);
    }
    Ok(type_ids)
}

fn write_octet_seq(s: &mut Xcdr2Serializer, bytes: &[u8]) -> Result<(), CdrError> {
    s.serialize_u32(bytes.len() as u32)?;
    s.buffer_mut().extend_from_slice(bytes);
    Ok(())
}

fn read_octet_seq(d: &mut Xcdr2Deserializer) -> Result<Vec<u8>, CdrError> {
    let len = d.deserialize_u32()? as usize;
    let pos = d.get_position();
    let data = d.get_data();
    if pos + len > data.len() {
        return Err(CdrError::InsufficientData);
    }
    let out = data[pos..pos + len].to_vec();
    d.set_position(pos + len);
    Ok(out)
}

fn read_mutable_members<F>(d: &mut Xcdr2Deserializer, mut handle: F) -> Result<(), CdrError>
where
    F: FnMut(&mut Xcdr2Deserializer, u32) -> Result<bool, CdrError>,
{
    let (object_size, start) = d.begin_struct()?;
    let end = start + object_size as usize;
    while d.get_position() < end {
        let (member_id, member_length) = d.read_member_header()?;
        let value_start = d.get_position();
        if !handle(d, member_id)? {
            d.set_position(value_start);
            d.skip(member_length as usize)?;
        }
    }
    d.end_struct(object_size, start)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SampleIdentity {
    pub writer_guid: Guid,
    pub sequence_number: SequenceNumber,
}

impl SampleIdentity {
    pub fn new(writer_guid: Guid, sequence_number: SequenceNumber) -> Self {
        Self { writer_guid, sequence_number }
    }

    fn write(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        s.buffer_mut().extend_from_slice(&self.writer_guid.to_bytes());
        s.serialize_i32(self.sequence_number.high)?;
        s.serialize_u32(self.sequence_number.low)?;
        Ok(())
    }

    fn read(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let pos = d.get_position();
        let data = d.get_data();
        if pos + 16 > data.len() {
            return Err(CdrError::InsufficientData);
        }
        let mut guid_bytes = [0u8; 16];
        guid_bytes.copy_from_slice(&data[pos..pos + 16]);
        d.set_position(pos + 16);
        let high = d.deserialize_i32()?;
        let low = d.deserialize_u32()?;
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
    fn write(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        self.request_id.write(s)?;
        s.serialize_string(&self.instance_name)
    }

    fn read(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let request_id = SampleIdentity::read(d)?;
        let instance_name = d.deserialize_string()?;
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
    fn write(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        self.related_request_id.write(s)?;
        s.serialize_i32(self.remote_exception_code)
    }

    fn read(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let related_request_id = SampleIdentity::read(d)?;
        let remote_exception_code = d.deserialize_i32()?;
        Ok(Self { related_request_id, remote_exception_code })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetTypeDependenciesIn {
    pub type_ids: Vec<TypeIdentifier>,
    pub continuation_point: Vec<u8>,
}

impl GetTypeDependenciesIn {
    fn write(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        let top = s.begin_struct()?;
        s.write_member_with_lc(hashid("type_ids"), false, LcHint::Auto, |s| {
            write_type_ids(s, &self.type_ids)
        })?;
        s.write_member_with_lc(hashid("continuation_point"), false, LcHint::Auto, |s| {
            write_octet_seq(s, &self.continuation_point)
        })?;
        s.end_struct(top)
    }

    fn read(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let mut type_ids = Vec::new();
        let mut continuation_point = Vec::new();
        let id_type_ids = hashid("type_ids");
        let id_cp = hashid("continuation_point");
        read_mutable_members(d, |d, member_id| {
            if member_id == id_type_ids {
                type_ids = read_type_ids(d)?;
            } else if member_id == id_cp {
                continuation_point = read_octet_seq(d)?;
            } else {
                return Ok(false);
            }
            Ok(true)
        })?;
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
    fn write(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        let top = s.begin_struct()?;
        s.write_member_with_lc(hashid("dependent_typeids"), false, LcHint::Auto, |s| {
            write_nonprimitive_seq(s, self.dependent_typeids.len(), |s| {
                for dep in &self.dependent_typeids {
                    dep.serialize_into(s.buffer_mut());
                }
                Ok(())
            })
        })?;
        s.write_member_with_lc(hashid("continuation_point"), false, LcHint::Auto, |s| {
            write_octet_seq(s, &self.continuation_point)
        })?;
        s.end_struct(top)
    }

    fn read(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let mut dependent_typeids = Vec::new();
        let mut continuation_point = Vec::new();
        let id_deps = hashid("dependent_typeids");
        let id_cp = hashid("continuation_point");
        read_mutable_members(d, |d, member_id| {
            if member_id == id_deps {
                let count = read_nonprimitive_seq_count(d)?;
                dependent_typeids = Vec::new();
                for _ in 0..count {
                    let pos = d.get_position();
                    let (dep, consumed) = TypeIdentifierWithSize::deserialize(&d.get_data()[pos..])
                        .map_err(CdrError::DeserializationError)?;
                    d.set_position(pos + consumed);
                    dependent_typeids.push(dep);
                }
            } else if member_id == id_cp {
                continuation_point = read_octet_seq(d)?;
            } else {
                return Ok(false);
            }
            Ok(true)
        })?;
        Ok(Self { dependent_typeids, continuation_point })
    }
}

/// `getTypes` input: the type ids whose TypeObjects are requested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetTypesIn {
    pub type_ids: Vec<TypeIdentifier>,
}

impl GetTypesIn {
    fn write(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        let top = s.begin_struct()?;
        s.write_member_with_lc(hashid("type_ids"), false, LcHint::Auto, |s| {
            write_type_ids(s, &self.type_ids)
        })?;
        s.end_struct(top)
    }

    fn read(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let mut type_ids = Vec::new();
        let id_type_ids = hashid("type_ids");
        read_mutable_members(d, |d, member_id| {
            if member_id == id_type_ids {
                type_ids = read_type_ids(d)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })?;
        Ok(Self { type_ids })
    }
}

/// `getTypes` output: each requested type id paired with its TypeObject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetTypesOut {
    pub types: Vec<(TypeIdentifier, TypeObject)>,
}

impl GetTypesOut {
    fn write(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        let top = s.begin_struct()?;
        s.write_member_with_lc(hashid("types"), false, LcHint::Auto, |s| {
            write_nonprimitive_seq(s, self.types.len(), |s| {
                for (type_id, type_object) in &self.types {
                    type_id.serialize_into(s.buffer_mut());
                    type_object.serialize_into(s.buffer_mut());
                }
                Ok(())
            })
        })?;
        s.end_struct(top)
    }

    fn read(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let mut types = Vec::new();
        let id_types = hashid("types");
        read_mutable_members(d, |d, member_id| {
            if member_id == id_types {
                let count = read_nonprimitive_seq_count(d)?;
                types = Vec::new();
                for _ in 0..count {
                    let mut pos = d.get_position();
                    let (type_id, consumed) = TypeIdentifier::deserialize(&d.get_data()[pos..])
                        .map_err(CdrError::DeserializationError)?;
                    pos += consumed;
                    let (type_object, consumed) = TypeObject::deserialize(&d.get_data()[pos..])
                        .map_err(CdrError::DeserializationError)?;
                    pos += consumed;
                    d.set_position(pos);
                    types.push((type_id, type_object));
                }
                Ok(true)
            } else {
                Ok(false)
            }
        })?;
        Ok(Self { types })
    }
}

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

    fn write(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        let top = s.begin_struct()?;
        s.serialize_i32(self.discriminator())?;
        match self {
            TypeLookupCall::GetTypes(v) => v.write(s)?,
            TypeLookupCall::GetTypeDependencies(v) => v.write(s)?,
        }
        s.end_struct(top)
    }

    fn read(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let (object_size, start) = d.begin_struct()?;
        let disc = d.deserialize_i32()?;
        let value = match disc {
            TYPE_LOOKUP_GETTYPES_HASH => TypeLookupCall::GetTypes(GetTypesIn::read(d)?),
            TYPE_LOOKUP_GETTYPE_DEPENDENCIES_HASH => {
                TypeLookupCall::GetTypeDependencies(GetTypeDependenciesIn::read(d)?)
            }
            other => {
                return Err(CdrError::DeserializationError(format!(
                    "Unknown TypeLookup call discriminator: 0x{:08x}",
                    other
                )))
            }
        };
        d.end_struct(object_size, start)?;
        Ok(value)
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

    fn write(&self, s: &mut Xcdr2Serializer) -> Result<(), CdrError> {
        let top = s.begin_struct()?;
        s.serialize_i32(self.discriminator())?;
        let result = s.begin_struct()?;
        s.serialize_i32(RETCODE_OK)?;
        match self {
            TypeLookupReturn::GetTypes(v) => v.write(s)?,
            TypeLookupReturn::GetTypeDependencies(v) => v.write(s)?,
        }
        s.end_struct(result)?;
        s.end_struct(top)
    }

    fn read(d: &mut Xcdr2Deserializer) -> Result<Self, CdrError> {
        let (object_size, start) = d.begin_struct()?;
        let disc = d.deserialize_i32()?;
        let (result_size, result_start) = d.begin_struct()?;
        let _retcode = d.deserialize_i32()?;
        let value = match disc {
            TYPE_LOOKUP_GETTYPES_HASH => TypeLookupReturn::GetTypes(GetTypesOut::read(d)?),
            TYPE_LOOKUP_GETTYPE_DEPENDENCIES_HASH => {
                TypeLookupReturn::GetTypeDependencies(GetTypeDependenciesOut::read(d)?)
            }
            other => {
                return Err(CdrError::DeserializationError(format!(
                    "Unknown TypeLookup return discriminator: 0x{:08x}",
                    other
                )))
            }
        };
        d.end_struct(result_size, result_start)?;
        d.end_struct(object_size, start)?;
        Ok(value)
    }
}

/// A TypeLookup request sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeLookupRequest {
    pub header: RequestHeader,
    pub data: TypeLookupCall,
}

impl TypeLookupRequest {
    pub fn serialize(&self) -> Vec<u8> {
        let mut s = Xcdr2Serializer::new(true, ExtensibilityKind::Final);
        let write = |s: &mut Xcdr2Serializer| -> Result<(), CdrError> {
            s.write_encapsulation_header()?;
            self.header.write(s)?;
            self.data.write(s)
        };
        let _ = write(&mut s);
        s.into_buffer()
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        let mut d = Xcdr2Deserializer::new(data).map_err(|e| format!("{:?}", e))?;
        let header = RequestHeader::read(&mut d).map_err(|e| format!("{:?}", e))?;
        let data = TypeLookupCall::read(&mut d).map_err(|e| format!("{:?}", e))?;
        Ok(Self { header, data })
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
        let mut s = Xcdr2Serializer::new(true, ExtensibilityKind::Final);
        let write = |s: &mut Xcdr2Serializer| -> Result<(), CdrError> {
            s.write_encapsulation_header()?;
            self.header.write(s)?;
            self.data.write(s)
        };
        let _ = write(&mut s);
        s.into_buffer()
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        let mut d = Xcdr2Deserializer::new(data).map_err(|e| format!("{:?}", e))?;
        let header = ReplyHeader::read(&mut d).map_err(|e| format!("{:?}", e))?;
        let data = TypeLookupReturn::read(&mut d).map_err(|e| format!("{:?}", e))?;
        Ok(Self { header, data })
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

    fn dep(seed: &[u8]) -> TypeIdentifierWithSize {
        TypeIdentifierWithSize::new(
            TypeIdentifier::CompleteTypeId(EquivalenceHash::compute(seed)),
            1,
        )
    }

    #[test]
    fn encapsulation_header_is_plain_cdr2_le() {
        let request = TypeLookupRequest {
            header: RequestHeader { request_id: sample_identity(), instance_name: String::new() },
            data: TypeLookupCall::GetTypes(GetTypesIn { type_ids: vec![] }),
        };
        let bytes = request.serialize();
        // PLAIN_CDR2 little-endian encapsulation id (0x0007), options 0x0000.
        assert_eq!(&bytes[0..4], &[0x00, 0x07, 0x00, 0x00]);
    }

    #[test]
    fn continuation_point_roundtrip() {
        for index in [0usize, 1, 2, 75, 1234] {
            let cp = continuation_point_for(index);
            assert_eq!(cp.len(), 32);
            assert_eq!(continuation_point_index(&cp), index);
        }
        assert_eq!(continuation_point_index(&[]), 0);
    }

    #[test]
    fn chunk_dependencies_single_reply_under_limit() {
        let all: Vec<_> = (0..MAX_DEPENDENCIES_PER_REPLY as u8 - 1).map(|i| dep(&[i])).collect();
        let out = chunk_dependencies(all.clone(), &[]);
        assert_eq!(out.dependent_typeids, all);
        assert!(out.continuation_point.is_empty());
    }

    #[test]
    fn chunk_dependencies_pages_in_order_until_drained() {
        let total = MAX_DEPENDENCIES_PER_REPLY * 2 + 10;
        let all: Vec<_> = (0..total as u16).map(|i| dep(&i.to_le_bytes())).collect();

        let mut cp = Vec::new();
        let mut collected = Vec::new();
        let mut rounds = 0;
        loop {
            let out = chunk_dependencies(all.clone(), &cp);
            assert!(out.dependent_typeids.len() <= MAX_DEPENDENCIES_PER_REPLY);
            collected.extend(out.dependent_typeids);
            rounds += 1;
            if out.continuation_point.is_empty() {
                break;
            }
            cp = out.continuation_point;
            assert!(rounds < 10, "continuation must terminate");
        }
        assert_eq!(rounds, 3);
        assert_eq!(collected, all, "paged chunks reassemble to the full list in order");
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

    #[test]
    fn reply_get_types_multi_pair_roundtrip() {
        // Exercise a larger payload to cover EMHEADER length-code selection.
        let type_object = sample_type_object();
        let types: Vec<_> = (0..5u8)
            .map(|i| {
                (
                    TypeIdentifier::CompleteTypeId(EquivalenceHash::compute(&[i])),
                    type_object.clone(),
                )
            })
            .collect();
        let reply = TypeLookupReply {
            header: ReplyHeader { related_request_id: sample_identity(), remote_exception_code: 0 },
            data: TypeLookupReturn::GetTypes(GetTypesOut { types }),
        };
        let bytes = reply.serialize();
        assert_eq!(TypeLookupReply::deserialize(&bytes).unwrap(), reply);
    }
}
