//! DDS-RPC common types (7.5.1.1.1)

use std::any::{Any, TypeId};

use int2dds::common::instance_handle::InstanceHandle;
use int2dds::dcps::core::error::{DdsError, DdsResult};
use int2dds::dcps::topic::type_support::{DdsType, SerializationFormat, TypeSupport};
use int2dds::rtps::common::types::SerializedData;
use int2dds::serialize::xcdr::ExtensibilityKind;
use int2dds::topic::sql::ast::Parameter;

// Re-export DDS layer types used for constructing SampleIdentity
pub use int2dds::rtps::common::guid::Guid;
pub use int2dds::rtps::common::sequence::SequenceNumber;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleIdentity {
    pub writer_guid: Guid,
    pub sequence_number: SequenceNumber,
}

// QueryCondition support for SampleIdentity (7.2.3)
//
// writer_guid is compared as a 32-char hex string.
// sequence_number is compared via nested .high (i32) and .low (u32) fields.
// This avoids modifying dds crate types while enabling SQL filter expressions
// like: "header.related_request_id.writer_guid = %0
//        AND header.related_request_id.sequence_number.high = %1
//        AND header.related_request_id.sequence_number.low = %2"

#[derive(Default)]
pub struct SampleIdentityTypeSupport;

impl TypeSupport for SampleIdentityTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<SampleIdentity>()
    }

    fn get_type_name(&self) -> &str {
        "SampleIdentity"
    }

    fn get_field_value(&self, data: &dyn Any, field_path: &str) -> DdsResult<Parameter> {
        let typed = data.downcast_ref::<SampleIdentity>().ok_or(DdsError::BadParameter)?;

        if let Some((first, rest)) = field_path.split_once('.') {
            match first {
                "sequence_number" => match rest {
                    "high" => Ok(Parameter::IntegerValue(typed.sequence_number.high)),
                    "low" => Ok(Parameter::IntegerValue(typed.sequence_number.low as i32)),
                    _ => Err(DdsError::Error(format!("Field '{}' not found", field_path))),
                },
                _ => Err(DdsError::Error(format!("Field '{}' not found", first))),
            }
        } else {
            match field_path {
                "writer_guid" => Ok(Parameter::String(typed.writer_guid.to_hex_string())),
                _ => Err(DdsError::Error(format!("Field '{}' not found", field_path))),
            }
        }
    }

    fn has_field(&self, field_path: &str) -> bool {
        matches!(field_path, "writer_guid" | "sequence_number.high" | "sequence_number.low")
    }

    fn serialize(&self, _: &dyn Any, _: Option<&SerializationFormat>) -> DdsResult<SerializedData> {
        Err(DdsError::Error("SampleIdentity is not a standalone topic type".to_string()))
    }

    fn deserialize(&self, _: &[u8], _: Option<&SerializationFormat>) -> DdsResult<Box<dyn Any>> {
        Err(DdsError::Error("SampleIdentity is not a standalone topic type".to_string()))
    }

    fn serialize_key(&self, _: &dyn Any) -> DdsResult<SerializedData> {
        Err(DdsError::Error("SampleIdentity is not a standalone topic type".to_string()))
    }

    fn deserialize_key(&self, _: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        Err(DdsError::Error("SampleIdentity is not a standalone topic type".to_string()))
    }

    fn compute_key(&self, _: &dyn Any) -> InstanceHandle {
        InstanceHandle::NIL
    }

    fn is_compute_key_provided(&self) -> bool {
        false
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        ExtensibilityKind::Final
    }
}

impl DdsType for SampleIdentity {
    type TypeSupport = SampleIdentityTypeSupport;
}

pub type InstanceName = String; // max 255 chars

#[derive(Debug, Clone)]
pub struct RequestHeader {
    pub request_id: SampleIdentity,
    pub instance_name: InstanceName,
}

#[derive(Debug, Clone)]
pub struct ReplyHeader {
    pub related_request_id: SampleIdentity,
    pub remote_ex: RemoteExceptionCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum RemoteExceptionCode {
    Ok = 0,
    Unsupported = 1,
    InvalidArgument = 2,
    OutOfResources = 3,
    UnknownOperation = 4,
    UnknownException = 5,
}

/// Trait for types that carry a RequestHeader (Basic Service Mapping).
/// User-defined request types must implement this.
pub trait RpcRequest {
    fn header(&self) -> &RequestHeader;
    fn header_mut(&mut self) -> &mut RequestHeader;
}

/// Trait for types that carry a ReplyHeader (Basic Service Mapping).
/// User-defined reply types must implement this.
pub trait RpcReply {
    fn header(&self) -> &ReplyHeader;
    fn header_mut(&mut self) -> &mut ReplyHeader;
}

/// Default case in Call/Return unions for unrecognized operations (7.5.1.1.6, 7.5.1.1.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownOperation;

/// Default case in Result unions for unrecognized exceptions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownException;

/// Dummy member for In/Out structs with no parameters (7.5.1.1.4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnusedMember;

/// Re-export from DDS layer (7.5.1.1.5 Rule 3)
pub use int2dds::dcps::core::error::RETCODE_OK;

#[cfg(test)]
mod tests {
    use super::*;
    use int2dds::rtps::common::entity_id::EntityId;

    fn make_test_identity() -> SampleIdentity {
        let prefix = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c];
        let entity_id = EntityId {
            entity_key: [0x0d, 0x0e, 0x0f],
            entity_kind: int2dds::rtps::common::entity_kind::EntityKind(0x10),
        };
        SampleIdentity {
            writer_guid: Guid::new(prefix, entity_id),
            sequence_number: SequenceNumber::new(0, 42),
        }
    }

    #[test]
    fn test_get_field_value_writer_guid() {
        let id = make_test_identity();
        let val = id.get_field_value("writer_guid").unwrap();
        assert_eq!(val, Parameter::String("0102030405060708090a0b0c0d0e0f10".to_string()));
    }

    #[test]
    fn test_get_field_value_sequence_number_high() {
        let id = make_test_identity();
        let val = id.get_field_value("sequence_number.high").unwrap();
        assert_eq!(val, Parameter::IntegerValue(0));
    }

    #[test]
    fn test_get_field_value_sequence_number_low() {
        let id = make_test_identity();
        let val = id.get_field_value("sequence_number.low").unwrap();
        assert_eq!(val, Parameter::IntegerValue(42));
    }

    #[test]
    fn test_has_field() {
        let id = make_test_identity();
        assert!(id.has_field("writer_guid").unwrap());
        assert!(id.has_field("sequence_number.high").unwrap());
        assert!(id.has_field("sequence_number.low").unwrap());
        assert!(!id.has_field("nonexistent").unwrap());
        assert!(!id.has_field("sequence_number").unwrap());
    }

    #[test]
    fn test_get_field_value_invalid_field() {
        let id = make_test_identity();
        assert!(id.get_field_value("nonexistent").is_err());
        assert!(id.get_field_value("sequence_number.invalid").is_err());
        assert!(id.get_field_value("writer_guid.prefix").is_err());
    }
}
