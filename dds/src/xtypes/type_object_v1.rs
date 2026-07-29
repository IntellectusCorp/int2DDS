//! Legacy ("v1") TypeObject decoding for interoperability
//!
//! `PID_TYPE_OBJECT` (0x0072) using the
//! DDS-XTypes **v1** `TypeObject` representation, which predates the v1.2+
//! `Minimal`/`Complete` `TypeObject` ([`super::TypeObject`]) that int2DDS uses
//! natively. The v1 representation is a `@extensibility(MUTABLE)` aggregate
//! serialized as XCDR1 PL_CDR: each member is framed by a parameter header
//! (short, or `PID_EXTENDED` long form), so unknown members are length-skippable.
//!
//! ```text
//! TypeObject (MUTABLE)               // v1, OMG DDS-XTypes 1.0/1.1 Annex
//!   member 0: TypeLibrary library    // @Shared, sequence<TypeLibraryElement>
//!   member 1: TypeIdSeq the_type
//! ```
//!
//! # Scope
//!
//! This module decodes the **outer MUTABLE framing** exactly (it positively
//! identifies a payload as v1 and walks every member by length), and recovers
//! type / member **names** from the member regions on a best-effort basis. The
//! inner `EXTENSIBLE`/union layouts (`TypeProperty`, `_TypeId`,
//! `TypeLibraryElement`) are preserved as [`TypeObjectV1::raw`] rather than
//! decoded, so no unvalidated structural claim is made. This is sufficient to
//! stop the "Unknown TypeObject kind" discovery noise and to retain the type
//! names for introspection; a precise inner decode can be layered on top once
//! validated against captured bytes.

use speedy::{Context, Readable, Reader, Writable, Writer};

use crate::serialize::cdr::{CdrDeserializer, PlCdrMemberHeader};
use crate::serialize::DeserializerReader;

/// Maximum members walked in a single MUTABLE aggregate (runaway guard).
const MAX_MEMBERS: usize = 1024;
/// `ObjectName` is `string<256>` in the v1 representation.
const MAX_NAME_LEN: usize = 256;

/// A decoded legacy (v1) `TypeObject` as received from a remote participant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeObjectV1 {
    pub raw: Vec<u8>,
    pub little_endian: bool,
    pub type_names: Vec<String>,
}

impl TypeObjectV1 {
    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), String> {
        let (body, forced_le) = match recognized_encapsulation(data) {
            Some(le) => (&data[4..], Some(le)),
            None => (data, None),
        };

        let (members, little_endian) = match forced_le {
            Some(le) => (walk_mutable(body, le)?, le),
            None => match walk_mutable(body, true) {
                Ok(members) => (members, true),
                Err(le_err) => match walk_mutable(body, false) {
                    Ok(members) => (members, false),
                    Err(_) => return Err(le_err),
                },
            },
        };

        let mut type_names = Vec::new();
        for region in &members {
            scan_cdr_strings(region, little_endian, &mut type_names);
        }

        Ok((Self { raw: data.to_vec(), little_endian, type_names }, data.len()))
    }

    pub fn serialize(&self) -> Vec<u8> {
        self.raw.clone()
    }
}

fn recognized_encapsulation(data: &[u8]) -> Option<bool> {
    if data.len() < 4 {
        return None;
    }
    match u16::from_be_bytes([data[0], data[1]]) {
        0x0000 | 0x0002 => Some(false), // CDR_BE / PL_CDR_BE
        0x0001 | 0x0003 => Some(true),  // CDR_LE / PL_CDR_LE
        _ => None,
    }
}

/// Walk a MUTABLE aggregate, returning each member's raw value bytes.
fn walk_mutable(body: &[u8], little_endian: bool) -> Result<Vec<Vec<u8>>, String> {
    let mut de = CdrDeserializer::new_without_header(body, little_endian);
    let mut members = Vec::new();

    loop {
        if members.len() > MAX_MEMBERS {
            return Err("v1 TypeObject: too many members".to_string());
        }
        // Need at least a 4-byte parameter header to continue.
        if de.check_available(4).is_err() {
            break;
        }
        let header = match de.read_parameter_header() {
            Ok(header) => header,
            Err(_) => break,
        };
        let length = match header {
            PlCdrMemberHeader::Sentinel => break,
            PlCdrMemberHeader::Short { length, .. } => length as usize,
            PlCdrMemberHeader::Long { length, .. } => length as usize,
        };

        let start = de.get_position();
        if de.check_available(length).is_err() {
            return Err(format!(
                "v1 TypeObject: member length {} overruns payload (little_endian={})",
                length, little_endian
            ));
        }
        let mut region = vec![0u8; length];
        de.copy_bytes_at(start, &mut region);
        members.push(region);

        de.skip(length).map_err(|_| "v1 TypeObject: truncated member".to_string())?;
    }

    if members.is_empty() {
        return Err("v1 TypeObject: no members decoded".to_string());
    }
    Ok(members)
}

