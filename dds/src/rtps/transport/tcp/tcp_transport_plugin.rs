//! Async TCP transport plugin — sync facade over the tcp stack.
//!
//! `TcpTransportPlugin` owns a dedicated `tokio::runtime::Runtime` and
//! bundles together the inbound `TcpMuxListener`, the outbound `TcpSender`,
//! and the three crossbeam channels (discovery / user data / dead peer)
//! that bridge async tasks back to the sync DDS layer.
//!
//! The `TransportPlugin` trait is sync — all methods take `&self`. Inside
//! this plugin:
//! - `send(...)` calls `sender.send_to_*()` which is sync (mpsc::try_send).
//! - `take_*_source()` returns a stored crossbeam Receiver (take-once).
//! - `close()` awaits the listener + sender shutdowns under `block_on`.
//!
//! Construction happens inside `runtime.block_on(...)` because the
//! listener / sender constructors call `tokio::spawn`, which needs a
//! runtime context. After construction returns, the plugin is fully
//! usable from sync code.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::bounded;
use dashmap::DashMap;
use log::{debug, info};

use crate::dcps::infrastructure::qos_policy::PublishModeQosPolicyKind;
use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::plugin::{IncomingMessage, MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
use crate::rtps::transport::tcp::tcp_sender::TcpSender;
use crate::rtps::transport::tcp::tls::TlsConfig;

/// Crossbeam capacity for discovery + dead-peer channels.
const CHANNEL_BUFFER_SIZE: usize = 512;

/// Crossbeam capacity for the inbound user_data channel that bridges the
/// listener-side dispatch into the sync DDS layer. Sized to absorb short
/// consumer stalls under bursty 1MB/60Hz × ~16 fragment workloads.
const USER_CHANNEL_CAPACITY: usize = 1024;

/// Default inbound idle timeout. Overridable via the existing `INT2DDS_TCP_*`
/// env vars (when those helpers exist).
const DEFAULT_INCOMING_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

// ── TcpTransportPlugin ──────────────────────────────────────────────────

/// Sync facade over the tcp stack. Owns the runtime and forwards
/// `TransportPlugin` trait calls into the async machinery.
pub(crate) struct TcpTransportPlugin {
    #[allow(dead_code)]
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,
    listener_port: u16,

    /// Dedicated runtime — keeps the tcp tasks isolated from any
    /// runtime the host application might run. Dropped last (after the
    /// listener and sender) so tasks can drain on shutdown.
    runtime: Arc<tokio::runtime::Runtime>,

    /// Outbound side. `Arc<TcpSender>` because send paths and connect tasks
    /// hold their own clones. The same sender services both publish modes:
    /// asynchronous writes flow through the per-connection inbox (queued +
    /// coalesced by `writer_task`), while synchronous writes acquire the
    /// connection's shared write half and perform the wire `writev` inline
    /// on the user thread.
    sender: Arc<TcpSender>,

    /// Maps each registered DataWriter's `EntityId` to the publish mode
    /// captured at writer creation. `send()` consults this map to decide
    /// whether to invoke the async or sync send method on `sender`.
    /// Unregistered writers fall back to the `PublishModeQosPolicy`
    /// default (`Synchronous`).
    writer_modes: DashMap<EntityId, PublishModeQosPolicyKind>,

    /// Inbound side. Wrapped in `Option` so `close()` can take and drop it,
    /// firing its cancel token.
    mux_listener: Mutex<Option<TcpMuxListener>>,

    /// Take-once receivers handed out via `take_*_source()`.
    discovery_rx: Mutex<Option<crossbeam_channel::Receiver<IncomingMessage>>>,
    user_data_rx: Mutex<Option<crossbeam_channel::Receiver<IncomingMessage>>>,
    dead_peer_rx: Mutex<Option<crossbeam_channel::Receiver<SocketAddr>>>,
}

impl TcpTransportPlugin {
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        working_ips: Vec<String>,
        guid_prefix: GuidPrefix,
    ) -> io::Result<Self> {
        Self::new_with_tls(domain_id, participant_id, working_ip, working_ips, guid_prefix, None)
    }

    /// Build the plugin. Internally creates a multi-thread runtime, then
    /// runs `block_on` to spawn the listener + sender within a runtime
    /// context (required by `tokio::spawn`).
    pub(crate) fn new_with_tls(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        working_ips: Vec<String>,
        guid_prefix: GuidPrefix,
        tls_config: Option<Arc<TlsConfig>>,
    ) -> io::Result<Self> {
        let physical_port = crate::common::env::get_tcp_port()
            .unwrap_or_else(|| PortManager::get_tcp_physical_port(domain_id));

        // Crossbeam bridges async → sync. Listener writes to *_tx; the DDS
        // layer reads from *_rx via `take_*_source()`.
        let (discovery_tx, discovery_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);
        let (user_data_tx, user_data_rx) = bounded::<IncomingMessage>(USER_CHANNEL_CAPACITY);
        let (dead_peer_tx, dead_peer_rx) = bounded::<SocketAddr>(CHANNEL_BUFFER_SIZE);

        // Runtime — worker count overridable via env for ops tuning. Default
        // keeps a small footprint suitable for most participant workloads.
        let worker_threads = worker_thread_count();
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(worker_threads)
                .thread_name("tcp_worker")
                .enable_all()
                .build()
                .map_err(|e| {
                    transport_io_error(
                        TransportErrorCode::TcpBindFailed,
                        format!("Failed to build tokio runtime: {}", e),
                    )
                })?,
        );

        let idle_timeout = DEFAULT_INCOMING_IDLE_TIMEOUT;

        // Build listener + sender inside a runtime context — both
        // constructors call `tokio::spawn`, which needs `Handle::current()`.
        let (mux_listener, sender) = runtime.block_on(async {
            let listener = TcpMuxListener::bind_and_spawn(
                physical_port,
                domain_id,
                participant_id,
                guid_prefix,
                discovery_tx,
                user_data_tx,
                tls_config.clone(),
                idle_timeout,
            )
            .map_err(|e| {
                log::error!(
                    "[TcpTransportPlugin] Failed to bind TCP listener on port {} \
                     (domain={}): {}. Another participant may already be using this port \
                     on the same host.",
                    physical_port,
                    domain_id,
                    e
                );
                transport_io_error(
                    TransportErrorCode::TcpBindFailed,
                    format!(
                        "Failed to bind TCP listener on port {} (domain={}): {}",
                        physical_port, domain_id, e
                    ),
                )
            })?;

            let listener_port = listener.port();

            // Share the listener's MuxState with the sender so outbound
            // connections register into the same per-connection map and
            // dispatch routes responses back into the same pending_ack slots.
            let shared = Arc::clone(listener.shared());

            let sender = TcpSender::new(
                domain_id,
                participant_id,
                working_ip,
                listener_port,
                guid_prefix,
                tls_config,
                shared,
            );

            sender.set_dead_peer_tx(dead_peer_tx);

            Ok::<_, io::Error>((listener, sender))
        })?;

        let listener_port = mux_listener.port();

        info!(
            "[TcpTransportPlugin] Created (domain={}, pid={}, port={}, workers={})",
            domain_id, participant_id, listener_port, worker_threads
        );

        Ok(Self {
            domain_id,
            participant_id,
            working_ips,
            listener_port,
            runtime,
            sender,
            writer_modes: DashMap::new(),
            mux_listener: Mutex::new(Some(mux_listener)),
            discovery_rx: Mutex::new(Some(discovery_rx)),
            user_data_rx: Mutex::new(Some(user_data_rx)),
            dead_peer_rx: Mutex::new(Some(dead_peer_rx)),
        })
    }

    /// Build the TCP locators this plugin advertises in SPDP. Mirrors the
    /// sync plugin's `advertised_tcp_locators`: WAN public address overrides
    /// the per-NIC list when set.
    fn advertised_tcp_locators(&self) -> Vec<Locator> {
        // 1. Explicit WAN/NAT public endpoint takes precedence.
        if let Some(public_addr) = crate::common::env::get_tcp_public_addr() {
            if let std::net::IpAddr::V4(v4) = public_addr.ip() {
                log::info!(
                    "[TcpTransportPlugin] WAN mode: advertising public address \
                     {} instead of :{}",
                    public_addr,
                    self.listener_port
                );
                return vec![Locator::from_tcp_v4(v4, public_addr.port() as u32)];
            }
            log::warn!(
                "[TcpTransportPlugin] Public address is not IPv4, \
                 falling back to LAN NICs"
            );
        }

        // 2. Generic IP override (carried over from develop's `init_locators`).
        if let Some(ext_ip) = crate::common::env::get_external_address() {
            return vec![Locator::from_tcp_v4(ext_ip, self.listener_port as u32)];
        }

        // 3. All local NICs.
        self.working_ips
            .iter()
            .filter_map(|ip_str| ip_str.parse::<Ipv4Addr>().ok())
            .map(|ip| Locator::from_tcp_v4(ip, self.listener_port as u32))
            .collect()
    }
}

