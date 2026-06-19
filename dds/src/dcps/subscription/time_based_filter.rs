//! Reader-side TIME_BASED_FILTER QoS enforcement.
//!
//! Per instance, at most one sample per `minimum_separation`. A sample inside the window is held
//! (not dropped); the latest held sample is delivered by a one-shot timer when the window elapses.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    common::instance_handle::InstanceHandle,
    rtps::{common::time::RtpsTime, entities::history::cache_change::CacheChange},
};

// Per-instance separation tracking.
struct InstanceFilterState {
    // Reader time at which the last sample was delivered to the cache.
    last_delivered: RtpsTime,
    // Most recent sample seen inside the current window, awaiting delayed delivery.
    pending: Option<Arc<CacheChange>>,
    // True while a delayed delivery is already scheduled for this instance.
    has_pending_timer: bool,
}

// Decision for an ALIVE sample fed to the filter.
pub(crate) enum FilterOutcome {
    // Deliver this sample to the reader cache now.
    Deliver,
    // Sample was held; schedule a delayed delivery after this duration when Some, otherwise a
    // delayed delivery is already scheduled for this instance.
    Held { deliver_after: Option<std::time::Duration> },
}

#[derive(Default)]
pub(crate) struct TimeBasedFilter {
    instances: Mutex<HashMap<InstanceHandle, InstanceFilterState>>,
}

impl TimeBasedFilter {
    pub(crate) fn new() -> Self {
        Self { instances: Mutex::new(HashMap::new()) }
    }

    // Decide whether an ALIVE sample passes now or is held (cloned into the pending slot).
    // `min_separation_nanos` is the QoS value in nanoseconds; `now` is the reader's current time.
    pub(crate) fn on_alive_sample(
        &self,
        change: &Arc<CacheChange>,
        min_separation_nanos: u64,
        now: RtpsTime,
    ) -> FilterOutcome {
        let instance_handle = change.instance_handle();
        let mut instances = self.lock();

        let Some(state) = instances.get_mut(&instance_handle) else {
            // First sample for this instance always passes.
            instances.insert(
                instance_handle,
                InstanceFilterState {
                    last_delivered: now,
                    pending: None,
                    has_pending_timer: false,
                },
            );
            return FilterOutcome::Deliver;
        };

        // A timer is already scheduled: keep only the newest sample. Checked before the window
        // test so a boundary arrival cannot race past the pending one and deliver alongside it.
        if state.has_pending_timer {
            state.pending = Some(Arc::clone(change));
            return FilterOutcome::Held { deliver_after: None };
        }

        // Outside the window: deliver now and advance last_delivered to `now`.
        let next_allowed = state.last_delivered.add_nanos(min_separation_nanos);
        if now >= next_allowed {
            state.last_delivered = now;
            return FilterOutcome::Deliver;
        }

        // Inside the window with nothing scheduled yet: hold and schedule a delayed delivery.
        state.pending = Some(Arc::clone(change));
        state.has_pending_timer = true;
        let remaining = next_allowed.to_nanos().saturating_sub(now.to_nanos());
        FilterOutcome::Held { deliver_after: Some(std::time::Duration::from_nanos(remaining)) }
    }

    // Take the held sample when the one-shot timer fires, advancing last_delivered to `now`.
    pub(crate) fn take_pending_on_timer(
        &self,
        instance_handle: InstanceHandle,
        now: RtpsTime,
    ) -> Option<Arc<CacheChange>> {
        let mut instances = self.lock();
        let state = instances.get_mut(&instance_handle)?;
        state.has_pending_timer = false;
        let change = state.pending.take()?;
        state.last_delivered = now;
        Some(change)
    }

    // Drop the held sample on dispose/unregister; the instance still exists, so keep its state.
    pub(crate) fn discard_pending(&self, instance_handle: InstanceHandle) {
        let mut instances = self.lock();
        if let Some(state) = instances.get_mut(&instance_handle) {
            state.pending = None;
        }
    }

    // Forget an instance's filter state when the instance is removed from the reader.
    pub(crate) fn remove_instance(&self, instance_handle: InstanceHandle) {
        let mut instances = self.lock();
        instances.remove(&instance_handle);
    }

