//! The descriptor queue and its wakeup.
//!
//! `Ring` is a bounded MPSC queue of fixed-size cells; each cell carries its
//! own sequence marker, so a consumer only observes a cell after its writer
//! published it. `Notifier` wakes the consumer: a named auto-reset event on
//! Windows, a futex on a word inside the ring header on Linux, and polling
//! elsewhere.

use std::io;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use crate::rtps::transport::shm::LayoutError;

pub(crate) const RING_INLINE: usize = 240;
pub(crate) const SPILL_NONE: u32 = u32::MAX;
const RING_MAGIC: u64 = 0x494E_5432_5A43_524E; // "INT2ZCRN"

#[repr(C, align(64))]
pub(crate) struct RingEntry {
    pub seq: AtomicU64,
    pub len: u32,
    pub spill: u32,
    pub inline: [u8; RING_INLINE],
}

// Cells are addressed by index across processes, so the stride is wire format.
const _: () = assert!(std::mem::size_of::<RingEntry>() == 256);

#[repr(C, align(64))]
pub(crate) struct RingHeader {
    pub magic: AtomicU64,
    pub enqueue_pos: AtomicU64,
    pub dequeue_pos: AtomicU64,
    pub capacity: u32,
    pub waiters: AtomicU32,
    pub futex_word: AtomicU32,
    pub _pad: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RingError {
    Full,
    /// The cell was reclaimed by `recover_if_wedged` while this producer was
    /// preempted; the payload was not published.
    Preempted,
}

pub(crate) struct Ring {
    base: *mut u8,
    capacity: u64,
    mask: u64,
}

// Safety: a producer only touches a cell it claimed via CAS on `enqueue_pos`,
// and a consumer only reads a cell after `seq` proves the write is published.
unsafe impl Send for Ring {}
unsafe impl Sync for Ring {}

const RING_HEADER_SIZE: u64 = std::mem::size_of::<RingHeader>() as u64;

impl Ring {
    pub(crate) fn size_for(capacity: u32) -> u64 {
        RING_HEADER_SIZE + std::mem::size_of::<RingEntry>() as u64 * capacity as u64
    }

    /// # Safety
    /// `base` must point to a zeroed writable region of `size_for(capacity)` bytes.
    pub(crate) unsafe fn init(base: *mut u8, capacity: u32) -> Result<Ring, LayoutError> {
        if !capacity.is_power_of_two() {
            return Err(LayoutError::BadConfig);
        }
        let header = base as *mut RingHeader;
        std::ptr::addr_of_mut!((*header).capacity).write(capacity);
        (*header).enqueue_pos.store(0, Ordering::Relaxed);
        (*header).dequeue_pos.store(0, Ordering::Relaxed);
        (*header).waiters.store(0, Ordering::Relaxed);
        (*header).futex_word.store(0, Ordering::Relaxed);
        let ring = Ring { base, capacity: capacity as u64, mask: capacity as u64 - 1 };
        for i in 0..capacity as u64 {
            ring.seq_of(i).store(i, Ordering::Relaxed);
        }
        (*header).magic.store(RING_MAGIC, Ordering::Release);
        Ok(ring)
    }

    /// # Safety
    /// `base` must point to a region initialized by `init` that is at least
    /// `region_size` bytes and stays mapped while the ring is used.
    pub(crate) unsafe fn attach(base: *mut u8, region_size: u64) -> Result<Ring, LayoutError> {
        if region_size < RING_HEADER_SIZE {
            return Err(LayoutError::SizeMismatch);
        }
        // Read the gate through a raw projection: the owner may still be
        // writing the rest of the header.
        let magic = &*std::ptr::addr_of!((*(base as *const RingHeader)).magic);
        if magic.load(Ordering::Acquire) != RING_MAGIC {
            return Err(LayoutError::NotReady);
        }
        let header = &*(base as *const RingHeader);
        let capacity = header.capacity as u64;
        if capacity == 0 || !capacity.is_power_of_two() {
            return Err(LayoutError::BadVersion);
        }
        // `capacity` comes from shared memory and feeds pointer math.
        if Ring::size_for(header.capacity) > region_size {
            return Err(LayoutError::BadOffset);
        }
        Ok(Ring { base, capacity, mask: capacity - 1 })
    }

