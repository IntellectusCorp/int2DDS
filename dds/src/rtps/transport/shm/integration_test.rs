//! Cross-process checks for the zero-copy substrate. The test binary re-executes
//! itself as a child; the role comes from an environment variable.

#![cfg(test)]

use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::registry::Registry;
use super::ring::{RING_INLINE, SPILL_NONE};
use super::segment::{unlink_segment, OwnedSegment, PeerSegment};
use super::slot_ref::SlotRef;
use super::test_region::AlignedRegion;

const ROLE_ENV: &str = "INT2DDS_SHM_SUBSTRATE_ROLE";
const CHILD_TEST: &str = "rtps::transport::shm::integration_test::shm_child_entry";
const DOMAIN: u32 = 237;
const PARENT_SLOT: u32 = 0;
const CHILD_SLOT: u32 = 1;

fn spawn_child(role: &str) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .env(ROLE_ENV, role)
        .args([CHILD_TEST, "--exact", "--ignored", "--nocapture"])
        .spawn()
        .unwrap()
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

    let mut child = spawn_child("idle");
    let child_pid = child.id();
    let (slot, _) = reg.claim(child_pid, [9; 12], 0).unwrap();
    assert!(process_alive(child_pid));

    child.kill().unwrap();
    child.wait().unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(child_pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!process_alive(child_pid));
    assert_eq!(reg.sweep_dead(100, 20), vec![slot]);
}
