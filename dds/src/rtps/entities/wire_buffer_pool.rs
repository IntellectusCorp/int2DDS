/// Pool of reusable byte buffers for RTPS message serialization (wire format).
///
/// Each Logic (UserLogic, SedpLogic, WlpLogic) acquires a buffer before
/// serializing an RTPS message, sends it over the socket, then releases
/// the buffer back to the pool so the next send reuses the same allocation.
#[derive(Debug)]
pub(crate) struct WireBufferPool {
    free_buffers: Vec<Vec<u8>>,
}

impl WireBufferPool {
    pub(crate) fn new() -> Self {
        Self { free_buffers: Vec::new() }
    }

    /// Acquire a buffer from the pool. If empty, creates a new one.
    /// The returned buffer retains its previous capacity.
    pub(crate) fn acquire(&mut self) -> Vec<u8> {
        self.free_buffers.pop().unwrap_or_default()
    }

    /// Return a buffer to the pool for reuse.
    pub(crate) fn release(&mut self, buffer: Vec<u8>) {
        self.free_buffers.push(buffer);
    }
}