    fn header(&self) -> &RingHeader {
        unsafe { &*(self.base as *const RingHeader) }
    }

    fn entry_ptr(&self, pos: u64) -> *mut RingEntry {
        unsafe {
            let entries = self.base.add(RING_HEADER_SIZE as usize) as *mut RingEntry;
            entries.add((pos & self.mask) as usize)
        }
    }

    /// Reached without forming a `&RingEntry`: a producer may still be
    /// writing the non-atomic body.
    fn seq_of(&self, pos: u64) -> &AtomicU64 {
        unsafe { &*std::ptr::addr_of!((*self.entry_ptr(pos)).seq) }
    }

    pub(crate) fn push(&self, payload: &[u8], spill: u32) -> Result<(), RingError> {
        if payload.len() > RING_INLINE {
            return Err(RingError::Full);
        }
        let header = self.header();
        let pos = loop {
            let pos = header.enqueue_pos.load(Ordering::Relaxed);
            let seq = self.seq_of(pos).load(Ordering::Acquire);
            let dif = seq as i64 - pos as i64;
            if dif == 0 {
                if header
                    .enqueue_pos
                    .compare_exchange_weak(pos, pos + 1, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    break pos;
                }
            } else if dif < 0 {
                return Err(RingError::Full);
            }
            std::hint::spin_loop();
        };

        // Safety: the CAS claimed this cell exclusively until the publishing
        // store below.
        unsafe {
            let e = self.entry_ptr(pos);
            std::ptr::addr_of_mut!((*e).len).write(payload.len() as u32);
            std::ptr::addr_of_mut!((*e).spill).write(spill);
            std::ptr::copy_nonoverlapping(
                payload.as_ptr(),
                std::ptr::addr_of_mut!((*e).inline) as *mut u8,
                payload.len(),
            );
        }
        // A CAS, not a store: `recover_if_wedged` may have freed this cell
        // while we were preempted, and overwriting its marker would close the
        // cell for good.
        if self
            .seq_of(pos)
            .compare_exchange(pos, pos + 1, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            return Err(RingError::Preempted);
        }
        Ok(())
    }

    pub(crate) fn pop(&mut self, out: &mut [u8; RING_INLINE]) -> Option<(u32, u32)> {
        let header = self.header();
        let pos = header.dequeue_pos.load(Ordering::Relaxed);
        if self.seq_of(pos).load(Ordering::Acquire) != pos + 1 {
            self.recover_if_wedged(pos);
            return None;
        }
        // Safety: the marker synchronizes with the producer's publishing store,
        // and no producer may claim this cell again until the store below.
        let (len, spill) = unsafe {
            let raw = self.entry_ptr(pos);
            let len = std::ptr::addr_of!((*raw).len).read();
            let spill = std::ptr::addr_of!((*raw).spill).read();
            let len_clamped = (len as usize).min(RING_INLINE);
            std::ptr::copy_nonoverlapping(
                std::ptr::addr_of!((*raw).inline) as *const u8,
                out.as_mut_ptr(),
                len_clamped,
            );
            (len_clamped as u32, spill)
        };
        self.seq_of(pos).store(pos + self.capacity, Ordering::Release);
        header.dequeue_pos.store(pos + 1, Ordering::Release);
        Some((len, spill))
    }

