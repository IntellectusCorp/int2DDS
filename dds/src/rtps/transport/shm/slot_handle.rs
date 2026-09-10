//! A pool slot held as a `CacheChange` payload, from either side.

use std::sync::Arc;

use crate::rtps::transport::shm::pool_reader::ClaimedSlot;
use crate::rtps::transport::shm::segment::{OwnedSegment, PeerSegment};
use crate::rtps::transport::shm::slot_ref::SlotRef;

pub(crate) struct ShmSlotHandle {
    inner: SlotOwnership,
    slot_ref: SlotRef,
}

enum SlotOwnership {
    /// The writer's own slot: `Drop` clears our own bit so it can be reused.
    /// `ptr` is captured once, in `own`, instead of re-taking `owner_mut()`'s
    /// `MutexGuard` inside `as_slice` -- that guard would die at the end of
    /// the statement anyway, so holding it there would be pointless: the
    /// pointer addresses the mapping, not anything the mutex protects, and
    /// stays valid regardless. Same shape as `SlotLease`. No `len` field here:
    /// `self.slot_ref.len`, on the outer struct, is the single source of truth.
    Owner { segment: Arc<OwnedSegment>, ptr: *mut u8 },
    /// A peer's slot we claimed. The `Arc` keeps the mapping and the
    /// `PoolReader` alive, which is what lets `as_slice` borrow from `&self`
    /// rather than from a guard the caller would have to hold. `claimed` is
    /// `Some` until `Drop` takes it: `release` consumes the slot, and consuming
    /// it is what forbids reading after release.
    Peer { segment: Arc<PeerSegment>, claimed: Option<ClaimedSlot> },
}

// Safety: `Owner`'s `ptr` addresses our own slot in the shared mapping, which
// stays `Ready` with our bit set until `Drop`'s `release_own` clears it --
// nothing else may recycle the slot before then. The other fields are
// already `Send`/`Sync` on their own (`Pool` itself is `unsafe impl Send +
// Sync`, `ClaimedSlot` is plain integers), so the raw pointer is the only
// thing this impl accounts for.
unsafe impl Send for ShmSlotHandle {}
unsafe impl Sync for ShmSlotHandle {}

impl ShmSlotHandle {
    /// The writer side, right after `PoolOwner::commit`.
    ///
    /// `None` when `slot_ref` doesn't validate against this segment's pool --
    /// `len` beyond the class's slot size, or a `class`/`index` out of range.
    /// A `commit`-fresh `SlotRef` always validates; this only guards a caller
    /// mistake, since an out-of-range `class` reaching `Drop` unchecked would
    /// panic inside it, which is an abort.
    ///
    /// The caller must not hold the pool guard: this takes it to read the
    /// slot's address, and re-locking from the same thread deadlocks rather
    /// than panicking (see `OwnedSegment::owner_mut`).
    pub(crate) fn own(segment: Arc<OwnedSegment>, slot_ref: SlotRef) -> Option<ShmSlotHandle> {
        let ptr = {
            let owner = segment.owner_mut();
            let pool = owner.pool();
            // Bounds-check `class` via `data`'s `Option` before indexing it
            // through `slot_size`, which does not bounds-check on its own --
            // same order `PoolReader::claim` uses for the same reason.
            let ptr = pool.data(slot_ref.class, slot_ref.index)?;
            if slot_ref.len > pool.slot_size(slot_ref.class) {
                return None;
            }
            ptr
        };
        Some(ShmSlotHandle { inner: SlotOwnership::Owner { segment, ptr }, slot_ref })
    }

    /// The reader side. `None` when 5.6's validation rejects the descriptor.
    pub(crate) fn claim(segment: Arc<PeerSegment>, r: &SlotRef) -> Option<ShmSlotHandle> {
        let claimed = segment.reader.claim(r)?;
        Some(ShmSlotHandle {
            inner: SlotOwnership::Peer { segment, claimed: Some(claimed) },
            slot_ref: *r,
        })
    }

    pub(crate) fn slot_ref(&self) -> SlotRef {
        self.slot_ref
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        match &self.inner {
            // Safety: see the `unsafe impl Send`/`Sync` note above.
            SlotOwnership::Owner { ptr, .. } => unsafe {
                std::slice::from_raw_parts(*ptr, self.slot_ref.len as usize)
            },
            SlotOwnership::Peer { segment, claimed } => match claimed {
                Some(c) => segment.reader.bytes(c),
                None => &[],
            },
        }
    }

    pub(crate) fn len(&self) -> u32 {
        self.slot_ref.len
    }
}

impl Drop for ShmSlotHandle {
    fn drop(&mut self) {
        match &mut self.inner {
            SlotOwnership::Owner { segment, .. } => {
                segment.owner_mut().release_own(self.slot_ref.class, self.slot_ref.index);
            }
            SlotOwnership::Peer { segment, claimed } => {
                if let Some(c) = claimed.take() {
                    segment.reader.release(c);
                }
            }
        }
    }
}

impl std::fmt::Debug for ShmSlotHandle {
    // Coordinates only: the slot's bytes are user data.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShmSlotHandle").field("slot_ref", &self.slot_ref).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use super::ShmSlotHandle;
    use crate::rtps::transport::shm::notify::notify_supported;
    use crate::rtps::transport::shm::segment::{unlink_segment, OwnedSegment, PeerSegment};

    #[test]
    fn a_handle_holds_the_bit_until_it_is_dropped() {
        if !notify_supported() {
            return;
        }
        const DOMAIN: u32 = 251;
        unlink_segment(DOMAIN, 0);
        let owned = OwnedSegment::create(DOMAIN, 0, 1, &[(64, 2)], 4).unwrap();
        let peer = Arc::new(PeerSegment::attach(DOMAIN, 0, 1).unwrap());

        let mut lease = owned.owner_mut().acquire(8).unwrap();
        lease.bytes_mut()[..3].copy_from_slice(b"abc");
        let r = owned.owner_mut().commit(lease, 3);

        let handle = ShmSlotHandle::claim(Arc::clone(&peer), &r).expect("claim");
        assert_eq!(handle.as_slice(), b"abc");
        assert_eq!(handle.slot_ref(), r);
        let meta = peer.reader.pool().meta(r.class, r.index).unwrap();
        assert_eq!(meta.refs.load(Ordering::Acquire) & (1 << 1), 1 << 1);

        drop(handle);
        assert_eq!(meta.refs.load(Ordering::Acquire) & (1 << 1), 0);

        drop(peer);
        drop(owned);
        unlink_segment(DOMAIN, 0);
    }

    #[test]
    fn an_owner_handle_frees_the_slot_when_it_is_dropped() {
        if !notify_supported() {
            return;
        }
        const DOMAIN: u32 = 252;
        unlink_segment(DOMAIN, 0);
        // One slot in the class, so the next `acquire` can only succeed once
        // this handle has given it back.
        let owned = Arc::new(OwnedSegment::create(DOMAIN, 0, 1, &[(64, 1)], 4).unwrap());

        let lease = owned.owner_mut().acquire(8).unwrap();
        let r = owned.owner_mut().commit(lease, 3);
        let handle = ShmSlotHandle::own(Arc::clone(&owned), r).expect("own");
        assert!(owned.owner_mut().acquire(8).is_none(), "the handle still holds the only slot");

        drop(handle);
        assert!(owned.owner_mut().acquire(8).is_some(), "dropping the handle frees it");

        drop(owned);
        unlink_segment(DOMAIN, 0);
    }
}
