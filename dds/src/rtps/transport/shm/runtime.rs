//! The SHM runtime a participant owns: its registry slot, its own segment,
//! the peers it has attached to, the thread that drains its ring, and the
//! counters that say why a sample took the copy path instead.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use log::{debug, info, warn};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::shm::pool::{SlotLease, MAX_CLASSES};
use crate::rtps::transport::shm::registry::{ParticipantSlot, Registry, HEARTBEAT_PERIOD};
use crate::rtps::transport::shm::ring::RING_INLINE;
use crate::rtps::transport::shm::segment::{unlink_segment, OwnedSegment, PeerSegment};
use crate::rtps::transport::shm::slot::ShmSlotHandle;

/// 4 MiB + 12 MiB + 16 MiB = 32 MiB per participant.
pub(crate) const DEFAULT_CLASSES: [(u32, u32); 3] = [(65536, 64), (1048576, 12), (8388608, 2)];
pub(crate) const DEFAULT_RING_ENTRIES: u32 = 1024;

/// Environment-driven configuration. A bad value never fails a participant;
/// it is logged once and replaced by the default.
pub(crate) struct ShmConfig {
    pub(crate) classes: Vec<(u32, u32)>,
    pub(crate) ring_entries: u32,
    pub(crate) enabled: bool,
}

impl ShmConfig {
    pub(crate) fn from_env() -> ShmConfig {
        let classes = match std::env::var("INT2DDS_SHM_POOL_CLASSES").ok().filter(|s| !s.is_empty())
        {
            Some(raw) => parse_classes(&raw).unwrap_or_else(|| {
                warn!("[shm] INT2DDS_SHM_POOL_CLASSES is not usable, falling back to defaults");
                DEFAULT_CLASSES.to_vec()
            }),
            None => DEFAULT_CLASSES.to_vec(),
        };
        let ring_entries =
            match std::env::var("INT2DDS_SHM_RING_ENTRIES").ok().filter(|s| !s.is_empty()) {
                Some(raw) => parse_ring_entries(&raw).unwrap_or_else(|| {
                    warn!("[shm] INT2DDS_SHM_RING_ENTRIES is not usable, falling back to default");
                    DEFAULT_RING_ENTRIES
                }),
                None => DEFAULT_RING_ENTRIES,
            };
        let enabled = match std::env::var("INT2DDS_SHM_ZERO_COPY").ok().filter(|s| !s.is_empty()) {
            Some(raw) => !(raw.eq_ignore_ascii_case("false") || raw == "0"),
            None => true,
        };
        ShmConfig { classes, ring_entries, enabled }
    }
}

/// `size:count` entries, comma separated, sizes strictly ascending.
fn parse_classes(raw: &str) -> Option<Vec<(u32, u32)>> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for entry in raw.split(',') {
        let (size, count) = entry.split_once(':')?;
        let size: u32 = size.trim().parse().ok()?;
        let count: u32 = count.trim().parse().ok()?;
        if size == 0 || count == 0 {
            return None;
        }
        if out.last().is_some_and(|(prev, _)| *prev >= size) {
            return None;
        }
        out.push((size, count));
    }
    (!out.is_empty() && out.len() <= MAX_CLASSES).then_some(out)
}

fn parse_ring_entries(raw: &str) -> Option<u32> {
    let value: u32 = raw.trim().parse().ok()?;
    (value >= 2 && value.is_power_of_two()).then_some(value)
}

/// Why a sample took the copy path. `NoLocalReader` through
/// `NotifyUnsupported` are decided at `write()`, `RingFull` and `PeerGone` at
/// send time. `NoSlotId` never reaches a counter: it applies exactly when no
/// runtime came up. `NotifyUnsupported` is never produced: a platform without
/// a kernel wakeup polls and keeps using slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FallbackReason {
    NoLocalReader,
    PeerNotRegistered,
    NoSlotId,
    SampleTooLarge,
    PoolExhausted,
    DurabilityTooStrong,
    NotifyUnsupported,
    RingFull,
    PeerGone,
}

