//! Per-participant segment: one mapping holding the receive ring and the
//! payload pool. The owner creates it; peers attach to write the ring and read
//! the pool.

use std::io;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::rtps::transport::shm::layout::{LayoutError, SegmentView};
use crate::rtps::transport::shm::notify::Notifier;
use crate::rtps::transport::shm::platform::SharedMemory;
use crate::rtps::transport::shm::pool::{Pool, PoolLayout};
use crate::rtps::transport::shm::pool_owner::PoolOwner;
use crate::rtps::transport::shm::pool_reader::PoolReader;
use crate::rtps::transport::shm::ring::{Ring, RingError};

const HEADER_RESERVE: u64 = 64;

pub(crate) fn registry_name(domain: u32) -> String {
    format!("int2dds_reg_d{}", domain)
}

pub(crate) fn segment_name(domain: u32, slot: u32) -> String {
    format!("int2dds_p_d{}_p{}", domain, slot)
}

pub(crate) fn event_name(domain: u32, slot: u32) -> String {
    format!("int2dds_ev_d{}_p{}", domain, slot)
}

pub(crate) struct OwnedSegment {
    owner: Mutex<PoolOwner>,
    // Mutual exclusion between threads sharing this `OwnedSegment`, not a
    // borrow-checker workaround. Re-locking from the same thread while a guard
    // is held will not reliably panic the way `RefCell` does -- `Mutex::lock`
    // leaves that case unspecified, and both backends here deadlock instead.
    // Same shape as `owner`.
    ring: Mutex<Ring>,
    pub(crate) notifier: Notifier,
    pub(crate) slot: u32,
    pub(crate) epoch: u64,
    // Declared last on purpose: fields drop in declaration order, so the
    // mapping must outlive everything that points into it.
    _shm: SharedMemory,
}

impl OwnedSegment {
    pub(crate) fn create(
        domain: u32,
        slot: u32,
        epoch: u64,
        classes: &[(u32, u32)],
        ring_capacity: u32,
    ) -> io::Result<OwnedSegment> {
        let pool_layout = PoolLayout::new(classes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad pool class config"))?;
        let ring_offset = HEADER_RESERVE;
        let pool_offset = ring_offset + Ring::size_for(ring_capacity);
        let total = pool_offset + pool_layout.total_size();

        let shm = SharedMemory::new(&segment_name(domain, slot), total as usize, true)?;
        // Never adopt a leftover segment. A crashed owner's segment survives
        // with its own size and its own live peers; mapping `total` over a
        // smaller object faults on first write, and a matching size would mean
        // re-initializing memory those peers are still reading. Reclaiming is
        // the caller's job: `unlink_segment`, then retry.
        if !shm.is_creator() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "segment already exists; unlink it before creating",
            ));
        }
        let base = shm.as_ptr();
        // Safety: `shm` was just created (OS-zeroed, exclusively ours so far) with
        // room for at least `total` bytes, and `ring_offset` places the ring
        // entirely inside that region.
        let ring = unsafe { Ring::init(base.add(ring_offset as usize), ring_capacity) }
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad ring capacity"))?;
        // Safety: same region argument as above; `pool_offset` places the pool
        // entirely inside the mapping, after the ring.
        let pool = unsafe { Pool::init(base.add(pool_offset as usize), &pool_layout) };
        // Safety: the ring and the pool are fully initialized above -- each set
        // its own magic last -- before this call publishes the segment magic,
        // so a peer that observes the segment as ready also observes both
        // sub-regions as ready. `base` addresses the whole mapping and no other
        // process has published this segment's header yet.
        unsafe { SegmentView::init(base, total, slot, epoch, ring_offset, pool_offset) };

        let notifier = Notifier::create(&event_name(domain, slot), ring.futex_ptr())?;
        Ok(OwnedSegment {
            owner: Mutex::new(PoolOwner::new(pool, slot, epoch)),
            ring: Mutex::new(ring),
            notifier,
            slot,
            epoch,
            _shm: shm,
        })
    }

    pub(crate) fn owner_mut(&self) -> MutexGuard<'_, PoolOwner> {
        // Poison is ignored on purpose: every shared field lives in the mapping
        // as an atomic, and the process-local free list cannot tear during an
        // unwind. Honouring poison would turn one recoverable panic into a
        // participant that can never touch its own pool again.
        self.owner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn ring_mut(&self) -> MutexGuard<'_, Ring> {
        // Same reasoning as `owner_mut`.
        self.ring.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sleeps until a message may be waiting or `timeout` elapses. A return is
    /// not a promise: a signal that arrives as this returns early leaves the
    /// wakeup latched, so the next call returns at once with nothing to pop.
    /// Callers must re-check the ring after every return.
    pub(crate) fn wait_for_message(&self, timeout: Duration) {
        // Sample before testing the condition, or a signal landing between the
        // test and the sleep is lost on the futex backend.
        let token = self.notifier.prepare_wait();
        {
            let ring = self.ring_mut();
            ring.waiters().fetch_add(1, Ordering::SeqCst);
            // Store-buffering pair with `push_and_signal`: without a total
            // order over `waiters` and the ring, both sides may read stale and
            // no signal is ever sent.
            std::sync::atomic::fence(Ordering::SeqCst);
            // Re-check after registering: a producer that pushed while we were
            // not yet visible as a waiter sends no signal at all.
            if !ring.is_empty() {
                ring.waiters().fetch_sub(1, Ordering::SeqCst);
                return;
            }
        }
        self.notifier.wait_if(token, timeout);
        self.ring_mut().waiters().fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) struct PeerSegment {
    pub(crate) reader: PoolReader,
    pub(crate) ring: Ring,
    pub(crate) notifier: Notifier,
    pub(crate) epoch: u64,
    // Declared last on purpose: fields drop in declaration order, so the
    // mapping must outlive everything that points into it.
    _shm: SharedMemory,
}

