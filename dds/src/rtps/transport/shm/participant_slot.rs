//! A participant's registry slot: claimed at startup, kept alive by a
//! heartbeat, released on drop.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::{debug, warn};

use crate::rtps::transport::shm::registry::Registry;
use crate::rtps::transport::shm::registry_segment::RegistrySegment;
use crate::rtps::transport::shm::segment::unlink_segment;

pub(crate) const HEARTBEAT_PERIOD: Duration = Duration::from_secs(1);
/// Five missed heartbeats. Generous, because `sweep_dead` only ever reclaims
/// a slot whose process is confirmed gone (see `process_alive`); a live
/// participant merely scheduled out for a long stretch is never evicted on
/// staleness alone.
pub(crate) const STALE_AFTER_TICKS: u64 = 5_000;

/// Milliseconds since the UNIX epoch. Ticks are compared across processes, so
/// they must come from a clock all of them share.
///
/// `unwrap_or(0)` is asymmetric on purpose. As a heartbeat, 0 makes this
/// participant look stale to every peer, and it keeps doing so for as long as
/// the clock reports a time before 1970 -- only `process_alive` then keeps it
/// from being evicted. As a sweep argument, 0 fails safe by reclaiming
/// nothing. A machine with a broken clock cannot get correct SHM liveness
/// either way, so this does not try to fake one.
pub(crate) fn now_tick() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

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

        // Clear out whatever a previous run left behind before competing for a slot.
        for dead in segment.registry().sweep_dead(now_tick(), STALE_AFTER_TICKS) {
            // Someone may have claimed it between the sweep and here; unlinking
            // then would leave the new owner's segment nameless.
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
                .inspect_err(|_| {
                    // No `ParticipantSlot` exists yet, so nothing else will ever
                    // give this entry back.
                    segment.registry().release(slot, epoch);
                })?
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

    /// Free the entries of participants that died, and unlink their segments.
    /// Returns the reclaimed slot ids so callers can drop their mappings.
    pub(crate) fn sweep(&self) -> Vec<u32> {
        let dead = self.registry().sweep_dead(now_tick(), STALE_AFTER_TICKS);
        for slot in &dead {
            // Someone may have claimed it between the sweep and here; unlinking
            // then would leave the new owner's segment nameless.
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
            // Wake it out of `park_timeout` so shutdown does not wait a full
            // heartbeat period per participant.
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
    use crate::rtps::transport::shm::registry_segment::unlink_registry;

    const DOMAIN: u32 = 243;

    #[test]
    fn claim_takes_a_slot_and_drop_returns_it() {
        unlink_registry(DOMAIN);
        let prefix = [7; 12];
        // Pins the registry object: on Windows it would otherwise vanish with
        // the last handle and the next claim would see a fresh epoch 1.
        let keeper = RegistrySegment::open(DOMAIN).unwrap();
        let slot_id = {
            let held = ParticipantSlot::claim(DOMAIN, prefix).unwrap();
            assert_eq!(held.registry().find_active(&prefix), Some((held.slot(), held.epoch())));
            held.slot()
        };
        assert_eq!(keeper.registry().find_active(&prefix), None, "drop must release the slot");
        let again = ParticipantSlot::claim(DOMAIN, prefix).unwrap();
        assert_eq!(again.slot(), slot_id, "released slot should be reused");
        assert!(again.epoch() > 1, "epoch must advance across reuse");
        drop(again);
        drop(keeper);
        unlink_registry(DOMAIN);
    }

    #[test]
    fn heartbeat_keeps_the_slot_out_of_the_stale_set() {
        unlink_registry(DOMAIN + 1);
        let claimed_at = now_tick();
        let held = ParticipantSlot::claim(DOMAIN + 1, [12; 12]).unwrap();
        std::thread::sleep(HEARTBEAT_PERIOD * 2);
        // Judged from a point past the claim: only a heartbeat written after
        // the claim can keep the slot out of the stale set.
        let as_of = claimed_at + STALE_AFTER_TICKS + 500;
        let stale = held.registry().stale_slots(as_of, STALE_AFTER_TICKS);
        assert!(!stale.contains(&held.slot()));
        drop(held);
        unlink_registry(DOMAIN + 1);
    }
}
