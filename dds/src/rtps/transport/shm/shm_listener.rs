//! Shared Memory Listener Implementation
//!
//! Provides high-performance data reception via shared memory
//! for user data communication between DDS participants on the same host.

#![allow(dead_code)]
#![allow(unused_variables)]

use bytes::Bytes;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::rtps::transport::shm::platform::{shm_segment_name, SharedMemory};
use crate::rtps::transport::shm::ring_buffer::{
    RingBufferHeader, RingBufferReader, DEFAULT_BUFFER_SIZE,
};
use crate::rtps::transport::Listener;
use log::{debug, info, warn};

/// Maximum message size for SHM transport
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Shared Memory Listener for user data reception
///
/// This listener receives data via shared memory for zero-copy
/// data transfer between participants on the same host.
#[derive(Debug)]
pub(crate) struct ShmListener {
    domain_id: u32,
    shm: Option<SharedMemory>,
    reader: Option<RingBufferReader>,
    recv_buffer: Box<[u8; MAX_MESSAGE_SIZE]>,
}

impl ShmListener {
    /// Create a new SHM listener for a domain
    ///
    /// # Arguments
    /// * `domain_id` - DDS domain ID
    pub(crate) fn new(domain_id: u32) -> std::io::Result<Self> {
        info!("[ShmListener] Creating SHM listener for domain {}", domain_id);

        let segment_name = shm_segment_name(domain_id);
        let total_size = RingBufferHeader::SIZE + DEFAULT_BUFFER_SIZE;

        // Try to attach to existing shared memory segment
        let shm = match SharedMemory::new(&segment_name, total_size, true) {
            Ok(shm) => {
                info!(
                    "[ShmListener] Shared memory segment '{}' {}",
                    segment_name,
                    if shm.is_creator() { "created" } else { "attached" }
                );

                shm
            }
            Err(e) => {
                warn!("[ShmListener] Failed to attach to shared memory: {}", e);
                return Ok(Self {
                    domain_id,
                    shm: None,
                    reader: None,
                    recv_buffer: Box::new([0; MAX_MESSAGE_SIZE]),
                });
            }
        };

        // Create ring buffer reader
        // Each reader maintains its own local read position, initialized to current write_pos
        // This allows multiple readers without interfering with each other
        let header = shm.as_ptr() as *const RingBufferHeader;
        let data = unsafe { shm.as_ptr().add(RingBufferHeader::SIZE) as *const u8 };
        let reader = unsafe { RingBufferReader::new(header, data) };
        info!("[ShmListener] Ring buffer reader created with local read position");

        Ok(Self {
            domain_id,
            shm: Some(shm),
            reader: Some(reader),
            recv_buffer: Box::new([0; MAX_MESSAGE_SIZE]),
        })
    }

    /// Check if shared memory is available
    pub fn is_available(&self) -> bool {
        self.shm.is_some()
    }

    /// Get a message from shared memory
    ///
    /// # Returns
    /// * `Some((data, sender_addr))` - Message data and pseudo sender address
    /// * `None` - No message available
    pub(crate) fn get_message(&mut self) -> Option<(Bytes, SocketAddr)> {
        let reader = self.reader.as_mut()?;

        match reader.read(&mut self.recv_buffer[..]) {
            Ok(Some((len, lost))) => {
                if lost > 0 {
                    warn!("[ShmListener] Lost {} messages (reader too slow)", lost);
                }
                debug!("[ShmListener] Read {} bytes from SHM", len);
                // Use a pseudo address to indicate SHM source
                // Port 0 indicates SHM transport
                let pseudo_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
                Some((Bytes::copy_from_slice(&self.recv_buffer[..len]), pseudo_addr))
            }
            Ok(None) => {
                // No data available
                None
            }
            Err(e) => {
                debug!("[ShmListener] Read failed: {}", e);
                None
            }
        }
    }

    /// Check if there are messages available to read
    pub fn has_data(&self) -> bool {
        self.reader.as_ref().map(|r| r.has_data()).unwrap_or(false)
    }

    /// Close the listener and release resources
    pub(crate) fn close(&mut self) {
        info!("[ShmListener] Closing SHM listener for domain {}", self.domain_id);
        self.reader = None;
        // SharedMemory will be dropped when self.shm is dropped
    }
}

impl Drop for ShmListener {
    fn drop(&mut self) {
        self.close();
    }
}

impl Listener for ShmListener {
    fn socket_udp(&mut self) -> Option<&mut mio::net::UdpSocket> {
        // SHM listener doesn't have a UDP socket
        None
    }

    fn socket_tcp(&mut self) -> Option<&mut mio::net::TcpListener> {
        // SHM listener doesn't have a TCP socket
        None
    }

    fn port(&self) -> u16 {
        // SHM doesn't use ports
        0
    }

    fn close(&mut self) {
        ShmListener::close(self)
    }
}
