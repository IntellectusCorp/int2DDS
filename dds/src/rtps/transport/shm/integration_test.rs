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
    spawn_child_with(role, &[])
}

/// `spawn_child` plus per-role environment. The child inherits everything else,
/// so a variable the parent does not set keeps whatever the test run had. An
/// empty value is not set at all, which is how an unset knob stays unset rather
/// than becoming an empty string the child then has to parse.
fn spawn_child_with(role: &str, env: &[(&str, String)]) -> std::process::Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.env(ROLE_ENV, role).args([CHILD_TEST, "--exact", "--ignored", "--nocapture"]);
    for (key, value) in env.iter().filter(|(_, value)| !value.is_empty()) {
        command.env(key, value);
    }
    command.spawn().unwrap()
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
        "dds_subscriber" => child_dds_subscriber(),
        "perf_pub" => child_perf_publisher(),
        "perf_sub" => child_perf_subscriber(),
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

    let mut lease = owned.owner_mut().acquire(16).unwrap();
    lease.bytes_mut()[..5].copy_from_slice(b"frame");
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

// ---------------------------------------------------------------------------
// DDS end to end. The substrate tests above stop at the ring; what follows runs
// real participants through the whole path -- writer loan, descriptor, ring,
// parser, claim -- in two processes on one host.
// ---------------------------------------------------------------------------

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::fallback::{descriptor_rejects, DescriptorRejectReason, FallbackReason};
use super::registry_segment::RegistrySegment;
use super::runtime::ShmRuntime;
use crate::common::instance_handle::InstanceHandle;
use crate::dcps::topic::type_support::DdsType;
use crate::domain::domain_participant::DomainParticipant;
use crate::domain::domain_participant_factory::DomainParticipantFactory;
use crate::domain::qos::DomainParticipantQos;
use crate::infrastructure::qos_policy::PROP_TRANSPORT;
use crate::infrastructure::status::StatusMask;
use crate::publication::qos::{DataWriterQos, PublisherQos};
use crate::subscription::data_reader::DataReader;
use crate::subscription::data_reader_listener::DataReaderListener;
use crate::subscription::qos::{DataReaderQos, SubscriberQos};
use crate::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
use crate::topic::qos::TopicQos;

const E2E_DOMAIN_ENV: &str = "INT2DDS_E2E_DOMAIN";
const E2E_PEER_PREFIX_ENV: &str = "INT2DDS_E2E_PEER_PREFIX";
const E2E_OUT_ENV: &str = "INT2DDS_E2E_OUT";
const E2E_TRANSPORT_ENV: &str = "INT2DDS_E2E_TRANSPORT";
const E2E_SIZE_ENV: &str = "INT2DDS_E2E_SIZE";
const E2E_COUNT_ENV: &str = "INT2DDS_E2E_COUNT";
const E2E_WARMUP_ENV: &str = "INT2DDS_E2E_WARMUP";
const E2E_PACE_ENV: &str = "INT2DDS_E2E_PACE_US";
/// Reader reliability for the measurement runs. The default BEST_EFFORT reader
/// never reassembles a sample that fragments into three or more pieces, so a
/// second table has to be taken with a RELIABLE one to compare the copy paths
/// against zero-copy at 1 MiB and 8 MiB at all.
const E2E_RELIABLE_ENV: &str = "INT2DDS_E2E_RELIABLE";
const E2E_DEPTH_ENV: &str = "INT2DDS_E2E_DEPTH";
/// Passed straight through to the children as `INT2DDS_UDP_SOCKET_BUFFER` when
/// set. Unset means the OS default, which is what a user who configures nothing
/// gets -- and, at the default 65000 fragment size, a receive window that holds
/// exactly one fragment. See `end_to_end_latency_across_three_transports`.
const E2E_UDP_BUFFER_ENV: &str = "INT2DDS_E2E_UDP_BUFFER";

/// What the children will actually see as `INT2DDS_UDP_SOCKET_BUFFER`. Reading
/// `E2E_UDP_BUFFER_ENV` alone is not the same question: `spawn_child_with` skips
/// an empty value, so a run that set the product variable on this process passes
/// it down by inheritance, and a header taken from the test knob alone reports
/// `(OS default)` for a table that was not measured at one.
fn effective_udp_buffer() -> String {
    std::env::var(E2E_UDP_BUFFER_ENV)
        .or_else(|_| std::env::var("INT2DDS_UDP_SOCKET_BUFFER"))
        .unwrap_or_default()
}
/// Set on the child only, never on this process: mutating the environment of a
/// running test binary would leak into every later test.
const ZERO_COPY_ENV: &str = "INT2DDS_SHM_ZERO_COPY";

