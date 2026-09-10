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

// Safety: the pointer addresses a shared mapping. The header is written once
// before publication; every entry field but `_pad` is atomic and `_pad` is
// never accessed, so concurrent readers and writers only ever load/store
// through atomics, never a plain reference.
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
    /// `base` must point to a region previously initialized by `init`, and
    /// `mapped_size` must be the number of bytes actually mapped there.
    pub(crate) unsafe fn attach(base: *mut u8, mapped_size: u64) -> Result<Registry, LayoutError> {
        // Gate first, through a raw projection: see `SegmentView::attach`.
        let magic = &*std::ptr::addr_of!((*(base as *const RegHeader)).magic);
        if magic.load(Ordering::Acquire) != REG_MAGIC {
            return Err(LayoutError::NotReady);
        }
        let header = &*(base as *const RegHeader);
        if header.version != 1 || header.entry_count as usize != MAX_PARTICIPANTS {
            return Err(LayoutError::BadVersion);
        }
        // The creator recorded what it actually sized the object to. Mapping
        // more than that is not an error on POSIX; touching it is a SIGBUS.
        // This covers the entry array, not the header read just above, which
        // assumes every build publishing REG_MAGIC has an `align(64)` header.
        if header.reg_size != mapped_size {
            return Err(LayoutError::SizeMismatch);
        }
        Ok(Registry { base })
    }

    /// Bounds-checked entry address. Never forms a reference to the entry:
    /// callers project to the one atomic field they need instead.
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

    // Each accessor projects to one atomic field, rather than borrowing the
    // whole entry, so it reads or writes only the field it needs.
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

    fn guid_prefix_of_raw(&self, slot: u32) -> Option<&[AtomicU32; 3]> {
        // Safety: as `state_of`.
        self.entry_ptr(slot).map(|p| unsafe { &*std::ptr::addr_of!((*p).guid_prefix) })
    }

    /// The reserved-but-unpublished prefix of a claim: everything a sweeper
    /// needs to judge the entry, written before it is visible as ACTIVE.
    fn begin_claim(&self, slot: u32, pid: u32, tick: u64) -> Option<()> {
        self.pid_of(slot)?.store(pid, Ordering::Relaxed);
        // Release so a sweeper that observes this heartbeat also observes
        // the pid above. The sweep reads these before ACTIVE is published,
        // so the ACTIVE store cannot be what publishes them.
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
            // pid and heartbeat first: a participant that dies before publishing
            // ACTIVE leaves a CLAIMING entry, and the sweep must find its own pid
            // and a fresh heartbeat there rather than the previous occupant's.
            self.begin_claim(slot, pid, tick)?;
            let epoch = self.epoch_of(slot)?.fetch_add(1, Ordering::AcqRel) + 1;
            let words = self.guid_prefix_of_raw(slot)?;
            for (w, chunk) in words.iter().zip(guid_prefix.chunks_exact(4)) {
                w.store(u32::from_le_bytes(chunk.try_into().ok()?), Ordering::Relaxed);
            }
            // Publish last: everything written above happens-before any Acquire
            // load that observes ACTIVE.
            state.store(STATE_ACTIVE, Ordering::Release);
            return Some((slot, epoch));
        }
        None
    }

    /// Give the slot back, but only if it is still ours. Releasing a slot that
    /// has been re-issued would evict a live participant.
    pub(crate) fn release(&self, slot: u32, epoch: u64) {
        if self.epoch(slot) != Some(epoch) {
            return;
        }
        let Some(state) = self.state_of(slot) else { return };
        let _ =
            state.compare_exchange(STATE_ACTIVE, STATE_EMPTY, Ordering::AcqRel, Ordering::Relaxed);
    }

    /// Refresh the heartbeat, but only while the entry is still the one the
    /// caller claimed. A slot reclaimed and re-issued must not have its
    /// liveness asserted by its previous owner.
    pub(crate) fn touch(&self, slot: u32, epoch: u64, tick: u64) {
        if self.epoch(slot) != Some(epoch) {
            return;
        }
        if let Some(hb) = self.heartbeat_of(slot) {
            hb.store(tick, Ordering::Release);
        }
    }

    /// Whether the slot currently holds no participant.
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
            // Not just ACTIVE: an entry stranded in CLAIMING by a crash must be
            // reclaimable too, and it now carries its claimer's pid.
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

            // Re-read everything the decision rests on, and remember the epoch
            // so a slot re-claimed while we deliberated is not mistaken for the
            // one we judged.
            let Some(epoch_before) = self.epoch(slot) else { continue };
            let observed = state.load(Ordering::Acquire);
            if observed == STATE_EMPTY {
                continue;
            }
            if now_tick.saturating_sub(hb.load(Ordering::Acquire)) < stale_after {
                continue;
            }
            let Some(pid) = self.pid(slot) else { continue };
            if crate::rtps::transport::shm::platform::process_alive(pid) {
                continue;
            }
            if self.epoch(slot) != Some(epoch_before) {
                continue;
            }

            // The CAS decides who performs the transition. What this
            // participant must clean up locally is a separate question: a
            // sweeper that loses the race still has to drop the dead peer's
            // refs bits, or they stay set forever.
            if state
                .compare_exchange(observed, STATE_EMPTY, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                freed.push(slot);
            } else if state.load(Ordering::Acquire) == STATE_EMPTY
                && self.epoch(slot) == Some(epoch_before)
            {
                // Another sweeper reclaimed the same generation. Our judgement
                // stands; only the transition was not ours.
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
            // `epoch` brackets the read: a slot released and re-claimed while we
            // look at it lands a different epoch, and we skip rather than pair
            // one participant's prefix with another's epoch.
            let before = self.epoch(slot)?;
            if state.load(Ordering::Acquire) != STATE_ACTIVE {
                continue;
            }
            let prefix = self.guid_prefix_of(slot)?;
            // Seqlock read side: an Acquire load only stops later accesses from
            // moving up. Without this fence the Relaxed prefix loads above may
            // complete after the validation below, which would validate an
            // entry we had not actually read yet.
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
        reg.release(slot, epoch);
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
        let (slot, epoch) = reg.claim(100, [1; 12], 0).unwrap();
        reg.touch(slot, epoch, 100);
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
    /// own process group and reports alive.
    const DEAD_PID: u32 = i32::MAX as u32;

    #[test]
    fn sweep_frees_a_stale_entry_whose_process_is_gone() {
        let (_region, reg) = registry();
        let (slot, _) = reg.claim(DEAD_PID, [1; 12], 0).unwrap();
        assert_eq!(reg.sweep_dead(100, 20), vec![slot]);
        let (again, _) = reg.claim(123, [2; 12], 0).unwrap();
        assert_eq!(again, slot);
    }

    #[test]
    fn find_active_locates_a_participant_by_guid_prefix() {
        let (_region, reg) = registry();
        let (slot, epoch) = reg.claim(std::process::id(), [7; 12], 0).unwrap();
        assert_eq!(reg.find_active(&[7; 12]), Some((slot, epoch)));
        assert_eq!(reg.find_active(&[8; 12]), None);
    }

    #[test]
    fn find_active_ignores_a_released_entry() {
        let (_region, reg) = registry();
        let (slot, epoch) = reg.claim(std::process::id(), [7; 12], 0).unwrap();
        reg.release(slot, epoch);
        assert_eq!(reg.find_active(&[7; 12]), None);
    }

    #[test]
    fn sweep_frees_an_entry_stranded_mid_claim() {
        let (_region, reg) = registry();
        let slot = reg.claim_without_publishing_for_test(DEAD_PID, 0).unwrap();
        // Stranded in CLAIMING: no ACTIVE store ever happened.
        assert!(reg.find_active(&[0; 12]).is_none());
        assert_eq!(reg.sweep_dead(100, 20), vec![slot]);
        // Freed, so the slot can be claimed again.
        let (again, _) = reg.claim(std::process::id(), [1; 12], 0).unwrap();
        assert_eq!(again, slot);
    }

    #[test]
    fn sweep_keeps_an_entry_stranded_mid_claim_by_a_live_process() {
        let (_region, reg) = registry();
        let slot = reg.claim_without_publishing_for_test(std::process::id(), 0).unwrap();
        assert!(reg.sweep_dead(100, 20).is_empty());
        assert_eq!(reg.pid(slot), Some(std::process::id()));
    }
}
