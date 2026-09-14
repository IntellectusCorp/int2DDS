//! Cross-process checks. The test binary re-executes itself as a child; the
//! role comes from an environment variable.

#![cfg(test)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::pool::TEST_POOL_SIZE;
use super::registry::{
    now_tick, unlink_registry, ParticipantSlot, RegistrySegment, STALE_AFTER_TICKS,
};
use super::ring::{RING_INLINE, SPILL_NONE};
use super::runtime::{descriptor_rejects, DescriptorRejectReason, FallbackReason, ShmRuntime};
use super::segment::{unlink_segment, OwnedSegment, PeerSegment};
use super::slot::SlotRef;
use crate::common::instance_handle::InstanceHandle;
use crate::dcps::topic::type_support::DdsType;
use crate::domain::domain_participant::DomainParticipant;
use crate::domain::domain_participant_factory::DomainParticipantFactory;
use crate::domain::qos::DomainParticipantQos;
use crate::infrastructure::qos_policy::PROP_TRANSPORT;
use crate::infrastructure::status::StatusMask;
use crate::publication::qos::{DataWriterQos, PublisherQos};
use crate::subscription::data_reader::DataReader;
use crate::subscription::qos::{DataReaderQos, SubscriberQos};
use crate::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
use crate::topic::qos::TopicQos;

const ROLE_ENV: &str = "INT2DDS_SHM_SUBSTRATE_ROLE";
const CHILD_TEST: &str = "rtps::transport::shm::integration_test::shm_child_entry";
const DOMAIN: u32 = 237;
const PARENT_SLOT: u32 = 0;
const CHILD_SLOT: u32 = 1;
// Separate domains so `ParticipantSlot::claim`'s startup sweep never unlinks
// a segment the other test owns.
const REGISTRY_DOMAIN: u32 = 249;
const SEGMENT_DOMAIN: u32 = 250;

fn spawn_child(role: &str) -> std::process::Child {
    spawn_child_with(role, &[])
}

/// The child inherits this process's environment; an empty value is not set,
/// so an unset knob stays unset.
fn spawn_child_with(role: &str, env: &[(&str, String)]) -> std::process::Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.env(ROLE_ENV, role).args([CHILD_TEST, "--exact", "--ignored", "--nocapture"]);
    for (key, value) in env.iter().filter(|(_, value)| !value.is_empty()) {
        command.env(key, value);
    }
    command.spawn().unwrap()
}

/// Kills the child on the way out, including when an assertion panics: a
/// parked child holds an ACTIVE entry with a live pid in a shared registry.
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
        "registry_slot" => child_registry_slot(),
        "notifier" => child_notifier(),
        "dds_subscriber" => child_dds_subscriber(),
        other => panic!("unknown role {other}"),
    }
}