impl FallbackReason {
    fn index(self) -> usize {
        match self {
            FallbackReason::NoLocalReader => 0,
            FallbackReason::PeerNotRegistered => 1,
            FallbackReason::NoSlotId => 2,
            FallbackReason::SampleTooLarge => 3,
            FallbackReason::PoolExhausted => 4,
            FallbackReason::DurabilityTooStrong => 5,
            FallbackReason::NotifyUnsupported => 6,
            FallbackReason::RingFull => 7,
            FallbackReason::PeerGone => 8,
        }
    }
}

pub(crate) const FALLBACK_REASONS: usize = 9;

#[derive(Default)]
pub(crate) struct FallbackCounters {
    counts: [AtomicU64; FALLBACK_REASONS],
}

impl FallbackCounters {
    /// True the first time a reason is seen, so the caller logs once.
    pub(crate) fn note(&self, reason: FallbackReason) -> bool {
        self.counts[reason.index()].fetch_add(1, Ordering::Relaxed) == 0
    }

    pub(crate) fn count(&self, reason: FallbackReason) -> u64 {
        self.counts[reason.index()].load(Ordering::Relaxed)
    }
}

/// Why a received descriptor was rejected, dropping that sample. Not a
/// `FallbackReason`: a rejected descriptor has no copy path to retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DescriptorRejectReason {
    /// No local `ShmRuntime`; a descriptor can still arrive over UDP or TCP.
    NoLocalRuntime,
    /// `PeerMap::resolve` could not map the remote writer's segment.
    PeerUnresolved,
    /// `PoolReader::claim` rejected the descriptor.
    ClaimRejected,
}

impl DescriptorRejectReason {
    fn index(self) -> usize {
        match self {
            DescriptorRejectReason::NoLocalRuntime => 0,
            DescriptorRejectReason::PeerUnresolved => 1,
            DescriptorRejectReason::ClaimRejected => 2,
        }
    }
}

const DESCRIPTOR_REJECT_REASONS: usize = 3;

/// Process-wide, unlike `FallbackCounters`: `NoLocalRuntime` has no runtime
/// to live inside.
#[derive(Default)]
pub(crate) struct DescriptorRejectCounters {
    counts: [AtomicU64; DESCRIPTOR_REJECT_REASONS],
}

impl DescriptorRejectCounters {
    pub(crate) fn note(&self, reason: DescriptorRejectReason) -> bool {
        self.counts[reason.index()].fetch_add(1, Ordering::Relaxed) == 0
    }

    pub(crate) fn count(&self, reason: DescriptorRejectReason) -> u64 {
        self.counts[reason.index()].load(Ordering::Relaxed)
    }
}

static DESCRIPTOR_REJECTS: DescriptorRejectCounters =
    DescriptorRejectCounters { counts: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)] };

pub(crate) fn descriptor_rejects() -> &'static DescriptorRejectCounters {
    &DESCRIPTOR_REJECTS
}

struct Peer {
    slot: u32,
    epoch: u64,
    segment: Arc<PeerSegment>,
}

/// Remote participants this one has attached to, keyed by GUID prefix.
pub(crate) struct PeerMap {
    domain: u32,
    my_slot: u32,
    peers: DashMap<[u8; 12], Peer>,
}

impl PeerMap {
    pub(crate) fn new(domain: u32, my_slot: u32) -> PeerMap {
        PeerMap { domain, my_slot, peers: DashMap::new() }
    }

    /// Clear a departed peer's bits unless the slot looks re-issued: refs
    /// bits are indexed by slot id, so a new owner's bits are
    /// indistinguishable from the old owner's. Leaking is the safer half.
    fn reclaim_if_not_reissued(
        &self,
        own: &OwnedSegment,
        registry: &Registry,
        slot: u32,
        known_epoch: Option<u64>,
    ) {
        let still_ours = known_epoch.is_some_and(|e| registry.epoch(slot) == Some(e));
        if !still_ours && !registry.is_free(slot) {
            debug!("[shm] slot {slot} was re-issued; its refs bits belong to the new owner");
            return;
        }
        let cleared = own.owner_mut().reclaim_participant(slot);
        debug!("[shm] cleared {cleared} slot refs for peer slot {slot}");
    }

