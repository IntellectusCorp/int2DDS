//! Per-participant segment: one mapping holding a header, the receive ring and
//! the payload pool. The owner creates it; peers attach to write the ring and
//! read the pool. The header magic is written last with Release ordering, so
//! a process that attaches mid-initialization sees zero and backs off.

use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::rtps::transport::shm::platform::SharedMemory;
use crate::rtps::transport::shm::pool::{Pool, PoolLayout, PoolOwner, PoolReader};
use crate::rtps::transport::shm::ring::{Notifier, Ring, RingError};
use crate::rtps::transport::shm::{LayoutError, SHARED_ALIGN};

const SEGMENT_MAGIC: u64 = 0x494E_5432_5A43_5347; // "INT2ZCSG"
const SEGMENT_VERSION: u32 = 1;
const HEADER_RESERVE: u64 = 64;

pub(crate) fn segment_name(domain: u32, slot: u32) -> String {
    format!("int2dds_p_d{}_p{}", domain, slot)
}

fn event_name(domain: u32, slot: u32) -> String {
    format!("int2dds_ev_d{}_p{}", domain, slot)
}

#[repr(C, align(64))]
pub(crate) struct SegmentHeader {
    pub magic: AtomicU64,
    pub segment_size: u64,
    pub owner_epoch: u64,
    pub ring_offset: u64,
    pub pool_offset: u64,
    pub version: u32,
    pub owner_slot: u32,
}

// The first thing a peer reads out of the mapping, so its size is wire format.
const _: () = assert!(std::mem::size_of::<SegmentHeader>() == 64);

#[derive(Debug)]
pub(crate) struct SegmentView {
    base: *mut u8,
}

// Safety: the header is written once before publication and read-only after.
unsafe impl Send for SegmentView {}
unsafe impl Sync for SegmentView {}

impl SegmentView {
    /// # Safety
    /// `base` must point to a writable region of at least `size` bytes that no
    /// other process has initialized yet.
    pub(crate) unsafe fn init(
        base: *mut u8,
        size: u64,
        owner_slot: u32,
        owner_epoch: u64,
        ring_offset: u64,
        pool_offset: u64,
    ) -> SegmentView {
        let header = base as *mut SegmentHeader;
        std::ptr::addr_of_mut!((*header).segment_size).write(size);
        std::ptr::addr_of_mut!((*header).owner_epoch).write(owner_epoch);
        std::ptr::addr_of_mut!((*header).ring_offset).write(ring_offset);
        std::ptr::addr_of_mut!((*header).pool_offset).write(pool_offset);
        std::ptr::addr_of_mut!((*header).version).write(SEGMENT_VERSION);
        std::ptr::addr_of_mut!((*header).owner_slot).write(owner_slot);
        (*header).magic.store(SEGMENT_MAGIC, Ordering::Release);
        SegmentView { base }
    }

    /// # Safety
    /// `base` must point to a readable region of at least `mapped_size` bytes
    /// that stays mapped for as long as the returned view is used.
    pub(crate) unsafe fn attach(
        base: *mut u8,
        mapped_size: u64,
    ) -> Result<SegmentView, LayoutError> {
        // Read the gate through a raw projection: another process may still be
        // writing the non-atomic fields.
        let magic = &*std::ptr::addr_of!((*(base as *const SegmentHeader)).magic);
        if magic.load(Ordering::Acquire) != SEGMENT_MAGIC {
            return Err(LayoutError::NotReady);
        }
        let header = &*(base as *const SegmentHeader);
        if header.version != SEGMENT_VERSION {
            return Err(LayoutError::BadVersion);
        }
        if header.segment_size != mapped_size {
            return Err(LayoutError::SizeMismatch);
        }
        // Offsets come from shared memory: bound and align-check them before
        // any pointer math.
        let least = std::mem::size_of::<SegmentHeader>() as u64;
        if header.ring_offset < least
            || header.pool_offset < least
            || header.ring_offset >= mapped_size
            || header.pool_offset >= mapped_size
            || !header.ring_offset.is_multiple_of(SHARED_ALIGN)
            || !header.pool_offset.is_multiple_of(SHARED_ALIGN)
        {
            return Err(LayoutError::BadOffset);
        }
        Ok(SegmentView { base })
    }

    pub(crate) fn header(&self) -> &SegmentHeader {
        unsafe { &*(self.base as *const SegmentHeader) }
    }

    pub(crate) fn ring_ptr(&self) -> *mut u8 {
        unsafe { self.base.add(self.header().ring_offset as usize) }
    }

    pub(crate) fn pool_ptr(&self) -> *mut u8 {
        unsafe { self.base.add(self.header().pool_offset as usize) }
    }
}

