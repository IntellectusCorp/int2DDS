//! Remote participants this one has attached to, keyed by GUID prefix.

use std::sync::Arc;

use dashmap::DashMap;
use log::debug;

use crate::rtps::transport::shm::registry::Registry;
use crate::rtps::transport::shm::segment::{OwnedSegment, PeerSegment};

struct Peer {
    slot: u32,
    epoch: u64,
    segment: Arc<PeerSegment>,
}

pub(crate) struct PeerMap {
    domain: u32,
    my_slot: u32,
    peers: DashMap<[u8; 12], Peer>,
}

impl PeerMap {
    pub(crate) fn new(domain: u32, my_slot: u32) -> PeerMap {
        PeerMap { domain, my_slot, peers: DashMap::new() }
    }

    /// Clear a departed peer's bits unless the slot looks re-issued. Refs bits
    /// are indexed by slot id, so a new owner's bits are indistinguishable from
    /// the old owner's; clearing them would erase a live reader's claim.
    /// Leaking the dead peer's bits is the safer half of that trade -- the
    /// slots stay unallocatable, nothing is corrupted, and a later owner of
    /// the same slot id clears the bit only for the pool slots it itself
    /// claims or releases; bits in slots that owner never touches stay
    /// leaked for the rest of this process's life.
    ///
    /// This narrows the race, it does not close it: both checks are loads
    /// separate from the `fetch_and` that follows, so a claim landing in
    /// between is still cleared. Closing it would need the epoch carried in the
    /// refs word itself, which one bit per participant cannot express.
    fn reclaim_if_not_reissued(
        &self,
        own: &OwnedSegment,
        registry: &Registry,
        slot: u32,
        known_epoch: Option<u64>,
    ) {
        let still_ours = known_epoch.is_some_and(|e| registry.epoch(slot) == Some(e));
        if !still_ours && !registry.is_free(slot) {
            debug!("[shm] slot {slot} was re-issued; its refs bits belong to the new owner");
            return;
        }
        let cleared = own.owner_mut().reclaim_participant(slot);
        debug!("[shm] cleared {cleared} slot refs for peer slot {slot}");
    }

    /// The registry entry of a peer, which is never ourselves.
    ///
    /// `SlotMeta.refs` is a per-participant bitmap indexed by registry slot, so
    /// our own segment holds no bit that could tell a reader's claim apart from
    /// the writer's own: an eviction's `release_own` would clear a live reader's
    /// claim and the pool would hand the slot out again underneath it. Self is
    /// therefore not a peer, at either end of the path -- this is what spec §7
    /// rule 1 asks about a matched reader, and what `resolve` asks about a
    /// descriptor's origin.
    pub(crate) fn find_peer(&self, registry: &Registry, prefix: &[u8; 12]) -> Option<(u32, u64)> {
        registry.find_active(prefix).filter(|(slot, _)| *slot != self.my_slot)
    }

    /// The peer's segment, attaching on first use. `None` when the participant
    /// is not in the registry, is ourselves, or its segment cannot be mapped --
    /// the caller falls back to UDP.
    pub(crate) fn resolve(
        &self,
        own: &OwnedSegment,
        registry: &Registry,
        prefix: [u8; 12],
    ) -> Option<Arc<PeerSegment>> {
        if let Some(existing) = self.peers.get(&prefix) {
            // A slot can be reclaimed and re-issued before the sweep notices.
            // Our mapping would then address a segment nobody reads, so treat a
            // stale epoch as a miss and attach afresh.
            if registry.epoch(existing.slot) == Some(existing.epoch) {
                return Some(Arc::clone(&existing.segment));
            }
            let (slot, epoch) = (existing.slot, existing.epoch);
            // Release the shard guard before removing from the same map.
            drop(existing);
            self.peers.remove(&prefix);
            // This drops the map's reference. A handle the caller still holds
            // keeps the peer's segment object alive a while longer -- on
            // Windows that is what blocks the new owner's `create` -- but the
            // map no longer pins it forever.
            self.reclaim_if_not_reissued(own, registry, slot, Some(epoch));
            debug!("[shm] peer slot {slot} was re-issued; re-resolving");
        }
        let (slot, epoch) = self.find_peer(registry, &prefix)?;
        let segment = match PeerSegment::attach(self.domain, slot, self.my_slot) {
            Ok(segment) => Arc::new(segment),
            Err(e) => {
                debug!("[shm] cannot attach to peer slot {slot}: {e}");
                return None;
            }
        };
        // The name can still resolve to the previous owner's segment: a slot is
        // published ACTIVE before the new owner unlinks and re-creates it, and
        // on Windows the old object outlives its owner for as long as any peer
        // holds a handle. The header's own epoch is what says whose segment this
        // is; the registry epoch alone would match forever afterwards.
        if segment.epoch != epoch {
            debug!(
                "[shm] peer slot {slot} still maps epoch {} while the registry says {epoch}",
                segment.epoch
            );
            return None;
        }
        let handle = Arc::clone(&segment);
        self.peers.insert(prefix, Peer { slot, epoch, segment });
        Some(handle)
    }

