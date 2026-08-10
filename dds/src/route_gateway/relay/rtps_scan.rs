//! Byte level scan of an RTPS datagram.
//!
//! The relay never deserializes a message. It only needs three facts to route
//! one: who sent it, who it is addressed to, and whether it carries a discovery
//! announcement whose locators must be rewritten. Everything else stays opaque
//! and is forwarded untouched.

use std::ops::Range;

pub(crate) type GuidPrefix = [u8; 12];

const RTPS_MAGIC: [u8; 4] = *b"RTPS";
const RTPS_HEADER_LEN: usize = 20;
const GUID_PREFIX_LEN: usize = 12;

const SUBMESSAGE_HEADER_LEN: usize = 4;
const SUBMESSAGE_ID_PAD: u8 = 0x01;
const SUBMESSAGE_ID_INFO_TS: u8 = 0x09;
const SUBMESSAGE_ID_INFO_DST: u8 = 0x0E;
const SUBMESSAGE_ID_DATA: u8 = 0x15;

const FLAG_ENDIANNESS: u8 = 0x01;
const FLAG_INLINE_QOS: u8 = 0x02;
const FLAG_DATA: u8 = 0x04;

/// ENTITYID_SPDP_BUILTIN_PARTICIPANT_WRITER.
const SPDP_WRITER_ENTITY_ID: [u8; 4] = [0x00, 0x01, 0x00, 0xC2];
/// ENTITYID_SEDP_BUILTIN_PUBLICATIONS_WRITER and its subscriptions twin.
const SEDP_PUBLICATIONS_WRITER_ENTITY_ID: [u8; 4] = [0x00, 0x00, 0x03, 0xC2];
const SEDP_SUBSCRIPTIONS_WRITER_ENTITY_ID: [u8; 4] = [0x00, 0x00, 0x04, 0xC2];

const PID_SENTINEL: u16 = 0x0001;
const PID_STATUS_INFO: u16 = 0x0071;
const STATUS_INFO_VALUE_LEN: usize = 4;
/// StatusInfo disposed and unregistered bits together.
const STATUS_INFO_DEPARTURE: u8 = 0x03;

/// What a discovery announcement describes. The two kinds are rewritten
/// differently and are the only datagrams the relay looks inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Announcement {
    Participant,
    Endpoint,
}

/// Routing facts extracted from one datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScannedMessage {
    pub(crate) source_prefix: GuidPrefix,
    pub(crate) dest_prefix: Option<GuidPrefix>,
    /// Set when the datagram carries a discovery announcement.
    pub(crate) announcement: Option<Announcement>,
    /// Set when that announcement says its subject is going away.
    pub(crate) is_departure: bool,
    /// Byte range of the announcement's serialized payload inside the
    /// datagram. Absent when the announcement carries no payload.
    pub(crate) payload: Option<Range<usize>>,
}

/// What one discovery DATA submessage says, relative to the submessage body.
struct ScannedData {
    announcement: Announcement,
    payload: Option<Range<usize>>,
    is_departure: bool,
}