    /// A producer that claimed this cell and died before publishing leaves
    /// `seq == pos` for good, and every cell behind it unreachable. Free it
    /// only once a full lap has been claimed since; the CAS leaves it alone
    /// if the producer publishes after all.
    fn recover_if_wedged(&mut self, pos: u64) {
        let header = self.header();
        if self.seq_of(pos).load(Ordering::Acquire) != pos {
            return;
        }
        if header.enqueue_pos.load(Ordering::Acquire).wrapping_sub(pos) < self.capacity {
            return;
        }
        if self
            .seq_of(pos)
            .compare_exchange(pos, pos + self.capacity, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        header.dequeue_pos.store(pos + 1, Ordering::Release);
    }

    pub(crate) fn futex_ptr(&self) -> *const AtomicU32 {
        &self.header().futex_word as *const AtomicU32
    }

    pub(crate) fn waiters(&self) -> &AtomicU32 {
        &self.header().waiters
    }

    /// Does not recover a wedged cell; only `pop` does.
    pub(crate) fn is_empty(&self) -> bool {
        let pos = self.header().dequeue_pos.load(Ordering::Relaxed);
        self.seq_of(pos).load(Ordering::Acquire) != pos + 1
    }
}

/// How long a polling waiter sleeps before handing control back.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_micros(10);

/// Whether this platform has a kernel wakeup. Without one SHM still runs,
/// polling instead of sleeping.
pub(crate) fn notify_supported() -> bool {
    cfg!(any(windows, target_os = "linux"))
}

pub(crate) struct Notifier {
    #[cfg(windows)]
    handle: winapi::shared::ntdef::HANDLE,
    #[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
    futex: *const AtomicU32,
    polling: bool,
}

// Safety: the event handle is safe to share across threads per WinAPI, and
// `futex` points into a mapping that outlives the notifier and is only
// touched atomically.
unsafe impl Send for Notifier {}
unsafe impl Sync for Notifier {}

impl Notifier {
    pub(crate) fn create(name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        Self::make(name, futex)
    }

    pub(crate) fn open(name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        Self::make(name, futex)
    }

    #[cfg(windows)]
    fn make(name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        use std::ffi::CString;
        use winapi::um::synchapi::CreateEventA;
        let cname = CString::new(format!("Local\\{}", name))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        // CreateEventA opens the existing event when the name is taken, which
        // is why `create` and `open` share this path.
        let handle = unsafe {
            CreateEventA(std::ptr::null_mut(), 0 /* auto-reset */, 0, cname.as_ptr())
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Notifier { handle, futex, polling: false })
    }

