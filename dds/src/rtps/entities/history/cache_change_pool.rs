//! LIFO pool for recycling `CacheChange` buffers.
//!
//! Held per-history, asymmetric by layer: writer pool lives in DCPS
//! (`DataWriterHistoryCache`), reader pool lives in RTPS (`ReaderHistoryCache`).
//! Placement follows acquire + last-Arc-drop site; data flow direction differs
//! between write and receive paths.

use std::sync::Arc;

use super::cache_change::CacheChange;

// Upper bound on pooled idle changes. Histories may ask for far more (KEEP_LAST with a large
// depth, or KEEP_ALL with unlimited resource limits reporting max_samples as i32::MAX), so both
// the writer and reader histories clamp their requested cap to this.
pub(crate) const MAX_POOL_CAP: usize = 1024;

#[derive(Debug)]
pub(crate) struct CacheChangePool {
    idle_changes: Vec<CacheChange>,
    cap: usize, // upper bound on retained changes;
}

impl CacheChangePool {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self { idle_changes: Vec::new(), cap: usize::MAX }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        let mut idle_changes = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            idle_changes.push(CacheChange::empty());
        }
        Self { idle_changes, cap: capacity }
    }

    // Raise the retention ceiling. Already-held idle changes are kept;
    // the new cap only bounds future releases. cap 0 retains nothing.
    pub(crate) fn set_cap(&mut self, cap: usize) {
        self.cap = cap;
    }

    /// Acquire a CacheChange from the pool. If empty, creates a new one.
    /// The returned change's data_value retains its previous capacity.
    pub(crate) fn acquire(&mut self) -> CacheChange {
        self.idle_changes.pop().unwrap_or_else(CacheChange::empty)
    }

    /// Return a CacheChange to the pool for reuse.
    /// Drops the change when the pool is already at capacity.
    pub(crate) fn release(&mut self, mut change: CacheChange) {
        // A pooled change must come back out with a writable buffer and must not
        // hold a shared resource while idle.
        change.drop_non_owned_payload();
        if self.idle_changes.len() < self.cap {
            self.idle_changes.push(change);
        }
    }

    // current pooled buffer count
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.idle_changes.len()
    }

    // Unwrap the Arc if this is the last reference, then return the inner
    // CacheChange to the pool. Any remaining clone (DCPS instance map, etc.)
    // makes try_unwrap fail, in which case we drop normally.
    pub(crate) fn try_release(&mut self, evicted: Arc<CacheChange>) {
        if let Ok(change) = Arc::try_unwrap(evicted) {
            self.release(change);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_beyond_cap_drops_excess() {
        // with_capacity(2) sets cap to 2 and pre-fills 2 changes
        let mut pool = CacheChangePool::with_capacity(2);
        assert_eq!(pool.len(), 2);

        // drain the pool, then release more than cap; len must stay <= cap
        let _a = pool.acquire();
        let _b = pool.acquire();
        assert_eq!(pool.len(), 0);

        pool.release(CacheChange::empty());
        pool.release(CacheChange::empty());
        pool.release(CacheChange::empty());
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn a_released_change_comes_back_with_a_writable_buffer() {
        let mut pool = CacheChangePool::new();
        let mut change = pool.acquire();
        change.set_shared_payload(bytes::Bytes::from_static(b"borrowed"));

        pool.release(change);
        let mut reused = pool.acquire();
        reused.data_mut().push(1);
    }
}