    /// The registry entry of a peer, which is never ourselves: our own segment
    /// holds no bit that could tell a reader's claim apart from the writer's.
    pub(crate) fn find_peer(&self, registry: &Registry, prefix: &[u8; 12]) -> Option<(u32, u64)> {
        registry.find_active(prefix).filter(|(slot, _)| *slot != self.my_slot)
    }

    /// The peer's segment, attaching on first use. `None` when the participant
    /// is not registered, is ourselves, or its segment cannot be mapped.
    pub(crate) fn resolve(
        &self,
        own: &OwnedSegment,
        registry: &Registry,
        prefix: [u8; 12],
    ) -> Option<Arc<PeerSegment>> {
        if let Some(existing) = self.peers.get(&prefix) {
            if registry.epoch(existing.slot) == Some(existing.epoch) {
                return Some(Arc::clone(&existing.segment));
            }
            // The slot was re-issued before the sweep noticed.
            let (slot, epoch) = (existing.slot, existing.epoch);
            drop(existing);
            self.peers.remove(&prefix);
            self.reclaim_if_not_reissued(own, registry, slot, Some(epoch));
            debug!("[shm] peer slot {slot} was re-issued; re-resolving");
        }
        let (slot, epoch) = self.find_peer(registry, &prefix)?;
        let segment = match PeerSegment::attach(self.domain, slot, self.my_slot) {
            Ok(segment) => Arc::new(segment),
            Err(e) => {
                debug!("[shm] cannot attach to peer slot {slot}: {e}");
                return None;
            }
        };
        // The name can still resolve to the previous owner's segment; only the
        // header's epoch says whose it is.
        if segment.epoch != epoch {
            debug!(
                "[shm] peer slot {slot} still maps epoch {} while the registry says {epoch}",
                segment.epoch
            );
            return None;
        }
        let handle = Arc::clone(&segment);
        self.peers.insert(prefix, Peer { slot, epoch, segment });
        Some(handle)
    }

    /// Drop a peer: clear its bits from our pool, then release the mapping.
    pub(crate) fn forget(&self, own: &OwnedSegment, registry: &Registry, prefix: &[u8; 12]) {
        if let Some((_, peer)) = self.peers.remove(prefix) {
            self.reclaim_if_not_reissued(own, registry, peer.slot, Some(peer.epoch));
        }
    }

    /// Drop the cached mapping without touching refs bits. An unmatched peer
    /// may still be alive and reading our slots; only a confirmed-dead sweep
    /// (`forget_slot`) reclaims.
    pub(crate) fn drop_mapping(&self, prefix: &[u8; 12]) {
        self.peers.remove(prefix);
    }

