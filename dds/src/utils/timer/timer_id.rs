use std::fmt;

use crate::common::instance_handle::InstanceHandle;
use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::common::sequence::SequenceNumber;
use crate::rtps::common::types::DomainId;
use crate::rtps::messages::submessage_id::SubmessageId;

// Structured timer identifier used as the HashMap key for all timers.
// Every timer in the system must use a `TimerId` variant, enforcing type safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    // Lifespan expiry timer for writer-side history cache
    LifespanWriter {
        writer_guid: Guid,
    },

    // Lifespan expiry timer for reader-side history cache
    LifespanReader {
        writer_guid: Guid,
    },

    // Autopurge disposed samples delay
    AutopurgeDisposed {
        reader_guid: Guid,
    },

    // Autopurge no-writer samples delay
    AutopurgeNowriter {
        reader_guid: Guid,
    },

    // SPDP periodic multicast announcement
    SpdpMulticast {
        domain_id: DomainId,
    },

    // WLP participant-to-participant liveliness
    WlpP2p {
        guid_prefix: GuidPrefix,
    },

    // SEDP periodic scheduled message per (remote participant, builtin writer)
    SedpScheduledMessage {
        remote_prefix: GuidPrefix,
        writer_entity_id: EntityId,
    },

    // Thread monitoring periodic timer
    ThreadMonitoring,

    // Reader: one-shot delivery of a held sample for TIME_BASED_FILTER
    TimeBasedFilter {
        reader_entity_id: EntityId,
        instance_handle: InstanceHandle,
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
            TimerId::SpdpMulticast { domain_id } => {
                write!(f, "spdp_multicast_{}", domain_id)
            }
            TimerId::WlpP2p { guid_prefix } => {
                write!(f, "wlp_p2p_{}", Guid::guid_prefix_to_string(guid_prefix))
            }
            TimerId::LifespanWriter { writer_guid } => {
                write!(f, "lifespan_writer_{:x}", Self::guid_u128(writer_guid))
            }
            TimerId::LifespanReader { writer_guid } => {
                write!(f, "lifespan_reader_{:x}", Self::guid_u128(writer_guid))
            }
            TimerId::AutopurgeDisposed { reader_guid } => {
                write!(f, "autopurge_disposed_{:x}", Self::guid_u128(reader_guid))
            }
            TimerId::AutopurgeNowriter { reader_guid } => {
                write!(f, "autopurge_nowriter_{:x}", Self::guid_u128(reader_guid))
            }
            TimerId::SedpScheduledMessage { remote_prefix, writer_entity_id } => {
                write!(
                    f,
                    "sedp_scheduled_{}_{}",
                    Guid::guid_prefix_to_string(remote_prefix),
                    Self::id_hex(writer_entity_id)
                )
            }
            TimerId::ThreadMonitoring => {
                write!(f, "thread_monitoring_timer")
            }
            TimerId::TimeBasedFilter { reader_entity_id, instance_handle } => {
                write!(f, "tbf_{}_{}", Self::id_hex(reader_entity_id), instance_handle)
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

    /// Returns true if this timer belongs to the given RTPS entity.
    /// Used for bulk removal when an entity (writer/reader) is deleted.
    pub(crate) fn belongs_to_entity(&self, entity_id: &EntityId) -> bool {
        match self {
            TimerId::PeriodicHeartbeat { entity_id: eid } => eid == entity_id,
            TimerId::PeriodicHeartbeatDelay { entity_id: eid } => eid == entity_id,
            TimerId::NackResponse { writer_entity_id, .. } => writer_entity_id == entity_id,
            TimerId::Acknack { reader_entity_id, .. } => reader_entity_id == entity_id,
            TimerId::NackFrag { reader_entity_id, .. } => reader_entity_id == entity_id,
            TimerId::PreemptiveHeartbeat { entity_id: eid, .. } => eid == entity_id,
            TimerId::PreemptiveAcknack { entity_id: eid, .. } => eid == entity_id,
            _ => false,
        }
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
    fn test_belongs_to_entity() {
        let writer_id = EntityId { entity_key: [0x00, 0x00, 0x03], entity_kind: EntityKind(0x02) };
        let reader_id = EntityId { entity_key: [0x00, 0x00, 0x07], entity_kind: EntityKind(0x07) };
        let remote_guid = Guid::new(
            [0x01; 12],
            EntityId { entity_key: [0x00, 0x00, 0x01], entity_kind: EntityKind(0x07) },
        );

        let hb = TimerId::PeriodicHeartbeat { entity_id: writer_id };
        let hb_delay = TimerId::PeriodicHeartbeatDelay { entity_id: writer_id };
        let nack =
            TimerId::NackResponse { writer_entity_id: writer_id, remote_reader_guid: remote_guid };
        let pre_hb =
            TimerId::PreemptiveHeartbeat { entity_id: writer_id, remote_reader_guid: remote_guid };

        // Writer timers belong to writer_id
        assert!(hb.belongs_to_entity(&writer_id));
        assert!(hb_delay.belongs_to_entity(&writer_id));
        assert!(nack.belongs_to_entity(&writer_id));
        assert!(pre_hb.belongs_to_entity(&writer_id));

        // Writer timers do NOT belong to reader_id
        assert!(!hb.belongs_to_entity(&reader_id));
        assert!(!hb_delay.belongs_to_entity(&reader_id));
        assert!(!nack.belongs_to_entity(&reader_id));

        // Reader timers
        let acknack =
            TimerId::Acknack { reader_entity_id: reader_id, remote_writer_guid: remote_guid };
        let nackfrag = TimerId::NackFrag {
            reader_entity_id: reader_id,
            remote_writer_guid: remote_guid,
            sequence_number: SequenceNumber::new(0, 1),
        };
        let pre_ack =
            TimerId::PreemptiveAcknack { entity_id: reader_id, remote_writer_guid: remote_guid };

        assert!(acknack.belongs_to_entity(&reader_id));
        assert!(nackfrag.belongs_to_entity(&reader_id));
        assert!(pre_ack.belongs_to_entity(&reader_id));
        assert!(!acknack.belongs_to_entity(&writer_id));

        // Non-entity timers never belong to any entity
        assert!(!TimerId::SpdpMulticast { domain_id: 0 }.belongs_to_entity(&writer_id));
        assert!(!TimerId::ThreadMonitoring.belongs_to_entity(&writer_id));
    }

    #[test]
    fn test_different_variants_same_entity_are_distinct() {
        let entity_id = EntityId { entity_key: [0x00, 0x00, 0x03], entity_kind: EntityKind(0x02) };
        let hb = TimerId::PeriodicHeartbeat { entity_id };
        let hb_delay = TimerId::PeriodicHeartbeatDelay { entity_id };

        assert_ne!(hb, hb_delay);
    }
}