    #[cfg(target_os = "linux")]
    fn make(_name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        Ok(Notifier { futex, polling: false })
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn make(_name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        Ok(Notifier { futex, polling: true })
    }

    /// The polling backend on a platform that has a real one, so the path the
    /// notification-less platforms take is exercised where tests run.
    #[cfg(test)]
    pub(crate) fn create_polling(name: &str, futex: *const AtomicU32) -> io::Result<Notifier> {
        let mut n = Self::make(name, futex)?;
        n.polling = true;
        Ok(n)
    }

    pub(crate) fn signal(&self) {
        if self.polling {
            return;
        }
        #[cfg(windows)]
        unsafe {
            winapi::um::synchapi::SetEvent(self.handle);
        }
        #[cfg(target_os = "linux")]
        unsafe {
            (*self.futex).fetch_add(1, Ordering::Release);
            libc::syscall(libc::SYS_futex, self.futex, libc::FUTEX_WAKE, 1i32, 0, 0, 0);
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let _ = self.futex;
        }
    }

    /// Sample the wakeup word before testing the condition; pass the result to
    /// `wait_if` so a signal landing in between is not lost.
    pub(crate) fn prepare_wait(&self) -> u32 {
        #[cfg(target_os = "linux")]
        unsafe {
            (*self.futex).load(Ordering::Acquire)
        }
        #[cfg(not(target_os = "linux"))]
        0
    }

    pub(crate) fn wait_if(&self, expected: u32, timeout: Duration) {
        if self.polling {
            let _ = expected;
            std::thread::sleep(timeout.min(POLL_INTERVAL));
            return;
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        let _ = timeout;
        #[cfg(windows)]
        unsafe {
            // Auto-reset: a signal that arrived before this call is latched.
            let _ = expected;
            winapi::um::synchapi::WaitForSingleObject(self.handle, timeout.as_millis() as u32);
        }
        #[cfg(target_os = "linux")]
        unsafe {
            let ts = libc::timespec {
                tv_sec: timeout.as_secs() as libc::time_t,
                tv_nsec: timeout.subsec_nanos() as libc::c_long,
            };
            libc::syscall(
                libc::SYS_futex,
                self.futex,
                libc::FUTEX_WAIT,
                expected as i32,
                &ts as *const libc::timespec,
                0,
                0,
            );
        }
    }
}

#[cfg(windows)]
impl Drop for Notifier {
    fn drop(&mut self) {
        unsafe {
            winapi::um::handleapi::CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::test_region::AlignedRegion;

    fn ring(capacity: u32) -> (AlignedRegion, Ring) {
        let region = AlignedRegion::new(Ring::size_for(capacity) as usize);
        let r = unsafe { Ring::init(region.ptr(), capacity) }.unwrap();
        (region, r)
    }

    #[test]
    fn push_then_pop_round_trips_and_an_empty_ring_pops_none() {
        let (_g, mut r) = ring(4);
        let mut out = [0u8; RING_INLINE];
        assert!(r.pop(&mut out).is_none());
        r.push(b"hello", 7).unwrap();
        let (len, spill) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"hello");
        assert_eq!(spill, 7);
        assert!(r.pop(&mut out).is_none());
    }

    #[test]
    fn a_full_ring_reports_full_and_never_overwrites() {
        let (_g, mut r) = ring(2);
        r.push(b"a", SPILL_NONE).unwrap();
        r.push(b"b", SPILL_NONE).unwrap();
        assert_eq!(r.push(b"c", SPILL_NONE), Err(RingError::Full));
        assert_eq!(r.push(&[0u8; RING_INLINE + 1], SPILL_NONE), Err(RingError::Full));
        let mut out = [0u8; RING_INLINE];
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"a");
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"b");
    }

    #[test]
    fn attach_validates_the_geometry_it_reads() {
        let region = AlignedRegion::new(Ring::size_for(8) as usize);
        for capacity in [0u32, 3, 100] {
            let got = unsafe { Ring::init(region.ptr(), capacity) };
            assert!(matches!(got, Err(LayoutError::BadConfig)), "capacity {capacity}");
        }
        unsafe { Ring::init(region.ptr(), 4) }.unwrap();
        let mut r = unsafe { Ring::attach(region.ptr(), Ring::size_for(4)) }.unwrap();
        r.push(b"z", SPILL_NONE).unwrap();
        let mut out = [0u8; RING_INLINE];
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"z");

        // A corrupt capacity whose entry array runs past the mapping.
        unsafe {
            std::ptr::addr_of_mut!((*(region.ptr() as *mut RingHeader)).capacity).write(1 << 20);
        }
        let got = unsafe { Ring::attach(region.ptr(), Ring::size_for(4)) };
        assert!(matches!(got, Err(LayoutError::BadOffset)));
    }

    #[test]
    fn concurrent_producers_lose_nothing() {
        const PRODUCERS: usize = 4;
        const PER_PRODUCER: usize = 500;
        let region = AlignedRegion::new(Ring::size_for(64) as usize);
        let produce = unsafe { Ring::init(region.ptr(), 64) }.unwrap();
        let mut consume = unsafe { Ring::attach(region.ptr(), Ring::size_for(64)) }.unwrap();

        let mut seen = vec![0usize; PRODUCERS];
        let mut out = [0u8; RING_INLINE];
        let total = PRODUCERS * PER_PRODUCER;

        std::thread::scope(|s| {
            for p in 0..PRODUCERS {
                let produce = &produce;
                s.spawn(move || {
                    for n in 0..PER_PRODUCER {
                        let msg = [p as u8, (n & 0xFF) as u8, (n >> 8) as u8];
                        while produce.push(&msg, SPILL_NONE).is_err() {
                            std::hint::spin_loop();
                        }
                    }
                });
            }

            let mut got = 0;
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            while got < total {
                if let Some((len, _)) = consume.pop(&mut out) {
                    assert_eq!(len, 3);
                    seen[out[0] as usize] += 1;
                    got += 1;
                } else if std::time::Instant::now() > deadline {
                    panic!("timed out after {got} of {total} entries");
                }
            }
        });

        assert_eq!(seen, vec![PER_PRODUCER; PRODUCERS]);
    }