const E2E_TOPIC: &str = "ShmZeroCopyE2eTopic";
const E2E_TYPE: &str = "E2eSample";
const E2E_BLOB_LEN: usize = 4096;

const ALL_FALLBACK_REASONS: [FallbackReason; 9] = [
    FallbackReason::NoLocalReader,
    FallbackReason::PeerNotRegistered,
    FallbackReason::NoSlotId,
    FallbackReason::SampleTooLarge,
    FallbackReason::PoolExhausted,
    FallbackReason::DurabilityTooStrong,
    FallbackReason::NotifyUnsupported,
    FallbackReason::RingFull,
    FallbackReason::PeerGone,
];

const ALL_REJECT_REASONS: [DescriptorRejectReason; 3] = [
    DescriptorRejectReason::NoLocalRuntime,
    DescriptorRejectReason::PeerUnresolved,
    DescriptorRejectReason::ClaimRejected,
];

#[derive(DdsType)]
struct E2eSample {
    seq: u64,
    stamp_nanos: u64,
    blob: Vec<u8>,
}

fn now_nanos() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

fn env_str(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} is not set"))
}

fn env_u64(key: &str) -> u64 {
    env_str(key).parse().unwrap_or_else(|_| panic!("{key} is not a number"))
}

fn prefix_to_hex(prefix: &[u8; 12]) -> String {
    prefix.iter().map(|b| format!("{b:02x}")).collect()
}

fn prefix_from_hex(text: &str) -> [u8; 12] {
    let mut out = [0u8; 12];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).expect("prefix hex");
    }
    out
}

