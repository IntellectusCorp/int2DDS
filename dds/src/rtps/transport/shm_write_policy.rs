//! The write()-time judgment: whether a sample may take an SHM pool slot at
//! all, and the size hint the next one is borrowed against.
//!
//! Outside `shm/` on purpose. `shm/` is the substrate -- every non-excluded
//! file in it refers only to `shm::*`, which is what lets the Unix
//! cross-check harness compile that tree from std alone. These three know
//! DDS QoS and RTPS GUIDs, so they belong to the policy layer that sits on
//! the substrate rather than inside it.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    infrastructure::qos_policy::DurabilityQosPolicyKind,
    rtps::{
        common::guid::{Guid, GuidPrefix},
        transport::shm::fallback::FallbackReason,
    },
};

/// A matched reader's SHM reachability, as this participant's `ShmRuntime`
/// registry would answer it, for the `NoLocalReader` rule.
///
/// Two-valued because the registry is per host: an entry in it, other than our
/// own, *is* a same-host SHM peer. That same lookup is what `PeerNotRegistered`
/// asks, so the `NoLocalReader` answer already carries it and no separate
/// judgment is left to make. A reader in this writer's own
/// participant is `NotLocal`: a descriptor cannot cross to it -- see
/// `PeerMap::find_peer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShmReaderReachability {
    NotLocal,
    Local,
}

/// `NoLocalReader` and `DurabilityTooStrong` -- the write()-time checks answerable
/// from this writer's own QoS and matched readers, with no live `ShmRuntime`
/// needed. `NoSlotId` (no `ShmRuntime` at all) and `SampleTooLarge`/`PoolExhausted`
/// (`PoolOwner::acquire`'s return) are decided at the call site.
pub(crate) fn shm_write_time_fallback(
    durability: DurabilityQosPolicyKind,
    matched_readers: &[Guid],
    reachability: impl Fn(GuidPrefix) -> ShmReaderReachability,
) -> Option<FallbackReason> {
    // `DurabilityTooStrong`: a TRANSIENT_LOCAL-or-stronger writer keeps changes for
    // late-joining readers indefinitely, so a slot it took would never come
    // back.
    if durability >= DurabilityQosPolicyKind::TransientLocal {
        return Some(FallbackReason::DurabilityTooStrong);
    }

    // `NoLocalReader`: the loan is per-`CacheChange`, not per-destination, so one
    // reachable same-host reader is enough to borrow a slot -- remote readers
    // are served from that same slot at send time.
    for reader in matched_readers {
        if reachability(reader.prefix()) == ShmReaderReachability::Local {
            return None;
        }
    }
    Some(FallbackReason::NoLocalReader)
}

/// The size of a writer's last serialized sample, used as the slot-size hint
/// for its next one. Per writer, but an SHM-only notion: nothing outside the
/// pool path reads it.
#[derive(Debug, Default)]
pub(crate) struct SlotSizeHint(AtomicUsize);

impl SlotSizeHint {
    pub(crate) fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }

    pub(crate) fn set(&self, len: usize) {
        self.0.store(len, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unknown_entity_reader(prefix: GuidPrefix) -> Guid {
        Guid::new(prefix, crate::rtps::common::entity_id::EntityId::UNKNOWN)
    }

    #[test]
    fn shm_write_time_fallback_flags_transient_local_durability_even_with_a_local_reader() {
        let reader = unknown_entity_reader([1; 12]);
        let reason =
            shm_write_time_fallback(DurabilityQosPolicyKind::TransientLocal, &[reader], |_| {
                ShmReaderReachability::Local
            });
        assert_eq!(reason, Some(FallbackReason::DurabilityTooStrong));
    }

    #[test]
    fn shm_write_time_fallback_passes_when_a_matched_reader_is_local() {
        let reader = unknown_entity_reader([1; 12]);
        let reason = shm_write_time_fallback(DurabilityQosPolicyKind::Volatile, &[reader], |_| {
            ShmReaderReachability::Local
        });
        assert_eq!(reason, None);
    }

    #[test]
    fn shm_write_time_fallback_flags_no_local_reader_when_none_are_matched() {
        let reason = shm_write_time_fallback(DurabilityQosPolicyKind::Volatile, &[], |_| {
            ShmReaderReachability::NotLocal
        });
        assert_eq!(reason, Some(FallbackReason::NoLocalReader));
    }

    #[test]
    fn shm_write_time_fallback_flags_no_local_reader_when_every_match_is_remote() {
        let readers = [unknown_entity_reader([1; 12]), unknown_entity_reader([2; 12])];
        let reason = shm_write_time_fallback(DurabilityQosPolicyKind::Volatile, &readers, |_| {
            ShmReaderReachability::NotLocal
        });
        assert_eq!(reason, Some(FallbackReason::NoLocalReader));
    }

    #[test]
    fn shm_write_time_fallback_prefers_a_local_match_over_an_earlier_remote_one() {
        let readers = [unknown_entity_reader([1; 12]), unknown_entity_reader([2; 12])];
        let reason =
            shm_write_time_fallback(DurabilityQosPolicyKind::Volatile, &readers, |prefix| {
                if prefix == [2; 12] {
                    ShmReaderReachability::Local
                } else {
                    ShmReaderReachability::NotLocal
                }
            });
        assert_eq!(reason, None);
    }
}
