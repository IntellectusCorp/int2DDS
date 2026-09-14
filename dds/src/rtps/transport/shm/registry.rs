//! Domain-wide participant registry. A claimed index is the participant's
//! slot id: its bit in `SlotMeta::refs` and the `{slot}` in its segment name.
//! The first participant in a domain creates the registry; it outlives any
//! one of them. Each participant keeps its entry alive with a heartbeat, and
//! a sweep frees entries whose heartbeat stalled and whose process is gone.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use log::{debug, warn};

use crate::rtps::transport::shm::platform::{process_alive, SharedMemory};
use crate::rtps::transport::shm::segment::unlink_segment;
use crate::rtps::transport::shm::LayoutError;

pub(crate) const MAX_PARTICIPANTS: usize = 64;
const REG_MAGIC: u64 = 0x494E_5432_5A43_5247; // "INT2ZCRG"

const STATE_EMPTY: u32 = 0;
/// Reserved by a CAS but not yet published; readers ignore it.
const STATE_CLAIMING: u32 = 1;
const STATE_ACTIVE: u32 = 2;

pub(crate) const HEARTBEAT_PERIOD: Duration = Duration::from_secs(1);
/// Five missed heartbeats. Staleness alone never evicts: `sweep_dead` also
/// requires the process to be gone.
pub(crate) const STALE_AFTER_TICKS: u64 = 5_000;

fn registry_name(domain: u32) -> String {
    format!("int2dds_reg_d{}", domain)
}

/// Milliseconds since the UNIX epoch: ticks are compared across processes.
/// A clock before 1970 yields 0, which reads as stale everywhere and reclaims
/// nothing as a sweep argument.
pub(crate) fn now_tick() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[repr(C, align(64))]
pub(crate) struct RegHeader {
    pub magic: AtomicU64,
    pub version: u32,
    pub entry_count: u32,
    pub reg_size: u64,
}

#[repr(C, align(64))]
pub(crate) struct RegEntry {
    pub state: AtomicU32,
    pub pid: AtomicU32,
    pub epoch: AtomicU64,
    pub heartbeat: AtomicU64,
    pub guid_prefix: [AtomicU32; 3],
    pub _pad: u32,
}

pub(crate) struct Registry {
    base: *mut u8,
}

// Safety: the header is written once before publication; every entry field
// is reached through its atomic, never a plain reference.
unsafe impl Send for Registry {}
unsafe impl Sync for Registry {}

const REG_HEADER_SIZE: u64 = std::mem::size_of::<RegHeader>() as u64;

impl Registry {
    pub(crate) fn size() -> u64 {
        REG_HEADER_SIZE + std::mem::size_of::<RegEntry>() as u64 * MAX_PARTICIPANTS as u64
    }

    /// # Safety
    /// `base` must point to a zeroed writable region of `size()` bytes.
    pub(crate) unsafe fn init(base: *mut u8) -> Registry {
        let header = base as *mut RegHeader;
        std::ptr::addr_of_mut!((*header).version).write(1);
        std::ptr::addr_of_mut!((*header).entry_count).write(MAX_PARTICIPANTS as u32);
        std::ptr::addr_of_mut!((*header).reg_size).write(Registry::size());
        (*header).magic.store(REG_MAGIC, Ordering::Release);
        Registry { base }
    }

    /// # Safety
    /// `base` must point to a region initialized by `init`, and `mapped_size`
    /// must be the number of bytes actually mapped there.
    pub(crate) unsafe fn attach(base: *mut u8, mapped_size: u64) -> Result<Registry, LayoutError> {
        let magic = &*std::ptr::addr_of!((*(base as *const RegHeader)).magic);
        if magic.load(Ordering::Acquire) != REG_MAGIC {
            return Err(LayoutError::NotReady);
        }
        let header = &*(base as *const RegHeader);
        if header.version != 1 || header.entry_count as usize != MAX_PARTICIPANTS {
            return Err(LayoutError::BadVersion);
        }
        // Mapping past the end of a POSIX object succeeds; touching it is SIGBUS.
        if header.reg_size != mapped_size {
            return Err(LayoutError::SizeMismatch);
        }
        Ok(Registry { base })
    }

    fn entry_ptr(&self, slot: u32) -> Option<*mut RegEntry> {
        if slot >= MAX_PARTICIPANTS as u32 {
            return None;
        }
        unsafe {
            let entries = self.base.add(REG_HEADER_SIZE as usize) as *mut RegEntry;
            Some(entries.add(slot as usize))
        }
    }

