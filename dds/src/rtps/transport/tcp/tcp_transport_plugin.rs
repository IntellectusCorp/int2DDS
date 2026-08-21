//! Sync facade over the async tcp stack.
//!
//! Owns a dedicated runtime and bundles the inbound `TcpMuxListener`, the
//! outbound `TcpSender`, and the three channels (discovery / user data / dead
//! peer) that bridge async tasks back to the sync DDS layer.
//!
//! The `TransportPlugin` trait is sync. Construction runs inside
//! `runtime.block_on(...)` because the listener / sender constructors call
//! `tokio::spawn`, which needs a runtime context.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use flume::bounded;
use log::{debug, info};
use tokio_util::sync::CancellationToken;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::plugin::{IncomingMessage, MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::connection_registry::{KeepaliveParams, TcpSocketTuning};
use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
use crate::rtps::transport::tcp::tcp_sender::TcpSender;
use crate::rtps::transport::tcp::tls::TlsConfig;
use crate::rtps::transport::{TcpConfig, TransportType};

/// Capacity of the inbound channel. Single-slot so a full channel
/// blocks the router at once, pushing backpressure onto the TCP window.
const TO_RTPS_CHANNEL_CAPACITY: usize = 1;

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

    /// Dial peers discovered at runtime that are not in `initial_peers`
    accept_undefined_peers: bool,

    /// Root of the whole plugin's cancellation tree. The listener and the
    /// sender each own a child of it, so one cancel reaches both sides
    /// regardless of which half is still reachable.
    cancel: CancellationToken,

    /// Outbound side. `Arc` because send paths and connect tasks hold clones.
    sender: Arc<TcpSender>,

    /// Inbound side. `Option` so `close()` can take and drop it, firing its
    /// cancel token.
    mux_listener: Mutex<Option<TcpMuxListener>>,

    /// Take-once receivers handed out via `take_*_source()`.
    discovery_rx: Mutex<Option<flume::Receiver<IncomingMessage>>>,
    user_data_rx: Mutex<Option<flume::Receiver<IncomingMessage>>>,

    /// runtime isolating tcp tasks. Dropped last (after listener
    /// and sender) so tasks can drain on shutdown.
    runtime: Arc<tokio::runtime::Runtime>,
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

        let initial_peers = tcp_config.initial_peers.clone();

        if tcp_config.transport_type == TransportType::TCP && initial_peers.is_empty() {
            log::warn!(
                "No initial peers configured: TCP has no multicast for discovery. \
                 Set the int2dds.initial_peers QoS property (or INT2DDS_INITIAL_PEERS) to the \
                 peer's ip:port. Ignore this if only endpoints \
                 within this participant are meant to match."
            );
        }

        // Bridge async → sync.
        let (discovery_tx, discovery_rx) = bounded::<IncomingMessage>(TO_RTPS_CHANNEL_CAPACITY);
        let (user_data_tx, user_data_rx) = bounded::<IncomingMessage>(TO_RTPS_CHANNEL_CAPACITY);

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

        let tuning = TcpSocketTuning {
            nodelay: tcp_config.nodelay,
            so_rcvbuf: tcp_config.so_rcvbuf,
            so_sndbuf: tcp_config.so_sndbuf,
            unacked_timeout: tcp_config.unacked_timeout,
            keepalive: Some(KeepaliveParams {
                time: tcp_config.keepalive_interval,
                interval: tcp_config.keepalive_timeout,
                retries: tcp_config.keepalive_max_misses,
            }),
        };

        let cancel = CancellationToken::new();

        // Build listener + sender inside a runtime context
        let listener_cancel = cancel.child_token();
        let sender_cancel = cancel.child_token();
        let (mux_listener, sender) = runtime.block_on(async {
            let listener = TcpMuxListener::bind_and_spawn(
                physical_port,
                domain_id,
                participant_id,
                guid_prefix,
                discovery_tx,
                user_data_tx,
                tls_config.clone(),
                tuning,
                tcp_config.tls_handshake_timeout,
                tcp_config.peer_handshake_timeout,
                listener_cancel,
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

            // Share the listener's ConnectionRegistry with the sender so outbound
            // connections register into the same per-connection map and
            // dispatch routes responses back into the same pending_ack slots.
            let shared = Arc::clone(listener.shared());

            // Every address this participant answers on, so the sender can tell
            // a frame aimed at ourselves from one aimed at a peer.
            let mut local_ips = working_ips.clone();
            if !local_ips.contains(&working_ip) {
                local_ips.push(working_ip);
            }

            let sender = TcpSender::new(
                domain_id,
                participant_id,
                local_ips,
                listener_port,
                guid_prefix,
                tls_config,
                shared,
                &tcp_config,
                sender_cancel,
            );

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
            accept_undefined_peers: tcp_config.accept_undefined_peers,
            cancel,
            sender,
            mux_listener: Mutex::new(Some(mux_listener)),
            discovery_rx: Mutex::new(Some(discovery_rx)),
            user_data_rx: Mutex::new(Some(user_data_rx)),
            runtime,
        })
    }

    /// Build the TCP locators this plugin advertises in SPDP. Mirrors the
    /// sync plugin's `advertised_tcp_locators`: WAN public address overrides
    /// the per-NIC list when set.
    /// Build advertised TCP locators carrying `logical_port` in the RTPS port
    /// field and the physical listener port in the address bytes, so a peer
    /// reserves the right logical port regardless of its own participant id.
    fn advertised_tcp_locators(&self, logical_port: u16) -> Vec<Locator> {
        // 1. Explicit WAN/NAT public endpoint takes precedence.
        if let Some(public_addr) = self.public_address {
            if let std::net::IpAddr::V4(v4) = public_addr.ip() {
                log::info!(
                    "[TcpTransportPlugin] WAN mode: advertising public address \
                     {} instead of :{}",
                    public_addr,
                    self.listener_port
                );
                return vec![Locator::from_tcp_v4_dual(v4, logical_port, public_addr.port())];
            }
            log::warn!(
                "[TcpTransportPlugin] Public address is not IPv4, \
                 falling back to LAN NICs"
            );
        }

        // 2. Generic IP override (carried over from develop's `init_locators`).
        if let Some(ext_ip) = crate::common::env::get_external_address() {
            return vec![Locator::from_tcp_v4_dual(ext_ip, logical_port, self.listener_port)];
        }

        // 3. All local NICs.
        self.working_ips
            .iter()
            .filter_map(|ip_str| ip_str.parse::<Ipv4Addr>().ok())
            .map(|ip| Locator::from_tcp_v4_dual(ip, logical_port, self.listener_port))
            .collect()
    }

    /// Whether an outbound dial to `addr` is permitted. Restricted to
    /// `initial_peers` unless `accept_undefined_peers` is set
    /// or `initial_peers` is empty (dial-all fallback).
    ///
    /// Our own listener is exempt: nobody lists themselves as an initial peer,
    /// and what a Reader and a Writer of this participant exchange is not a dial
    /// — the sender keeps it in-process.
    fn should_dial(&self, addr: &SocketAddr) -> bool {
        self.sender.is_self_connection(addr)
            || self.accept_undefined_peers
            || self.initial_peers.is_empty()
            || self.initial_peers.contains(addr)
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
            // SEDP and user data both dial the peer's physical port and reserve
            // the logical port the peer advertised in its own locator — so
            // delivery no longer assumes the two sides share a participant id.
            SendTarget::SEDPDiscovery(locator) | SendTarget::UserData(locator) => {
                if !locator.is_tcp() {
                    return Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()));
                }
                let addr = SocketAddr::new(
                    std::net::IpAddr::V4(locator.to_ip_v4_addr()),
                    locator.tcp_physical_port(),
                );
                if !self.should_dial(&addr) {
                    log::debug!("[TcpTransportPlugin] skip non-initial-peer locator {}", addr);
                    return Ok(());
                }
                self.sender.send_to(addr, locator.tcp_logical_port(), data)
            }
        }
    }

    fn can_handle(&self, locator: &Locator) -> bool {
        locator.is_tcp()
    }

    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
        // Metatraffic (discovery/SEDP) carries the discovery logical port.
        let logical_port =
            PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id);
        self.advertised_tcp_locators(logical_port)
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        // User data shares the same physical listener but a distinct logical
        // port, so the peer reserves the right mux port per traffic type.
        let logical_port =
            PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id);
        self.advertised_tcp_locators(logical_port)
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
        // 1. One cancel covers both halves, so every task is told to stop
        //    before either side is awaited.
        self.cancel.cancel();

        // 2. Take the listener out and await its tasks under block_on.
        //    NOTE: block_on panics if called from inside a tokio runtime
        //    context. The TransportPlugin contract is that close() runs
        //    from the sync DDS shutdown path, never from inside our runtime.
        if let Ok(mut guard) = self.mux_listener.lock() {
            if let Some(listener) = guard.take() {
                self.runtime.block_on(listener.shutdown());
            }
        }

        // 3. Await the outbound tasks the cancel above already woke.
        self.runtime.block_on(self.sender.shutdown());

        debug!("[TcpTransportPlugin] Closed");
    }

    fn disconnect_peer(&self, locators: &[Locator]) {
        let mut seen: Vec<SocketAddr> = Vec::new();
        for locator in locators {
            if !locator.is_tcp() {
                continue;
            }
            let addr = SocketAddr::new(
                std::net::IpAddr::V4(locator.to_ip_v4_addr()),
                locator.tcp_physical_port(),
            );
            if !seen.contains(&addr) {
                seen.push(addr);
                self.sender.disconnect_peer(addr);
            }
        }
    }
}

