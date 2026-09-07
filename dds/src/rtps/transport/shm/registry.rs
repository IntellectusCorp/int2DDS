//! Domain-wide participant registry. The claimed index is the participant's
//! slot id, which is also its bit in `SlotMeta::refs` and the `{slot}` in its
//! segment name.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::rtps::transport::shm::layout::LayoutError;

pub(crate) const MAX_PARTICIPANTS: usize = 64;
pub(crate) const REG_MAGIC: u64 = 0x494E_5432_5A43_5247; // "INT2ZCRG"

const STATE_EMPTY: u32 = 0;
/// Reserved by a CAS but not yet published; readers must ignore it.
const STATE_CLAIMING: u32 = 1;
const STATE_ACTIVE: u32 = 2;

#[repr(C, align(64))]
pub(crate) struct RegHeader {
    pub magic: AtomicU64,
    pub version: u32,
    pub entry_count: u32,
}

#[repr(C, align(64))]
pub(crate) struct RegEntry {
    pub state: AtomicU32,
    pub pid: AtomicU32,
    pub epoch: AtomicU64,
    pub heartbeat: AtomicU64,
    pub guid_prefix: [u8; 12],
    pub _pad: u32,
}

pub(crate) struct Registry {
    base: *mut u8,
}

// Safety: the pointer addresses a shared mapping. The header is written once
// before publication; entry state is mutated only through atomics, and the one
// non-atomic field is written only by the participant that reserved the entry,
// before it publishes ACTIVE.
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
        (*header).magic.store(REG_MAGIC, Ordering::Release);
        Registry { base }
    }

    /// # Safety
    /// `base` must point to a region previously initialized by `init`.
    pub(crate) unsafe fn attach(base: *mut u8) -> Result<Registry, LayoutError> {
        // Gate first, through a raw projection: see `SegmentView::attach`.
        let magic = &*std::ptr::addr_of!((*(base as *const RegHeader)).magic);
        if magic.load(Ordering::Acquire) != REG_MAGIC {
            return Err(LayoutError::NotReady);
        }
        let header = &*(base as *const RegHeader);
        if header.entry_count as usize != MAX_PARTICIPANTS {
            return Err(LayoutError::BadVersion);
        }
        Ok(Registry { base })
    }

    /// Bounds-checked entry address. Never forms a reference to the entry:
    /// the participant that reserved it may be writing its non-atomic field.
    fn entry_ptr(&self, slot: u32) -> Option<*mut RegEntry> {
        if slot >= MAX_PARTICIPANTS as u32 {
            return None;
        }
        // Safety: bounds-checked above, and `size()` reserved this many entries.
        unsafe {
            let entries = self.base.add(REG_HEADER_SIZE as usize) as *mut RegEntry;
            Some(entries.add(slot as usize))
        }
    }

    // Each accessor projects to one atomic field. A projection never
    // materializes a `&RegEntry`, which would assert that nobody is writing
    // `guid_prefix`.
    fn state_of(&self, slot: u32) -> Option<&AtomicU32> {
        // Safety: `entry_ptr` bounded the address; the target is an atomic.
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).state) })
    }

    fn pid_of(&self, slot: u32) -> Option<&AtomicU32> {
        // Safety: as `state_of`.
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).pid) })
    }

    fn epoch_of(&self, slot: u32) -> Option<&AtomicU64> {
        // Safety: as `state_of`.
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).epoch) })
    }

    fn heartbeat_of(&self, slot: u32) -> Option<&AtomicU64> {
        // Safety: as `state_of`.
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).heartbeat) })
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
            let epoch = self.epoch_of(slot)?.fetch_add(1, Ordering::AcqRel) + 1;
            self.pid_of(slot)?.store(pid, Ordering::Relaxed);
            self.heartbeat_of(slot)?.store(tick, Ordering::Relaxed);
            // Safety: the CAS reserved this entry and readers ignore CLAIMING,
            // so this write has no concurrent observer.
            unsafe {
                let raw = self.entry_ptr(slot)?;
                std::ptr::addr_of_mut!((*raw).guid_prefix).write(guid_prefix);
            }
            // Publish last: everything written above happens-before any Acquire
            // load that observes ACTIVE.
            state.store(STATE_ACTIVE, Ordering::Release);
            return Some((slot, epoch));
        }
        None
    }

    pub(crate) fn release(&self, slot: u32) {
        if let Some(state) = self.state_of(slot) {
            state.store(STATE_EMPTY, Ordering::Release);
        }
    }

    pub(crate) fn touch(&self, slot: u32, tick: u64) {
        if let Some(hb) = self.heartbeat_of(slot) {
            hb.store(tick, Ordering::Release);
        }
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
            if state.load(Ordering::Acquire) != STATE_ACTIVE {
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
            let Some(pid) = self.pid(slot) else { continue };
            if crate::rtps::transport::shm::platform::process_alive(pid) {
                continue;
            }
            self.release(slot);
            freed.push(slot);
        }
        freed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::test_region::AlignedRegion;

    /// The region must outlive the registry, so callers bind both.
    fn registry() -> (AlignedRegion, Registry) {
        let region = AlignedRegion::new(Registry::size() as usize);
        let reg = unsafe { Registry::init(region.ptr()) };
        (region, reg)
    }

    #[test]
    fn claim_assigns_increasing_slots() {
        let (_region, reg) = registry();
        let (a, _) = reg.claim(100, [1; 12], 0).unwrap();
        let (b, _) = reg.claim(101, [2; 12], 0).unwrap();
        assert_eq!((a, b), (0, 1));
    }

    #[test]
    fn released_slot_is_reused_with_a_new_epoch() {
        let (_region, reg) = registry();
        let (slot, epoch) = reg.claim(100, [1; 12], 0).unwrap();
        reg.release(slot);
        let (again, epoch2) = reg.claim(200, [3; 12], 0).unwrap();
        assert_eq!(again, slot);
        assert!(epoch2 > epoch);
        assert_eq!(reg.pid(slot), Some(200));
    }

    #[test]
    fn claim_fails_when_full() {
        let (_region, reg) = registry();
        for i in 0..MAX_PARTICIPANTS {
            assert!(reg.claim(i as u32, [0; 12], 0).is_some());
        }
        assert!(reg.claim(999, [0; 12], 0).is_none());
    }

    #[test]
    fn stale_slots_reports_only_expired_entries() {
        let (_region, reg) = registry();
        let (fresh, _) = reg.claim(100, [1; 12], 50).unwrap();
        let (old, _) = reg.claim(101, [2; 12], 0).unwrap();
        let stale = reg.stale_slots(60, 20);
        assert!(stale.contains(&old));
        assert!(!stale.contains(&fresh));
    }

    #[test]
    fn touch_refreshes_the_heartbeat() {
        let (_region, reg) = registry();
        let (slot, _) = reg.claim(100, [1; 12], 0).unwrap();
        reg.touch(slot, 100);
        assert!(reg.stale_slots(110, 20).is_empty());
    }

    #[test]
    fn current_process_is_reported_alive() {
        assert!(crate::rtps::transport::shm::platform::process_alive(std::process::id()));
    }

    #[test]
    fn sweep_keeps_a_stale_entry_whose_process_is_alive() {
        let (_region, reg) = registry();
        let (slot, _) = reg.claim(std::process::id(), [1; 12], 0).unwrap();
        assert!(reg.sweep_dead(100, 20).is_empty());
        assert_eq!(reg.pid(slot), Some(std::process::id()));
    }

    /// Never a live pid: Linux caps pids far below this, and Windows pids are
    /// multiples of 4. Do NOT use 0 — on Unix `kill(0, 0)` signals the caller's
    /// own process group and reports alive. Task 10 exercises a real kill.
    const DEAD_PID: u32 = i32::MAX as u32;

    #[test]
    fn sweep_frees_a_stale_entry_whose_process_is_gone() {
        let (_region, reg) = registry();
        let (slot, _) = reg.claim(DEAD_PID, [1; 12], 0).unwrap();
        assert_eq!(reg.sweep_dead(100, 20), vec![slot]);
        let (again, _) = reg.claim(123, [2; 12], 0).unwrap();
        assert_eq!(again, slot);
    }
}