    /// Drop a peer: clear its bits from our pool, then release the mapping.
    /// Does not unlink the peer's segment -- an unmatched peer may still be
    /// alive. `ParticipantSlot::sweep` unlinks the confirmed dead.
    pub(crate) fn forget(&self, own: &OwnedSegment, registry: &Registry, prefix: &[u8; 12]) {
        if let Some((_, peer)) = self.peers.remove(prefix) {
            self.reclaim_if_not_reissued(own, registry, peer.slot, Some(peer.epoch));
        }
    }

    /// Drop a peer's cached mapping without touching its refs bits. For DDS
    /// unmatch: an unmatched peer may still be alive and still reading slots
    /// out of our pool, so only a confirmed-dead sweep (`forget_slot`) may
    /// reclaim its bits. The next `resolve` re-attaches if the peer is still
    /// registered.
    pub(crate) fn drop_mapping(&self, prefix: &[u8; 12]) {
        self.peers.remove(prefix);
    }

    /// Same as `forget`, addressed by registry slot instead of GUID prefix --
    /// what a sweep produces.
    pub(crate) fn forget_slot(&self, own: &OwnedSegment, registry: &Registry, slot: u32) {
        // The guard from `iter()` must be released before `forget` locks the
        // same shard to remove. Keep this as its own statement: folding it into
        // the `match` scrutinee keeps the iterator alive across `remove` and
        // deadlocks -- silently, as a hang, not a test failure.
        let prefix = self.peers.iter().find(|e| e.slot == slot).map(|e| *e.key());
        match prefix {
            Some(prefix) => self.forget(own, registry, &prefix),
            // A slot we never mapped: the sweep freed it, so there is no epoch
            // of ours to match and only `is_free` can license the reclaim.
            None => self.reclaim_if_not_reissued(own, registry, slot, None),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.peers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::notify::notify_supported;
    use crate::rtps::transport::shm::participant_slot::now_tick;
    use crate::rtps::transport::shm::registry_segment::{unlink_registry, RegistrySegment};
    use crate::rtps::transport::shm::segment::unlink_segment;

    const DOMAIN: u32 = 245;
    const ME: u32 = 0;
    const PEER: u32 = 1;

    fn fresh() -> (RegistrySegment, OwnedSegment, OwnedSegment) {
        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, ME);
        unlink_segment(DOMAIN, PEER);
        let reg = RegistrySegment::open(DOMAIN).unwrap();
        let mine = OwnedSegment::create(DOMAIN, ME, 1, &[(64, 2)], 4).unwrap();
        let theirs = OwnedSegment::create(DOMAIN, PEER, 1, &[(64, 2)], 4).unwrap();
        (reg, mine, theirs)
    }

    #[test]
    fn resolve_finds_a_registered_peer_and_caches_it() {
        // `OwnedSegment::create` needs a notifier, so there is nothing to
        // exercise where notification is unsupported.
        if !notify_supported() {
            return;
        }
        let (reg, mine, _theirs) = fresh();
        // Occupy our own registry slot first so the peer lands elsewhere --
        // otherwise the claim below would land on slot 0 == ME, and `resolve`
        // refuses our own slot.
        reg.registry().claim(std::process::id(), [0; 12], now_tick()).unwrap();
        reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        let map = PeerMap::new(DOMAIN, ME);

        let first = map.resolve(&mine, reg.registry(), [9; 12]).unwrap();
        let second = map.resolve(&mine, reg.registry(), [9; 12]).unwrap();
        assert!(Arc::ptr_eq(&first, &second), "resolve must cache, not re-attach");
        assert_eq!(map.len(), 1);

        drop(first);
        drop(second);
        map.forget(&mine, reg.registry(), &[9; 12]);
        assert_eq!(map.len(), 0);
        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, ME);
        unlink_segment(DOMAIN, PEER);
    }