impl Drop for TcpTransportPlugin {
    fn drop(&mut self) {
        // Best-effort fallback when close() was not called explicitly.
        // We cannot `block_on` inside Drop safely (it panics if Drop runs
        // inside the runtime), so cancellation is all we can do. Firing the
        // root reaches both halves without depending on the listener still
        // being in its slot or on the last sender Arc dying here; the
        // runtime's own Drop then drains or aborts the remaining tasks.
        self.cancel.cancel();

        if let Ok(mut guard) = self.mux_listener.lock() {
            if let Some(listener) = guard.take() {
                drop(listener);
            }
        }
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
        // Bind an ephemeral port rather than the domain-derived fixed port: the
        // latter lingers in TIME_WAIT and makes back-to-back suite runs fail with
        // AddrInUse. The tests below check against the *actual* listener port, so
        // the ephemeral choice is transparent to them.
        cfg.bind_port = Some(0);
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

        plugin.close();
    }

    /// TCP transport has no multicast — multicast source is always `None`.
    #[test]
    fn multicast_discovery_source_is_none() {
        let plugin = make_plugin(next_test_domain());
        assert!(plugin.take_discovery_multicast_source().is_none());
        plugin.close();
    }

    /// Dropping the plugin cancels both halves through the one root, even when
    /// neither half's own `Drop` can do it: the listener has already left its
    /// slot and an outside `Arc` keeps the sender alive past the plugin.
    #[test]
    fn drop_cancels_both_sides_through_the_root() {
        let plugin = make_plugin(next_test_domain());
        let port = plugin.tcp_listener_port().expect("listener port");

        // An inbound connection gives us a token from the listener's subtree.
        let client =
            std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).expect("client connect");

