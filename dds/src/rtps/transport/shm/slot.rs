//! The 20-byte slot descriptor that replaces SerializedPayload on the SHM
//! path, and the handle that holds a slot as a `CacheChange` payload.

use std::sync::Arc;

use crate::rtps::transport::shm::pool::ClaimedSlot;
use crate::rtps::transport::shm::segment::{OwnedSegment, PeerSegment};

pub(crate) const SLOT_REF_LEN: usize = 20;
pub(crate) const SLOT_REF_PAYLOAD_LEN: usize = 4 + SLOT_REF_LEN;
/// Encapsulation id of a SlotRef payload; `EncodingKind::ShmSlotRef` mirrors it.
pub(crate) const SLOT_REF_ENCAPSULATION_ID: u16 = 0x8001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotRef {
    pub(crate) owner_slot: u16,
    pub(crate) owner_epoch_low: u16,
    pub(crate) class: u16,
    pub(crate) index: u32,
    pub(crate) generation: u32,
    pub(crate) len: u32,
}

impl SlotRef {
    pub(crate) fn encode(&self) -> [u8; SLOT_REF_LEN] {
        let mut out = [0u8; SLOT_REF_LEN];
        out[0..2].copy_from_slice(&self.owner_slot.to_le_bytes());
        out[2..4].copy_from_slice(&self.owner_epoch_low.to_le_bytes());
        out[4..6].copy_from_slice(&self.class.to_le_bytes());
        // out[6..8] reserved
        out[8..12].copy_from_slice(&self.index.to_le_bytes());
        out[12..16].copy_from_slice(&self.generation.to_le_bytes());
        out[16..20].copy_from_slice(&self.len.to_le_bytes());
        out
    }

    pub(crate) fn decode(bytes: &[u8]) -> Option<SlotRef> {
        if bytes.len() < SLOT_REF_LEN {
            return None;
        }
        Some(SlotRef {
            owner_slot: u16::from_le_bytes(bytes[0..2].try_into().ok()?),
            owner_epoch_low: u16::from_le_bytes(bytes[2..4].try_into().ok()?),
            class: u16::from_le_bytes(bytes[4..6].try_into().ok()?),
            index: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
            generation: u32::from_le_bytes(bytes[12..16].try_into().ok()?),
            len: u32::from_le_bytes(bytes[16..20].try_into().ok()?),
        })
    }

    pub(crate) fn to_payload(self) -> [u8; SLOT_REF_PAYLOAD_LEN] {
        let mut out = [0u8; SLOT_REF_PAYLOAD_LEN];
        out[0..2].copy_from_slice(&SLOT_REF_ENCAPSULATION_ID.to_be_bytes());
        // out[2..4] options, reserved
        out[4..].copy_from_slice(&self.encode());
        out
    }

    pub(crate) fn from_payload(bytes: &[u8]) -> Option<SlotRef> {
        if bytes.len() < SLOT_REF_PAYLOAD_LEN {
            return None;
        }
        if u16::from_be_bytes(bytes[0..2].try_into().ok()?) != SLOT_REF_ENCAPSULATION_ID {
            return None;
        }
        SlotRef::decode(&bytes[4..])
    }
}

pub(crate) struct ShmSlotHandle {
    inner: SlotOwnership,
    slot_ref: SlotRef,
}

enum SlotOwnership {
    /// The writer's own slot; `Drop` clears the owner bit so it can be reused.
    Owner { segment: Arc<OwnedSegment>, ptr: *mut u8 },
    /// A peer's slot we claimed; `Drop` releases the claim.
    Peer { segment: Arc<PeerSegment>, claimed: Option<ClaimedSlot> },
}

// Safety: `Owner::ptr` addresses our own slot, which stays Ready with our bit
// set until `Drop` clears it, so nothing recycles it underneath the handle.
unsafe impl Send for ShmSlotHandle {}
unsafe impl Sync for ShmSlotHandle {}

impl ShmSlotHandle {
    /// The writer side, right after `PoolOwner::commit`. The caller must not
    /// hold the pool guard: this takes it, and re-locking deadlocks.
    pub(crate) fn own(segment: Arc<OwnedSegment>, slot_ref: SlotRef) -> Option<ShmSlotHandle> {
        let ptr = {
            let owner = segment.owner_mut();
            let pool = owner.pool();
            let ptr = pool.data(slot_ref.class, slot_ref.index)?;
            if slot_ref.len > pool.slot_size(slot_ref.class) {
                return None;
            }
            ptr
        };
        Some(ShmSlotHandle { inner: SlotOwnership::Owner { segment, ptr }, slot_ref })
    }

    /// The reader side. `None` when the descriptor does not validate.
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShmSlotHandle").field("slot_ref", &self.slot_ref).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use super::*;
    use crate::rtps::transport::shm::segment::unlink_segment;

    fn sample() -> SlotRef {
        SlotRef {
            owner_slot: 7,
            owner_epoch_low: 0xBEEF,
            class: 2,
            index: 11,
            generation: 5,
            len: 4096,
        }
    }

    #[test]
    fn descriptor_round_trips_bare_and_encapsulated() {
        let bytes = sample().encode();
        assert_eq!(SlotRef::decode(&bytes), Some(sample()));
        assert_eq!(SlotRef::decode(&bytes[..SLOT_REF_LEN - 1]), None);

        let payload = sample().to_payload();
        assert_eq!(&payload[..2], &0x8001u16.to_be_bytes());
        assert_eq!(SlotRef::from_payload(&payload), Some(sample()));

        let mut cdr_le = payload;
        cdr_le[0] = 0x00;
        cdr_le[1] = 0x01;
        assert_eq!(SlotRef::from_payload(&cdr_le), None);
    }

    #[test]
    fn a_peer_handle_holds_the_bit_until_it_is_dropped() {
        const DOMAIN: u32 = 251;
        unlink_segment(DOMAIN, 0);
        let owned = OwnedSegment::create(DOMAIN, 0, 1, &[(64, 2)], 4).unwrap();
        let peer = Arc::new(PeerSegment::attach(DOMAIN, 0, 1).unwrap());

        let mut lease = owned.owner_mut().acquire(8).unwrap();
        lease.bytes_mut()[..3].copy_from_slice(b"abc");
        let r = owned.owner_mut().commit(lease, 3);

        let handle = ShmSlotHandle::claim(Arc::clone(&peer), &r).unwrap();
        assert_eq!(handle.as_slice(), b"abc");
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
        const DOMAIN: u32 = 252;
        unlink_segment(DOMAIN, 0);
        let owned = Arc::new(OwnedSegment::create(DOMAIN, 0, 1, &[(64, 1)], 4).unwrap());

        let lease = owned.owner_mut().acquire(8).unwrap();
        let r = owned.owner_mut().commit(lease, 3);
        let handle = ShmSlotHandle::own(Arc::clone(&owned), r).unwrap();
        assert!(owned.owner_mut().acquire(8).is_none());

        drop(handle);
        assert!(owned.owner_mut().acquire(8).is_some());

        drop(owned);
        unlink_segment(DOMAIN, 0);
    }
}
