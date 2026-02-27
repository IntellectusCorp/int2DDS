use std::fmt;

use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::guid::Guid;
use crate::rtps::common::sequence::SequenceNumber;
use crate::rtps::messages::submessage_id::SubmessageId;

// Structured timer identifier for entity-related timers.
//
// Format: `{entity_id_hex}_{submessage_id_hex}[_{remote_guid_hex}][_{extra}]`
//
// The entity_id prefix (8 hex chars + underscore) enables bulk removal
// of all timers belonging to a specific writer or reader via `entity_prefix()`.
pub(crate) enum TimerId {
    // Writer: periodic HEARTBEAT sender
    PeriodicHeartbeat {
        entity_id: EntityId,
    },

    // Writer: initial delay before periodic HEARTBEAT
    PeriodicHeartbeatDelay {
        entity_id: EntityId,
    },

    // Writer: delayed DATA resend in response to NACK
    NackResponse {
        writer_entity_id: EntityId,
        remote_reader_guid: Guid,
    },

    // Reader: delayed ACKNACK in response to HEARTBEAT
    Acknack {
        reader_entity_id: EntityId,
        remote_writer_guid: Guid,
    },

    // Reader: delayed NACK_FRAG for missing fragments
    NackFrag {
        reader_entity_id: EntityId,
        remote_writer_guid: Guid,
        sequence_number: SequenceNumber,
    },

    // Writer: one-shot preemptive HEARTBEAT on new reader match
    PreemptiveHeartbeat {
        entity_id: EntityId,
        remote_reader_guid: Guid,
    },

    // Reader: one-shot preemptive ACKNACK on new writer match
    PreemptiveAcknack {
        entity_id: EntityId,
        remote_writer_guid: Guid,
    },
}

impl fmt::Display for TimerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimerId::PeriodicHeartbeat { entity_id } => {
                write!(f, "{}_{:02x}", Self::id_hex(entity_id), SubmessageId::HEARTBEAT.as_u8())
            }
            TimerId::PeriodicHeartbeatDelay { entity_id } => {
                write!(
                    f,
                    "{}_{:02x}_delay",
                    Self::id_hex(entity_id),
                    SubmessageId::HEARTBEAT.as_u8()
                )
            }
            TimerId::NackResponse { writer_entity_id, remote_reader_guid } => {
                write!(
                    f,
                    "{}_{:02x}_{:x}",
                    Self::id_hex(writer_entity_id),
                    SubmessageId::DATA.as_u8(),
                    Self::guid_u128(remote_reader_guid)
                )
            }
            TimerId::Acknack { reader_entity_id, remote_writer_guid } => {
                write!(
                    f,
                    "{}_{:02x}_{:x}",
                    Self::id_hex(reader_entity_id),
                    SubmessageId::ACKNACK.as_u8(),
                    Self::guid_u128(remote_writer_guid)
                )
            }
            TimerId::NackFrag { reader_entity_id, remote_writer_guid, sequence_number } => {
                write!(
                    f,
                    "{}_{:02x}_{:x}_{}",
                    Self::id_hex(reader_entity_id),
                    SubmessageId::NACK_FRAG.as_u8(),
                    Self::guid_u128(remote_writer_guid),
                    sequence_number.to_i64()
                )
            }
            TimerId::PreemptiveHeartbeat { entity_id, remote_reader_guid } => {
                write!(
                    f,
                    "{}_{:02x}_pre_{:x}",
                    Self::id_hex(entity_id),
                    SubmessageId::HEARTBEAT.as_u8(),
                    Self::guid_u128(remote_reader_guid)
                )
            }
            TimerId::PreemptiveAcknack { entity_id, remote_writer_guid } => {
                write!(
                    f,
                    "{}_{:02x}_pre_{:x}",
                    Self::id_hex(entity_id),
                    SubmessageId::ACKNACK.as_u8(),
                    Self::guid_u128(remote_writer_guid)
                )
            }
        }
    }
}

impl TimerId {
    fn id_hex(entity_id: &EntityId) -> String {
        let b = entity_id.to_bytes();
        format!("{:02x}{:02x}{:02x}{:02x}", b[0], b[1], b[2], b[3])
    }

    fn guid_u128(guid: &Guid) -> u128 {
        u128::from_be_bytes(guid.to_bytes())
    }

    // Returns the entity_id prefix string for bulk timer removal.
    // All timers belonging to the given entity start with this prefix.
    pub(crate) fn entity_prefix(entity_id: EntityId) -> String {
        let b = entity_id.to_bytes();
        format!("{:02x}{:02x}{:02x}{:02x}_", b[0], b[1], b[2], b[3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::entity_kind::EntityKind;

    #[test]
    fn test_periodic_heartbeat_display() {
        let entity_id = EntityId { entity_key: [0x00, 0x00, 0x03], entity_kind: EntityKind(0x02) };
        let timer_id = TimerId::PeriodicHeartbeat { entity_id };
        assert_eq!(timer_id.to_string(), "00000302_07");
    }

    #[test]
    fn test_periodic_heartbeat_delay_display() {
        let entity_id = EntityId { entity_key: [0x00, 0x00, 0x03], entity_kind: EntityKind(0x02) };
        let timer_id = TimerId::PeriodicHeartbeatDelay { entity_id };
        assert_eq!(timer_id.to_string(), "00000302_07_delay");
    }

    #[test]
    fn test_entity_prefix() {
        let entity_id = EntityId { entity_key: [0x00, 0x00, 0x03], entity_kind: EntityKind(0x02) };
        assert_eq!(TimerId::entity_prefix(entity_id), "00000302_");
    }

    #[test]
    fn test_prefix_matches_timer_ids() {
        let writer_id = EntityId { entity_key: [0x00, 0x00, 0x03], entity_kind: EntityKind(0x02) };
        let remote_guid = Guid::new(
            [0x01; 12],
            EntityId { entity_key: [0x00, 0x00, 0x07], entity_kind: EntityKind(0x07) },
        );

        let prefix = TimerId::entity_prefix(writer_id);

        let hb = TimerId::PeriodicHeartbeat { entity_id: writer_id }.to_string();
        let hb_delay = TimerId::PeriodicHeartbeatDelay { entity_id: writer_id }.to_string();
        let nack =
            TimerId::NackResponse { writer_entity_id: writer_id, remote_reader_guid: remote_guid }
                .to_string();

        assert!(hb.starts_with(&prefix));
        assert!(hb_delay.starts_with(&prefix));
        assert!(nack.starts_with(&prefix));

        // Reader timer with different entity_id should NOT match
        let reader_id = EntityId { entity_key: [0x00, 0x00, 0x07], entity_kind: EntityKind(0x07) };
        let reader_prefix = TimerId::entity_prefix(reader_id);
        assert!(!hb.starts_with(&reader_prefix));
    }
}