    #[cfg(test)]
    pub(crate) fn tracks_instance(&self, instance_handle: InstanceHandle) -> bool {
        self.lock().contains_key(&instance_handle)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<InstanceHandle, InstanceFilterState>> {
        match self.instances.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::{guid::Guid, sequence::SequenceNumber, types::ChangeKind};

    const MIN_SEP_NANOS: u64 = 2_000_000_000; // 2s
    const ONE_SEC: u64 = 1_000_000_000;

    fn alive(seq: i64, instance: u8) -> Arc<CacheChange> {
        Arc::new(CacheChange::new(
            ChangeKind::Alive,
            Guid::UNKNOWN,
            InstanceHandle::new([instance; 16]),
            SequenceNumber::from_i64(seq),
            Vec::new(),
            None,
        ))
    }

    // A sample arriving exactly at the window boundary while another is held must not be
    // delivered alongside the pending one; only the latest held sample is delivered, once.
    #[test]
    fn boundary_arrival_does_not_race_past_pending() {
        let filter = TimeBasedFilter::new();
        let t0 = RtpsTime::now();
        let t1 = t0.add_nanos(ONE_SEC);
        let t2 = t0.add_nanos(2 * ONE_SEC);

        // A: first sample passes immediately.
        assert!(matches!(
            filter.on_alive_sample(&alive(1, 1), MIN_SEP_NANOS, t0),
            FilterOutcome::Deliver
        ));

        // B at t1 is inside the window: held, schedules a delayed delivery.
        assert!(matches!(
            filter.on_alive_sample(&alive(2, 1), MIN_SEP_NANOS, t1),
            FilterOutcome::Held { deliver_after: Some(_) }
        ));

        // C lands at the boundary before the timer fires: must be held (replacing B), NOT
        // delivered immediately.
        assert!(matches!(
            filter.on_alive_sample(&alive(3, 1), MIN_SEP_NANOS, t2),
            FilterOutcome::Held { deliver_after: None }
        ));

        // The timer delivers exactly the latest held sample (C), and only once.
        let delivered = filter.take_pending_on_timer(InstanceHandle::new([1; 16]), t2).unwrap();
        assert_eq!(delivered.sequence_number().to_i64(), 3);
        assert!(filter.take_pending_on_timer(InstanceHandle::new([1; 16]), t2).is_none());
    }

    // Once the window has elapsed and nothing is held, the next sample passes immediately.
    #[test]
    fn sample_after_window_passes() {
        let filter = TimeBasedFilter::new();
        let t0 = RtpsTime::now();
        let t2 = t0.add_nanos(2 * ONE_SEC);

        assert!(matches!(
            filter.on_alive_sample(&alive(1, 1), MIN_SEP_NANOS, t0),
            FilterOutcome::Deliver
        ));
        assert!(matches!(
            filter.on_alive_sample(&alive(2, 1), MIN_SEP_NANOS, t2),
            FilterOutcome::Deliver
        ));
    }

    // Each instance has its own window: the first sample of a second instance passes even
    // while the first instance is mid-window.
    #[test]
    fn windows_are_per_instance() {
        let filter = TimeBasedFilter::new();
        let t0 = RtpsTime::now();

        assert!(matches!(
            filter.on_alive_sample(&alive(1, 1), MIN_SEP_NANOS, t0),
            FilterOutcome::Deliver
        ));
        assert!(matches!(
            filter.on_alive_sample(&alive(2, 2), MIN_SEP_NANOS, t0),
            FilterOutcome::Deliver
        ));
    }

    // A held sample is dropped (not delivered) when discarded before the timer fires.
    #[test]
    fn discard_drops_held_sample() {
        let filter = TimeBasedFilter::new();
        let t0 = RtpsTime::now();
        let t1 = t0.add_nanos(ONE_SEC);

        filter.on_alive_sample(&alive(1, 1), MIN_SEP_NANOS, t0);
        assert!(matches!(
            filter.on_alive_sample(&alive(2, 1), MIN_SEP_NANOS, t1),
            FilterOutcome::Held { deliver_after: Some(_) }
        ));

        filter.discard_pending(InstanceHandle::new([1; 16]));
        assert!(filter.take_pending_on_timer(InstanceHandle::new([1; 16]), t1).is_none());
    }
}
