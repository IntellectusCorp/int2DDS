//! INFO submessages for providing context information.
//!
//! This module implements INFO submessages (InfoTimestamp, InfoSource, InfoDestination,
//! InfoReply, InfoReplyIp4) that provide context for subsequent submessages in an RTPS
//! message, such as timestamp, source GUID, destination GUID, and reply locators.

use std::{io, mem};

use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use speedy::{Context, Error, Readable, Reader, Writable, Writer};

use crate::rtps::{
    common::{
        guid::GuidPrefix,
        locator::{Locator, LocatorUDPv4},
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        types::{ProtocolVersion, VendorId},
    },
    messages::submessage_header::SubmessageHeader,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InfoTimestamp {
    timestamp: DateTime<Utc>,
}

#[allow(dead_code)]
impl InfoTimestamp {
    pub(crate) fn new(timestamp: DateTime<Utc>) -> Self {
        Self { timestamp }
    }

    pub(crate) fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Get seconds component (i32) - same as Writable implementation
    pub(crate) fn seconds(&self) -> i32 {
        self.timestamp.timestamp() as i32
    }

    /// Get fraction component (u32) - same as Writable implementation
    pub(crate) fn fraction(&self) -> u32 {
        let nanos = self.timestamp.timestamp_subsec_nanos();
        // DDS-style fraction = (nanos * 2^32) / 1_000_000_000
        (((nanos as u64) << 32) / 1_000_000_000) as u32
    }

    pub(crate) fn length(&self) -> u16 {
        mem::size_of::<u64>() as u16
    }
}

impl<C: Context> Writable<C> for InfoTimestamp {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        let seconds = self.seconds();
        let fraction = self.fraction();

        writer.write_value(&seconds.to_le_bytes())?;
        writer.write_value(&fraction.to_le_bytes())?;
        Ok(())
    }
}

impl<'a, C: Context> Readable<'a, C> for InfoTimestamp {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let seconds: i32 = reader.read_value::<i32>()?;
        let fraction: u32 = reader.read_value::<u32>()?;

        // Convert DDS-style fraction to nanoseconds: (fraction * 1_000_000_000) / 2^32

        let nanos = ((fraction as u64) * 1_000_000_000) >> 32;
        let dt = Utc.timestamp_opt(seconds as i64, nanos as u32).unwrap();

        Ok(Self::new(dt))
    }
}

/// Serialised size of a `Locator`: kind + port + address.
const LOCATOR_WIRE_SIZE: usize = 4 + 4 + 16;