pub(crate) struct OwnedSegment {
    // Both mutexes are for threads sharing this segment. Re-locking from the
    // thread that holds a guard deadlocks rather than panics.
    owner: Mutex<PoolOwner>,
    ring: Mutex<Ring>,
    pub(crate) notifier: Notifier,
    pub(crate) slot: u32,
    pub(crate) epoch: u64,
    // Declared last: fields drop in declaration order, and the mapping must
    // outlive everything pointing into it.
    _shm: Arc<SharedMemory>,
}

impl OwnedSegment {
    pub(crate) fn create(
        domain: u32,
        slot: u32,
        epoch: u64,
        pool_size: u64,
        ring_capacity: u32,
    ) -> io::Result<OwnedSegment> {
        let pool_layout = PoolLayout::new(pool_size)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad pool size"))?;
        let ring_offset = HEADER_RESERVE;
        let pool_offset = ring_offset + Ring::size_for(ring_capacity);
        let total = pool_offset + pool_layout.total_size();

        let shm = SharedMemory::new(&segment_name(domain, slot), total as usize, true)?;
        // A leftover segment may have live peers and its own size; never adopt
        // it. The caller unlinks and retries.
        if !shm.is_creator() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "segment already exists; unlink it before creating",
            ));
        }
        let base = shm.as_ptr();
        // Safety: `shm` is freshly created, zeroed and exclusively ours, with
        // room for `total` bytes; both sub-regions lie inside it, and each sets
        // its own magic before the segment magic publishes them together.
        let ring = unsafe { Ring::init(base.add(ring_offset as usize), ring_capacity) }
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad ring capacity"))?;
        let pool = unsafe { Pool::init(base.add(pool_offset as usize), &pool_layout) };
        unsafe { SegmentView::init(base, total, slot, epoch, ring_offset, pool_offset) };

        let notifier = Notifier::create(&event_name(domain, slot), ring.futex_ptr())?;
        let shm = Arc::new(shm);
        Ok(OwnedSegment {
            owner: Mutex::new(PoolOwner::new(
                pool,
                slot,
                epoch,
                Arc::clone(&shm) as Arc<dyn Send + Sync>,
            )),
            ring: Mutex::new(ring),
            notifier,
            slot,
            epoch,
            _shm: shm,
        })
    }

    /// Poison is ignored: every shared field is an atomic in the mapping and
    /// the free list cannot tear during an unwind.
    pub(crate) fn owner_mut(&self) -> MutexGuard<'_, PoolOwner> {
        self.owner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn ring_mut(&self) -> MutexGuard<'_, Ring> {
        self.ring.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sleeps until a message may be waiting or `timeout` elapses. A return is
    /// not a promise; callers re-check the ring after every return.
    pub(crate) fn wait_for_message(&self, timeout: Duration) {
        // Sample before testing the condition, or a signal landing between the
        // test and the sleep is lost on the futex backend.
        let token = self.notifier.prepare_wait();
        {
            let ring = self.ring_mut();
            ring.waiters().fetch_add(1, Ordering::SeqCst);
            // Store-buffering pair with `push_and_signal`.
            std::sync::atomic::fence(Ordering::SeqCst);
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
    // Declared last: see `OwnedSegment`.
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
        // Map the header alone to learn the real size, then remap exactly.
        let probe = SharedMemory::new(name, HEADER_RESERVE as usize, false)?;
        // Safety: `probe` maps exactly one header. The magic is read through a
        // raw projection, and the Acquire load orders the size read after the
        // owner's Release publish.
        let declared = unsafe {
            let header_ptr = probe.as_ptr() as *const SegmentHeader;
            let magic = &*std::ptr::addr_of!((*header_ptr).magic);
            if magic.load(Ordering::Acquire) != SEGMENT_MAGIC {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "segment not ready"));
            }
            std::ptr::addr_of!((*header_ptr).segment_size).read()
        };
        drop(probe);

        let shm = SharedMemory::new(name, declared as usize, false)?;
        // Safety: `shm` maps exactly `declared` bytes, and `attach` bounds the
        // offsets the sub-region attaches below rely on.
        let view = unsafe { SegmentView::attach(shm.as_ptr(), declared) }.map_err(|e| match e {
            LayoutError::NotReady => io::Error::new(io::ErrorKind::WouldBlock, "segment not ready"),
            _ => io::Error::other("segment layout mismatch"),
        })?;
        let ring_bytes = view.header().pool_offset.saturating_sub(view.header().ring_offset);
        let ring = unsafe { Ring::attach(view.ring_ptr(), ring_bytes) }
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "ring not ready"))?;
        let pool_bytes = declared.saturating_sub(view.header().pool_offset);
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
        std::sync::atomic::fence(Ordering::SeqCst);
        if self.ring.waiters().load(Ordering::SeqCst) > 0 {
            self.notifier.signal();
        }
        Ok(())
    }
}

