//! RTPS parameter types for discovery and QoS communication.
//!
//! This module defines the Parameter and ParameterList types used in RTPS protocol
//! for encoding QoS policies, discovery information, and inline QoS in DATA messages.
//! Parameters use a type-length-value (TLV) encoding scheme.

use smallvec::SmallVec;
use speedy::{Context, Readable, Reader, Writable, Writer};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::{
    infrastructure::qos_policy::{
        DataRepresentationQosPolicy, DestinationOrderQosPolicy, DurabilityQosPolicy,
        DurabilityServiceQosPolicy, HistoryQosPolicy, LivelinessQosPolicy, OwnershipQosPolicy,
        PresentationQosPolicy, ReliabilityQosPolicy, ResourceLimitsQosPolicy,
        TypeConsistencyEnforcementQosPolicy,
    },
    rtps::{
        builtin::data::content_filtered_topic::{ContentFilterInfo, ContentFilterProperty},
        common::{
            guid::Guid,
            locator::Locator,
            time::RtpsDuration,
            types::{Count, GroupInfo, ProtocolVersion, VendorId},
        },
    },
    xtypes::{TypeIdentifier, TypeObject},
};

// pub type ParameterId = i16;
/*
Table 9.6 - ParameterId subspaces
ParameterId & 0x8000 = 1 -> Vendor-specific ParameterID
ParameterId & 0x4000 = 0 -> If unrecognized, can be skipped
ParameterId & 0x4000 = 1 -> If unrecognized, treat as error (ignore entire message if in HeaderExtension, ignore only submessage if in Submessage)
*/
/*
DDS-XTypes 1.1 ~ 1.3 - x0069, 0x0072, 0x0073, 0x0074, 0x0075, 0x0076
DDS-Security 1.1 - 0x1000 ~ 0x1FFF, 0x5000 ~ 0x5FFF
DDS-RPC 1.0 - 0x0080, 0x0081, 0x0082, 0x0083
*/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Readable, Writable, EnumIter)]
#[repr(u16)]
pub enum ParameterId {
    /* From Table 9.18 of DDS-RTPS 2.5 */
    PidPad = 0x0000,
    PidSentinel = 0x0001,
    PidUserData = 0x002c,
    PidTopicName = 0x0005,
    PidTypeName = 0x0007,
    PidGroupData = 0x002d,
    PidTopicData = 0x002e,
    PidDurability = 0x001d,
    PidDurabilityService = 0x001e,
    PidDeadline = 0x0023,
    PidLatencyBudget = 0x0027,
    PidLiveliness = 0x001b,
    PidReliability = 0x001a,
    PidLifespan = 0x002b,
    PidDestinationOrder = 0x0025,
    PidHistory = 0x0040,
    PidResourceLimits = 0x0041,
    PidOwnership = 0x001f,
    PidOwnershipStrength = 0x0006,
    PidPresentation = 0x0021,
    PidPartition = 0x0029,
    PidTimeBasedFilter = 0x0004,
    PidTransportPriority = 0x0049,
    PidDomainId = 0x000f,
    PidDomainTag = 0x4014,
    PidProtocolVersion = 0x0015,
    PidVendorId = 0x0016,
    PidUnicastLocator = 0x002f,
    PidMulticastLocator = 0x0030,
    PidDefaultUnicastLocator = 0x0031,
    PidDefaultMulticastLocator = 0x0048,
    PidMetatrafficUnicastLocator = 0x0032,
    PidMetatrafficMulticastLocator = 0x0033,
    PidExpectsInlineQos = 0x0043,
    PidParticipantManualLivelinessCount = 0x0034,
    PidParticipantLeaseDuration = 0x0002,
    PidContentFilterProperty = 0x0035,
    PidParticipantGuid = 0x0050,
    PidGroupGuid = 0x0052,
    PidGroupEntityId = 0x0053,
    PidBuiltinEndpointSet = 0x0058,
    PidBuiltinEndpointQos = 0x0077,
    PidPropertyList = 0x0059,
    PidTypeMaxSizeSerialized = 0x0060,
    PidEntityName = 0x0062,
    PidEndpointGuid = 0x005a,

