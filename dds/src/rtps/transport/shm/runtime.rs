//! The SHM runtime a participant owns: its registry slot, its own segment,
//! and the peers it has attached to.

use std::sync::Arc;

use log::{info, warn};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::shm::config::ShmConfig;
use crate::rtps::transport::shm::participant_slot::{ParticipantSlot, HEARTBEAT_PERIOD};
use crate::rtps::transport::shm::peer_map::PeerMap;
use crate::rtps::transport::shm::registry::Registry;
use crate::rtps::transport::shm::segment::{unlink_segment, OwnedSegment};

pub(crate) struct ShmRuntime {
    // Declared before `slot` on purpose: fields drop in declaration order.
    // Dropping `own` (which unlinks our segment on Unix) before `slot`
    // (which releases our registry entry) keeps our registry entry ACTIVE
    // for the whole window in which our segment name is disappearing, so a
    // peer racing to attach during that window only sees a benign attach
    // failure -> UDP fallback, never a claim on our slot racing our own
    // unlink. Same pattern as `segment.rs`'s `_shm` field.
    own: OwnedSegment,
    slot: ParticipantSlot,
    peers: PeerMap,
}

impl ShmRuntime {
    /// `None` whenever SHM cannot be brought up. Every caller falls back to the
    /// copy path; no failure here is fatal to the participant.
    pub(crate) fn start(domain: u32, guid_prefix: GuidPrefix) -> Option<Arc<ShmRuntime>> {
        let config = ShmConfig::from_env();
        if !config.enabled {
            info!("[shm] zero-copy disabled by INT2DDS_SHM_ZERO_COPY");
            return None;
        }

        let slot = match ParticipantSlot::claim(domain, guid_prefix) {
            Ok(slot) => slot,
            Err(e) => {
                warn!("[shm] no registry slot, falling back to the copy path: {e}");
                return None;
            }
        };

        // A slot we just claimed may still carry a dead predecessor's segment.
        // True on Unix, where this actually unlinks the name. On Windows
        // `unlink_segment` is a no-op, so if a peer still holds a handle to
        // the dead owner's segment, `create` below keeps failing with
        // `AlreadyExists`; `start` does not retry, and it only runs once per
        // plugin, so this participant can be permanently without SHM for its
        // whole process lifetime. Unix is sound.
        unlink_segment(domain, slot.slot());

        let own = match OwnedSegment::create(
            domain,
            slot.slot(),
            slot.epoch(),
            &config.classes,
            config.ring_entries,
        ) {
            Ok(own) => own,
            Err(e) => {
                warn!("[shm] cannot create segment, falling back to the copy path: {e}");
                return None;
            }
        };

        let peers = PeerMap::new(domain, slot.slot());
        info!("[shm] runtime started on domain {domain} slot {}", slot.slot());
        let runtime = Arc::new(ShmRuntime { own, slot, peers });

        // Reclaim peers that died without unmatching. Holds a Weak so it never
        // keeps the runtime alive on its own, and exits on the first tick
        // after the last Arc is gone. `sweep()` upgrades the Weak to a strong
        // Arc for the duration of the call -- the `drop(rt)` right after it,
        // before the sleep, is what makes "never keeps it alive" true again;
        // without that line the loop would hold a strong ref across the
        // sleep and the runtime could never be freed while the thread parks.
        let weak = Arc::downgrade(&runtime);
        let spawned =
            std::thread::Builder::new().name("int2dds-shm-sweep".into()).spawn(move || {
                while let Some(rt) = weak.upgrade() {
                    rt.sweep();
                    drop(rt);
                    std::thread::sleep(HEARTBEAT_PERIOD);
                }
            });
        if let Err(e) = spawned {
            warn!("[shm] no maintenance thread; peers that die without unmatching stay: {e}");
        }
        Some(runtime)
    }

    pub(crate) fn own(&self) -> &OwnedSegment {
        &self.own
    }

    pub(crate) fn peers(&self) -> &PeerMap {
        &self.peers
    }

    pub(crate) fn registry(&self) -> &Registry {
        self.slot.registry()
    }

    pub(crate) fn slot(&self) -> u32 {
        self.slot.slot()
    }

    /// DDS unmatch is not death: an unmatched peer may still be alive and
    /// still reading payload slots out of our pool, so this must not clear
    /// its refs bits -- only `sweep`, which confirms the process is gone, may
    /// do that. This only drops the cached mapping.
    pub(crate) fn peer_lost(&self, prefix: GuidPrefix) {
        self.peers.drop_mapping(&prefix);
    }