    /// `forget`, addressed by registry slot, which is what a sweep produces.
    pub(crate) fn forget_slot(&self, own: &OwnedSegment, registry: &Registry, slot: u32) {
        // Kept as its own statement: the `iter()` guard must be released
        // before `forget` locks the same shard, or this deadlocks.
        let prefix = self.peers.iter().find(|e| e.slot == slot).map(|e| *e.key());
        match prefix {
            Some(prefix) => self.forget(own, registry, &prefix),
            None => self.reclaim_if_not_reissued(own, registry, slot, None),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.peers.len()
    }
}

/// Bounds how long a wedged ring stays wedged before a `pop` recovers it, and
/// how long the receive thread takes to notice the runtime is gone. Delivery
/// latency comes from the notifier.
pub(crate) const RECV_WAIT: Duration = Duration::from_secs(1);

/// Pops until the ring is empty, handing each message to `sink`. Every `pop`
/// takes and releases the ring guard in its own statement, so no guard is
/// alive when `sink` runs or when the caller waits.
pub(crate) fn drain<F: FnMut(&[u8])>(segment: &OwnedSegment, mut sink: F) -> usize {
    let mut buf = [0u8; RING_INLINE];
    let mut delivered = 0;
    loop {
        let popped = segment.ring_mut().pop(&mut buf);
        let Some((len, _spill)) = popped else { return delivered };
        sink(&buf[..len as usize]);
        delivered += 1;
    }
}

/// Runs until `session` returns `None`. The sink is built fresh each round
/// and dropped before the wait: a DDS-layer sink transitively owns the
/// `ShmRuntime` whose absence is this loop's exit condition.
pub(crate) fn recv_loop<S, F>(mut session: S)
where
    S: FnMut() -> Option<(Arc<OwnedSegment>, F)>,
    F: FnMut(&[u8]),
{
    loop {
        let Some((segment, sink)) = session() else { return };
        // Drain first: `pop` is the only thing that recovers a wedged cell.
        drain(&segment, sink);
        segment.wait_for_message(RECV_WAIT);
    }
}

/// Failing to spawn is not fatal: the ring goes unread and the send side
/// falls back to the copy path once it fills.
pub(crate) fn spawn_receiver<S, F>(session: S)
where
    S: FnMut() -> Option<(Arc<OwnedSegment>, F)> + Send + 'static,
    F: FnMut(&[u8]),
{
    let spawned = std::thread::Builder::new().name("int2dds-shm-recv".into()).spawn(move || {
        recv_loop(session);
    });
    if let Err(e) = spawned {
        warn!("[shm] no receive thread; the zero-copy ring stays unread: {e}");
    }
}

pub(crate) struct ShmRuntime {
    // Declared before `slot`: dropping our segment before releasing our
    // registry entry keeps the entry ACTIVE while the segment name disappears,
    // so a racing peer sees a benign attach failure, never a claim on our slot.
    // `Arc` because a writer-side `ShmSlotHandle` holds a clone.
    own: Arc<OwnedSegment>,
    slot: ParticipantSlot,
    peers: PeerMap,
    fallbacks: FallbackCounters,
}

impl ShmRuntime {
    /// `None` whenever SHM cannot be brought up; every caller falls back to
    /// the copy path.
    pub(crate) fn start(domain: u32, guid_prefix: GuidPrefix) -> Option<Arc<ShmRuntime>> {
        let config = ShmConfig::from_env();
        if !config.enabled {
            info!("[shm] zero-copy disabled by INT2DDS_SHM_ZERO_COPY");
            return None;
        }

        let slot = match ParticipantSlot::claim(domain, guid_prefix) {
            Ok(slot) => slot,
            Err(e) => {
                warn!("[shm] no registry slot, falling back to the copy path: {e}");
                return None;
            }
        };

        // A slot just claimed may still carry a dead predecessor's segment.
        // No-op on Windows, where a peer's lingering handle makes `create`
        // fail with `AlreadyExists` and this participant stays on UDP.
        unlink_segment(domain, slot.slot());

        let own = match OwnedSegment::create(
            domain,
            slot.slot(),
            slot.epoch(),
            &config.classes,
            config.ring_entries,
        ) {
            Ok(own) => own,
            Err(e) => {
                warn!("[shm] cannot create segment, falling back to the copy path: {e}");
                return None;
            }
        };

        let peers = PeerMap::new(domain, slot.slot());
        info!("[shm] runtime started on domain {domain} slot {}", slot.slot());
        let runtime = Arc::new(ShmRuntime {
            own: Arc::new(own),
            slot,
            peers,
            fallbacks: FallbackCounters::default(),
        });

        // Reclaims peers that died without unmatching. Holds a Weak, so it
        // exits on the first tick after the last Arc is gone.
        let weak = Arc::downgrade(&runtime);
        let spawned =
            std::thread::Builder::new().name("int2dds-shm-sweep".into()).spawn(move || {
                while let Some(rt) = weak.upgrade() {
                    rt.sweep();
                    drop(rt);
                    std::thread::sleep(HEARTBEAT_PERIOD);
                }
            });
        if let Err(e) = spawned {
            warn!("[shm] no maintenance thread; peers that die without unmatching stay: {e}");
        }
        Some(runtime)
    }