    #[test]
    fn a_cell_claimed_and_never_published_is_freed_after_a_full_lap() {
        let (_region, mut r) = ring(4);
        // A producer claimed cell 0 and died before publishing.
        r.header().enqueue_pos.store(1, Ordering::Release);
        for i in 0..3u8 {
            r.push(&[i], SPILL_NONE).unwrap();
        }

        let mut out = [0u8; RING_INLINE];
        assert!(r.pop(&mut out).is_none(), "the wedged cell yields nothing");
        let (len, _) = r.pop(&mut out).expect("the cell behind it becomes reachable");
        assert_eq!(&out[..len as usize], &[0u8]);

        while r.pop(&mut out).is_some() {}
        r.push(b"after", SPILL_NONE).unwrap();
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"after");
    }

    #[test]
    fn a_late_publish_after_recovery_loses_only_its_own_sample() {
        let (_region, mut r) = ring(4);
        r.header().enqueue_pos.store(1, Ordering::Release);
        for i in 0..3u8 {
            r.push(&[i], SPILL_NONE).unwrap();
        }

        let mut out = [0u8; RING_INLINE];
        assert!(r.pop(&mut out).is_none());
        assert_eq!(
            r.seq_of(0).compare_exchange(0, 1, Ordering::Release, Ordering::Relaxed),
            Err(4),
            "the freed cell must refuse the late publish"
        );
        for i in 0..3u8 {
            let (len, _) = r.pop(&mut out).unwrap();
            assert_eq!(&out[..len as usize], &[i]);
        }
        r.push(b"after", SPILL_NONE).unwrap();
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"after");
    }

    #[test]
    fn signal_wakes_a_waiter_and_a_silent_wait_times_out() {
        if !notify_supported() {
            return;
        }
        let word = std::sync::Arc::new(AtomicU32::new(0));
        let name = format!("int2dds_test_notify_{}", std::process::id());
        let producer = Notifier::create(&name, word.as_ref() as *const AtomicU32).unwrap();
        let consumer = Notifier::open(&name, word.as_ref() as *const AtomicU32).unwrap();

        let token = consumer.prepare_wait();
        let handle = std::thread::spawn(move || {
            let start = std::time::Instant::now();
            consumer.wait_if(token, Duration::from_secs(5));
            start.elapsed()
        });
        std::thread::sleep(Duration::from_millis(50));
        producer.signal();
        let waited = handle.join().unwrap();
        assert!(waited < Duration::from_secs(1), "wait was not woken: {waited:?}");

        let token = producer.prepare_wait();
        let start = std::time::Instant::now();
        producer.wait_if(token, Duration::from_millis(50));
        assert!(start.elapsed() >= Duration::from_millis(40));
    }

    #[test]
    fn a_polling_waiter_hands_control_back_and_never_touches_the_backend() {
        let word = AtomicU32::new(0);
        let name = format!("int2dds_test_polling_{}", std::process::id());
        let n = Notifier::create_polling(&name, &word as *const AtomicU32).unwrap();

        let token = n.prepare_wait();
        let start = std::time::Instant::now();
        n.wait_if(token, Duration::from_secs(5));
        assert!(start.elapsed() < Duration::from_millis(50));

        n.signal();
        assert_eq!(word.load(Ordering::Relaxed), 0);
    }
}