/// A payload whose every byte is a function of its offset, so a subscriber can
/// tell a correct sample from a truncated or shifted one without carrying a
/// copy of what was sent.
fn e2e_blob(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// KEEP_LAST depth for both endpoints of a measurement run. `dds/tests/frag/basic.rs`
/// carries 1 MiB samples with depth 10 on both sides; depth 1 is the QoS default.
fn perf_depth() -> i32 {
    std::env::var(E2E_DEPTH_ENV).ok().and_then(|v| v.parse().ok()).unwrap_or(1)
}

/// RELIABLE on both endpoints by default. A BEST_EFFORT reader never completes a
/// sample that fragments -- it has no repair to ask with -- so at 1 MiB it receives
/// nothing at any pace, and the copy-path rows would then be empty for a reason that
/// has nothing to do with the transport. `dds/tests/frag/basic.rs` carries 1 MiB the
/// same way.
fn perf_reliable() -> bool {
    std::env::var(E2E_RELIABLE_ENV).map_or(true, |v| v != "0")
}

fn perf_reader_qos() -> DataReaderQos {
    let mut qos = DataReaderQos::default();
    if perf_reliable() {
        qos.reliability.kind =
            crate::infrastructure::qos_policy::ReliabilityQosPolicyKind::Reliable;
    }
    qos.history.kind =
        crate::infrastructure::qos_policy::HistoryQosPolicyKind::KeepLast(perf_depth());
    qos
}

fn perf_writer_qos() -> DataWriterQos {
    let mut qos = DataWriterQos::default();
    qos.history.kind =
        crate::infrastructure::qos_policy::HistoryQosPolicyKind::KeepLast(perf_depth());
    qos
}

/// The participant's own `ShmRuntime`, through the bridge rather than through a
/// writer. The guard is dropped before returning: the write path takes the same
/// lock, so holding it across a `write()` would deadlock.
fn participant_shm_runtime(participant: &DomainParticipant) -> Option<Arc<ShmRuntime>> {
    let bridge = participant.get_dcps_bridge().ok()?;
    let runtime = bridge.as_ref()?.transport().shm_runtime();
    drop(bridge);
    runtime
}

fn e2e_participant(domain: i32, transport: &str) -> DomainParticipant {
    let mut qos = DomainParticipantQos::default();
    qos.property.add_property(PROP_TRANSPORT, transport, false);
    DomainParticipantFactory::get_instance()
        .create_participant(domain, qos, None, StatusMask::default())
        .expect("participant")
}

fn wait_until(budget: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if ready() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// How many slots of `peer`'s pool carry `bit` in their `refs` bitmap.
fn slots_referenced_by(peer: &PeerSegment, bit: u64) -> usize {
    let pool = peer.reader.pool();
    let mut held = 0;
    for class in 0..pool.class_count() {
        for index in 0..pool.slot_count(class) {
            if let Some(meta) = pool.meta(class, index) {
                if meta.refs.load(Ordering::Acquire) & bit != 0 {
                    held += 1;
                }
            }
        }
    }
    held
}

fn report_path(tag: &str, domain: i32) -> PathBuf {
    std::env::temp_dir().join(format!("int2dds_e2e_{}_{tag}_{domain}.txt", std::process::id()))
}

/// The DDS end-to-end case.
///
/// The green has to mean "through a pool slot", not merely "delivered". Three
/// checks together say that:
///
/// 1. the child compares what it read against the pattern the parent wrote;
/// 2. the parent's own `FallbackCounters` -- this runtime's instance, so `== 0`
///    is not order-dependent the way the process-wide reject counters are --
///    must be zero on all nine `FallbackReason`s;
/// 3. the child, from a mapping it opens itself rather than one the DDS layer
///    handed it, must find its own bit in the parent's pool `refs` bitmap.
///
/// Each covers the others' blind spot: (1) alone passes over plain UDP, (2)
/// alone passes on a run that delivered nothing, and (3) is the direct
/// observation that the bytes came out of the parent's pool.
#[test]
fn a_dds_sample_crosses_two_processes_through_a_pool_slot() {
    let domain = crate::test_utils::unique_domain_id();
    let participant = e2e_participant(domain, "shm");
    let topic = participant
        .create_topic::<E2eSample>(
            E2E_TOPIC,
            E2E_TYPE,
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let writer = publisher
        .create_datawriter::<E2eSample>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let runtime =
        participant_shm_runtime(&participant).expect("the shm transport must have a runtime");

    let out = report_path("sub", domain);
    let _ = std::fs::remove_file(&out);
    let mut child = KillOnDrop(spawn_child_with(
        "dds_subscriber",
        &[
            (E2E_DOMAIN_ENV, domain.to_string()),
            (E2E_PEER_PREFIX_ENV, prefix_to_hex(&writer.guid().prefix())),
            (E2E_OUT_ENV, out.display().to_string()),
        ],
    ));

    // Not one sample before the match: with no matched reader yet, `NoLocalReader`
    // is the correct answer and would bump the counter this test then reads.
    //
    // The wait watches the child as well as the status, so a failure here says
    // which of the three happened -- the child died, or it stayed up and never
    // matched -- and carries whatever the child wrote. A bare status poll leaves
    // a loaded host looking the same as a broken one.
    let mut child_exit = None;
    let matched = wait_until(Duration::from_secs(60), || {
        if child_exit.is_none() {
            child_exit = child.0.try_wait().unwrap_or(None);
        }
        writer.get_publication_matched_status().map(|s| s.current_count() > 0).unwrap_or(false)
    });
    assert!(
        matched,
        "the child's reader never matched in 60s (child: {}, child report: {})",
        match child_exit {
            Some(status) => format!("already exited with {status}"),
            None => "still running".to_string(),
        },
        std::fs::read_to_string(&out).unwrap_or_else(|_| "(nothing written)".to_string())
    );

    let mut sample = E2eSample { seq: 0, stamp_nanos: 0, blob: e2e_blob(E2E_BLOB_LEN) };
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut status = None;
    while Instant::now() < deadline {
        if let Some(exited) = child.0.try_wait().unwrap() {
            status = Some(exited);
            break;
        }
        sample.seq += 1;
        sample.stamp_nanos = now_nanos();
        writer.write(&sample, InstanceHandle::NIL).unwrap();
        std::thread::sleep(Duration::from_millis(50));
    }

    let said = std::fs::read_to_string(&out).unwrap_or_else(|_| "(no report)".to_string());
    let status = status.expect("the child never finished");
    assert!(status.success(), "child rejected the delivery: {said}");

    for reason in ALL_FALLBACK_REASONS {
        assert_eq!(
            runtime.fallbacks().count(reason),
            0,
            "{reason:?} fired, so the sample did not go through a slot"
        );
    }

    let _ = std::fs::remove_file(&out);
    drop(child);
    participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
}

const SELF_TOPIC: &str = "ShmZeroCopySelfDeliveryTopic";
/// More than the 64 slots of the smallest default class (64 KiB x 64), so the free
/// queue has to wrap while the reader is still holding what it kept.
const SELF_SAMPLES: u64 = 100;

/// A blob whose every byte depends on `seq`, so a sample overwritten by a later one
/// cannot pass for itself.
fn self_blob(seq: u64) -> Vec<u8> {
    (0..E2E_BLOB_LEN).map(|i| ((i as u64).wrapping_add(seq) % 251) as u8).collect()
}

/// Match a reader in another participant that this domain's SHM registry knows, on a
/// loopback UDP locator nothing listens on.
///
/// The `NoLocalReader` rule asks only whether some matched reader is an SHM peer, and a
/// reader in the writer's own participant is not one (`PeerMap::find_peer`). Without this
/// the write path would fall back at `write()` time and the caller's test would pass
/// without ever borrowing a slot. The locator is UDP so the fake peer never gets a
/// descriptor, and its pid is ours so the registry sweep never reclaims it.
fn match_a_registered_shm_peer(
    writer: &crate::publication::data_writer::DataWriter<E2eSample>,
    rt: &ShmRuntime,
) {
    use crate::common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData;
    use crate::rtps::common::entity_id::EntityId;
    use crate::rtps::common::guid::Guid;
    use crate::rtps::common::locator::Locator;
    use crate::rtps::common::sequence::SequenceNumber;
    use crate::rtps::entities::writer::reader_proxy::ReaderProxy;
    use crate::rtps::entities::writer::StatefulWriter;

    const PEER_PREFIX: [u8; 12] = [0xAB; 12];
    assert_ne!(writer.guid().prefix(), PEER_PREFIX, "the fake peer must not be us");
    rt.registry()
        .claim(std::process::id(), PEER_PREFIX, now_tick())
        .expect("a registry slot for the fake peer");

    let nowhere = Locator::from_ip_v4_addr_and_port(&std::net::Ipv4Addr::LOCALHOST, 7999);
    let rtps_writer = writer.get_rtps_writer().unwrap();
    let stateful = rtps_writer
        .as_any()
        .downcast_ref::<StatefulWriter>()
        .expect("a default-QoS writer is reliable");
    stateful.matched_reader_add(ReaderProxy::new(
        Guid::new(PEER_PREFIX, EntityId::UNKNOWN),
        EntityId::UNKNOWN,
        vec![nowhere],
        Vec::new(),
        SequenceNumber::new(0, 0),
        SequenceNumber::new(0, 0),
        false,
        true,
        SubscriptionBuiltinTopicData::new(
            &DataReaderQos::default(),
            &SubscriberQos::default(),
            &TopicQos::default(),
        ),
        SequenceNumber::new(0, 0),
    ));
}

/// Every sample the reader is still holding, newest read included.
fn kept_samples(reader: &DataReader<E2eSample>) -> Vec<E2eSample> {
    reader
        .read(
            SELF_SAMPLES as i32 * 2,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .map(|samples| samples.iter().filter_map(|s| s.data().ok()).collect())
        .unwrap_or_default()
}

/// H1's repro, pinned: a writer and a KEEP_ALL reader in one participant, and more
/// samples than the smallest pool class has slots.
///
/// `SlotMeta.refs` is a bitmap indexed by registry slot, so a descriptor delivered
/// into this participant's own segment gives the reader's claim the writer's own bit.
/// Evicting the change then lowers that one bit, the slot returns to the free queue
/// while a stored sample still points at it, and the next `acquire` writes over it.
/// The reader keeps `SELF_SAMPLES` samples that collapse onto as many distinct
/// payloads as the class has slots -- with no error, no counter and no log. What the
/// reader kept is the only thing that shows it, which is why this test reads the
/// samples back instead of asserting on the path.
#[test]
fn samples_kept_by_a_reader_in_the_writers_own_participant_stay_distinct() {
    let domain = crate::test_utils::unique_domain_id();
    let participant = e2e_participant(domain, "shm");
    let topic = participant
        .create_topic::<E2eSample>(
            SELF_TOPIC,
            E2E_TYPE,
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let writer = publisher
        .create_datawriter::<E2eSample>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let mut reader_qos = DataReaderQos::default();
    reader_qos.reliability.kind =
        crate::infrastructure::qos_policy::ReliabilityQosPolicyKind::Reliable;
    reader_qos.history.kind = crate::infrastructure::qos_policy::HistoryQosPolicyKind::KeepAll;
    let reader = subscriber
        .create_datareader::<E2eSample>(&topic, reader_qos, None, StatusMask::default())
        .unwrap();
    let runtime =
        participant_shm_runtime(&participant).expect("the shm transport must have a runtime");

    assert!(
        wait_until(Duration::from_secs(60), || {
            writer.get_publication_matched_status().map(|s| s.current_count() > 0).unwrap_or(false)
        }),
        "the reader in the writer's own participant never matched"
    );
    match_a_registered_shm_peer(&writer, &runtime);

    // Paced, one sample at a time: a KEEP_LAST(1) writer drops a change on the next
    // write whether or not it has been delivered, and this test is about what the
    // reader kept, not about how much of it arrived.
    for seq in 1..=SELF_SAMPLES {
        writer
            .write(
                &E2eSample { seq, stamp_nanos: now_nanos(), blob: self_blob(seq) },
                InstanceHandle::NIL,
            )
            .unwrap();
        assert!(
            wait_until(Duration::from_secs(10), || kept_samples(&reader).len() as u64 == seq),
            "sample {seq} did not reach the reader"
        );
    }

    let kept = kept_samples(&reader);
    assert_eq!(kept.len() as u64, SELF_SAMPLES, "the KEEP_ALL reader must hold every sample");
    assert_eq!(
        runtime.fallbacks().count(FallbackReason::NoLocalReader),
        0,
        "NoLocalReader fired, so no write reached the pool and this proves nothing"
    );

    let mut seen: Vec<u64> = kept.iter().map(|s| s.seq).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        kept.len(),
        "the reader kept {} samples but only {} distinct ones: a slot was reused under a          sample still pointing at it",
        kept.len(),
        seen.len()
    );
    for sample in &kept {
        assert_eq!(
            sample.blob,
            self_blob(sample.seq),
            "sample {} carries another's bytes",
            sample.seq
        );
    }

    participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
}

fn child_dds_subscriber() {
    let out = PathBuf::from(env_str(E2E_OUT_ENV));
    let (code, message) = match dds_subscriber_check() {
        Ok(message) => (0, message),
        Err(message) => (1, message),
    };
    let _ = std::fs::write(&out, message);
    std::process::exit(code);
}

fn dds_subscriber_check() -> Result<String, String> {
    let domain = env_str(E2E_DOMAIN_ENV).parse::<i32>().map_err(|e| e.to_string())?;
    let peer_prefix = prefix_from_hex(&env_str(E2E_PEER_PREFIX_ENV));

    let participant = e2e_participant(domain, "shm");
    let topic = participant
        .create_topic::<E2eSample>(
            E2E_TOPIC,
            E2E_TYPE,
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .map_err(|e| format!("{e:?}"))?;
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .map_err(|e| format!("{e:?}"))?;
    let reader = subscriber
        .create_datareader::<E2eSample>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .map_err(|e| format!("{e:?}"))?;

    // `read`, not `take`: the change has to stay in the reader history, because
    // the `ShmSlotHandle` it holds is exactly what keeps our bit standing in
    // the writer's `refs` bitmap for the check below.
    let mut got: Option<E2eSample> = None;
    wait_until(Duration::from_secs(60), || {
        let Ok(samples) = reader.read(
            1,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) else {
            return false;
        };
        got = samples.first().and_then(|s| s.data().ok());
        got.is_some()
    });
    let sample = got.ok_or_else(|| "no sample arrived within 60s".to_string())?;

    let expected = e2e_blob(E2E_BLOB_LEN);
    if sample.blob != expected {
        return Err(format!(
            "payload mismatch: got {} bytes, expected {}",
            sample.blob.len(),
            expected.len()
        ));
    }

    // Safe as an equality here, unlike inside the lib test binary: this process
    // was spawned for this one check, so nothing else could have counted.
    for reason in ALL_REJECT_REASONS {
        let count = descriptor_rejects().count(reason);
        if count != 0 {
            return Err(format!("descriptor reject {reason:?} x{count}"));
        }
    }

    let registry = RegistrySegment::open(domain as u32).map_err(|e| e.to_string())?;
    let my_prefix = reader.guid().prefix();
    let (my_slot, _) = registry
        .registry()
        .find_active(&my_prefix)
        .ok_or_else(|| "our own participant is not in the registry".to_string())?;
    let (peer_slot, _) = registry
        .registry()
        .find_active(&peer_prefix)
        .ok_or_else(|| "the publisher is not in the registry".to_string())?;
    let peer = PeerSegment::attach(domain as u32, peer_slot, my_slot).map_err(|e| e.to_string())?;
    let bit = 1u64 << my_slot;
    if !wait_until(Duration::from_secs(10), || slots_referenced_by(&peer, bit) > 0) {
        return Err(format!(
            "no slot in the publisher's pool (slot {peer_slot}) carries our bit (slot {my_slot})"
        ));
    }

    Ok(format!(
        "ok: {} bytes via publisher slot {peer_slot}, our slot {my_slot}",
        sample.blob.len()
    ))
}

/// (payload bytes, microseconds between writes, samples measured, warm-up samples).
///
/// The pace has to clear the *slowest* axis's service time for that size, not the
/// fastest. On the copy paths a fragmented sample is not delivered until its repair
/// round trips finish (~230 ms at 1 MiB, ~1.96 s at 8 MiB on the host this was
/// calibrated on), and a publisher that writes again before then supersedes the
/// sample being repaired: the subscriber receives nothing at all and the row reads
/// as a transport failure that is really a pacing mistake. The sample counts shrink
/// as the pace grows so one combination stays inside a couple of minutes.
const PERF_SIZES: [(usize, u64, u64, u64); 4] = [
    (1024, 1_000, 1000, 100),
    (65_536, 20_000, 1000, 100),
    (1_048_576, 500_000, 60, 5),
    (8_388_608, 3_000_000, 20, 2),
];

/// The default 8 MiB class cannot hold an 8 MiB user payload -- header and
/// encapsulation push the serialized sample past it, and `SampleTooLarge` rules it
/// out -- so measuring the zero-copy path at 8 MiB needs a class that fits.
const PERF_POOL_CLASSES: &str = "65536:64,1048576:12,16777216:4";

/// (`int2dds.transport` value, `INT2DDS_SHM_ZERO_COPY` value, label). Same code,
/// same topic, same QoS on all three; only these two knobs move.
const PERF_AXES: [(&str, &str, &str); 3] =
    [("udp", "1", "UDP"), ("shm", "0", "legacy SHM"), ("shm", "1", "zero-copy SHM")];

fn report_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('=').map(|v| v.to_string()))
}

/// The smallest sample at or above `pct` percent of the set. Below n = 100 the
/// 99th percentile is the maximum, and the reports label it as such rather than
/// print a "p99" that is really the second largest of twenty.
fn percentile(sorted: &[u64], pct: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * pct).div_ceil(100).max(1);
    sorted[rank - 1]
}

/// Three significant digits. The spread between runs is tens of percent, so more
/// digits than that would be inventing precision the measurement does not have.
fn micros(text: Option<String>) -> String {
    let Some(ns) = text.and_then(|v| v.parse::<u64>().ok()) else {
        return "-".to_string();
    };
    let us = ns as f64 / 1000.0;
    if us >= 100.0 {
        format!("{us:.0}")
    } else if us >= 10.0 {
        format!("{us:.1}")
    } else {
        format!("{us:.2}")
    }
}

/// Non-zero counters only, so a clean run reads as one word.
fn nonzero_counters(text: &str, prefix: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .filter_map(|rest| rest.split_once('='))
        .filter(|(_, value)| *value != "0")
        .map(|(name, value)| format!("{name} x{value}"))
        .collect()
}

/// The performance comparison.
///
/// In-crate because a zero-copy row is only known to be one by reading
/// `FallbackCounters`, which is `pub(crate)`. `#[ignore]` because 12
/// combinations of 1100 samples in two spawned processes each is minutes of
/// wall clock.
///
/// Run it with `--ignored --nocapture --test-threads=1`; the table goes to
/// stdout.
///
/// **Combinations interfere.** Back to back in one process, a combination
/// inherits the previous one's load and can read `received=0` for a transport
/// that delivers everything on its own. Numbers worth quoting are taken one
/// combination per process; use this test for the wiring, not for numbers.
///
/// **Two configurations.** At the OS default socket buffer the receive window
/// holds one DATA_FRAG, so the copy paths spend a repair round trip per
/// fragment. `INT2DDS_E2E_UDP_BUFFER` raises it out of that regime. The first
/// is what an unconfigured deployment gets; only the second compares the copy
/// and zero-copy paths fairly.
#[test]
#[ignore]
fn end_to_end_latency_across_three_transports() {
    println!(
        "config: INT2DDS_UDP_SOCKET_BUFFER={}, INT2DDS_SHM_POOL_CLASSES={PERF_POOL_CLASSES}",
        match effective_udp_buffer() {
            v if v.is_empty() => "(OS default)".to_string(),
            v => v,
        }
    );
    println!("the high column is the 99th percentile; below n=100 that is the maximum");
    println!(
        "| axis | size | sent | received | n | median us | p99/max us | median us (to user) | p99/max us (to user) | counters |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for (transport, zero_copy, label) in PERF_AXES {
        for (size, pace_us, count, warmup) in PERF_SIZES {
            println!(
                "{}",
                run_perf_combo(transport, zero_copy, label, size, pace_us, count, warmup)
            );
        }
    }
}

fn run_perf_combo(
    transport: &str,
    zero_copy: &str,
    label: &str,
    size: usize,
    pace_us: u64,
    count: u64,
    warmup: u64,
) -> String {
    let domain = crate::test_utils::unique_domain_id();
    let sub_out = report_path("perf_sub", domain);
    let pub_out = report_path("perf_pub", domain);
    let _ = std::fs::remove_file(&sub_out);
    let _ = std::fs::remove_file(&pub_out);

    let child_env = |out: &PathBuf| {
        vec![
            (E2E_DOMAIN_ENV, domain.to_string()),
            (E2E_OUT_ENV, out.display().to_string()),
            (E2E_TRANSPORT_ENV, transport.to_string()),
            (ZERO_COPY_ENV, zero_copy.to_string()),
            (E2E_SIZE_ENV, size.to_string()),
            (E2E_COUNT_ENV, count.to_string()),
            (E2E_WARMUP_ENV, warmup.to_string()),
            (E2E_PACE_ENV, pace_us.to_string()),
            ("INT2DDS_SHM_POOL_CLASSES", PERF_POOL_CLASSES.to_string()),
            ("INT2DDS_UDP_SOCKET_BUFFER", effective_udp_buffer()),
        ]
    };
    // Subscriber first: the publisher waits for a match and would otherwise
    // burn its own budget waiting for a process that has not started.
    let mut subscriber = KillOnDrop(spawn_child_with("perf_sub", &child_env(&sub_out)));
    let mut publisher = KillOnDrop(spawn_child_with("perf_pub", &child_env(&pub_out)));

    let budget = Duration::from_micros(pace_us * (count + warmup)) + Duration::from_secs(300);
    let finished = wait_until(budget, || {
        matches!(subscriber.0.try_wait(), Ok(Some(_)))
            && matches!(publisher.0.try_wait(), Ok(Some(_)))
    });

    let sub_report = std::fs::read_to_string(&sub_out).unwrap_or_default();
    let pub_report = std::fs::read_to_string(&pub_out).unwrap_or_default();
    let _ = std::fs::remove_file(&sub_out);
    let _ = std::fs::remove_file(&pub_out);
    drop(publisher);
    drop(subscriber);

    if !finished {
        return format!("| {label} | {size} | timed out after {budget:?} | | | | | | | |");
    }

    let mut counters = nonzero_counters(&pub_report, "fb_");
    counters.extend(nonzero_counters(&sub_report, "rej_"));
    let counters = if pub_report.contains("fb=none") && counters.is_empty() {
        "n/a (no zero-copy runtime)".to_string()
    } else if counters.is_empty() {
        "none".to_string()
    } else {
        counters.join(", ")
    };

    format!(
        "| {label} | {size} | {} | {} | {} | {} | {} | {} | {} | {} |",
        report_field(&pub_report, "sent").unwrap_or_else(|| "-".into()),
        report_field(&sub_report, "received").unwrap_or_else(|| "-".into()),
        report_field(&sub_report, "kept").unwrap_or_else(|| "-".into()),
        micros(report_field(&sub_report, "notify_median_ns")),
        micros(report_field(&sub_report, "notify_p99_ns")),
        micros(report_field(&sub_report, "data_median_ns")),
        micros(report_field(&sub_report, "data_p99_ns")),
        counters,
    )
}

fn child_perf_publisher() {
    let out = PathBuf::from(env_str(E2E_OUT_ENV));
    let domain = env_str(E2E_DOMAIN_ENV).parse::<i32>().expect("domain");
    let transport = env_str(E2E_TRANSPORT_ENV);
    let size = env_u64(E2E_SIZE_ENV) as usize;
    let count = env_u64(E2E_COUNT_ENV);
    let warmup = env_u64(E2E_WARMUP_ENV);
    let pace = Duration::from_micros(env_u64(E2E_PACE_ENV));

    let participant = e2e_participant(domain, &transport);
    let topic = participant
        .create_topic::<E2eSample>(
            E2E_TOPIC,
            E2E_TYPE,
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("topic");
    let publisher = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .expect("publisher");
    let writer = publisher
        .create_datawriter::<E2eSample>(&topic, perf_writer_qos(), None, StatusMask::default())
        .expect("writer");

    if !wait_until(Duration::from_secs(120), || {
        writer.get_publication_matched_status().map(|s| s.current_count() > 0).unwrap_or(false)
    }) {
        let _ = std::fs::write(&out, "error=no subscriber matched\n");
        std::process::exit(1);
    }
    std::thread::sleep(Duration::from_millis(500));

    // The blob is built once and reused: rebuilding it inside the loop would
    // put an allocation of the payload's size between the timestamp and the
    // write, and land in every measured latency.
    let mut sample = E2eSample { seq: 0, stamp_nanos: 0, blob: e2e_blob(size) };
    let mut sent = 0u64;
    let mut failed = 0u64;
    for seq in 0..(warmup + count) {
        sample.seq = seq;
        sample.stamp_nanos = now_nanos();
        match writer.write(&sample, InstanceHandle::NIL) {
            Ok(_) => sent += 1,
            Err(_) => failed += 1,
        }
        std::thread::sleep(pace);
    }
    // Let what is still in flight land before the counters are read.
    std::thread::sleep(Duration::from_secs(3));

    let mut report = format!("sent={sent}\nwrite_failed={failed}\n");
    match participant_shm_runtime(&participant) {
        Some(runtime) => {
            for reason in ALL_FALLBACK_REASONS {
                report.push_str(&format!("fb_{reason:?}={}\n", runtime.fallbacks().count(reason)));
            }
        }
        None => report.push_str("fb=none\n"),
    }
    let _ = std::fs::write(&out, report);
    std::process::exit(0);
}

/// Stamps the arrival twice: once on entry, before anything is deserialized,
/// and once after `data()` has handed the sample over. The gap between the two
/// columns is what the reader-side copy costs -- `CacheChange::data_bytes()`
/// still copies a `ShmSlot` out of the mapping; a reader loan API is what would
/// remove it.
struct PerfListener {
    seen: Arc<Mutex<Vec<(u64, u64, u64)>>>,
    /// When the last sample landed. A run that loses samples never reaches the
    /// expected count, and without this the subscriber would sit out its whole
    /// budget on every such combination.
    last: Arc<AtomicU64>,
}

impl DataReaderListener for PerfListener {
    type Foo = E2eSample;

    fn on_data_available(&self, reader: &DataReader<E2eSample>) {
        let notified = now_nanos();
        let Ok(samples) = reader.take(
            32,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) else {
            return;
        };
        for sample in samples.iter() {
            let Ok(data) = sample.data() else { continue };
            let handed = now_nanos();
            if let Ok(mut seen) = self.seen.lock() {
                seen.push((
                    data.seq,
                    notified.saturating_sub(data.stamp_nanos),
                    handed.saturating_sub(data.stamp_nanos),
                ));
            }
            self.last.store(handed, Ordering::Relaxed);
        }
    }
}

fn child_perf_subscriber() {
    let out = PathBuf::from(env_str(E2E_OUT_ENV));
    let domain = env_str(E2E_DOMAIN_ENV).parse::<i32>().expect("domain");
    let transport = env_str(E2E_TRANSPORT_ENV);
    let count = env_u64(E2E_COUNT_ENV);
    let warmup = env_u64(E2E_WARMUP_ENV);
    let pace_us = env_u64(E2E_PACE_ENV);

    let participant = e2e_participant(domain, &transport);
    let topic = participant
        .create_topic::<E2eSample>(
            E2E_TOPIC,
            E2E_TYPE,
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("topic");
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .expect("subscriber");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let last = Arc::new(AtomicU64::new(0));
    let _reader = subscriber
        .create_datareader::<E2eSample>(
            &topic,
            perf_reader_qos(),
            Some(Arc::new(PerfListener { seen: Arc::clone(&seen), last: Arc::clone(&last) })),
            StatusMask::default(),
        )
        .expect("reader");

    // The publisher's own budget for finding us is 120 s, so a subscriber that
    // started its delivery clock at reader creation could run out before the
    // publisher had written anything. The clock starts at the match instead.
    let matched = wait_until(Duration::from_secs(120), || {
        _reader.get_subscription_matched_status().map(|s| s.current_count() > 0).unwrap_or(false)
    });
    if !matched {
        let _ = std::fs::write(
            &out,
            "error=no publisher matched
",
        );
        std::process::exit(1);
    }

    let total = warmup + count;
    let hard = Instant::now() + Duration::from_micros(pace_us * total) + Duration::from_secs(60);
    let idle_cutoff = 15_000_000_000u64;
    loop {
        if seen.lock().map(|s| s.len() as u64 >= total).unwrap_or(false) {
            break;
        }
        if Instant::now() >= hard {
            break;
        }
        let seen_at = last.load(Ordering::Relaxed);
        if seen_at != 0 && now_nanos().saturating_sub(seen_at) > idle_cutoff {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // A short grace period so a tail that arrived just after the last check is
    // counted rather than reported as loss.
    std::thread::sleep(Duration::from_secs(2));

    let collected = seen.lock().map(|s| s.clone()).unwrap_or_default();
    let mut notify: Vec<u64> =
        collected.iter().filter(|(seq, _, _)| *seq >= warmup).map(|(_, n, _)| *n).collect();
    let mut handed: Vec<u64> =
        collected.iter().filter(|(seq, _, _)| *seq >= warmup).map(|(_, _, h)| *h).collect();
    notify.sort_unstable();
    handed.sort_unstable();

    let mut report = format!("received={}\nkept={}\n", collected.len(), notify.len());
    report.push_str(&format!("notify_median_ns={}\n", percentile(&notify, 50)));
    report.push_str(&format!("notify_p99_ns={}\n", percentile(&notify, 99)));
    report.push_str(&format!("data_median_ns={}\n", percentile(&handed, 50)));
    report.push_str(&format!("data_p99_ns={}\n", percentile(&handed, 99)));
    for reason in ALL_REJECT_REASONS {
        report.push_str(&format!("rej_{reason:?}={}\n", descriptor_rejects().count(reason)));
    }
    let _ = std::fs::write(&out, report);
    std::process::exit(0);
}