/// How many locators it is safe to reserve for, given what a sender declared and how many
/// bytes are actually left to read them from.
///
/// The declared count is unvalidated wire data, so reserving that many entries lets a small
/// datagram ask for an arbitrary amount of memory. An allocation that large aborts the
/// process instead of unwinding, so it cannot be caught either; the reservation has to be
/// bounded by what the remaining bytes could really contain. An honest count is always
/// smaller than that bound and is therefore still reserved exactly.
fn bounded_locator_capacity(declared: u32, remaining_bytes: usize) -> usize {
    (declared as usize).min(remaining_bytes / LOCATOR_WIRE_SIZE)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InfoReply {
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
}

impl InfoReply {
    pub(crate) fn unicast_locator_list(&self) -> &[Locator] {
        &self.unicast_locator_list
    }

    pub(crate) fn multicast_locator_list(&self) -> &[Locator] {
        &self.multicast_locator_list
    }

    pub(crate) fn deserialize(
        buffer: &Bytes,
        submessage_header: &SubmessageHeader,
    ) -> RtpsResult<Self> {
        let mut cursor = io::Cursor::new(&buffer);
        let map_speedy_err = |p: Error| RtpsError::new(RtpsErrorCode::Io, p.to_string());

        let endianness = submessage_header
            .endianness_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?;

        let num_locators = u32::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let remaining = buffer.len().saturating_sub(cursor.position() as usize);
        let mut unicast_locator_list: Vec<Locator> =
            Vec::with_capacity(bounded_locator_capacity(num_locators, remaining));
        for _i in 0..num_locators {
            let locator = Locator::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
            unicast_locator_list.push(locator);
        }

        // 8.3.7.8.4 Only present when the MulticastFlag is set
        let mut multicast_locator_list: Vec<Locator> = Vec::new();
        if submessage_header
            .multicast_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?
        {
            let num_locators = u32::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
            let remaining = buffer.len().saturating_sub(cursor.position() as usize);
            multicast_locator_list =
                Vec::with_capacity(bounded_locator_capacity(num_locators, remaining));
            for _i in 0..num_locators {
                let locator =
                    Locator::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                        .map_err(map_speedy_err)?;
                multicast_locator_list.push(locator);
            }
        }

        Ok(Self { unicast_locator_list, multicast_locator_list })
    }
}

impl<C: Context> Writable<C> for InfoReply {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_u32(self.unicast_locator_list.len() as u32)?;
        for locator in self.unicast_locator_list() {
            writer.write_value(&locator)?;
        }
        if !self.multicast_locator_list.is_empty() {
            writer.write_u32(self.multicast_locator_list.len() as u32)?;
            for locator in self.unicast_locator_list() {
                writer.write_value(&locator)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InfoReplyIp4 {
    unicast_locator: LocatorUDPv4,
    multicast_locator: Option<LocatorUDPv4>,
}

impl InfoReplyIp4 {
    pub(crate) fn unicast_locator(&self) -> LocatorUDPv4 {
        self.unicast_locator
    }

    pub(crate) fn multicast_locator(&self) -> Option<LocatorUDPv4> {
        self.multicast_locator
    }

    pub(crate) fn deserialize(
        buffer: &Bytes,
        submessage_header: &SubmessageHeader,
    ) -> RtpsResult<Self> {
        let mut cursor = io::Cursor::new(&buffer);
        let map_speedy_err = |p: Error| RtpsError::new(RtpsErrorCode::Io, p.to_string());

        let endianness = submessage_header
            .endianness_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?;

        let unicast_locator =
            LocatorUDPv4::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;

        // 9.4.5.14.1 Only present when the MulticastFlag is set
        let mut multicast_locator = None;
        if submessage_header
            .multicast_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?
        {
            multicast_locator = Some(
                LocatorUDPv4::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                    .map_err(map_speedy_err)?,
            );
        }

        Ok(Self { unicast_locator, multicast_locator })
    }
}

impl<C: Context> Writable<C> for InfoReplyIp4 {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_value(&self.unicast_locator)?;
        if let Some(multicast_locator) = &self.multicast_locator {
            writer.write_value(multicast_locator)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub(crate) struct InfoDestination {
    guid_prefix: GuidPrefix,
}

impl InfoDestination {
    pub(crate) fn new(guid_prefix: GuidPrefix) -> Self {
        Self { guid_prefix }
    }

    pub(crate) fn guid_prefix(&self) -> GuidPrefix {
        self.guid_prefix
    }
    pub(crate) fn length(&self) -> u16 {
        mem::size_of::<GuidPrefix>() as u16
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InfoSource {
    protocol_version: ProtocolVersion,
    vendor_id: VendorId,
    guid_prefix: GuidPrefix,
}

impl InfoSource {
    pub(crate) fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    pub(crate) fn vendor_id(&self) -> VendorId {
        self.vendor_id
    }
    pub(crate) fn guid_prefix(&self) -> GuidPrefix {
        self.guid_prefix
    }
}

impl<C: Context> Writable<C> for InfoSource {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_i32(0)?; // Unused long
        writer.write_value(&self.protocol_version)?;
        writer.write_value(&self.vendor_id)?;
        writer.write_value(&self.guid_prefix)?;

        Ok(())
    }
}

impl<'a, C: Context> Readable<'a, C> for InfoSource {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let _unused_long: i32 = reader.read_value()?;
        let protocol_version: ProtocolVersion = reader.read_value()?;
        let vendor_id: VendorId = reader.read_value()?;
        let guid_prefix: GuidPrefix = reader.read_value()?;

        Ok(Self { protocol_version, vendor_id, guid_prefix })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A locator count is read straight off the wire, so it is whatever a remote sender chose
    /// to put there. Reserving that many entries up front lets a 28-byte datagram ask for tens
    /// of gigabytes, and an allocation failure aborts the process rather than unwinding, so it
    /// cannot even be caught. The reservation has to be bounded by the locators the buffer
    /// could actually hold.
    #[test]
    fn locator_capacity_is_bounded_by_the_bytes_available() {
        // Nothing but the count itself: no locator can follow, so reserve nothing.
        assert_eq!(bounded_locator_capacity(u32::MAX, 0), 0);
        assert_eq!(bounded_locator_capacity(1, 0), 0);

        // A declared count far beyond what the remaining bytes can hold is clamped to them.
        assert_eq!(bounded_locator_capacity(u32::MAX, 100), 100 / LOCATOR_WIRE_SIZE);

        // A partial locator does not count.
        assert_eq!(bounded_locator_capacity(u32::MAX, LOCATOR_WIRE_SIZE - 1), 0);
        assert_eq!(bounded_locator_capacity(u32::MAX, LOCATOR_WIRE_SIZE), 1);

        // An honest count is reserved exactly, so the normal path still allocates once.
        assert_eq!(bounded_locator_capacity(2, 4 * LOCATOR_WIRE_SIZE), 2);
    }

    /// `Locator` is serialised as kind + port + address, and the bound above is only correct
    /// if that is really its wire size.
    #[test]
    fn locator_wire_size_matches_the_serialised_locator() {
        let locator = Locator::new(1, 7400, [0u8; 16]);
        let bytes = locator.write_to_vec_with_ctx(speedy::Endianness::LittleEndian).unwrap();
        assert_eq!(bytes.len(), LOCATOR_WIRE_SIZE);
    }
}
