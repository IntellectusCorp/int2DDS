//! Traffic classification for a serialized RTPS message.
//!
//! A byte stream that carries both discovery and user traffic on one listener
//! port cannot tell the two apart from the connection alone, so the class is
//! recovered from the message content.

use crate::rtps::common::types::RTPS_HEADER_LENGTH;
use crate::rtps::messages::submessage_id::SubmessageId;

const SUBMESSAGE_HEADER_LEN: usize = 4;
const ENTITY_ID_LEN: usize = 4;
const ENTITY_KIND_BUILT_IN: u8 = 0xC0;
/// `writerId` sits right after `readerId` in ACKNACK, HEARTBEAT, GAP,
/// NACK_FRAG and HEARTBEAT_FRAG.
const WRITER_ID_AFTER_READER_ID: usize = ENTITY_ID_LEN;
/// DATA and DATA_FRAG prepend extraFlags and octetsToInlineQos.
const WRITER_ID_IN_DATA: usize = 2 + 2 + ENTITY_ID_LEN;

/// Reports whether the first `writerId` in the message belongs to a builtin
/// endpoint. `readerId` is never used for this: it may be `ENTITYID_UNKNOWN`,
/// whose kind byte silently fails the builtin test. A message that carries no
/// `writerId` at all counts as user traffic.
pub(crate) fn carries_builtin_writer(message: &[u8]) -> bool {
    let mut offset = RTPS_HEADER_LENGTH as usize;

    while offset + SUBMESSAGE_HEADER_LEN <= message.len() {
        let submessage_id = SubmessageId::new(message[offset]);
        let little_endian = message[offset + 1] & 0x01 != 0;
        let length_bytes = [message[offset + 2], message[offset + 3]];
        let octets_to_next_header = if little_endian {
            u16::from_le_bytes(length_bytes)
        } else {
            u16::from_be_bytes(length_bytes)
        } as usize;

        let body = offset + SUBMESSAGE_HEADER_LEN;
        let writer_id_offset = match submessage_id {
            SubmessageId::DATA | SubmessageId::DATA_FRAG => Some(WRITER_ID_IN_DATA),
            SubmessageId::ACKNACK
            | SubmessageId::HEARTBEAT
            | SubmessageId::GAP
            | SubmessageId::NACK_FRAG
            | SubmessageId::HEARTBEAT_FRAG => Some(WRITER_ID_AFTER_READER_ID),
            _ => None,
        };

        if let Some(writer_id_offset) = writer_id_offset {
            // An EntityId is an octet array, so the kind byte keeps its
            // position regardless of the submessage endianness.
            let kind_index = body + writer_id_offset + ENTITY_ID_LEN - 1;
            return message.get(kind_index).is_some_and(|kind| kind & 0xF0 == ENTITY_KIND_BUILT_IN);
        }

        // A zero length means the submessage runs to the end of the message,
        // so nothing further can be walked.
        if octets_to_next_header == 0 {
            return false;
        }
        offset = body + octets_to_next_header;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(submessages: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; RTPS_HEADER_LENGTH as usize];
        bytes[..4].copy_from_slice(b"RTPS");
        bytes.extend_from_slice(submessages);
        bytes
    }

    fn info_dst(guid_prefix_tail: u8) -> Vec<u8> {
        let mut bytes = vec![SubmessageId::INFO_DST.as_u8(), 0x01, 12, 0];
        bytes.extend_from_slice(&[guid_prefix_tail; 12]);
        bytes
    }

    fn data(writer_kind: u8) -> Vec<u8> {
        let mut bytes = vec![SubmessageId::DATA.as_u8(), 0x01, 24, 0];
        bytes.extend_from_slice(&[0, 0, 16, 0]); // extraFlags, octetsToInlineQos
        bytes.extend_from_slice(&[0, 0, 0, 0]); // readerId: ENTITYID_UNKNOWN
        bytes.extend_from_slice(&[0, 1, 0, writer_kind]);
        bytes.extend_from_slice(&[0; 12]); // writerSN + payload stub
        bytes
    }

    fn heartbeat(writer_kind: u8) -> Vec<u8> {
        let mut bytes = vec![SubmessageId::HEARTBEAT.as_u8(), 0x01, 28, 0];
        bytes.extend_from_slice(&[0, 0, 0, 0]); // readerId: ENTITYID_UNKNOWN
        bytes.extend_from_slice(&[0, 0, 3, writer_kind]);
        bytes.extend_from_slice(&[0; 20]);
        bytes
    }

    #[test]
    fn builtin_and_user_writers_are_separated() {
        assert!(carries_builtin_writer(&message(&data(0xC2))));
        assert!(!carries_builtin_writer(&message(&data(0x02))));
        assert!(carries_builtin_writer(&message(&heartbeat(0xC2))));
        assert!(!carries_builtin_writer(&message(&heartbeat(0x07))));
    }

    #[test]
    fn leading_submessages_without_a_writer_id_are_walked() {
        let mut submessages = info_dst(0xAB);
        submessages.extend_from_slice(&data(0xC2));
        assert!(carries_builtin_writer(&message(&submessages)));

        let mut submessages = info_dst(0xAB);
        submessages.extend_from_slice(&data(0x03));
        assert!(!carries_builtin_writer(&message(&submessages)));
    }

    #[test]
    fn big_endian_length_is_honoured_while_walking() {
        let mut leading = info_dst(0xCD);
        leading[1] = 0x00;
        leading[2] = 0;
        leading[3] = 12;
        let mut submessages = leading;
        submessages.extend_from_slice(&data(0xC2));
        assert!(carries_builtin_writer(&message(&submessages)));
    }

    #[test]
    fn a_message_without_any_writer_id_is_user_traffic() {
        assert!(!carries_builtin_writer(&message(&info_dst(0x01))));
        assert!(!carries_builtin_writer(&message(&[])));
    }

    #[test]
    fn a_truncated_writer_id_does_not_panic() {
        let mut truncated = message(&data(0xC2));
        truncated.truncate(RTPS_HEADER_LENGTH as usize + 10);
        assert!(!carries_builtin_writer(&truncated));
    }
}
