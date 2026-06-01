use super::cache_change::CacheChange;

/// LIFO pool for reusing CacheChange objects (and their internal Vec<u8> capacity).
#[derive(Debug)]
pub(crate) struct CacheChangePool {
    free_changes: Vec<CacheChange>,
}

impl CacheChangePool {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self { free_changes: Vec::new() }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        let mut free_changes = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            free_changes.push(CacheChange::empty());
        }
        Self { free_changes }
    }

    /// Acquire a CacheChange from the pool. If empty, creates a new one.
    /// The returned change's data_value retains its previous capacity.
    pub(crate) fn acquire(&mut self) -> CacheChange {
        self.free_changes.pop().unwrap_or_else(CacheChange::empty)
    }

    /// Return a CacheChange to the pool for reuse.P
    pub(crate) fn release(&mut self, change: CacheChange) {
        self.free_changes.push(change);
    }

    // debug: current pooled buffer count
    pub(crate) fn len(&self) -> usize {
        self.free_changes.len()
    }
}