    pub(crate) fn own(&self) -> &OwnedSegment {
        &self.own
    }

    pub(crate) fn own_arc(&self) -> Arc<OwnedSegment> {
        Arc::clone(&self.own)
    }

    /// Publish a lease as a slot the writer holds. `None` only if the slot
    /// fails validation, which a fresh commit never does; the slot is freed.
    pub(crate) fn commit(&self, lease: SlotLease, len: u32) -> Option<ShmSlotHandle> {
        // The guard must be gone before `own` re-locks the pool.
        let slot_ref = self.own.owner_mut().commit(lease, len);
        let handle = ShmSlotHandle::own(self.own_arc(), slot_ref);
        if handle.is_none() {
            self.own.owner_mut().release_own(slot_ref.class, slot_ref.index);
        }
        handle
    }

    pub(crate) fn abort(&self, lease: SlotLease) {
        self.own.owner_mut().abort(lease);
    }

    pub(crate) fn peers(&self) -> &PeerMap {
        &self.peers
    }

    /// The `NoLocalReader` rule: is `prefix` a participant reachable through SHM?
    pub(crate) fn peer_is_registered(&self, prefix: &GuidPrefix) -> bool {
        self.peers.find_peer(self.registry(), prefix).is_some()
    }

    pub(crate) fn fallbacks(&self) -> &FallbackCounters {
        &self.fallbacks
    }

    pub(crate) fn registry(&self) -> &Registry {
        self.slot.registry()
    }

    pub(crate) fn slot(&self) -> u32 {
        self.slot.slot()
    }

    /// DDS unmatch is not death: only drops the cached mapping.
    pub(crate) fn peer_lost(&self, prefix: GuidPrefix) {
        self.peers.drop_mapping(&prefix);
    }