    #[test]
    fn resolve_returns_none_for_an_unregistered_prefix() {
        if !notify_supported() {
            return;
        }
        let (reg, mine, _theirs) = fresh();
        let map = PeerMap::new(DOMAIN, ME);
        assert!(map.resolve(&mine, reg.registry(), [200; 12]).is_none());
        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, ME);
        unlink_segment(DOMAIN, PEER);
    }

    #[test]
    fn resolve_refuses_our_own_slot() {
        if !notify_supported() {
            return;
        }
        let (reg, mine, _theirs) = fresh();
        // The first claim lands on slot 0, which is ME: this prefix is us.
        let (slot, _) = reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        assert_eq!(slot, ME, "the first claim must land on our own slot");
        let map = PeerMap::new(DOMAIN, ME);

        assert!(
            map.resolve(&mine, reg.registry(), [9; 12]).is_none(),
            "our own segment must never resolve as a peer"
        );
        assert_eq!(map.len(), 0, "nothing is cached for ourselves");

        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, ME);
        unlink_segment(DOMAIN, PEER);
    }

    #[test]
    fn forget_clears_the_peers_bit_from_our_pool() {
        if !notify_supported() {
            return;
        }
        let (reg, mine, _theirs) = fresh();
        // Occupy our own registry slot first so the peer lands elsewhere --
        // otherwise the first claim below would land on slot 0 == ME and the
        // peer would be indistinguishable from ourselves.
        reg.registry().claim(std::process::id(), [0; 12], now_tick()).unwrap();
        reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        let map = PeerMap::new(DOMAIN, ME);
        let peer_slot = reg.registry().find_active(&[9; 12]).unwrap().0;
        assert_ne!(peer_slot, ME, "the peer must not land on our own slot");
        map.resolve(&mine, reg.registry(), [9; 12]).unwrap();

        // The peer holds a slot of ours.
        let lease = mine.owner_mut().acquire(8).unwrap();
        let slot_ref = mine.owner_mut().commit(lease, 1);
        let meta = {
            let owner = mine.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            m.refs.fetch_or(1u64 << peer_slot, std::sync::atomic::Ordering::AcqRel);
            m.refs.load(std::sync::atomic::Ordering::Acquire)
        };
        assert_ne!(meta & (1u64 << peer_slot), 0);

        map.forget(&mine, reg.registry(), &[9; 12]);

        let owner = mine.owner_mut();
        let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
        assert_eq!(m.refs.load(std::sync::atomic::Ordering::Acquire) & (1u64 << peer_slot), 0);
        drop(owner);

        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, ME);
        unlink_segment(DOMAIN, PEER);
    }

    #[test]
    fn drop_mapping_releases_the_peer_without_touching_its_bits() {
        if !notify_supported() {
            return;
        }
        let (reg, mine, _theirs) = fresh();
        // Occupy our own registry slot first so the peer lands elsewhere --
        // otherwise the first claim below would land on slot 0 == ME and the
        // peer would be indistinguishable from ourselves.
        reg.registry().claim(std::process::id(), [0; 12], now_tick()).unwrap();
        reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        let map = PeerMap::new(DOMAIN, ME);
        let peer_slot = reg.registry().find_active(&[9; 12]).unwrap().0;
        assert_ne!(peer_slot, ME, "the peer must not land on our own slot");
        map.resolve(&mine, reg.registry(), [9; 12]).unwrap();

        // The peer holds a slot of ours.
        let lease = mine.owner_mut().acquire(8).unwrap();
        let slot_ref = mine.owner_mut().commit(lease, 1);
        {
            let owner = mine.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            m.refs.fetch_or(1u64 << peer_slot, std::sync::atomic::Ordering::AcqRel);
        }

        // Unmatch is not death: an unmatched peer may still be reading our
        // pool slots, and clearing its bits would let the owner overwrite
        // them.
        map.drop_mapping(&[9; 12]);
        assert_eq!(map.len(), 0, "the mapping must be released");
        let owner = mine.owner_mut();
        let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
        assert_ne!(
            m.refs.load(std::sync::atomic::Ordering::Acquire) & (1u64 << peer_slot),
            0,
            "an unmatched peer's bits must survive"
        );
        drop(owner);

        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, ME);
        unlink_segment(DOMAIN, PEER);
    }

    #[test]
    fn a_reissued_slot_is_not_served_from_the_cache() {
        if !notify_supported() {
            return;
        }
        const DOMAIN2: u32 = 246;
        const ME2: u32 = 0;
        const PEER2: u32 = 1;
        const PEER_PREFIX: [u8; 12] = [9; 12];
        const OTHER: [u8; 12] = [7; 12];

        unlink_registry(DOMAIN2);
        unlink_segment(DOMAIN2, ME2);
        unlink_segment(DOMAIN2, PEER2);
        let reg = RegistrySegment::open(DOMAIN2).unwrap();
        let own = OwnedSegment::create(DOMAIN2, ME2, 1, &[(64, 2)], 4).unwrap();
        let _theirs = OwnedSegment::create(DOMAIN2, PEER2, 1, &[(64, 2)], 4).unwrap();

        // Occupy our own slot first so the peer lands at PEER2, matching the
        // segment created above.
        reg.registry().claim(std::process::id(), [0; 12], now_tick()).unwrap();
        let (peer_slot, peer_epoch) =
            reg.registry().claim(std::process::id(), PEER_PREFIX, now_tick()).unwrap();
        assert_eq!(peer_slot, PEER2);

        let map = PeerMap::new(DOMAIN2, ME2);
        let first = map.resolve(&own, reg.registry(), PEER_PREFIX).unwrap();
        drop(first);

        // The peer holds a slot of ours.
        let lease = own.owner_mut().acquire(8).unwrap();
        let slot_ref = own.owner_mut().commit(lease, 1);
        {
            let owner = own.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            m.refs.fetch_or(1u64 << peer_slot, std::sync::atomic::Ordering::AcqRel);
        }

        // The peer goes away, someone else takes its slot, and then leaves too.
        // Evicting the stale entry must reclaim the bits now that the slot is
        // free -- had it stayed occupied they would belong to the new owner.
        reg.registry().release(peer_slot, peer_epoch);
        let (again, again_epoch) =
            reg.registry().claim(std::process::id(), OTHER, now_tick()).unwrap();
        assert_eq!(again, peer_slot, "the freed slot should be reused");
        reg.registry().release(again, again_epoch);

        assert!(
            map.resolve(&own, reg.registry(), PEER_PREFIX).is_none(),
            "must not serve a stale peer"
        );
        assert_eq!(map.len(), 0, "the stale entry must be evicted");
        {
            let owner = own.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            assert_eq!(
                m.refs.load(std::sync::atomic::Ordering::Acquire) & (1u64 << peer_slot),
                0,
                "evicting a stale entry must reclaim the freed slot's bits"
            );
        }

        unlink_registry(DOMAIN2);
        unlink_segment(DOMAIN2, ME2);
        unlink_segment(DOMAIN2, PEER2);
    }

    #[test]
    fn a_leftover_segment_at_an_older_epoch_is_not_adopted() {
        if !notify_supported() {
            return;
        }
        // A slot is published ACTIVE before its new owner unlinks and re-creates
        // the segment, and on Windows the dead owner's object outlives it while
        // any peer holds a handle. So the name can resolve to the previous
        // owner's segment while the registry already names the new one; the
        // header's own epoch is the only thing that tells them apart.
        let (reg, mine, _theirs) = fresh();
        reg.registry().claim(std::process::id(), [0; 12], now_tick()).unwrap();
        let (slot, epoch) = reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        let map = PeerMap::new(DOMAIN, ME);
        assert!(
            map.resolve(&mine, reg.registry(), [9; 12]).is_some(),
            "the segment and the registry agree at this point"
        );
        map.drop_mapping(&[9; 12]);

        // The slot changes hands. `_theirs` still carries the old epoch, which
        // is what a leftover looks like.
        reg.registry().release(slot, epoch);
        let (again, _) = reg.registry().claim(std::process::id(), [8; 12], now_tick()).unwrap();
        assert_eq!(again, slot, "the freed slot should be reused");
        assert!(
            map.resolve(&mine, reg.registry(), [8; 12]).is_none(),
            "a segment stamped with an older epoch must not be adopted"
        );
        assert_eq!(map.len(), 0, "nothing is cached for a peer we refused");

        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, ME);
        unlink_segment(DOMAIN, PEER);
    }

    #[test]
    fn forget_slot_reclaims_only_when_the_slot_is_free_or_still_unmapped() {
        if !notify_supported() {
            return;
        }
        let (reg, mine, _theirs) = fresh();
        // Occupy our own slot first so the participants below land elsewhere.
        reg.registry().claim(std::process::id(), [0; 12], now_tick()).unwrap();
        let (dead_slot, dead_epoch) =
            reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        let map = PeerMap::new(DOMAIN, ME);

        let lease = mine.owner_mut().acquire(8).unwrap();
        let slot_ref = mine.owner_mut().commit(lease, 1);

        // Case 1: the slot is cached, so `forget_slot` takes the `Some(prefix)`
        // arm -- the one whose `iter()` guard must be released before `forget`
        // removes from the same shard. Done first, while the registry epoch
        // still matches the epoch `fresh()` baked into the peer's segment:
        // `ShmRuntime::start` always creates a segment at the epoch its claim
        // returned, so a mismatch means the segment is a leftover.
        map.resolve(&mine, reg.registry(), [9; 12]).unwrap();
        assert_eq!(map.len(), 1);
        {
            let owner = mine.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            m.refs.fetch_or(1u64 << dead_slot, std::sync::atomic::Ordering::AcqRel);
        }
        map.forget_slot(&mine, reg.registry(), dead_slot);
        assert_eq!(map.len(), 0, "the cached entry must be removed");
        {
            let owner = mine.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            assert_eq!(
                m.refs.load(std::sync::atomic::Ordering::Acquire) & (1u64 << dead_slot),
                0,
                "a still-current peer's bits are cleared when we forget it"
            );
        }

        // Case 2: the slot is free -- `forget_slot`'s `None` branch reclaims
        // the leaked bits (nothing is cached for this slot any more).
        reg.registry().release(dead_slot, dead_epoch);
        {
            let owner = mine.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            m.refs.fetch_or(1u64 << dead_slot, std::sync::atomic::Ordering::AcqRel);
        }
        map.forget_slot(&mine, reg.registry(), dead_slot);
        {
            let owner = mine.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            assert_eq!(
                m.refs.load(std::sync::atomic::Ordering::Acquire) & (1u64 << dead_slot),
                0,
                "a free slot's leaked bits must be reclaimed"
            );
        }

        // Case 3: the slot has been re-issued to a new participant -- its bits
        // belong to the new owner and must be left alone.
        let (new_slot, _new_epoch) =
            reg.registry().claim(std::process::id(), [8; 12], now_tick()).unwrap();
        assert_eq!(new_slot, dead_slot, "the freed slot should be reused");
        {
            let owner = mine.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            m.refs.fetch_or(1u64 << new_slot, std::sync::atomic::Ordering::AcqRel);
        }
        map.forget_slot(&mine, reg.registry(), new_slot);
        {
            let owner = mine.owner_mut();
            let m = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
            assert_ne!(
                m.refs.load(std::sync::atomic::Ordering::Acquire) & (1u64 << new_slot),
                0,
                "a re-issued slot's bits belong to the new owner"
            );
        }

        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, ME);
        unlink_segment(DOMAIN, PEER);
    }
}