/// Recover CDR strings (length-prefixed, NUL-terminated, 4-aligned) that look
/// like type/member names from a member region, appending new ones to `out`.
fn scan_cdr_strings(region: &[u8], little_endian: bool, out: &mut Vec<String>) {
    let mut i = 0;

    while i + 4 <= region.len() {
        let len_bytes = [region[i], region[i + 1], region[i + 2], region[i + 3]];
        let len = if little_endian {
            u32::from_le_bytes(len_bytes)
        } else {
            u32::from_be_bytes(len_bytes)
        } as usize;

        if (2..=MAX_NAME_LEN).contains(&len) && i + 4 + len <= region.len() {
            let bytes = &region[i + 4..i + 4 + len];
            if let Some(name) = as_identifier(bytes) {
                if !out.iter().any(|existing| existing == name) {
                    out.push(name.to_string());
                }
                i += 4 + len;
                i = (i + 3) & !3; // re-align to 4 for the next candidate
                continue;
            }
        }
        i += 4;
    }
}

/// Validate `bytes` as a NUL-terminated, printable-ASCII identifier and return
/// it without the terminator, or `None` if it doesn't look like a name.
fn as_identifier(bytes: &[u8]) -> Option<&str> {
    let (&nul, body) = bytes.split_last()?;
    if nul != 0 || body.is_empty() {
        return None;
    }
    let first = body[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    if !body.iter().all(|&b| b.is_ascii_graphic()) {
        return None;
    }
    std::str::from_utf8(body).ok()
}

impl<C: Context> Writable<C> for TypeObjectV1 {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        let len = self.raw.len() as u32;
        writer.write_value(&len)?;
        writer.write_bytes(&self.raw)?;
        Ok(())
    }
}

impl<'a, C: Context> Readable<'a, C> for TypeObjectV1 {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let len: u32 = reader.read_value()?;
        let bytes = reader.read_vec(len as usize)?;
        TypeObjectV1::deserialize(&bytes)
            .map(|(obj, _)| obj)
            .map_err(|_| speedy::Error::custom("Failed to deserialize v1 TypeObject").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_cdr_string(buf: &mut Vec<u8>, s: &str) {
        let len = s.len() as u32 + 1; // + trailing NUL
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(s.as_bytes());
        buf.push(0);
        while buf.len() % 4 != 0 {
            buf.push(0);
        }
    }

    fn synthetic_v1_payload(names: &[&str]) -> Vec<u8> {
        let mut member = Vec::new();
        for name in names {
            push_cdr_string(&mut member, name);
        }

        let mut buf = Vec::new();
        // Short header: must-understand | PID_EXTENDED, plen = 8.
        let pid = 0x4000u16 | 0x3F01u16;
        buf.extend_from_slice(&pid.to_le_bytes());
        buf.extend_from_slice(&8u16.to_le_bytes());
        // Long header: member_id = 0, member_length.
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&(member.len() as u32).to_le_bytes());
        buf.extend_from_slice(&member);
        while buf.len() % 4 != 0 {
            buf.push(0);
        }
        // Sentinel.
        buf.extend_from_slice(&0x3F02u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf
    }

    #[test]
    fn decodes_framing_and_recovers_names() {
        let data = synthetic_v1_payload(&["LogMessage", "message"]);
        let (obj, consumed) = TypeObjectV1::deserialize(&data).expect("should decode v1");

        assert_eq!(consumed, data.len());
        assert!(obj.little_endian);
        assert_eq!(obj.raw, data);
        assert_eq!(obj.type_names, vec!["LogMessage".to_string(), "message".to_string()]);
    }

    #[test]
    fn deduplicates_recovered_names() {
        let data = synthetic_v1_payload(&["Foo", "Foo", "Bar"]);
        let (obj, _) = TypeObjectV1::deserialize(&data).unwrap();
        assert_eq!(obj.type_names, vec!["Foo".to_string(), "Bar".to_string()]);
    }

    #[test]
    fn rejects_payload_that_frames_in_neither_endianness() {
        // A short member header claiming a length far past the buffer end fails
        // the overrun check for both little- and big-endian interpretations.
        let data = [0x00u8, 0x00, 0xFF, 0xFF];
        assert!(TypeObjectV1::deserialize(&data).is_err());
    }

    #[test]
    fn ignores_non_identifier_byte_runs() {
        // Member payload is binary noise with no NUL-terminated identifier.
        let mut buf = Vec::new();
        let pid = 0x4000u16 | 0x3F01u16;
        buf.extend_from_slice(&pid.to_le_bytes());
        buf.extend_from_slice(&8u16.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&8u32.to_le_bytes());
        buf.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0xFE, 0xFD, 0xFC, 0xFB]);
        buf.extend_from_slice(&0x3F02u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());

        let (obj, _) = TypeObjectV1::deserialize(&buf).expect("framing is valid");
        assert!(obj.type_names.is_empty());
    }

    #[test]
    fn speedy_round_trip_preserves_payload() {
        use speedy::{Readable, Writable};

        let data = synthetic_v1_payload(&["LogMessage"]);
        let (obj, _) = TypeObjectV1::deserialize(&data).unwrap();

        let encoded = obj.write_to_vec().expect("write");
        let decoded = TypeObjectV1::read_from_buffer(&encoded).expect("read");
        assert_eq!(decoded.raw, obj.raw);
        assert_eq!(decoded.type_names, obj.type_names);
    }
}