    /// Reclaim the slots of participants that died without unmatching. A slot
    /// re-claimed before the next tick is no longer dead here, so its old
    /// owner's bits stay leaked; a capacity loss, not corruption.
    pub(crate) fn sweep(&self) {
        for slot in self.slot.sweep() {
            self.peers.forget_slot(&self.own, self.registry(), slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::registry::{now_tick, unlink_registry, RegistrySegment};
    use crate::rtps::transport::shm::ring::{notify_supported, POLL_INTERVAL, SPILL_NONE};
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use std::sync::Mutex;

    #[test]
    fn config_parses_valid_input_and_rejects_the_rest() {
        assert_eq!(parse_classes("65536:64,1048576:12"), Some(vec![(65536, 64), (1048576, 12)]));
        assert_eq!(parse_classes("1048576:12,65536:64"), None, "not ascending");
        assert_eq!(parse_classes("65536:64,65536:12"), None);
        assert_eq!(parse_classes("65536:0"), None);
        assert_eq!(parse_classes("0:64"), None);
        assert_eq!(parse_classes("1:1,2:1,3:1,4:1,5:1"), None, "more than MAX_CLASSES");
        assert_eq!(parse_classes(""), None);
        assert_eq!(parse_classes("65536"), None);
        assert_eq!(parse_classes("abc:64"), None);

        assert_eq!(parse_ring_entries("1024"), Some(1024));
        assert_eq!(parse_ring_entries("2"), Some(2));
        assert_eq!(parse_ring_entries("1000"), None);
        assert_eq!(parse_ring_entries("1"), None);
        assert_eq!(parse_ring_entries("x"), None);

        let raw: Vec<String> = DEFAULT_CLASSES.iter().map(|(s, c)| format!("{s}:{c}")).collect();
        assert_eq!(parse_classes(&raw.join(",")), Some(DEFAULT_CLASSES.to_vec()));
        assert_eq!(
            parse_ring_entries(&DEFAULT_RING_ENTRIES.to_string()),
            Some(DEFAULT_RING_ENTRIES)
        );
    }

    #[test]
    fn counters_count_each_reason_separately_and_log_once() {
        let c = FallbackCounters::default();
        assert!(c.note(FallbackReason::NoLocalReader));
        assert!(!c.note(FallbackReason::NoLocalReader));
        assert!(c.note(FallbackReason::PoolExhausted));
        assert_eq!(c.count(FallbackReason::NoLocalReader), 2);
        assert_eq!(c.count(FallbackReason::PoolExhausted), 1);

        let d = DescriptorRejectCounters::default();
        assert!(d.note(DescriptorRejectReason::PeerUnresolved));
        assert!(!d.note(DescriptorRejectReason::PeerUnresolved));
        assert_eq!(d.count(DescriptorRejectReason::PeerUnresolved), 2);
        assert_eq!(d.count(DescriptorRejectReason::NoLocalRuntime), 0);
    }

    const ME: u32 = 0;
    const PEER: u32 = 1;

    struct Fixture {
        reg: RegistrySegment,
        mine: OwnedSegment,
        _theirs: OwnedSegment,
        domain: u32,
    }

    /// Our own registry slot is claimed first so the peer lands on `PEER`.
    fn fixture(domain: u32) -> Fixture {
        unlink_registry(domain);
        unlink_segment(domain, ME);
        unlink_segment(domain, PEER);
        let reg = RegistrySegment::open(domain).unwrap();
        let mine = OwnedSegment::create(domain, ME, 1, &[(64, 2)], 4).unwrap();
        let theirs = OwnedSegment::create(domain, PEER, 1, &[(64, 2)], 4).unwrap();
        reg.registry().claim(std::process::id(), [0; 12], now_tick()).unwrap();
        Fixture { reg, mine, _theirs: theirs, domain }
    }

    impl Fixture {
        fn peer_bit_set(&self, class: u16, index: u32, slot: u32) {
            let owner = self.mine.owner_mut();
            let m = owner.pool().meta(class, index).unwrap();
            m.refs.fetch_or(1u64 << slot, Ordering::AcqRel);
        }

        fn peer_bit(&self, class: u16, index: u32, slot: u32) -> u64 {
            let owner = self.mine.owner_mut();
            let m = owner.pool().meta(class, index).unwrap();
            m.refs.load(Ordering::Acquire) & (1u64 << slot)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            unlink_registry(self.domain);
            unlink_segment(self.domain, ME);
            unlink_segment(self.domain, PEER);
        }
    }

    #[test]
    fn resolve_caches_a_registered_peer_and_refuses_the_unregistered_and_ourselves() {
        let f = fixture(245);
        let (slot, _) = f.reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        assert_eq!(slot, PEER);
        let map = PeerMap::new(f.domain, ME);

        let first = map.resolve(&f.mine, f.reg.registry(), [9; 12]).unwrap();
        let second = map.resolve(&f.mine, f.reg.registry(), [9; 12]).unwrap();
        assert!(Arc::ptr_eq(&first, &second), "resolve must cache");
        assert_eq!(map.len(), 1);

        assert!(map.resolve(&f.mine, f.reg.registry(), [200; 12]).is_none(), "unregistered");
        assert!(map.resolve(&f.mine, f.reg.registry(), [0; 12]).is_none(), "ourselves");
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn forget_clears_the_peers_bit_but_drop_mapping_keeps_it() {
        let f = fixture(246);
        let (slot, _) = f.reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        let map = PeerMap::new(f.domain, ME);
        map.resolve(&f.mine, f.reg.registry(), [9; 12]).unwrap();

        let lease = f.mine.owner_mut().acquire(8).unwrap();
        let r = f.mine.owner_mut().commit(lease, 1);
        f.peer_bit_set(r.class, r.index, slot);

        // Unmatch is not death: the peer may still be reading our slots.
        map.drop_mapping(&[9; 12]);
        assert_eq!(map.len(), 0);
        assert_ne!(f.peer_bit(r.class, r.index, slot), 0, "an unmatched peer's bits survive");

        map.resolve(&f.mine, f.reg.registry(), [9; 12]).unwrap();
        map.forget(&f.mine, f.reg.registry(), &[9; 12]);
        assert_eq!(map.len(), 0);
        assert_eq!(f.peer_bit(r.class, r.index, slot), 0);
    }

    #[test]
    fn a_reissued_slot_is_neither_served_from_the_cache_nor_adopted_from_a_leftover() {
        let f = fixture(247);
        let (slot, epoch) =
            f.reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        let map = PeerMap::new(f.domain, ME);
        map.resolve(&f.mine, f.reg.registry(), [9; 12]).unwrap();

        let lease = f.mine.owner_mut().acquire(8).unwrap();
        let r = f.mine.owner_mut().commit(lease, 1);
        f.peer_bit_set(r.class, r.index, slot);

        // The peer leaves, someone else takes its slot, and leaves too.
        f.reg.registry().release(slot, epoch);
        let (again, again_epoch) =
            f.reg.registry().claim(std::process::id(), [7; 12], now_tick()).unwrap();
        assert_eq!(again, slot);
        f.reg.registry().release(again, again_epoch);

        assert!(map.resolve(&f.mine, f.reg.registry(), [9; 12]).is_none(), "stale peer");
        assert_eq!(map.len(), 0, "the stale entry is evicted");
        assert_eq!(f.peer_bit(r.class, r.index, slot), 0, "a free slot's bits are reclaimed");

        // A new owner whose segment name still resolves to the old epoch.
        let (again, _) = f.reg.registry().claim(std::process::id(), [8; 12], now_tick()).unwrap();
        assert_eq!(again, slot);
        assert!(map.resolve(&f.mine, f.reg.registry(), [8; 12]).is_none(), "older epoch");
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn forget_slot_reclaims_only_when_the_slot_is_free_or_still_the_one_mapped() {
        let f = fixture(248);
        let (dead_slot, dead_epoch) =
            f.reg.registry().claim(std::process::id(), [9; 12], now_tick()).unwrap();
        let map = PeerMap::new(f.domain, ME);
        let lease = f.mine.owner_mut().acquire(8).unwrap();
        let r = f.mine.owner_mut().commit(lease, 1);

        // Cached and still current: cleared.
        map.resolve(&f.mine, f.reg.registry(), [9; 12]).unwrap();
        f.peer_bit_set(r.class, r.index, dead_slot);
        map.forget_slot(&f.mine, f.reg.registry(), dead_slot);
        assert_eq!(map.len(), 0);
        assert_eq!(f.peer_bit(r.class, r.index, dead_slot), 0);

        // Not cached, slot free: cleared.
        f.reg.registry().release(dead_slot, dead_epoch);
        f.peer_bit_set(r.class, r.index, dead_slot);
        map.forget_slot(&f.mine, f.reg.registry(), dead_slot);
        assert_eq!(f.peer_bit(r.class, r.index, dead_slot), 0);

        // Re-issued to a new participant: its bits are left alone.
        let (new_slot, _) =
            f.reg.registry().claim(std::process::id(), [8; 12], now_tick()).unwrap();
        assert_eq!(new_slot, dead_slot);
        f.peer_bit_set(r.class, r.index, new_slot);
        map.forget_slot(&f.mine, f.reg.registry(), new_slot);
        assert_ne!(f.peer_bit(r.class, r.index, new_slot), 0);
    }

    const RECV_DOMAIN: u32 = 232;

    #[test]
    fn the_loop_drains_every_queued_message_before_it_waits() {
        unlink_segment(RECV_DOMAIN, 0);
        let owned = Arc::new(OwnedSegment::create(RECV_DOMAIN, 0, 1, &[(64, 2)], 8).unwrap());
        let peer = PeerSegment::attach(RECV_DOMAIN, 0, 1).unwrap();
        for msg in [b"one".as_slice(), b"two".as_slice(), b"three".as_slice()] {
            peer.push_and_signal(msg, SPILL_NONE).unwrap();
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let worker = {
            let segment = Arc::clone(&owned);
            let mut rounds = 0u32;
            std::thread::spawn(move || {
                recv_loop(move || {
                    rounds += 1;
                    if rounds > 1 {
                        return None;
                    }
                    let tx = tx.clone();
                    Some((Arc::clone(&segment), move |bytes: &[u8]| {
                        let _ = tx.send(bytes.to_vec());
                    }))
                });
            })
        };

        // Already-queued messages come out in a fraction of `RECV_WAIT`; the
        // loop then waits one round and exits.
        let mut seen = Vec::new();
        for _ in 0..3 {
            seen.push(rx.recv_timeout(Duration::from_millis(200)).expect("queued message"));
        }
        assert_eq!(seen, vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]);
        worker.join().unwrap();

        drop(peer);
        drop(owned);
        unlink_segment(RECV_DOMAIN, 0);
    }

    #[test]
    fn an_empty_ring_does_not_spin_the_loop() {
        const OBSERVE: Duration = Duration::from_millis(300);
        unlink_segment(RECV_DOMAIN, 6);
        let owned = Arc::new(OwnedSegment::create(RECV_DOMAIN, 6, 1, &[(64, 2)], 8).unwrap());

        let rounds = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let segment = Arc::clone(&owned);
            let rounds = Arc::clone(&rounds);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                recv_loop(move || {
                    if stop.load(Ordering::Relaxed) {
                        return None;
                    }
                    rounds.fetch_add(1, Ordering::Relaxed);
                    Some((Arc::clone(&segment), |_: &[u8]| {}))
                });
            })
        };

        std::thread::sleep(OBSERVE);
        let spun = rounds.load(Ordering::Relaxed);
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap();

        // A kernel wakeup parks the loop for `RECV_WAIT`; a polling waiter
        // returns every `POLL_INTERVAL`.
        let bound = if notify_supported() {
            10
        } else {
            (OBSERVE.as_micros() / POLL_INTERVAL.as_micros()) as usize * 2
        };
        assert!(spun < bound, "the loop spun {spun} times on an empty ring");

        drop(owned);
        unlink_segment(RECV_DOMAIN, 6);
    }

    #[test]
    fn start_registers_the_participant_and_stops_cleanly() {
        const DOMAIN: u32 = 249;
        unlink_registry(DOMAIN);
        let rt = ShmRuntime::start(DOMAIN, [21; 12]).unwrap();
        let slot = rt.slot();
        assert_eq!(rt.registry().find_active(&[21; 12]).map(|(s, _)| s), Some(slot));
        assert!(!rt.peer_is_registered(&[21; 12]), "we are not our own peer");
        drop(rt);
        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, slot);
    }

    #[test]
    fn drain_hands_each_message_to_the_sink_in_order() {
        unlink_segment(RECV_DOMAIN, 8);
        let owned = OwnedSegment::create(RECV_DOMAIN, 8, 1, &[(64, 2)], 8).unwrap();
        let peer = PeerSegment::attach(RECV_DOMAIN, 8, 9).unwrap();
        for msg in [b"a".as_slice(), b"b".as_slice()] {
            peer.push_and_signal(msg, SPILL_NONE).unwrap();
        }
        let seen = Mutex::new(Vec::new());
        assert_eq!(drain(&owned, |b| seen.lock().unwrap().push(b.to_vec())), 2);
        assert_eq!(seen.into_inner().unwrap(), vec![b"a".to_vec(), b"b".to_vec()]);
        drop(peer);
        drop(owned);
        unlink_segment(RECV_DOMAIN, 8);
    }
}
