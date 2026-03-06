//! DDS-RPC common types (7.5.1.1.1)

use int2dds::rtps::common::guid::Guid;
use int2dds::rtps::common::sequence::SequenceNumber;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleIdentity {
    pub writer_guid: Guid,
    pub sequence_number: SequenceNumber,
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