/// Best effort; Unix only. On Windows the object disappears with its last handle.
pub(crate) fn unlink_segment(domain: u32, slot: u32) {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        if let Ok(name) = CString::new(format!("/{}", segment_name(domain, slot))) {
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
    use crate::rtps::transport::shm::pool::TEST_POOL_SIZE;
    use crate::rtps::transport::shm::ring::{notify_supported, RING_INLINE, SPILL_NONE};
    use crate::rtps::transport::shm::slot::SlotRef;
    use crate::rtps::transport::shm::test_region::AlignedRegion;

    const DOMAIN: u32 = 231;

    #[test]
    fn view_attach_gates_on_the_magic_and_exposes_offsets() {
        let r = AlignedRegion::new(4096);
        let err = unsafe { SegmentView::attach(r.ptr(), 4096) }.unwrap_err();
        assert_eq!(err, LayoutError::NotReady);

        unsafe { SegmentView::init(r.ptr(), 4096, 7, 3, 64, 1024) };
        let view = unsafe { SegmentView::attach(r.ptr(), 4096) }.unwrap();
        assert_eq!(view.header().owner_slot, 7);
        assert_eq!(view.header().owner_epoch, 3);
        assert_eq!(view.ring_ptr(), unsafe { r.ptr().add(64) });
        assert_eq!(view.pool_ptr(), unsafe { r.ptr().add(1024) });
    }

    #[test]
    fn view_attach_rejects_a_header_it_cannot_trust() {
        let r = AlignedRegion::new(4096);
        unsafe { SegmentView::init(r.ptr(), 4096, 0, 1, 64, 1024) };
        let err = unsafe { SegmentView::attach(r.ptr(), 2048) }.unwrap_err();
        assert_eq!(err, LayoutError::SizeMismatch);

        unsafe { SegmentView::init(r.ptr(), 4096, 0, 1, 64, 8192) };
        let err = unsafe { SegmentView::attach(r.ptr(), 4096) }.unwrap_err();
        assert_eq!(err, LayoutError::BadOffset);

        unsafe { SegmentView::init(r.ptr(), 4096, 0, 1, 64, 1025) };
        let err = unsafe { SegmentView::attach(r.ptr(), 4096) }.unwrap_err();
        assert_eq!(err, LayoutError::BadOffset);
    }

    #[test]
    fn owner_and_peer_share_one_segment() {
        unlink_segment(DOMAIN, 0);
        let owned = OwnedSegment::create(DOMAIN, 0, 1, TEST_POOL_SIZE, 4).unwrap();
        let peer = PeerSegment::attach(DOMAIN, 0, 1).unwrap();
        assert!(PeerSegment::attach(DOMAIN, 63, 1).is_err(), "no such segment");

        let mut lease = { owned.owner_mut().acquire(8) }.unwrap();
        lease.bytes_mut()[..4].copy_from_slice(b"ping");
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
    fn a_peer_push_wakes_the_owner() {
        if !notify_supported() {
            return;
        }
        unlink_segment(DOMAIN, 2);
        let owned = Arc::new(OwnedSegment::create(DOMAIN, 2, 1, TEST_POOL_SIZE, 4).unwrap());
        let peer = PeerSegment::attach(DOMAIN, 2, 3).unwrap();

        let waiter = {
            let owned = Arc::clone(&owned);
            std::thread::spawn(move || {
                let start = Instant::now();
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
    fn waiting_returns_at_once_when_a_message_is_already_queued() {
        unlink_segment(DOMAIN, 4);
        let owned = OwnedSegment::create(DOMAIN, 4, 1, TEST_POOL_SIZE, 4).unwrap();
        let peer = PeerSegment::attach(DOMAIN, 4, 5).unwrap();
        peer.push_and_signal(b"early", SPILL_NONE).unwrap();

        let start = Instant::now();
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
        let owned = OwnedSegment::create(DOMAIN, 6, 1, TEST_POOL_SIZE, 4).unwrap();

        let start = Instant::now();
        owned.wait_for_message(Duration::from_millis(50));
        assert!(start.elapsed() >= Duration::from_millis(40));
        assert_eq!(owned.ring_mut().waiters().load(Ordering::SeqCst), 0);

        drop(owned);
        unlink_segment(DOMAIN, 6);
    }
}