impl PeerSegment {
    pub(crate) fn attach(domain: u32, slot: u32, my_slot: u32) -> io::Result<PeerSegment> {
        let name = segment_name(domain, slot);
        let deadline = Instant::now() + Duration::from_millis(100);
        loop {
            match Self::try_attach(domain, slot, my_slot, &name) {
                Ok(seg) => return Ok(seg),
                Err(e) if Instant::now() < deadline => {
                    if e.kind() != io::ErrorKind::WouldBlock {
                        return Err(e);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn try_attach(domain: u32, slot: u32, my_slot: u32, name: &str) -> io::Result<PeerSegment> {
        use crate::rtps::transport::shm::layout::{SegmentHeader, SEGMENT_MAGIC};
        use std::sync::atomic::Ordering;

        // Map the header alone to learn the real size, then remap exactly.
        let probe = SharedMemory::new(name, HEADER_RESERVE as usize, false)?;
        let declared = unsafe {
            // Safety: `probe` maps `HEADER_RESERVE` == `size_of::<SegmentHeader>()`
            // bytes, so `header_ptr` is valid for reads of that size. The magic
            // field is reached through a raw projection rather than a
            // struct-wide `&SegmentHeader`, so this does not assert that no one
            // else is writing to the rest of the header -- which is exactly
            // what an owner mid-`init` is doing until it publishes the magic.
            let header_ptr = probe.as_ptr() as *const SegmentHeader;
            let magic = &*std::ptr::addr_of!((*header_ptr).magic);
            if magic.load(Ordering::Acquire) != SEGMENT_MAGIC {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "segment not ready"));
            }
            // Safety: the Acquire load above observed SEGMENT_MAGIC, which
            // `SegmentView::init` stores with Release ordering only after
            // `segment_size` (and every other header field) has already been
            // written. That establishes happens-before, so this plain read of
            // `segment_size` sees a fully published value, not a torn one.
            std::ptr::addr_of!((*header_ptr).segment_size).read()
        };
        drop(probe);

        let shm = SharedMemory::new(name, declared as usize, false)?;
        // Safety: `shm` maps exactly `declared` bytes, the size this same
        // segment reported through its own gated header above.
        let view = unsafe { SegmentView::attach(shm.as_ptr(), declared) }.map_err(|e| match e {
            LayoutError::NotReady => io::Error::new(io::ErrorKind::WouldBlock, "segment not ready"),
            _ => io::Error::other("segment layout mismatch"),
        })?;
        // The ring occupies exactly the bytes between the two offsets.
        let ring_bytes = view.header().pool_offset.saturating_sub(view.header().ring_offset);
        // Safety: `view` proved `ring_offset` falls inside the mapping, so
        // `ring_ptr()` addresses memory the owner initialized as a ring (or is
        // still initializing, which `Ring::attach`'s own gate handles), and
        // `ring_bytes` never reaches past `pool_offset`, itself inside it.
        let ring = unsafe { Ring::attach(view.ring_ptr(), ring_bytes) }
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "ring not ready"))?;
        let pool_bytes = declared.saturating_sub(view.header().pool_offset);
        // Safety: `view` proved `pool_offset` falls inside the mapping, and
        // `pool_bytes` is exactly the bytes remaining from there to the end of
        // it, so `Pool::attach` cannot read past the mapping.
        let pool = unsafe { Pool::attach(view.pool_ptr(), pool_bytes) }
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "pool not ready"))?;
        let notifier = Notifier::open(&event_name(domain, slot), ring.futex_ptr())?;
        let epoch = view.header().owner_epoch;
        Ok(PeerSegment {
            reader: PoolReader::new(pool, my_slot, slot as u16, epoch as u16),
            ring,
            notifier,
            epoch,
            _shm: shm,
        })
    }

    /// Pushes into the owner's ring and wakes it only if it is waiting.
    pub(crate) fn push_and_signal(&self, payload: &[u8], spill: u32) -> Result<(), RingError> {
        self.ring.push(payload, spill)?;
        // Pairs with the fence in `wait_for_message`; see the note there.
        std::sync::atomic::fence(Ordering::SeqCst);
        if self.ring.waiters().load(Ordering::SeqCst) > 0 {
            self.notifier.signal();
        }
        Ok(())
    }
}