impl TransportPlugin for TcpTransportPlugin {
    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        match target {
            SendTarget::SPDPDiscovery { initial_peers } => {
                // SPDP fan-out — best-effort; per-peer failures must not
                // abort the broadcast.
                for peer_addr in *initial_peers {
                    let _ = self.sender.send_to_discovery(peer_addr, data);
                }
                Ok(())
            }
            SendTarget::SEDPDiscovery(locator) => {
                if !locator.is_tcp() {
                    return Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()));
                }
                let addr = SocketAddr::new(
                    std::net::IpAddr::V4(locator.to_ip_v4_addr()),
                    locator.port() as u16,
                );
                self.sender.send_to_discovery(&addr, data)
            }
            SendTarget::UserData { locator, writer_guid } => {
                if !locator.is_tcp() {
                    return Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()));
                }
                let addr = SocketAddr::new(
                    std::net::IpAddr::V4(locator.to_ip_v4_addr()),
                    locator.port() as u16,
                );
                // Per-writer dispatch on PublishMode. Unknown writers (no
                // local DataWriter context, e.g. reader-side NACK_FRAG, or
                // a writer that has not yet been registered) fall back to
                // the PublishModeQosPolicy default (`Synchronous`).
                let mode = writer_guid
                    .and_then(|g| self.writer_modes.get(&g.entity_id()).map(|m| *m))
                    .unwrap_or_default();
                match mode {
                    PublishModeQosPolicyKind::Synchronous => {
                        self.sender.send_to_user_data_sync(&addr, data)
                    }
                    PublishModeQosPolicyKind::Asynchronous => {
                        self.sender.send_to_user_data(&addr, data)
                    }
                }
            }
        }
    }

    fn can_handle(&self, locator: &Locator) -> bool {
        locator.is_tcp()
    }

    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
        self.advertised_tcp_locators()
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        // TCP multiplexes discovery and user-data over the same listener;
        // metatraffic and default locators resolve to identical endpoints.
        self.advertised_tcp_locators()
    }

    fn take_discovery_multicast_source(&self) -> Option<MessageSource> {
        // TCP has no multicast — discovery uses unicast fan-out via SPDP.
        None
    }

    fn take_discovery_unicast_source(&self) -> Option<MessageSource> {
        let rx = self.discovery_rx.lock().expect("discovery_rx lock").take()?;
        Some(MessageSource::Channel { rx })
    }

    fn take_user_data_unicast_source(&self) -> Option<MessageSource> {
        let rx = self.user_data_rx.lock().expect("user_data_rx lock").take()?;
        Some(MessageSource::Channel { rx })
    }

    fn take_dead_peer_receiver(&self) -> Option<crossbeam_channel::Receiver<SocketAddr>> {
        self.dead_peer_rx.lock().expect("dead_peer_rx lock").take()
    }

    fn port(&self) -> u16 {
        self.listener_port
    }

    fn tcp_listener_port(&self) -> Option<u16> {
        Some(self.listener_port)
    }

    fn participant_id(&self) -> u32 {
        self.participant_id
    }

    fn register_writer(&self, writer_guid: &Guid, kind: PublishModeQosPolicyKind) {
        self.writer_modes.insert(writer_guid.entity_id(), kind);
        debug!("[TcpTransportPlugin] register_writer guid={:?} kind={:?}", writer_guid, kind);
    }

    fn unregister_writer(&self, writer_guid: &Guid) {
        if self.writer_modes.remove(&writer_guid.entity_id()).is_some() {
            debug!("[TcpTransportPlugin] unregister_writer guid={:?}", writer_guid);
        }
    }

    fn close(&self) {
        // 1. Take the listener out and await its tasks under block_on.
        //    NOTE: block_on panics if called from inside a tokio runtime
        //    context. The TransportPlugin contract is that close() runs
        //    from the sync DDS shutdown path, never from inside our runtime.
        if let Ok(mut guard) = self.mux_listener.lock() {
            if let Some(listener) = guard.take() {
                self.runtime.block_on(listener.shutdown());
            }
        }

        // 2. Tear down the sender's lifecycle tasks (keepalive, orphan prune).
        self.runtime.block_on(self.sender.shutdown());

        debug!("[TcpTransportPlugin] Closed");
    }
}

