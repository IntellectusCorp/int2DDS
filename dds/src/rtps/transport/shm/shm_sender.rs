//! Shared Memory Sender Implementation
//!
//! Provides high-performance data transmission via shared memory
//! for user data communication between DDS participants on the same host.

#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::SocketAddr;
use std::sync::Mutex;

use crate::rtps::transport::shm::platform::{shm_segment_name, SharedMemory};
use crate::rtps::transport::shm::ring_buffer::{
    get_buffer_size, RingBufferHeader, RingBufferWriter, DEFAULT_MAX_MESSAGE_SIZE,
};
use crate::rtps::transport::{Transport, TransportType};
use log::{debug, info, warn};

/// Shared Memory Sender for user data transmission
///
/// This sender uses shared memory for zero-copy data transfer
/// between participants on the same host.
#[derive(Debug)]
pub(crate) struct ShmSender {
    domain_id: u32,
    shm: Option<SharedMemory>,
    writer: Mutex<Option<RingBufferWriter>>,
}

impl ShmSender {
    /// Create a new SHM sender for a domain
    ///
    /// # Arguments
    /// * `domain_id` - DDS domain ID
    pub(crate) fn new(domain_id: u32) -> io::Result<Self> {
        info!("[ShmSender] Creating SHM sender for domain {}", domain_id);

        let segment_name = shm_segment_name(domain_id);
        let buffer_size = get_buffer_size();
        let total_size = RingBufferHeader::SIZE + buffer_size;

        // Create or attach to shared memory segment
        let shm = match SharedMemory::new(&segment_name, total_size, true) {
            Ok(shm) => {
                info!(
                    "[ShmSender] Shared memory segment '{}' {}",
                    segment_name,
                    if shm.is_creator() { "created" } else { "attached" }
                );

                // Only initialize header if we created the segment
                if shm.is_creator() {
                    let header = shm.as_ptr() as *mut RingBufferHeader;
                    unsafe {
                        (*header).init(buffer_size as u32, DEFAULT_MAX_MESSAGE_SIZE as u32);
                    }
                    info!("[ShmSender] Ring buffer header initialized");
                }

                shm
            }
            Err(e) => {
                warn!("[ShmSender] Failed to create shared memory: {}", e);
                return Ok(Self { domain_id, shm: None, writer: Mutex::new(None) });
            }
        };

        // Create ring buffer writer
        let header = shm.as_ptr() as *mut RingBufferHeader;
        let data = unsafe { shm.as_ptr().add(RingBufferHeader::SIZE) };
        let writer = unsafe { RingBufferWriter::new(header, data) };

        Ok(Self { domain_id, shm: Some(shm), writer: Mutex::new(Some(writer)) })
    }

    /// Check if shared memory is available
    pub fn is_available(&self) -> bool {
        self.shm.is_some()
    }

    /// Force close the sender
    pub(crate) fn force_close(&self) {
        info!("[ShmSender] Force closing SHM sender");
        if let Ok(mut guard) = self.writer.lock() {
            *guard = None;
        }
    }
}

impl Transport for ShmSender {
    fn send(&self, addr: &SocketAddr, data: &[u8]) -> io::Result<usize> {
        let mut guard = self.writer.lock().map_err(|_| io::Error::other("Mutex poisoned"))?;

        let writer = guard
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "SHM not initialized"))?;

        match writer.write(data) {
            Ok(len) => {
                debug!("[ShmSender] Wrote {} bytes to SHM", len);
                Ok(len)
            }
            Err(e) => {
                warn!("[ShmSender] Write failed: {}", e);
                Err(io::Error::other(e.to_string()))
            }
        }
    }

    fn send_multicast(&self, domain_id: u32, data: &[u8]) -> io::Result<usize> {
        // SHM doesn't support multicast in the traditional sense
        // Discovery should use UDP, so this shouldn't be called for SHM
        warn!("[ShmSender] send_multicast called on SHM - not supported");
        Err(io::Error::new(io::ErrorKind::Unsupported, "SHM does not support multicast"))
    }

    fn port(&self) -> u16 {
        // SHM doesn't use ports
        0
    }

    fn transport_type(&self) -> TransportType {
        TransportType::SHM
    }

    fn close(self) {
        info!("[ShmSender] Closing SHM sender for domain {}", self.domain_id);
        // SharedMemory will be dropped automatically
    }
}
