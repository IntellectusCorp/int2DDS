//! Sync facade over the async tcp stack.
//!
//! Owns a dedicated runtime and bundles the inbound `TcpMuxListener`, the
//! outbound `TcpSender`, and the three crossbeam channels (discovery / user
//! data / dead peer) that bridge async tasks back to the sync DDS layer.
//!
//! The `TransportPlugin` trait is sync. Construction runs inside
//! `runtime.block_on(...)` because the listener / sender constructors call
//! `tokio::spawn`, which needs a runtime context.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use crossbeam_channel::bounded;
use log::{debug, info};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::plugin::{IncomingMessage, MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::mux_state::TcpSocketTuning;
use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
use crate::rtps::transport::tcp::tcp_sender::TcpSender;
use crate::rtps::transport::tcp::tls::TlsConfig;
use crate::rtps::transport::{TcpConfig, TransportType};

/// Crossbeam capacity for discovery + dead-peer channels.
const CHANNEL_BUFFER_SIZE: usize = 512;

/// Crossbeam capacity for the inbound user_data channel. Sized to absorb
/// short consumer stalls under bursty fragmented workloads.
const USER_CHANNEL_CAPACITY: usize = 1024;

// ── TcpTransportPlugin ──────────────────────────────────────────────────

/// Sync facade owning the runtime and forwarding trait calls to the async stack.
pub(crate) struct TcpTransportPlugin {
    #[allow(dead_code)]
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,
    listener_port: u16,

    /// Configured SPDP initial peers — the dial gate. When non-empty, only
    /// these operator-declared addresses are dialed; a peer's other advertised
    /// locators are ignored. Empty falls back to dialing every advertised
    /// locator. Expects one reachable address per peer.
    initial_peers: Vec<SocketAddr>,

    /// Public endpoint advertised in SPDP for WAN/NAT traversal (per participant).
    public_address: Option<SocketAddr>,

    /// runtime isolating tcp tasks. Dropped last (after listener
    /// and sender) so tasks can drain on shutdown.
    runtime: Arc<tokio::runtime::Runtime>,

    /// Outbound side. `Arc` because send paths and connect tasks hold clones.
    sender: Arc<TcpSender>,

    /// Inbound side. `Option` so `close()` can take and drop it, firing its
    /// cancel token.
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
        tcp_config: TcpConfig,
    ) -> io::Result<Self> {
        Self::new_with_tls(
            domain_id,
            participant_id,
            working_ip,
            working_ips,
            guid_prefix,
            None,
            tcp_config,
        )
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
        tcp_config: TcpConfig,
    ) -> io::Result<Self> {
        let physical_port =
            tcp_config.bind_port.unwrap_or_else(|| PortManager::get_tcp_physical_port(domain_id));

        // Dial gate, resolved per participant (int2dds.initial_peers property →
        // INT2DDS_INITIAL_PEERS env).
        let initial_peers = tcp_config.initial_peers.clone();

        // Pure TCP has no multicast, so discovery cannot bootstrap without
        // initial peers — fail fast with a clear message. Hybrid embeds this
        // plugin but bootstraps over UDP multicast, so its `transport_type`
        // (not `TCP`) exempts it from this requirement.
        if tcp_config.transport_type == TransportType::TCP && initial_peers.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TCP transport requires initial peers: TCP has no multicast for discovery. \
                 Set the int2dds.initial_peers QoS property (or INT2DDS_INITIAL_PEERS) to the \
                 peer's ip:port.",
            ));
        }

        // Crossbeam bridges async → sync. Listener writes to *_tx; the DDS
        // layer reads from *_rx via `take_*_source()`.
        let (discovery_tx, discovery_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);
        let (user_data_tx, user_data_rx) = bounded::<IncomingMessage>(USER_CHANNEL_CAPACITY);
        let (dead_peer_tx, dead_peer_rx) = bounded::<SocketAddr>(CHANNEL_BUFFER_SIZE);

        // Runtime worker count (per participant). Default keeps a small footprint.
        let worker_threads = tcp_config.async_workers.unwrap_or_else(default_worker_count);
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

        let idle_timeout = tcp_config.incoming_idle_timeout;
        let tuning = TcpSocketTuning {
            nodelay: tcp_config.nodelay,
            so_rcvbuf: tcp_config.so_rcvbuf,
            so_sndbuf: tcp_config.so_sndbuf,
        };

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
                tuning,
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
                &tcp_config,
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
            initial_peers,
            public_address: tcp_config.public_address,
            runtime,
            sender,
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
        if let Some(public_addr) = self.public_address {
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

    /// Whether an outbound dial to `addr` is permitted under the initial-peers
    /// policy. With initial peers configured, only those addresses are dialed
    /// (so unreachable advertised locators are never attempted); with none
    /// configured, every advertised locator is allowed as a fallback.
    fn should_dial(&self, addr: &SocketAddr) -> bool {
        self.initial_peers.is_empty() || self.initial_peers.contains(addr)
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
                if !self.should_dial(&addr) {
                    log::debug!(
                        "[TcpTransportPlugin] SEDP: skip non-initial-peer locator {}",
                        addr
                    );
                    return Ok(());
                }
                self.sender.send_to_discovery(&addr, data)
            }
            SendTarget::UserData(locator) => {
                if !locator.is_tcp() {
                    return Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()));
                }
                let addr = SocketAddr::new(
                    std::net::IpAddr::V4(locator.to_ip_v4_addr()),
                    locator.port() as u16,
                );
                if !self.should_dial(&addr) {
                    log::debug!(
                        "[TcpTransportPlugin] UserData: skip non-initial-peer locator {}",
                        addr
                    );
                    return Ok(());
                }
                self.sender.send_to_user_data(&addr, data)
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

/// Default tokio worker thread count when `TcpConfig.async_workers` is unset:
/// `min(4, available_parallelism)`.
fn default_worker_count() -> usize {
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
        // These tests exercise the listener/runtime mechanics, not discovery, so
        // satisfy the pure-TCP initial-peers requirement with a dummy peer.
        let mut cfg = TcpConfig::default();
        cfg.initial_peers = vec!["127.0.0.1:7400".parse().unwrap()];
        TcpTransportPlugin::new(
            domain,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0u8; 12],
            cfg,
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
        let target = SendTarget::UserData(&locator);
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