pub(crate) fn scan(buf: &[u8]) -> Option<ScannedMessage> {
    if buf.len() < RTPS_HEADER_LEN || buf[0..4] != RTPS_MAGIC {
        return None;
    }

    let mut scanned = ScannedMessage {
        source_prefix: prefix_at(buf, 8)?,
        dest_prefix: None,
        announcement: None,
        is_departure: false,
        payload: None,
    };

    let mut pos = RTPS_HEADER_LEN;
    while pos + SUBMESSAGE_HEADER_LEN <= buf.len() {
        let id = buf[pos];
        let flags = buf[pos + 1];
        let little_endian = flags & FLAG_ENDIANNESS != 0;
        let declared = read_u16(&buf[pos + 2..pos + 4], little_endian) as usize;

        let body_start = pos + SUBMESSAGE_HEADER_LEN;
        // 8.3.3.2.3: a zero length means the submessage runs to the end of the
        // message, except for PAD and INFO_TS where zero is a real length.
        let body_len = if declared == 0 && id != SUBMESSAGE_ID_PAD && id != SUBMESSAGE_ID_INFO_TS {
            buf.len() - body_start
        } else {
            declared
        };
        let body_end = body_start.checked_add(body_len)?;
        if body_end > buf.len() {
            return None;
        }
        let body = &buf[body_start..body_end];

        match id {
            SUBMESSAGE_ID_INFO_DST => {
                scanned.dest_prefix = prefix_at(body, 0);
            }
            SUBMESSAGE_ID_DATA => {
                if let Some(data) = scan_data(body, flags, little_endian) {
                    scanned.announcement = Some(data.announcement);
                    scanned.is_departure = data.is_departure;
                    scanned.payload =
                        data.payload.map(|r| body_start + r.start..body_start + r.end);
                }
            }
            _ => {}
        }

        pos = body_end;
    }

    Some(scanned)
}

/// Reads the DATA submessage when it comes from a discovery builtin writer, and
/// nothing otherwise.
fn scan_data(body: &[u8], flags: u8, little_endian: bool) -> Option<ScannedData> {
    // extraFlags(2) octetsToInlineQos(2) readerId(4) writerId(4) sequenceNumber(8)
    if body.len() < 24 {
        return None;
    }
    let announcement = match body[8..12].try_into().ok()? {
        SPDP_WRITER_ENTITY_ID => Announcement::Participant,
        SEDP_PUBLICATIONS_WRITER_ENTITY_ID | SEDP_SUBSCRIPTIONS_WRITER_ENTITY_ID => {
            Announcement::Endpoint
        }
        _ => return None,
    };

    let octets_to_inline_qos = read_u16(&body[2..4], little_endian) as usize;
    let mut payload_start = 4usize.checked_add(octets_to_inline_qos)?;
    if payload_start > body.len() {
        return None;
    }

    let mut is_departure = false;
    if flags & FLAG_INLINE_QOS != 0 {
        let (end, departure) = scan_inline_qos(body, payload_start, little_endian)?;
        payload_start = end;
        is_departure = departure;
    }

    let payload = (flags & FLAG_DATA != 0).then(|| payload_start..body.len());
    Some(ScannedData { announcement, payload, is_departure })
}

/// Walks the inline QoS from `start`, returning the offset just past its
/// sentinel along with whether the parameters mark the sample as gone.
fn scan_inline_qos(buf: &[u8], start: usize, little_endian: bool) -> Option<(usize, bool)> {
    let mut pos = start;
    let mut is_departure = false;

    while pos + 4 <= buf.len() {
        let id = read_u16(&buf[pos..pos + 2], little_endian);
        let length = read_u16(&buf[pos + 2..pos + 4], little_endian) as usize;
        let value = pos + 4;

        // 9.6.3.9: StatusInfo is a fixed four octet bitmap whose flags sit in
        // the last octet, so it reads the same under either endianness.
        if id == PID_STATUS_INFO && value + STATUS_INFO_VALUE_LEN <= buf.len() {
            is_departure = buf[value + 3] & STATUS_INFO_DEPARTURE != 0;
        }

        pos = value.checked_add(length)?;
        if id == PID_SENTINEL {
            return Some((pos.min(buf.len()), is_departure));
        }
    }
    None
}

fn prefix_at(buf: &[u8], offset: usize) -> Option<GuidPrefix> {
    let end = offset + GUID_PREFIX_LEN;
    if end > buf.len() {
        return None;
    }
    let mut prefix = [0u8; GUID_PREFIX_LEN];
    prefix.copy_from_slice(&buf[offset..end]);
    Some(prefix)
}

