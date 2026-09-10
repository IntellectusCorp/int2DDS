//! Why a sample took the copy path instead of a pool slot. Spec §7.

use std::sync::atomic::{AtomicU64, Ordering};

/// Spec §7. 1-7 are decided at `write()`, 8-9 at send time.
///
/// `NoSlotId` and `NotifyUnsupported` (rules 3 and 7) can never reach these
/// counters: the counters live inside `ShmRuntime`, and those two rules apply
/// exactly when no runtime came up. Spec §7 reports them through
/// `ShmRuntime::start`'s `warn` instead, once per participant.
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
    /// An exhaustive match, not `as usize`: adding a variant must fail to
    /// compile here rather than index past the counter array at runtime.
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
    /// Records one occurrence. Returns true the first time a reason is seen, so
    /// the caller logs once and stays quiet after.
    pub(crate) fn note(&self, reason: FallbackReason) -> bool {
        self.counts[reason.index()].fetch_add(1, Ordering::Relaxed) == 0
    }

    pub(crate) fn count(&self, reason: FallbackReason) -> u64 {
        self.counts[reason.index()].load(Ordering::Relaxed)
    }
}

/// Why a receive-time descriptor was rejected, dropping just that sample.
/// Spec §5.6's validation, §6.4, §10.
///
/// Deliberately not a `FallbackReason`: those nine are spec §7's write/send-time
/// rules, each of which retries the same sample on the copy path. A rejected
/// descriptor has no copy path to retry -- the 24 bytes that arrived are a
/// pointer, not the sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DescriptorRejectReason {
    /// This participant has no local `ShmRuntime` at all -- `handle_data_message`
    /// also runs for UDP and TCP arrivals, so a slot descriptor can reach it even
    /// when SHM never came up locally.
    NoLocalRuntime,
    /// `PeerMap::resolve` could not map the remote writer's segment.
    PeerUnresolved,
    /// `ShmSlotHandle::claim` rejected the descriptor (spec §5.6 steps 1-4).
    ClaimRejected,
}

impl DescriptorRejectReason {
    /// An exhaustive match, not `as usize`: adding a variant must fail to
    /// compile here rather than index past the counter array at runtime.
    fn index(self) -> usize {
        match self {
            DescriptorRejectReason::NoLocalRuntime => 0,
            DescriptorRejectReason::PeerUnresolved => 1,
            DescriptorRejectReason::ClaimRejected => 2,
        }
    }
}

const DESCRIPTOR_REJECT_REASONS: usize = 3;

/// Same shape as `FallbackCounters`: a count per reason plus a read accessor, so a test
/// (Task 10's zero-copy path) can assert "no rejects" instead of only grepping logs.
///
/// Process-wide, not one instance per `ShmRuntime` like `FallbackCounters`: `NoLocalRuntime`
/// has no `ShmRuntime` to live inside by definition, and splitting the three reasons across
/// two storage granularities would make "were there any rejects" unreliable to check.
#[derive(Default)]
pub(crate) struct DescriptorRejectCounters {
    counts: [AtomicU64; DESCRIPTOR_REJECT_REASONS],
}

impl DescriptorRejectCounters {
    /// Records one occurrence. Returns true the first time a reason is seen, so
    /// the caller logs once and stays quiet after.
    pub(crate) fn note(&self, reason: DescriptorRejectReason) -> bool {
        self.counts[reason.index()].fetch_add(1, Ordering::Relaxed) == 0
    }

    pub(crate) fn count(&self, reason: DescriptorRejectReason) -> u64 {
        self.counts[reason.index()].load(Ordering::Relaxed)
    }
}

static DESCRIPTOR_REJECTS: DescriptorRejectCounters =
    DescriptorRejectCounters { counts: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)] };

/// The process-wide receive-time descriptor-rejection counters. See `DescriptorRejectCounters`.
pub(crate) fn descriptor_rejects() -> &'static DescriptorRejectCounters {
    &DESCRIPTOR_REJECTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_reason_counts_separately_and_logs_once() {
        let c = FallbackCounters::default();
        assert_eq!(c.count(FallbackReason::NoLocalReader), 0);
        assert!(c.note(FallbackReason::NoLocalReader), "the first must ask for a log line");
        assert!(!c.note(FallbackReason::NoLocalReader), "the second must not");
        assert!(c.note(FallbackReason::PoolExhausted), "a different reason logs once too");
        assert_eq!(c.count(FallbackReason::NoLocalReader), 2);
        assert_eq!(c.count(FallbackReason::PoolExhausted), 1);
    }

    #[test]
    fn descriptor_reject_counters_count_separately_and_log_once() {
        let c = DescriptorRejectCounters::default();
        assert_eq!(c.count(DescriptorRejectReason::PeerUnresolved), 0);
        assert!(
            c.note(DescriptorRejectReason::PeerUnresolved),
            "the first must ask for a log line"
        );
        assert!(!c.note(DescriptorRejectReason::PeerUnresolved), "the second must not");
        assert!(c.note(DescriptorRejectReason::ClaimRejected), "a different reason logs once too");
        assert_eq!(c.count(DescriptorRejectReason::PeerUnresolved), 2);
        assert_eq!(c.count(DescriptorRejectReason::ClaimRejected), 1);
        assert_eq!(c.count(DescriptorRejectReason::NoLocalRuntime), 0);
    }
}
