//! UDP socket buffer sizing shared by the listener and sender sockets.
//!
//! `INT2DDS_UDP_SOCKET_BUFFER` sets both directions. Without it, the OS default is kept unless
//! below a floor, which is then requested: 1 MiB receive, 64 KiB send (still capped by the OS).

use socket2::Socket;

use crate::common::env::get_udp_socket_buffer_size_override;

pub(crate) const DEFAULT_MIN_RECV_BUFFER_BYTES: usize = 1024 * 1024;
pub(crate) const DEFAULT_MIN_SEND_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BufferRequest {
    /// Configured by the user: always requested.
    Exact(usize),
    /// Default policy: requested only when the OS default is smaller.
    AtLeast(usize),
}

impl BufferRequest {
    fn from_override(size: Option<usize>, floor: usize) -> Self {
        size.map_or(Self::AtLeast(floor), Self::Exact)
    }

    /// Size to pass to setsockopt given what the socket reports now, or `None` to leave it.
    pub(crate) fn size_to_request(self, current: usize) -> Option<usize> {
        match self {
            Self::Exact(size) => Some(size),
            Self::AtLeast(floor) => (current < floor).then_some(floor),
        }
    }
}

pub(crate) fn recv_buffer_request() -> BufferRequest {
    BufferRequest::from_override(
        get_udp_socket_buffer_size_override(),
        DEFAULT_MIN_RECV_BUFFER_BYTES,
    )
}

pub(crate) fn send_buffer_request() -> BufferRequest {
    BufferRequest::from_override(
        get_udp_socket_buffer_size_override(),
        DEFAULT_MIN_SEND_BUFFER_BYTES,
    )
}

/// Apply the receive policy and return what the kernel granted, read back after the set.
/// A refused set is not an error: the OS may cap the size silently anyway.
pub(crate) fn configure_recv_buffer(socket: &Socket) -> Option<usize> {
    let current = socket.recv_buffer_size().unwrap_or(0);
    if let Some(size) = recv_buffer_request().size_to_request(current) {
        let _ = socket.set_recv_buffer_size(size);
    }
    socket.recv_buffer_size().ok()
}

/// Apply the send policy and return what the kernel granted, read back after the set.
/// A refused set is not an error: the OS may cap the size silently anyway.
pub(crate) fn configure_send_buffer(socket: &Socket) -> Option<usize> {
    let current = socket.send_buffer_size().unwrap_or(0);
    if let Some(size) = send_buffer_request().size_to_request(current) {
        let _ = socket.set_send_buffer_size(size);
    }
    socket.send_buffer_size().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_only_raises_a_smaller_os_default() {
        let floor = BufferRequest::AtLeast(DEFAULT_MIN_RECV_BUFFER_BYTES);
        assert_eq!(floor.size_to_request(212_992), Some(DEFAULT_MIN_RECV_BUFFER_BYTES));
        assert_eq!(floor.size_to_request(8 * 1024 * 1024), None, "a larger default is kept");
        assert_eq!(floor.size_to_request(DEFAULT_MIN_RECV_BUFFER_BYTES), None);
    }

    #[test]
    fn a_configured_size_is_always_requested() {
        let exact = BufferRequest::Exact(4096);
        assert_eq!(exact.size_to_request(8 * 1024 * 1024), Some(4096), "even below the default");
        assert_eq!(exact.size_to_request(0), Some(4096));
    }

    #[test]
    fn an_unset_override_falls_back_to_the_floor() {
        assert_eq!(BufferRequest::from_override(None, 64), BufferRequest::AtLeast(64));
        assert_eq!(BufferRequest::from_override(Some(8), 64), BufferRequest::Exact(8));
    }
}