    /// Reclaim the slots of participants that died without unmatching.
    ///
    /// Known cost: a slot can outrun this. If a peer dies and, before our
    /// next tick, a new participant starts, runs its own startup sweep
    /// (which frees that same stale slot), and claims it into a fresh
    /// epoch, the slot is ACTIVE with an epoch we have never seen by the
    /// time we look at it -- `Registry::sweep_dead` returns slots it judged
    /// dead in the epoch it observed, whether it or another sweeper performed
    /// the CAS. A slot re-claimed into a newer epoch is filtered out earlier
    /// than that: the new owner refreshes the heartbeat, so `stale_slots` never
    /// offers it, and its pid is alive. So it is not `dead` here and
    /// `forget_slot` never runs for it. Every
    /// pool-slot bit the original dead peer held stays
    /// leaked for the rest of this process's life; not corruption, a
    /// capacity leak that eventually degrades to the copy path. Closing
    /// this needs a `PeerMap` iteration API to reconcile the cache against
    /// the registry directly -- out of scope for this round.
    pub(crate) fn sweep(&self) {
        for slot in self.slot.sweep() {
            self.peers.forget_slot(&self.own, self.registry(), slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::config::{DEFAULT_CLASSES, DEFAULT_RING_ENTRIES};
    use crate::rtps::transport::shm::notify::notify_supported;
    use crate::rtps::transport::shm::participant_slot::now_tick;
    use crate::rtps::transport::shm::registry_segment::unlink_registry;
    use crate::rtps::transport::shm::segment::unlink_segment;

    const DOMAIN: u32 = 247;

    #[test]
    fn start_registers_the_participant_and_stops_cleanly() {
        // `OwnedSegment::create` needs a notifier, so there is nothing to
        // exercise where notification is unsupported.
        if !notify_supported() {
            return;
        }
        crate::rtps::transport::shm::registry_segment::unlink_registry(DOMAIN);
        let rt = ShmRuntime::start(DOMAIN, [21; 12]).unwrap();
        let slot = rt.slot();
        assert_eq!(rt.registry().find_active(&[21; 12]).map(|(s, _)| s), Some(slot));
        // Dropping the last Arc stops the maintenance thread on its next tick.
        drop(rt);
        crate::rtps::transport::shm::registry_segment::unlink_registry(DOMAIN);
        crate::rtps::transport::shm::segment::unlink_segment(DOMAIN, slot);
    }

    #[test]
    fn peer_lost_releases_the_mapping_but_keeps_the_peers_bits() {
        if !notify_supported() {
            return;
        }
        // Unmatch is not death: the peer may still be reading our pool slots,
        // and clearing its bits would let the owner overwrite them. Only the
        // sweep, which checks `process_alive`, may reclaim.
        const PEER_DOMAIN: u32 = 248;
        const OWN_PREFIX: [u8; 12] = [31; 12];
        const PEER_PREFIX: [u8; 12] = [32; 12];

        unlink_registry(PEER_DOMAIN);
        let rt = ShmRuntime::start(PEER_DOMAIN, OWN_PREFIX).unwrap();
        let own_slot = rt.slot();

        let (peer_slot, peer_epoch) =
            rt.registry().claim(std::process::id(), PEER_PREFIX, now_tick()).unwrap();
        assert_ne!(peer_slot, own_slot, "the peer must not land on our own slot");
        unlink_segment(PEER_DOMAIN, peer_slot);
        let peer_segment = OwnedSegment::create(
            PEER_DOMAIN,
            peer_slot,
            peer_epoch,
            &DEFAULT_CLASSES,
            DEFAULT_RING_ENTRIES,
        )
        .unwrap();
        rt.peers().resolve(rt.own(), rt.registry(), PEER_PREFIX).unwrap();
        assert_eq!(rt.peers().len(), 1);

        // The peer holds a slot of ours.
        let lease = rt.own().owner_mut().acquire(8).unwrap();
        let slot_ref = rt.own().owner_mut().commit(lease, 1);
        {
            let owner = rt.own().owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            m.refs.fetch_or(1u64 << peer_slot, std::sync::atomic::Ordering::AcqRel);
        }

        rt.peer_lost(PEER_PREFIX);

        assert_eq!(rt.peers().len(), 0, "the mapping must be released");
        let owner = rt.own().owner_mut();
        let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
        assert_ne!(
            m.refs.load(std::sync::atomic::Ordering::Acquire) & (1u64 << peer_slot),
            0,
            "an unmatched peer's bits must survive"
        );
        drop(owner);

        drop(peer_segment);
        drop(rt);
        unlink_registry(PEER_DOMAIN);
        unlink_segment(PEER_DOMAIN, own_slot);
        unlink_segment(PEER_DOMAIN, peer_slot);
    }
}
