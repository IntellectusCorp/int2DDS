//! Cross-process checks for the zero-copy substrate. The test binary re-executes
//! itself as a child; the role comes from an environment variable.

#![cfg(test)]

use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::participant_slot::{now_tick, ParticipantSlot, STALE_AFTER_TICKS};
use super::registry::Registry;
use super::registry_segment::unlink_registry;
use super::ring::{RING_INLINE, SPILL_NONE};
use super::segment::{unlink_segment, OwnedSegment, PeerSegment};
use super::slot_ref::SlotRef;
use super::test_region::AlignedRegion;

const ROLE_ENV: &str = "INT2DDS_SHM_SUBSTRATE_ROLE";
const CHILD_TEST: &str = "rtps::transport::shm::integration_test::shm_child_entry";
const DOMAIN: u32 = 237;
const PARENT_SLOT: u32 = 0;
const CHILD_SLOT: u32 = 1;
// Split so `ParticipantSlot::claim`'s startup sweep in the registry domain
// never unlinks a segment name owned by the segment domain's test.
const RUNTIME_REGISTRY_DOMAIN: u32 = 249;
const RUNTIME_SEGMENT_DOMAIN: u32 = 250;

fn spawn_child(role: &str) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .env(ROLE_ENV, role)
        .args([CHILD_TEST, "--exact", "--ignored", "--nocapture"])
        .spawn()
        .unwrap()
}

/// Kills the child on the way out, including when an assertion panics. A child
/// left parked holds an ACTIVE entry with a live pid in a shared named
/// registry, and the next run of this test then reclaims nothing.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn attach_retry(domain: u32, slot: u32, my_slot: u32) -> PeerSegment {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match PeerSegment::attach(domain, slot, my_slot) {
            Ok(p) => return p,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => panic!("attach d{domain} p{slot} failed: {e}"),
        }
    }
}

#[test]
#[ignore]
fn shm_child_entry() {
    let Ok(role) = std::env::var(ROLE_ENV) else {
        return;
    };
    match role.as_str() {
        "claim_and_die" => child_claim_and_die(),
        "idle" => child_idle(),
        "registry_slot" => child_registry_slot(),
        "notifier" => child_notifier(),
        other => panic!("unknown role {other}"),
    }
}