impl Drop for TcpTransportPlugin {
    fn drop(&mut self) {
        // Best-effort fallback when close() was not called explicitly.
        // We cannot `block_on` inside Drop safely (it panics if Drop runs
        // inside the runtime). Just fire cancellation: TcpMuxListener::Drop
        // and TcpSender::Drop both cancel their tokens, and the runtime's
        // own Drop drains or aborts the remaining tasks.
        if let Ok(mut guard) = self.mux_listener.lock() {
            if let Some(listener) = guard.take() {
                drop(listener);
            }
        }
        // sender's cancel fires via its Drop when the last Arc is dropped.
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Decide how many worker threads to spawn on the runtime.
/// Default: `min(4, available_parallelism)`.
/// Override via `INT2DDS_TCP_ASYNC_WORKERS`.
fn worker_thread_count() -> usize {
    if let Ok(s) = std::env::var("INT2DDS_TCP_ASYNC_WORKERS") {
        if let Ok(n) = s.parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    cpus.min(4).max(1)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Keep domain IDs unique across tests to avoid port collisions when
    /// the test suite runs in parallel.
    fn next_test_domain() -> u32 {
        static NEXT: AtomicU32 = AtomicU32::new(900);
        NEXT.fetch_add(1, Ordering::SeqCst)
    }

    fn make_plugin(domain: u32) -> TcpTransportPlugin {
        TcpTransportPlugin::new(
            domain,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0u8; 12],
        )
        .expect("plugin creation")
    }

    /// Plugin construction succeeds and the OS accepts TCP connections on
    /// the reported listener port — proves the accept task is actually
    /// running inside the runtime.
    #[test]
    fn plugin_creates_and_listens() {
        let plugin = make_plugin(next_test_domain());
        let port = plugin.tcp_listener_port().expect("listener port");
        assert!(port != 0);

        let _stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .expect("connect to listener");

        plugin.close();
    }

    /// Each `take_*` returns `Some` exactly once.
    #[test]
    fn take_sources_are_one_shot() {
        let plugin = make_plugin(next_test_domain());

        assert!(plugin.take_discovery_unicast_source().is_some());
        assert!(plugin.take_discovery_unicast_source().is_none());

        assert!(plugin.take_user_data_unicast_source().is_some());
        assert!(plugin.take_user_data_unicast_source().is_none());

        assert!(plugin.take_dead_peer_receiver().is_some());
        assert!(plugin.take_dead_peer_receiver().is_none());

        plugin.close();
    }

    /// TCP transport has no multicast — multicast source is always `None`.
    #[test]
    fn multicast_discovery_source_is_none() {
        let plugin = make_plugin(next_test_domain());
        assert!(plugin.take_discovery_multicast_source().is_none());
        plugin.close();
    }

    /// `close()` is idempotent and does not hang on the second call.
    #[test]
    fn close_is_idempotent() {
        let plugin = make_plugin(next_test_domain());
        plugin.close();
        plugin.close();
    }

    /// Sending to a TCP locator does not panic / block the caller.
    /// The connect_task internally either succeeds or notifies dead_peer;
    /// the sync `send` call itself is fire-and-forget.
    #[test]
    fn send_to_unreachable_does_not_panic() {
        let plugin = make_plugin(next_test_domain());

        let locator = Locator::from_tcp_v4(std::net::Ipv4Addr::new(127, 0, 0, 1), 1);
        let target = SendTarget::UserData { locator: &locator, writer_guid: None };
        let _ = plugin.send(b"\x52\x54\x50\x53", &target);

        plugin.close();
    }

    /// Advertised locators include the bound listener port for each working IP.
    #[test]
    fn advertised_locators_carry_listener_port() {
        let plugin = make_plugin(next_test_domain());
        let port = plugin.tcp_listener_port().expect("listener port");

        let locators = plugin.advertised_metatraffic_unicast_locators();
        assert!(!locators.is_empty(), "expected at least one advertised locator");
        for loc in &locators {
            assert!(loc.is_tcp());
            assert_eq!(loc.port() as u16, port);
        }

        plugin.close();
    }
}