fn read_u16(bytes: &[u8], little_endian: bool) -> u16 {
    let raw = [bytes[0], bytes[1]];
    if little_endian {
        u16::from_le_bytes(raw)
    } else {
        u16::from_be_bytes(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: GuidPrefix = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    const DEST: GuidPrefix = [21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32];

    fn header(source: GuidPrefix) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&RTPS_MAGIC);
        buf.extend_from_slice(&[2, 4, 0x01, 0x19]);
        buf.extend_from_slice(&source);
        buf
    }

    fn info_dst(dest: GuidPrefix) -> Vec<u8> {
        let mut buf = vec![SUBMESSAGE_ID_INFO_DST, FLAG_ENDIANNESS];
        buf.extend_from_slice(&(GUID_PREFIX_LEN as u16).to_le_bytes());
        buf.extend_from_slice(&dest);
        buf
    }

    fn data(writer: [u8; 4], flags: u8, inline_qos: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&[0, 0]); // extraFlags
        body.extend_from_slice(&16u16.to_le_bytes()); // octetsToInlineQos
        body.extend_from_slice(&[0x00, 0x01, 0x00, 0xC7]); // readerId
        body.extend_from_slice(&writer);
        body.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 0]); // sequenceNumber
        body.extend_from_slice(inline_qos);
        body.extend_from_slice(payload);

        let mut buf = vec![SUBMESSAGE_ID_DATA, flags];
        buf.extend_from_slice(&(body.len() as u16).to_le_bytes());
        buf.extend_from_slice(&body);
        buf
    }

    #[test]
    fn rejects_non_rtps_datagram() {
        assert!(scan(b"not an rtps datagram at all").is_none());
        assert!(scan(&[]).is_none());
    }

    #[test]
    fn reads_source_and_destination_prefix() {
        let mut buf = header(SOURCE);
        buf.extend_from_slice(&info_dst(DEST));
        buf.extend_from_slice(&data(
            [0x00, 0x00, 0x01, 0x03],
            FLAG_ENDIANNESS | FLAG_DATA,
            &[],
            &[9, 9, 9, 9],
        ));

        let scanned = scan(&buf).expect("scan failed");
        assert_eq!(scanned.source_prefix, SOURCE);
        assert_eq!(scanned.dest_prefix, Some(DEST));
        assert_eq!(scanned.announcement, None);
    }

    #[test]
    fn finds_spdp_payload_range() {
        let payload = [0x00u8, 0x03, 0x00, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];
        let mut buf = header(SOURCE);
        buf.extend_from_slice(&data(
            SPDP_WRITER_ENTITY_ID,
            FLAG_ENDIANNESS | FLAG_DATA,
            &[],
            &payload,
        ));

        let scanned = scan(&buf).expect("scan failed");
        assert_eq!(scanned.announcement, Some(Announcement::Participant));
        assert_eq!(scanned.dest_prefix, None);
        let range = scanned.payload.expect("no payload range");
        assert_eq!(&buf[range], &payload);
    }

    #[test]
    fn skips_inline_qos_before_spdp_payload() {
        let payload = [0x00u8, 0x03, 0x00, 0x00, 0x12, 0x34, 0x56, 0x78];
        let mut inline_qos = Vec::new();
        inline_qos.extend_from_slice(&0x0071u16.to_le_bytes()); // some parameter
        inline_qos.extend_from_slice(&4u16.to_le_bytes());
        inline_qos.extend_from_slice(&[1, 0, 0, 0]);
        inline_qos.extend_from_slice(&PID_SENTINEL.to_le_bytes());
        inline_qos.extend_from_slice(&0u16.to_le_bytes());

        let mut buf = header(SOURCE);
        buf.extend_from_slice(&data(
            SPDP_WRITER_ENTITY_ID,
            FLAG_ENDIANNESS | FLAG_INLINE_QOS | FLAG_DATA,
            &inline_qos,
            &payload,
        ));

        let scanned = scan(&buf).expect("scan failed");
        let range = scanned.payload.expect("no payload range");
        assert_eq!(&buf[range], &payload);
    }

    fn status_info(flags: u32) -> Vec<u8> {
        let mut inline_qos = Vec::new();
        inline_qos.extend_from_slice(&PID_STATUS_INFO.to_le_bytes());
        inline_qos.extend_from_slice(&(STATUS_INFO_VALUE_LEN as u16).to_le_bytes());
        inline_qos.extend_from_slice(&flags.to_be_bytes());
        inline_qos.extend_from_slice(&PID_SENTINEL.to_le_bytes());
        inline_qos.extend_from_slice(&0u16.to_le_bytes());
        inline_qos
    }

    #[test]
    fn dispose_announcement_has_no_payload() {
        let mut inline_qos = Vec::new();
        inline_qos.extend_from_slice(&PID_SENTINEL.to_le_bytes());
        inline_qos.extend_from_slice(&0u16.to_le_bytes());

        let mut buf = header(SOURCE);
        buf.extend_from_slice(&data(
            SPDP_WRITER_ENTITY_ID,
            FLAG_ENDIANNESS | FLAG_INLINE_QOS,
            &inline_qos,
            &[],
        ));

        let scanned = scan(&buf).expect("scan failed");
        assert_eq!(scanned.announcement, Some(Announcement::Participant));
        assert_eq!(scanned.payload, None);
    }

    #[test]
    fn endpoint_announcements_are_told_apart_from_participant_ones() {
        let payload = [0x00u8, 0x03, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44];
        for writer in [SEDP_PUBLICATIONS_WRITER_ENTITY_ID, SEDP_SUBSCRIPTIONS_WRITER_ENTITY_ID] {
            let mut buf = header(SOURCE);
            buf.extend_from_slice(&data(writer, FLAG_ENDIANNESS | FLAG_DATA, &[], &payload));

            let scanned = scan(&buf).expect("scan failed");
            assert_eq!(scanned.announcement, Some(Announcement::Endpoint));
            let range = scanned.payload.expect("no payload range");
            assert_eq!(&buf[range], &payload);
        }
    }

    #[test]
    fn status_info_marks_a_departing_participant() {
        let mut buf = header(SOURCE);
        buf.extend_from_slice(&data(
            SPDP_WRITER_ENTITY_ID,
            FLAG_ENDIANNESS | FLAG_INLINE_QOS,
            &status_info(0x03),
            &[],
        ));

        let scanned = scan(&buf).expect("scan failed");
        assert_eq!(scanned.announcement, Some(Announcement::Participant));
        assert!(scanned.is_departure);
    }

    #[test]
    fn a_live_announcement_is_not_a_departure() {
        let payload = [0x00u8, 0x03, 0x00, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];
        let mut buf = header(SOURCE);
        buf.extend_from_slice(&data(
            SPDP_WRITER_ENTITY_ID,
            FLAG_ENDIANNESS | FLAG_INLINE_QOS | FLAG_DATA,
            &status_info(0x00),
            &payload,
        ));

        let scanned = scan(&buf).expect("scan failed");
        assert!(!scanned.is_departure);
        let range = scanned.payload.expect("no payload range");
        assert_eq!(&buf[range], &payload);
    }

    #[test]
    fn trailing_submessage_may_declare_zero_length() {
        let payload = [0x00u8, 0x03, 0x00, 0x00, 0x42, 0x42, 0x42, 0x42];
        let mut submessage =
            data(SPDP_WRITER_ENTITY_ID, FLAG_ENDIANNESS | FLAG_DATA, &[], &payload);
        submessage[2] = 0;
        submessage[3] = 0;

        let mut buf = header(SOURCE);
        buf.extend_from_slice(&submessage);

        let scanned = scan(&buf).expect("scan failed");
        let range = scanned.payload.expect("no payload range");
        assert_eq!(&buf[range], &payload);
    }

    #[test]
    fn truncated_submessage_is_rejected() {
        let mut buf = header(SOURCE);
        buf.extend_from_slice(&info_dst(DEST));
        buf.truncate(buf.len() - 4);
        assert!(scan(&buf).is_none());
    }
}