/// Claims the slot the parent published, then exits without releasing it.
fn child_claim_and_die() {
    let mine = OwnedSegment::create(DOMAIN, CHILD_SLOT, 1, &[(64, 1)], 4).expect("child segment");
    let parent = attach_retry(DOMAIN, PARENT_SLOT, CHILD_SLOT);
    let mut out = [0u8; RING_INLINE];
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some((len, _)) = mine.ring_mut().pop(&mut out) {
            let r = SlotRef::decode(&out[..len as usize]).expect("descriptor");
            let claimed = parent.reader.claim(&r).expect("claim");
            assert_eq!(parent.reader.bytes(&claimed), b"frame");
            std::process::exit(0);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    std::process::exit(1);
}

/// Stays alive until killed. Used to exercise pid liveness.
fn child_idle() {
    std::thread::sleep(Duration::from_secs(30));
}

/// Claims a registry slot in the shared domain registry and stays alive
/// until killed.
fn child_registry_slot() {
    // Bound to `_held`, not read again: it must stay claimed for the sleep,
    // not report anything back to the parent.
    let _held = match ParticipantSlot::claim(RUNTIME_REGISTRY_DOMAIN, [77; 12]) {
        Ok(held) => held,
        Err(_) => std::process::exit(2),
    };
    std::thread::sleep(Duration::from_secs(30));
}

/// Attaches to the parent's segment and pushes one message, waking it.
fn child_notifier() {
    let parent = attach_retry(RUNTIME_SEGMENT_DOMAIN, PARENT_SLOT, CHILD_SLOT);
    std::thread::sleep(Duration::from_millis(100));
    parent.push_and_signal(b"across", SPILL_NONE).expect("push");
    std::process::exit(0);
}

#[test]
fn payload_survives_the_process_boundary() {
    unlink_segment(DOMAIN, PARENT_SLOT);
    unlink_segment(DOMAIN, CHILD_SLOT);

    let owned = OwnedSegment::create(DOMAIN, PARENT_SLOT, 1, &[(1024, 4)], 8).unwrap();
    let mut child = spawn_child("claim_and_die");

    let lease = owned.owner_mut().acquire(16).unwrap();
    owned.owner_mut().slot_mut(&lease)[..5].copy_from_slice(b"frame");
    let slot_ref = owned.owner_mut().commit(lease, 5);

    let child_seg = attach_retry(DOMAIN, CHILD_SLOT, PARENT_SLOT);
    child_seg.ring.push(&slot_ref.encode(), SPILL_NONE).unwrap();

    let status = child.wait().unwrap();
    assert!(status.success(), "child failed to claim the slot");

    let owner = owned.owner_mut();
    let meta = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
    let bit = 1u64 << CHILD_SLOT;
    assert_eq!(meta.refs.load(Ordering::Acquire) & bit, bit, "dead child still holds its bit");
    drop(owner);

    assert_eq!(owned.owner_mut().reclaim_participant(CHILD_SLOT), 1);
    let owner = owned.owner_mut();
    let meta = owner.pool().meta(slot_ref.class, slot_ref.index).unwrap();
    assert_eq!(meta.refs.load(Ordering::Acquire) & bit, 0);
    drop(owner);

    drop(child_seg);
    drop(owned);
    unlink_segment(DOMAIN, PARENT_SLOT);
    unlink_segment(DOMAIN, CHILD_SLOT);
}

#[test]
fn registry_sweep_frees_a_killed_participant() {
    use super::platform::process_alive;

    let region = AlignedRegion::new(Registry::size() as usize);
    let reg = unsafe { Registry::init(region.ptr()) };

    // The idle child stays alive until killed, so a panic before the kill below
    // would leave it running for good.
    let mut child = KillOnDrop(spawn_child("idle"));
    let child_pid = child.0.id();
    let (slot, _) = reg.claim(child_pid, [9; 12], 0).unwrap();
    assert!(process_alive(child_pid));

    child.0.kill().unwrap();
    child.0.wait().unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(child_pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!process_alive(child_pid));
    assert_eq!(reg.sweep_dead(100, 20), vec![slot]);
}

#[test]
fn two_processes_share_one_registry() {
    unlink_registry(RUNTIME_REGISTRY_DOMAIN);
    let mine = ParticipantSlot::claim(RUNTIME_REGISTRY_DOMAIN, [76; 12]).unwrap();

    let mut child = KillOnDrop(spawn_child("registry_slot"));
    // The child's entry must appear in OUR mapping of the registry.
    let deadline = Instant::now() + Duration::from_secs(5);
    let found = loop {
        if let Some((slot, _)) = mine.registry().find_active(&[77; 12]) {
            break Some(slot);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let child_slot = found.expect("child never registered in the shared registry");
    assert_eq!(
        mine.registry().pid(child_slot),
        Some(child.0.id()),
        "the entry found must belong to the child we spawned"
    );
    assert_ne!(child_slot, mine.slot(), "two participants must not share a slot");

    child.0.kill().unwrap();
    child.0.wait().unwrap();

    // The forced tick makes every occupied entry stale, this test's own
    // slot included; `process_alive` is what actually tells the dead child
    // from the still-running parent.
    let freed = mine.registry().sweep_dead(now_tick() + STALE_AFTER_TICKS + 1, STALE_AFTER_TICKS);
    assert!(freed.contains(&child_slot), "a dead participant's slot must be reclaimed");
    assert!(!freed.contains(&mine.slot()), "a live participant must survive the sweep");

    drop(mine);
    unlink_registry(RUNTIME_REGISTRY_DOMAIN);
}

#[test]
fn a_peer_process_wakes_a_blocked_owner() {
    unlink_segment(RUNTIME_SEGMENT_DOMAIN, PARENT_SLOT);
    let owned =
        OwnedSegment::create(RUNTIME_SEGMENT_DOMAIN, PARENT_SLOT, 1, &[(64, 2)], 4).unwrap();

    let mut child = KillOnDrop(spawn_child("notifier"));
    let deadline = Instant::now() + Duration::from_secs(5);
    // `wait_for_message` may return with nothing queued, so keep re-checking
    // the ring against the remaining budget rather than trusting one return.
    // `start` is set on entry to the wait branch below, not here, so `waited`
    // times the wakeup itself and not process spawn or harness startup.
    let mut start = None;
    let mut out = [0u8; RING_INLINE];
    let len = loop {
        // The MutexGuard this `pop` returns lives until the end of this whole
        // `if let` (no `else` here), so it is gone before `wait_for_message`
        // runs below -- that's the only reason this doesn't deadlock, since
        // `wait_for_message` re-locks the same ring mutex internally.
        if let Some((len, _)) = owned.ring_mut().pop(&mut out) {
            break len;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(remaining > Duration::ZERO, "owner was not woken across processes");
        start.get_or_insert_with(Instant::now);
        owned.wait_for_message(remaining);
    };
    let waited = start.map(|s| s.elapsed()).unwrap_or(Duration::ZERO);

    assert!(child.0.wait().unwrap().success(), "child failed to push");
    assert!(waited < Duration::from_secs(4), "owner was not woken across processes: {waited:?}");
    assert_eq!(&out[..len as usize], b"across");

    drop(owned);
    unlink_segment(RUNTIME_SEGMENT_DOMAIN, PARENT_SLOT);
}