    fn state_of(&self, slot: u32) -> Option<&AtomicU32> {
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).state) })
    }

    fn pid_of(&self, slot: u32) -> Option<&AtomicU32> {
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).pid) })
    }

    fn epoch_of(&self, slot: u32) -> Option<&AtomicU64> {
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).epoch) })
    }

    fn heartbeat_of(&self, slot: u32) -> Option<&AtomicU64> {
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).heartbeat) })
    }

    fn guid_prefix_of_raw(&self, slot: u32) -> Option<&[AtomicU32; 3]> {
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).guid_prefix) })
    }

    /// pid and heartbeat go in before ACTIVE: a claimer that dies in between
    /// leaves a CLAIMING entry the sweep can still judge.
    fn begin_claim(&self, slot: u32, pid: u32, tick: u64) -> Option<()> {
        self.pid_of(slot)?.store(pid, Ordering::Relaxed);
        self.heartbeat_of(slot)?.store(tick, Ordering::Release);
        Some(())
    }

    pub(crate) fn claim(&self, pid: u32, guid_prefix: [u8; 12], tick: u64) -> Option<(u32, u64)> {
        for slot in 0..MAX_PARTICIPANTS as u32 {
            let state = self.state_of(slot)?;
            if state
                .compare_exchange(STATE_EMPTY, STATE_CLAIMING, Ordering::AcqRel, Ordering::Relaxed)
                .is_err()
            {
                continue;
            }
            self.begin_claim(slot, pid, tick)?;
            let epoch = self.epoch_of(slot)?.fetch_add(1, Ordering::AcqRel) + 1;
            let words = self.guid_prefix_of_raw(slot)?;
            for (w, chunk) in words.iter().zip(guid_prefix.chunks_exact(4)) {
                w.store(u32::from_le_bytes(chunk.try_into().ok()?), Ordering::Relaxed);
            }
            state.store(STATE_ACTIVE, Ordering::Release);
            return Some((slot, epoch));
        }
        None
    }

    /// Only if the slot is still ours; releasing a re-issued slot would evict
    /// a live participant.
    pub(crate) fn release(&self, slot: u32, epoch: u64) {
        if self.epoch(slot) != Some(epoch) {
            return;
        }
        let Some(state) = self.state_of(slot) else { return };
        let _ =
            state.compare_exchange(STATE_ACTIVE, STATE_EMPTY, Ordering::AcqRel, Ordering::Relaxed);
    }

    pub(crate) fn touch(&self, slot: u32, epoch: u64, tick: u64) {
        if self.epoch(slot) != Some(epoch) {
            return;
        }
        if let Some(hb) = self.heartbeat_of(slot) {
            hb.store(tick, Ordering::Release);
        }
    }

    pub(crate) fn is_free(&self, slot: u32) -> bool {
        self.state_of(slot).map(|s| s.load(Ordering::Acquire) == STATE_EMPTY).unwrap_or(false)
    }

    pub(crate) fn epoch(&self, slot: u32) -> Option<u64> {
        Some(self.epoch_of(slot)?.load(Ordering::Acquire))
    }

    pub(crate) fn pid(&self, slot: u32) -> Option<u32> {
        Some(self.pid_of(slot)?.load(Ordering::Acquire))
    }

    pub(crate) fn stale_slots(&self, now_tick: u64, stale_after: u64) -> Vec<u32> {
        let mut out = Vec::new();
        for slot in 0..MAX_PARTICIPANTS as u32 {
            let Some(state) = self.state_of(slot) else { continue };
            if state.load(Ordering::Acquire) == STATE_EMPTY {
                continue;
            }
            let Some(hb) = self.heartbeat_of(slot) else { continue };
            if now_tick.saturating_sub(hb.load(Ordering::Acquire)) >= stale_after {
                out.push(slot);
            }
        }
        out
    }

    /// Free entries whose heartbeat stalled and whose process is gone. Returns
    /// the freed slot ids so the caller can clear their bits from its pool.
    pub(crate) fn sweep_dead(&self, now_tick: u64, stale_after: u64) -> Vec<u32> {
        let mut freed = Vec::new();
        for slot in self.stale_slots(now_tick, stale_after) {
            let Some(state) = self.state_of(slot) else { continue };
            let Some(hb) = self.heartbeat_of(slot) else { continue };

            // Re-read under the epoch so a slot re-claimed meanwhile is not
            // mistaken for the one judged.
            let Some(epoch_before) = self.epoch(slot) else { continue };
            let observed = state.load(Ordering::Acquire);
            if observed == STATE_EMPTY {
                continue;
            }
            if now_tick.saturating_sub(hb.load(Ordering::Acquire)) < stale_after {
                continue;
            }
            let Some(pid) = self.pid(slot) else { continue };
            if process_alive(pid) {
                continue;
            }
            if self.epoch(slot) != Some(epoch_before) {
                continue;
            }

            // A sweeper that loses the CAS to another sweeper still has to drop
            // the dead peer's refs bits locally, so it reports the slot too.
            if state
                .compare_exchange(observed, STATE_EMPTY, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
                || (state.load(Ordering::Acquire) == STATE_EMPTY
                    && self.epoch(slot) == Some(epoch_before))
            {
                freed.push(slot);
            }
        }
        freed
    }

    fn guid_prefix_of(&self, slot: u32) -> Option<[u8; 12]> {
        let words = self.guid_prefix_of_raw(slot)?;
        let mut out = [0u8; 12];
        for (i, w) in words.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&w.load(Ordering::Relaxed).to_le_bytes());
        }
        Some(out)
    }

    /// The slot and epoch of the ACTIVE participant with this GUID prefix.
    pub(crate) fn find_active(&self, guid_prefix: &[u8; 12]) -> Option<(u32, u64)> {
        for slot in 0..MAX_PARTICIPANTS as u32 {
            let state = self.state_of(slot)?;
            // Seqlock read: the epoch brackets the prefix loads.
            let before = self.epoch(slot)?;
            if state.load(Ordering::Acquire) != STATE_ACTIVE {
                continue;
            }
            let prefix = self.guid_prefix_of(slot)?;
            std::sync::atomic::fence(Ordering::Acquire);
            if state.load(Ordering::Acquire) != STATE_ACTIVE || self.epoch(slot)? != before {
                continue;
            }
            if prefix == *guid_prefix {
                return Some((slot, before));
            }
        }
        None
    }

    /// Reserves a slot and stops before publishing it, reproducing a crash
    /// between the CAS and the ACTIVE store.
    #[cfg(test)]
    pub(crate) fn claim_without_publishing_for_test(&self, pid: u32, tick: u64) -> Option<u32> {
        for slot in 0..MAX_PARTICIPANTS as u32 {
            let state = self.state_of(slot)?;
            if state
                .compare_exchange(STATE_EMPTY, STATE_CLAIMING, Ordering::AcqRel, Ordering::Relaxed)
                .is_err()
            {
                continue;
            }
            self.begin_claim(slot, pid, tick)?;
            return Some(slot);
        }
        None
    }
}