        let listener = plugin.mux_listener.lock().expect("listener lock").take().expect("listener");
        let shared = Arc::clone(listener.shared());

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let inbound = loop {
            if let Some(entry) = shared.connections.iter().next() {
                break entry.cancel.clone();
            }
            assert!(std::time::Instant::now() < deadline, "inbound connection never registered");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };

        // Leak the listener rather than dropping it: dropping would fire its
        // token, which is exactly the path this test must not rely on.
        std::mem::forget(listener);

        let sender = Arc::clone(&plugin.sender);
        assert!(!inbound.is_cancelled());
        assert!(!sender.cancel_token().is_cancelled());

        drop(plugin);

        assert!(inbound.is_cancelled(), "plugin Drop must cancel the inbound side via the root");
        assert!(
            sender.cancel_token().is_cancelled(),
            "plugin Drop must cancel the outbound side via the root"
        );

        drop(client);
    }

    /// `close()` is idempotent and does not hang on the second call.
    #[test]
    fn close_is_idempotent() {
        let plugin = make_plugin(next_test_domain());
        plugin.close();
        plugin.close();
    }

    /// Sending to a TCP locator does not panic / block the caller.
    /// The connect path either succeeds or returns an error; the sync `send`
    /// call itself is fire-and-forget.
    #[test]
    fn send_to_unreachable_does_not_panic() {
        let plugin = make_plugin(next_test_domain());

        // Physical port (7400) is the make_plugin initial peer, so should_dial
        // passes and the connect path is exercised (then refused → dead peer).
        let locator = Locator::from_tcp_v4_dual(std::net::Ipv4Addr::new(127, 0, 0, 1), 100, 7400);
        let target = SendTarget::UserData(&locator);
        let _ = plugin.send(b"\x52\x54\x50\x53", &target);

        plugin.close();
    }

    /// Advertised locators carry the logical RTPS port in the `port` field and
    /// the physical listener port in the address bytes.
    #[test]
    fn advertised_locators_carry_logical_and_physical_ports() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);
        let port = plugin.tcp_listener_port().expect("listener port");

        let meta = plugin.advertised_metatraffic_unicast_locators();
        assert!(!meta.is_empty(), "expected at least one advertised locator");
        let expected_disc = PortManager::get_discovery_traffic_unicast_port(domain, 0);
        for loc in &meta {
            assert!(loc.is_tcp());
            assert_eq!(loc.tcp_physical_port(), port, "physical port in address bytes");
            assert_eq!(loc.tcp_logical_port(), expected_disc, "metatraffic logical port");
        }

        // Default (user-data) locators advertise a distinct logical port.
        let def = plugin.advertised_default_unicast_locators();
        let expected_user = PortManager::get_user_traffic_unicast_port(domain, 0);
        for loc in &def {
            assert_eq!(loc.tcp_physical_port(), port);
            assert_eq!(loc.tcp_logical_port(), expected_user, "user-data logical port");
        }

        plugin.close();
    }

    /// `access_port()` returns the port a peer is actually reached at: the `port`
    /// field for UDP, but the physical (address-packed) port for a TCP dual-port
    /// locator whose `port` field holds the logical/mux port. A dead-peer dial
    /// address carries the physical port, so the TCP branch must not return the
    /// logical port (the dead-peer cleanup match relies on this).
    #[test]
    fn access_port_returns_physical_for_tcp_and_port_field_for_udp() {
        let ip = std::net::Ipv4Addr::new(127, 0, 0, 1);
        let logical: u16 = 7410;
        let physical: u16 = 7401;

        let tcp = Locator::from_tcp_v4_dual(ip, logical, physical);
        assert_eq!(tcp.access_port(), physical as u32, "TCP must match the physical dial port");
        assert_ne!(tcp.access_port(), logical as u32, "TCP must not match the logical port");

        let udp = Locator::from_ip(ip, logical as u32);
        assert_eq!(udp.access_port(), logical as u32, "UDP port field is already physical");
    }

    /// A sender at participant_id 0 reaches a listener at participant_id 1 by
    /// reserving the logical port the listener advertised in its own locator —
    /// the cross-pid case that used to fail (Hybrid single-host), because the
    /// sender recomputed the destination port from its own participant id.
    #[test]
    fn send_reaches_listener_with_mismatched_participant_id() {
        let domain = next_test_domain();
        let recv_port: u16 = 17601;
        let send_port: u16 = 17602;

        // Receiver at participant_id 1 (its logical ports differ from pid 0).
        let mut recv_cfg = TcpConfig::default();
        recv_cfg.bind_port = Some(recv_port);
        recv_cfg.initial_peers = vec![format!("127.0.0.1:{recv_port}").parse().unwrap()];
        let receiver = TcpTransportPlugin::new(
            domain,
            1,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0x11u8; 12],
            recv_cfg,
        )
        .expect("receiver");

        // Sender at participant_id 0, allowed to dial the receiver.
        let mut send_cfg = TcpConfig::default();
        send_cfg.bind_port = Some(send_port);
        send_cfg.initial_peers = vec![format!("127.0.0.1:{recv_port}").parse().unwrap()];
        let sender = TcpTransportPlugin::new(
            domain,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0x00u8; 12],
            send_cfg,
        )
        .expect("sender");

        let rx = match receiver.take_user_data_unicast_source().expect("user source") {
            MessageSource::Channel { rx } => rx,
            _ => panic!("expected channel source"),
        };

        // Receiver's advertised user-data locator carries pid-1's logical port
        // plus the physical bind port — distinct values, proving the carry.
        let loc =
            receiver.advertised_default_unicast_locators().into_iter().next().expect("locator");
        assert_eq!(loc.tcp_physical_port(), recv_port);
        assert_eq!(loc.tcp_logical_port(), PortManager::get_user_traffic_unicast_port(domain, 1));
        assert_ne!(loc.tcp_logical_port(), loc.tcp_physical_port());

        let rtps: &[u8] = b"RTPS\x02\x04\x00\x00\x01\x02\x03\x04\x05\x06\x07\x08";
        sender.send(rtps, &SendTarget::UserData(&loc)).expect("send");

        let msg = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("receiver got frame despite pid mismatch");
        assert_eq!(&msg.data[..rtps.len()], rtps);

        sender.close();
        receiver.close();
    }
}
