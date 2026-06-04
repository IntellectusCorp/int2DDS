use super::cache_change::CacheChange;

/// LIFO pool for reusing CacheChange objects (and their internal Vec<u8> capacity).
#[derive(Debug)]
pub(crate) struct CacheChangePool {
    free_changes: Vec<CacheChange>,
    cap: usize, // upper bound on retained changes;
}

impl CacheChangePool {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self { free_changes: Vec::new(), cap: usize::MAX }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        let mut free_changes = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            free_changes.push(CacheChange::empty());
        }
        Self { free_changes, cap: capacity }
    }

    /// Acquire a CacheChange from the pool. If empty, creates a new one.
    /// The returned change's data_value retains its previous capacity.
    pub(crate) fn acquire(&mut self) -> CacheChange {
        self.free_changes.pop().unwrap_or_else(CacheChange::empty)
    }

    /// Return a CacheChange to the pool for reuse.
    /// Drops the change when the pool is already at capacity.
    pub(crate) fn release(&mut self, change: CacheChange) {
        if self.free_changes.len() < self.cap {
            self.free_changes.push(change);
        }
    }

    // current pooled buffer count
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.free_changes.len()
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
}