/// The registry on real shared memory.
pub(crate) struct RegistrySegment {
    registry: Registry,
    // Declared last: the mapping must outlive everything pointing into it.
    _shm: SharedMemory,
}

impl RegistrySegment {
    /// A timeout (creator died before publishing) or a layout mismatch (object
    /// from another build) leaves a stale object nothing clears on its own;
    /// the caller may `unlink_registry` and retry.
    pub(crate) fn open(domain: u32) -> io::Result<RegistrySegment> {
        let name = registry_name(domain);
        let size = Registry::size() as usize;
        let mut shm = SharedMemory::new(&name, size, true)?;

        if shm.is_creator() {
            let registry = unsafe { Registry::init(shm.as_ptr()) };
            // The registry outlives this participant.
            shm.disown_creation();
            return Ok(RegistrySegment { registry, _shm: shm });
        }

        let deadline = Instant::now() + Duration::from_millis(100);
        loop {
            match unsafe { Registry::attach(shm.as_ptr(), size as u64) } {
                Ok(registry) => return Ok(RegistrySegment { registry, _shm: shm }),
                Err(LayoutError::NotReady) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(LayoutError::NotReady) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "registry never became ready",
                    ))
                }
                Err(_) => return Err(io::Error::other("registry layout mismatch")),
            }
        }
    }

    pub(crate) fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Unix only; on Windows the mapping disappears with its last handle.