    /* From table 9.20 of DDS-RTPS 2.5 - inline QoS only */
    PidContentFilterInfo = 0x0055,
    PidCoherentSet = 0x0056,
    PidDirectedWrite = 0x0057,
    PidOriginalWriterInfo = 0x0061,
    PidGroupCoherentSet = 0x0063,
    PidGroupSeqNum = 0x0064,
    PidWriterGroupInfo = 0x0065,
    PidSecureWriterGroupInfo = 0x0066,
    PidKeyHash = 0x0070,
    PidStatusInfo = 0x0071,
    PidTypeObject = 0x0072,

    /* From table 9.25 of DDS-RTPS 2.5 - Deprecated */
    PidPersistence = 0x0003,
    PidTypeChecksum = 0x0008,
    PidType2Name = 0x0009,
    PidType2Checksum = 0x000a,
    PidExpectsAck = 0x0010,
    PidManagerKey = 0x0012,
    PidSendQueueSize = 0x0013,
    PidReliabilityEnabled = 0x0014,
    PidVargappsSequenceNumberLast = 0x0017,
    PidRecvQueueSize = 0x0018,
    PidMulticastIpaddress = 0x0011,
    PidDefaultUnicastIpaddress = 0x000c,
    PidDefaultUnicastPort = 0x000e,
    PidMetatrafficUnicastIpaddress = 0x0045,
    PidMetatrafficUnicastPort = 0x000d,
    PidMetatrafficMulticastIpaddress = 0x000b,
    PidMetatrafficMulticastPort = 0x0046,
    PidParticipantBuiltinEndpoints = 0x0044,
    PidParticipantEntityId = 0x0051,

    PidDataRepresentation = 0x0073,

    /* DDS-XTypes 1.3 */
    PidTypeInformation = 0x0069,
    PidTypeConsistencyEnforcement = 0x0074,

    UNKNOWN = 0xffff,
}

/// Helper function to convert u16 value to ParameterId
pub fn u16_to_parameter_id(value: u16) -> ParameterId {
    for pid in ParameterId::iter() {
        if pid as u16 == value {
            return pid;
        }
    }
    ParameterId::UNKNOWN
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    parameter_id: ParameterId,
    // Multiple of 4
    length: i16,
    // [u8, length]
    value: SmallVec<[u8; 16]>,
}

impl Parameter {
    pub fn new<V>(parameter_id: ParameterId, value: V) -> Self
    where
        V: Into<SmallVec<[u8; 16]>>,
    {
        let value = value.into();
        Self { parameter_id, length: value.len() as i16, value }
    }
    pub fn parameter_id(&self) -> ParameterId {
        self.parameter_id
    }
    pub fn length(&self) -> i16 {
        self.length
    }
    pub fn value(&self) -> &[u8] {
        &self.value
    }
}

impl<'a, C: Context> Readable<'a, C> for Parameter {
    #[allow(clippy::needless_maybe_sized)]
    fn read_from<T: ?Sized + Reader<'a, C>>(reader: &mut T) -> Result<Self, C::Error> {
        let parameter_id = reader.read_u16()?;
        let length = reader.read_u16()? as i16;
        let mut value: Vec<u8> = vec![0u8; length as usize];
        reader.read_bytes(&mut value)?;
        let value = SmallVec::from_vec(value);

        for pid in ParameterId::iter() {
            if pid as u16 == parameter_id {
                return Ok(Parameter { parameter_id: pid, length, value });
            }
        }
        //TODO: Exception handling for unhandled parameters
        Ok(Parameter { parameter_id: ParameterId::UNKNOWN, length, value })
    }
}

impl<C: Context> Writable<C> for Parameter {
    #[inline]
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        let length = self.value.len();
        let pad = if !length.is_multiple_of(4) { 4 - (length % 4) } else { 0 };

        //#[repr(u16)] does not work.
        writer.write_value(&(self.parameter_id as u16))?;
        writer.write_u16((length + pad) as u16)?;
        writer.write_bytes(&self.value)?;

        for _ in 0..pad {
            writer.write_u8(0x00)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParameterList {
    parameter: SmallVec<[Parameter; 4]>,
}
impl ParameterList {
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameter
    }

    pub fn add_parameter(&mut self, param: Parameter) {
        self.parameter.push(param);
    }

    pub fn length(&self) -> i16 {
        let mut total_length = 0;

        for param in &self.parameter {
            if param.parameter_id() == ParameterId::PidSentinel {
                break;
            }

            total_length += 4; // parameter_id(2) + length(2)
            let value_len = param.value().len();
            total_length += value_len;

            let padding = if value_len % 4 != 0 { 4 - (value_len % 4) } else { 0 };
            total_length += padding;
        }

        // Sentinel parameter (parameter_id(2) + length(2))
        total_length += 4;

        total_length as i16
    }

    pub fn iter(&self) -> impl Iterator<Item = &Parameter> + '_ {
        self.parameter.iter()
    }