/// Claims the slot the parent published, then exits without releasing it.
fn child_claim_and_die() {
    let mine =
        OwnedSegment::create(DOMAIN, CHILD_SLOT, 1, TEST_POOL_SIZE, 4).expect("child segment");
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

/// Claims a registry slot and stays alive until killed.
fn child_registry_slot() {
    let _held = match ParticipantSlot::claim(REGISTRY_DOMAIN, [77; 12]) {
        Ok(held) => held,
        Err(_) => std::process::exit(2),
    };
    std::thread::sleep(Duration::from_secs(30));
}

/// Attaches to the parent's segment and pushes one message, waking it.
fn child_notifier() {
    let parent = attach_retry(SEGMENT_DOMAIN, PARENT_SLOT, CHILD_SLOT);
    std::thread::sleep(Duration::from_millis(100));
    parent.push_and_signal(b"across", SPILL_NONE).expect("push");
    std::process::exit(0);
}

#[test]
fn payload_survives_the_process_boundary() {
    unlink_segment(DOMAIN, PARENT_SLOT);
    unlink_segment(DOMAIN, CHILD_SLOT);

    let owned = OwnedSegment::create(DOMAIN, PARENT_SLOT, 1, TEST_POOL_SIZE, 8).unwrap();
    let mut child = spawn_child("claim_and_die");

    let mut lease = owned.owner_mut().acquire(16).unwrap();
    lease.bytes_mut()[..5].copy_from_slice(b"frame");
    let slot_ref = owned.owner_mut().commit(lease, 5);

    let child_seg = attach_retry(DOMAIN, CHILD_SLOT, PARENT_SLOT);
    child_seg.ring.push(&slot_ref.encode(), SPILL_NONE).unwrap();

    let status = child.wait().unwrap();
    assert!(status.success(), "child failed to claim the slot");

    // The dead child's bit survives in shared memory until reclaimed.
    let bit = 1u64 << CHILD_SLOT;
    let refs = |owned: &OwnedSegment| {
        let owner = owned.owner_mut();
        owner.pool().meta(slot_ref.order, slot_ref.index).unwrap().refs.load(Ordering::Acquire)
    };
    assert_eq!(refs(&owned) & bit, bit, "dead child still holds its bit");
    assert_eq!(owned.owner_mut().reclaim_participant(CHILD_SLOT), 1);
    assert_eq!(refs(&owned) & bit, 0);

    drop(child_seg);
    drop(owned);
    unlink_segment(DOMAIN, PARENT_SLOT);
    unlink_segment(DOMAIN, CHILD_SLOT);
}

#[test]
fn two_processes_share_one_registry() {
    unlink_registry(REGISTRY_DOMAIN);
    let mine = ParticipantSlot::claim(REGISTRY_DOMAIN, [76; 12]).unwrap();

    let mut child = KillOnDrop(spawn_child("registry_slot"));
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
    assert_eq!(mine.registry().pid(child_slot), Some(child.0.id()));
    assert_ne!(child_slot, mine.slot());

    child.0.kill().unwrap();
    child.0.wait().unwrap();

    // The forced tick makes every entry stale; `process_alive` tells the dead
    // child from the running parent.
    let freed = mine.registry().sweep_dead(now_tick() + STALE_AFTER_TICKS + 1, STALE_AFTER_TICKS);
    assert!(freed.contains(&child_slot), "a dead participant's slot must be reclaimed");
    assert!(!freed.contains(&mine.slot()), "a live participant must survive the sweep");

    drop(mine);
    unlink_registry(REGISTRY_DOMAIN);
}

#[test]
fn a_peer_process_wakes_a_blocked_owner() {
    unlink_segment(SEGMENT_DOMAIN, PARENT_SLOT);
    let owned = OwnedSegment::create(SEGMENT_DOMAIN, PARENT_SLOT, 1, TEST_POOL_SIZE, 4).unwrap();

    let mut child = KillOnDrop(spawn_child("notifier"));
    let deadline = Instant::now() + Duration::from_secs(5);
    // `wait_for_message` may return with nothing queued, so re-check the ring
    // against the remaining budget. `start` times the wakeup, not the spawn.
    let mut start = None;
    let mut out = [0u8; RING_INLINE];
    let len = loop {
        // The guard from `pop` dies with this `if let`, before the wait re-locks.
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
    unlink_segment(SEGMENT_DOMAIN, PARENT_SLOT);
}

// ---------------------------------------------------------------------------
// DDS end to end: real participants through the whole path -- writer loan,
// descriptor, ring, parser, claim -- in two processes on one host.
// ---------------------------------------------------------------------------

const E2E_DOMAIN_ENV: &str = "INT2DDS_E2E_DOMAIN";
const E2E_PEER_PREFIX_ENV: &str = "INT2DDS_E2E_PEER_PREFIX";
const E2E_OUT_ENV: &str = "INT2DDS_E2E_OUT";
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

/// Every byte a function of its offset, so a truncated or shifted sample shows.
fn e2e_blob(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// The guard is dropped before returning: the write path takes the same lock.
fn participant_shm_runtime(participant: &DomainParticipant) -> Option<Arc<ShmRuntime>> {
    let bridge = participant.get_dcps_bridge().ok()?;
    let runtime = bridge.as_ref()?.transport().shm_runtime();
    drop(bridge);
    runtime
}

fn shm_participant(domain: i32) -> DomainParticipant {
    let mut qos = DomainParticipantQos::default();
    qos.property.add_property(PROP_TRANSPORT, "shm", false);
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
    for order in 0..=pool.max_order() {
        for index in 0..pool.block_count(order) {
            if let Some(meta) = pool.meta(order, index) {
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

/// Green means "through a pool slot", not merely "delivered": the child checks
/// the bytes, this runtime's fallback counters must all be zero, and the child
/// must find its own bit in the parent's pool from a mapping it opens itself.
#[test]
fn a_dds_sample_crosses_two_processes_through_a_pool_slot() {
    let domain = crate::test_utils::unique_domain_id();
    let participant = shm_participant(domain);
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

    // Not one sample before the match: `NoLocalReader` would be the correct
    // answer and bump the counter this test then reads.
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
/// More than the smallest default class has slots, so the free queue wraps
/// while the reader still holds what it kept.
const SELF_SAMPLES: u64 = 100;

fn self_blob(seq: u64) -> Vec<u8> {
    (0..E2E_BLOB_LEN).map(|i| ((i as u64).wrapping_add(seq) % 251) as u8).collect()
}

/// Match a reader in another participant this registry knows, on a UDP locator
/// nothing listens on, so the `NoLocalReader` rule passes without the fake peer
/// ever receiving a descriptor. Its pid is ours, so the sweep never reclaims it.
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

/// A writer and a KEEP_ALL reader in one participant, with more samples than
/// the smallest pool class has slots. A descriptor delivered into the writer's
/// own segment would give the reader's claim the writer's own bit, and eviction
/// would then free a slot a kept sample still points at; only what the reader
/// kept shows it.
#[test]
fn samples_kept_by_a_reader_in_the_writers_own_participant_stay_distinct() {
    let domain = crate::test_utils::unique_domain_id();
    let participant = shm_participant(domain);
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

    // Paced: a KEEP_LAST(1) writer drops a change on the next write whether or
    // not it has been delivered.
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
        "the reader kept {} samples but only {} distinct ones: a slot was reused under a sample still pointing at it",
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

    let participant = shm_participant(domain);
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

    // `read`, not `take`: the change must stay in the reader history, since the
    // `ShmSlotHandle` it holds keeps our bit standing for the check below.
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

    // Safe as an equality: this process was spawned for this one check.
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
