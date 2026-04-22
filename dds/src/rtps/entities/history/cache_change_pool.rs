//! LIFO pool for recycling `CacheChange` buffers.
//!
//! Held per-history, asymmetric by layer: writer pool lives in DCPS
//! (`DataWriterHistoryCache`), reader pool lives in RTPS (`ReaderHistoryCache`).
//! Placement follows acquire + last-Arc-drop site; data flow direction differs
//! between write and receive paths.

use std::sync::Arc;

use super::cache_change::CacheChange;

#[derive(Debug)]
pub(crate) struct CacheChangePool {
    idle_changes: Vec<CacheChange>,
}

impl CacheChangePool {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self { idle_changes: Vec::new() }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        let mut idle_changes = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            idle_changes.push(CacheChange::empty());
        }
        Self { idle_changes }
    }

    /// Acquire a CacheChange from the pool. If empty, creates a new one.
    /// The returned change's data_value retains its previous capacity.
    pub(crate) fn acquire(&mut self) -> CacheChange {
        self.idle_changes.pop().unwrap_or_else(CacheChange::empty)
    }

    /// Return a CacheChange to the pool for reuse.
    pub(crate) fn release(&mut self, change: CacheChange) {
        self.idle_changes.push(change);
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