    pub(crate) fn retain_parameters<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&Parameter) -> bool,
    {
        self.parameter.retain(|param| predicate(param));
    }
}

impl<'a, C: Context> Readable<'a, C> for ParameterList {
    #[allow(clippy::needless_maybe_sized)]
    fn read_from<T: ?Sized + Reader<'a, C>>(reader: &mut T) -> Result<Self, C::Error> {
        let mut parameter: SmallVec<[Parameter; 4]> = SmallVec::new();
        loop {
            let param = Parameter::read_from(reader)?;

            if param.parameter_id() == ParameterId::PidSentinel {
                break;
            }
            parameter.push(param);
        }

        Ok(ParameterList { parameter })
    }
}

impl<C: Context> Writable<C> for ParameterList {
    #[inline]
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        for param in &self.parameter {
            writer.write_value(param)?;
        }
        writer.write_u16(0x0001)?; // ParameterId: PidSentinel
        writer.write_u16(0x0000)?; // length = 0

        Ok(())
    }
}

pub type KeyHash = [u8; 16];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Readable, Writable)]
pub struct StatusInfo(u32);

impl StatusInfo {
    pub const DISPOSED: u32 = 0x01;
    pub const UNREGISTERED: u32 = 0x02;
    pub const FILTERED: u32 = 0x04;
    pub const MASK: u32 = Self::DISPOSED | Self::UNREGISTERED | Self::FILTERED;

    pub fn new(flags: u32) -> Self {
        StatusInfo(flags & Self::MASK) // Remove reserved bits
    }
    pub fn flags(self) -> u32 {
        self.0
    }
    pub fn disposed(self) -> bool {
        self.0 & Self::DISPOSED != 0
    }
    pub fn unregistered(self) -> bool {
        self.0 & Self::UNREGISTERED != 0
    }
    pub fn filtered(self) -> bool {
        self.0 & Self::FILTERED != 0
    }
    pub fn to_bytes(self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub struct Property {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub enum ParameterValue<'a> {
    ProtocolVersion(ProtocolVersion),
    VendorId(VendorId),
    DomainId(u32),
    Locator(Locator),
    ExpectsInlineQos(bool),
    ParticipantLeaseDuration(RtpsDuration),
    ParticipantGuid(Guid),
    BuiltinEndpointSet(u32),
    EntityName(String),
    UserData(&'a [u8]),
    UserDataOwned(Vec<u8>),
    PropertyList(Vec<Property>),
    TopicName(String),
    TypeName(String),
    Reliability(ReliabilityQosPolicy),
    Durability(DurabilityQosPolicy),
    Deadline(RtpsDuration),
    LatencyBudget(RtpsDuration),
    Ownership(OwnershipQosPolicy),
    OwnershipStrength(u32),
    Liveliness(LivelinessQosPolicy),
    Partition(Vec<String>),
    GroupData(&'a [u8]),
    TopicData(&'a [u8]),
    TimeBasedFilter(RtpsDuration),
    Presentation(PresentationQosPolicy),
    ContentFilter(ContentFilterInfo),
    ContentFilterProperty(ContentFilterProperty),
    WriterGroupInfo(GroupInfo<'a>),
    DestinationOrder(DestinationOrderQosPolicy),
    HistoryQosPolicy(HistoryQosPolicy),
    ResourceLimits(ResourceLimitsQosPolicy),
    TransportPriority(u32),
    Lifespan(RtpsDuration),
    DurabilityService(DurabilityServiceQosPolicy),
    DataRepresentation(DataRepresentationQosPolicy),
    TypeInformation(TypeIdentifier),
    TypeConsistencyEnforcement(TypeConsistencyEnforcementQosPolicy),
    TypeObject(TypeObject),
    KeyHash([u8; 16]),
    StatusInfo(StatusInfo),
    MaxSerializedSize(u32),
    Sentinel,
    Count(Count),
    Unknown(&'a [u8]),
}

// Parameter struct for PL-CDR (for serialization)
#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub struct PlCdrParameter<'a> {
    pub id: ParameterId,
    pub value: ParameterValue<'a>,
}
