//! Synchronous TCP send path — **stub**, real implementation pending.
//!
//! `SyncTcpSender` is the counterpart of [`super::tcp_sender::TcpSender`]
//! for DataWriters whose [`PublishModeQosPolicy`] selects `Synchronous`.
//! The intent is to perform the wire `writev` inline on the calling user
//! thread, against a blocking [`std::net::TcpStream`], so the only thread
//! handoff between `write()` and the kernel is the syscall itself.
//!
//! This file currently provides:
//! - the struct skeleton with the identity fields the eventual full impl
//!   will need (matching [`super::tcp_sender::TcpSender::new`] for an
//!   easy drop-in once both senders coexist in the plugin),
//! - a [`TcpSenderImpl`] impl whose send/disconnect methods short-circuit
//!   with `io::ErrorKind::Unsupported`. This keeps the plugin compiling
//!   end-to-end and surfaces a clear runtime error for any DataWriter
//!   that ends up routed here before the full implementation lands.
//!
//! The full implementation will, on first send to a given peer, perform a
//! blocking outbound handshake (TCP connect → PEER_HELLO → PORT_RESERVE →
//! PORT_BIND), cache the resulting blocking `TcpStream` keyed by
//! `(SocketAddr, logical_port)`, and serialise concurrent user-thread
//! writes on that stream via a per-connection `Mutex`. The receive side
//! stays on the async listener — there is no sync read helper.
//!
//! [`PublishModeQosPolicy`]: crate::infrastructure::qos_policy::PublishModeQosPolicy

#![allow(dead_code)]

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::tcp::mux_state::MuxState;
use crate::rtps::transport::tcp::sender::TcpSenderImpl;
use crate::rtps::transport::tcp::tls::TlsConfig;

/// Synchronous outbound TCP sender. **Stub** — see the module-level docs.
///
/// Construction matches [`super::tcp_sender::TcpSender::new`] so the
/// transport plugin can build a `SyncTcpSender` alongside the async one
/// without parameter-shape divergence.
pub(crate) struct SyncTcpSender {
    #[allow(dead_code)]
    domain_id: u32,
    #[allow(dead_code)]
    participant_id: u32,
    #[allow(dead_code)]
    working_ip: String,
    #[allow(dead_code)]
    listener_port: u16,
    #[allow(dead_code)]
    local_guid_prefix: GuidPrefix,
    #[allow(dead_code)]
    tls_config: Option<Arc<TlsConfig>>,
    /// Shared with the async listener. The full implementation will use
    /// this to register inbound responses for outbound handshake round
    /// trips (PORT_RESERVE ack) the same way the async sender does.
    #[allow(dead_code)]
    shared: Arc<MuxState>,
    // TODO(phase-1-G): outbound connection cache.
    //   connections: DashMap<(SocketAddr, u16), Arc<std::sync::Mutex<std::net::TcpStream>>>,
    //   plus per-key setup serialisation so two parallel writers don't
    //   double-handshake to the same peer/logical_port.
}

impl SyncTcpSender {
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        listener_port: u16,
        local_guid_prefix: GuidPrefix,
        tls_config: Option<Arc<TlsConfig>>,
        shared: Arc<MuxState>,
    ) -> Arc<Self> {
        log::info!("[SyncTcpSender] stub constructed (no actual send path yet)");
        Arc::new(Self {
            domain_id,
            participant_id,
            working_ip,
            listener_port,
            local_guid_prefix,
            tls_config,
            shared,
        })
    }
}

impl TcpSenderImpl for SyncTcpSender {
    fn send_to_discovery(self: &Arc<Self>, addr: &SocketAddr, data: &[u8]) -> io::Result<()> {
        log::warn!(
            "[SyncTcpSender] send_to_discovery({:?}, {} bytes) not yet implemented — \
             returning Unsupported. PublishMode=Synchronous is currently a stub.",
            addr,
            data.len()
        );
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "SyncTcpSender::send_to_discovery is not yet implemented",
        ))
    }

    fn send_to_user_data(self: &Arc<Self>, addr: &SocketAddr, data: &[u8]) -> io::Result<()> {
        log::warn!(
            "[SyncTcpSender] send_to_user_data({:?}, {} bytes) not yet implemented — \
             returning Unsupported. PublishMode=Synchronous is currently a stub.",
            addr,
            data.len()
        );
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "SyncTcpSender::send_to_user_data is not yet implemented",
        ))
    }

    fn disconnect_peer(&self, _addr: SocketAddr) {
        // No-op until there is a connection cache to evict.
    }
}