pub(crate) fn unlink_segment(domain: u32, slot: u32) {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        if let Ok(name) = CString::new(format!("/{}", segment_name(domain, slot))) {
            // Safety: `name` is a valid NUL-terminated C string kept alive for
            // the call. `shm_unlink` on a name that does not exist (already
            // unlinked, or never created) just returns an error this function
            // ignores by design -- callers use it as a best-effort cleanup.
            unsafe { libc::shm_unlink(name.as_ptr()) };
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (domain, slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::notify::notify_supported;
    use crate::rtps::transport::shm::ring::{RING_INLINE, SPILL_NONE};
    use crate::rtps::transport::shm::slot_ref::SlotRef;

    const DOMAIN: u32 = 231;

    #[test]
    fn owner_and_peer_share_one_segment() {
        // `OwnedSegment::create` needs a notifier, so there is nothing to
        // exercise where notification is unsupported.
        if !notify_supported() {
            return;
        }
        // A leftover segment is no longer adopted, so clear one first.
        unlink_segment(DOMAIN, 0);
        let owned = OwnedSegment::create(DOMAIN, 0, 1, &[(64, 2)], 4).unwrap();
        let peer = PeerSegment::attach(DOMAIN, 0, 1).unwrap();

        let lease = { owned.owner_mut().acquire(8) }.unwrap();
        owned.owner_mut().slot_mut(&lease)[..4].copy_from_slice(b"ping");
        let r = owned.owner_mut().commit(lease, 4);

        peer.ring.push(&r.encode(), SPILL_NONE).unwrap();
        let mut out = [0u8; RING_INLINE];
        let (len, _) = owned.ring_mut().pop(&mut out).unwrap();
        let decoded = SlotRef::decode(&out[..len as usize]).unwrap();

        let claimed = peer.reader.claim(&decoded).unwrap();
        assert_eq!(peer.reader.bytes(&claimed), b"ping");
        peer.reader.release(claimed);

        drop(peer);
        drop(owned);
        unlink_segment(DOMAIN, 0);
    }

    #[test]
    fn attach_to_missing_segment_fails() {
        assert!(PeerSegment::attach(DOMAIN, 63, 1).is_err());
    }

    #[test]
    fn a_peer_push_wakes_the_owner() {
        if !notify_supported() {
            return;
        }
        unlink_segment(DOMAIN, 2);
        let owned = std::sync::Arc::new(OwnedSegment::create(DOMAIN, 2, 1, &[(64, 2)], 4).unwrap());
        let peer = PeerSegment::attach(DOMAIN, 2, 3).unwrap();

        let waiter = {
            let owned = std::sync::Arc::clone(&owned);
            std::thread::spawn(move || {
                let start = std::time::Instant::now();
                owned.wait_for_message(Duration::from_secs(5));
                start.elapsed()
            })
        };
        std::thread::sleep(Duration::from_millis(50));
        peer.push_and_signal(b"wake", SPILL_NONE).unwrap();

        let waited = waiter.join().unwrap();
        assert!(waited < Duration::from_secs(1), "owner was not woken: {waited:?}");

        let mut out = [0u8; RING_INLINE];
        let (len, _) = owned.ring_mut().pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"wake");
        assert_eq!(owned.ring_mut().waiters().load(Ordering::SeqCst), 0);

        drop(peer);
        drop(owned);
        unlink_segment(DOMAIN, 2);
    }

    #[test]
    fn waiting_returns_immediately_when_the_ring_already_has_a_message() {
        if !notify_supported() {
            return;
        }
        unlink_segment(DOMAIN, 4);
        let owned = OwnedSegment::create(DOMAIN, 4, 1, &[(64, 2)], 4).unwrap();
        let peer = PeerSegment::attach(DOMAIN, 4, 5).unwrap();
        peer.push_and_signal(b"early", SPILL_NONE).unwrap();

        let start = std::time::Instant::now();
        owned.wait_for_message(Duration::from_secs(5));
        assert!(start.elapsed() < Duration::from_secs(1), "wait did not re-check the ring");
        assert_eq!(owned.ring_mut().waiters().load(Ordering::SeqCst), 0);

        drop(peer);
        drop(owned);
        unlink_segment(DOMAIN, 4);
    }

    #[test]
    fn waiting_times_out_on_an_empty_ring_and_leaves_no_waiter() {
        if !notify_supported() {
            return;
        }
        unlink_segment(DOMAIN, 6);
        let owned = OwnedSegment::create(DOMAIN, 6, 1, &[(64, 2)], 4).unwrap();

        let start = Instant::now();
        owned.wait_for_message(Duration::from_millis(50));
        assert!(start.elapsed() >= Duration::from_millis(40), "should have slept");
        assert_eq!(owned.ring_mut().waiters().load(Ordering::SeqCst), 0);

        drop(owned);
        unlink_segment(DOMAIN, 6);
    }

    #[test]
    fn owned_segment_is_sync() {
        fn assert_sync<T: Sync + Send>() {}
        assert_sync::<OwnedSegment>();
    }
}