pub(crate) fn unlink_registry(domain: u32) {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        if let Ok(name) = CString::new(format!("/{}", registry_name(domain))) {
            unsafe { libc::shm_unlink(name.as_ptr()) };
        }
    }
    #[cfg(not(unix))]
    {
        let _ = domain;
    }
}

/// A participant's registry entry: claimed at startup, kept alive by a
/// heartbeat thread, released on drop.
pub(crate) struct ParticipantSlot {
    segment: Arc<RegistrySegment>,
    domain: u32,
    slot: u32,
    epoch: u64,
    stop: Arc<AtomicBool>,
    ticker: Option<JoinHandle<()>>,
}

impl ParticipantSlot {
    pub(crate) fn claim(domain: u32, guid_prefix: [u8; 12]) -> io::Result<ParticipantSlot> {
        let segment = Arc::new(RegistrySegment::open(domain)?);

        for dead in segment.registry().sweep_dead(now_tick(), STALE_AFTER_TICKS) {
            // Someone may have claimed it since; unlinking then would leave the
            // new owner's segment nameless.
            if segment.registry().is_free(dead) {
                debug!("[shm] reclaimed registry slot {dead} from a dead participant");
                unlink_segment(domain, dead);
            }
        }

        let (slot, epoch) =
            segment
                .registry()
                .claim(std::process::id(), guid_prefix, now_tick())
                .ok_or_else(|| io::Error::new(io::ErrorKind::OutOfMemory, "registry is full"))?;

        let stop = Arc::new(AtomicBool::new(false));
        let ticker = {
            let thread_segment = Arc::clone(&segment);
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("int2dds-shm-hb".into())
                .spawn(move || {
                    while !stop.load(Ordering::Acquire) {
                        thread_segment.registry().touch(slot, epoch, now_tick());
                        std::thread::park_timeout(HEARTBEAT_PERIOD);
                    }
                })
                .inspect_err(|_| segment.registry().release(slot, epoch))?
        };

        Ok(ParticipantSlot { segment, domain, slot, epoch, stop, ticker: Some(ticker) })
    }

    pub(crate) fn slot(&self) -> u32 {
        self.slot
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(crate) fn registry(&self) -> &Registry {
        self.segment.registry()
    }

    /// Free the entries of participants that died and unlink their segments.
    /// Returns the reclaimed slot ids so callers can drop their mappings.
    pub(crate) fn sweep(&self) -> Vec<u32> {
        let dead = self.registry().sweep_dead(now_tick(), STALE_AFTER_TICKS);
        for slot in &dead {
            if self.registry().is_free(*slot) {
                unlink_segment(self.domain, *slot);
            }
        }
        dead
    }
}

impl Drop for ParticipantSlot {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(ticker) = self.ticker.take() {
            ticker.thread().unpark();
            if ticker.join().is_err() {
                warn!("[shm] heartbeat thread panicked");
            }
        }
        self.registry().release(self.slot, self.epoch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::test_region::AlignedRegion;

    /// Never a live pid: Linux caps pids far below this and Windows pids are
    /// multiples of 4. Not 0: on Unix `kill(0, 0)` reports the caller alive.
    const DEAD_PID: u32 = i32::MAX as u32;

    fn registry() -> (AlignedRegion, Registry) {
        let region = AlignedRegion::new(Registry::size() as usize);
        let reg = unsafe { Registry::init(region.ptr()) };
        (region, reg)
    }

    #[test]
    fn claim_assigns_slots_and_a_released_slot_is_reused_with_a_new_epoch() {
        let (_region, reg) = registry();
        let (a, epoch) = reg.claim(100, [1; 12], 0).unwrap();
        let (b, _) = reg.claim(101, [2; 12], 0).unwrap();
        assert_eq!((a, b), (0, 1));

        reg.release(a, epoch);
        let (again, epoch2) = reg.claim(200, [3; 12], 0).unwrap();
        assert_eq!(again, a);
        assert!(epoch2 > epoch);
        assert_eq!(reg.pid(a), Some(200));

        for i in 2..MAX_PARTICIPANTS {
            assert!(reg.claim(i as u32, [0; 12], 0).is_some());
        }
        assert!(reg.claim(999, [0; 12], 0).is_none(), "full");
    }

    #[test]
    fn touch_keeps_an_entry_out_of_the_stale_set() {
        let (_region, reg) = registry();
        let (fresh, epoch) = reg.claim(100, [1; 12], 50).unwrap();
        let (old, _) = reg.claim(101, [2; 12], 0).unwrap();
        let stale = reg.stale_slots(60, 20);
        assert!(stale.contains(&old));
        assert!(!stale.contains(&fresh));

        reg.touch(fresh, epoch, 100);
        assert!(!reg.stale_slots(110, 20).contains(&fresh));
    }

    #[test]
    fn sweep_frees_only_stale_entries_whose_process_is_gone() {
        let (_region, reg) = registry();
        let (alive, _) = reg.claim(std::process::id(), [1; 12], 0).unwrap();
        let (dead, _) = reg.claim(DEAD_PID, [2; 12], 0).unwrap();
        let stranded_dead = reg.claim_without_publishing_for_test(DEAD_PID, 0).unwrap();
        let stranded_alive = reg.claim_without_publishing_for_test(std::process::id(), 0).unwrap();

        let freed = reg.sweep_dead(100, 20);
        assert!(freed.contains(&dead));
        assert!(freed.contains(&stranded_dead), "an entry stranded mid-claim is reclaimable");
        assert!(!freed.contains(&alive));
        assert!(!freed.contains(&stranded_alive));
        assert_eq!(reg.pid(alive), Some(std::process::id()));

        let (again, _) = reg.claim(123, [3; 12], 0).unwrap();
        assert_eq!(again, dead);
    }

    #[test]
    fn find_active_locates_a_participant_by_prefix_and_ignores_released_ones() {
        let (_region, reg) = registry();
        let (slot, epoch) = reg.claim(std::process::id(), [7; 12], 0).unwrap();
        assert_eq!(reg.find_active(&[7; 12]), Some((slot, epoch)));
        assert_eq!(reg.find_active(&[8; 12]), None);
        reg.release(slot, epoch);
        assert_eq!(reg.find_active(&[7; 12]), None);
    }

    #[test]
    fn open_twice_shares_one_registry_that_outlives_its_creator() {
        const D: u32 = 241;
        unlink_registry(D);
        let first = RegistrySegment::open(D).unwrap();
        let (slot, _) = first.registry().claim(std::process::id(), [5; 12], 1).unwrap();
        let second = RegistrySegment::open(D).unwrap();
        assert_eq!(second.registry().find_active(&[5; 12]).map(|(s, _)| s), Some(slot));

        drop(first);
        let third = RegistrySegment::open(D).unwrap();
        assert_eq!(third.registry().find_active(&[5; 12]).map(|(s, _)| s), Some(slot));
        drop(second);
        drop(third);
        unlink_registry(D);
    }

    #[test]
    fn open_waits_for_the_creator_to_publish() {
        const D: u32 = 9243;
        unlink_registry(D);
        let raw = SharedMemory::new(&registry_name(D), Registry::size() as usize, true).unwrap();
        assert!(raw.is_creator());
        let ptr = raw.as_ptr() as usize;

        let publisher = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            unsafe { Registry::init(ptr as *mut u8) };
        });

        let seg = RegistrySegment::open(D).expect("should wait for the creator");
        publisher.join().unwrap();
        assert!(seg.registry().find_active(&[0; 12]).is_none());
        drop(seg);
        drop(raw);
        unlink_registry(D);
    }

    #[test]
    fn a_participant_slot_is_claimed_kept_alive_and_returned_on_drop() {
        const D: u32 = 243;
        unlink_registry(D);
        let prefix = [7; 12];
        // Pins the registry object on Windows across the drop below.
        let keeper = RegistrySegment::open(D).unwrap();
        let claimed_at = now_tick();
        let slot_id = {
            let held = ParticipantSlot::claim(D, prefix).unwrap();
            assert_eq!(held.registry().find_active(&prefix), Some((held.slot(), held.epoch())));
            std::thread::sleep(HEARTBEAT_PERIOD * 2);
            let as_of = claimed_at + STALE_AFTER_TICKS + 500;
            assert!(!held.registry().stale_slots(as_of, STALE_AFTER_TICKS).contains(&held.slot()));
            held.slot()
        };
        assert_eq!(keeper.registry().find_active(&prefix), None, "drop must release the slot");
        let again = ParticipantSlot::claim(D, prefix).unwrap();
        assert_eq!(again.slot(), slot_id);
        assert!(again.epoch() > 1, "epoch must advance across reuse");
        drop(again);
        drop(keeper);
        unlink_registry(D);
    }
}
