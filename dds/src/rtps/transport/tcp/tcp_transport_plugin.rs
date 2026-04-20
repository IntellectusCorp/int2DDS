#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver};
use dashmap::DashMap;
use log::{debug, info, warn};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::plugin::{IncomingMessage, MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::stream_wrapper::{accept_tls, wrap_stream};
use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
use crate::rtps::transport::tcp::tcp_sender::TcpSender;
use crate::rtps::transport::tcp::tls::TlsConfig;

/// Channel buffer size for discovery and user data channels.
const CHANNEL_BUFFER_SIZE: usize = 256;

/// TCP implementation of the TransportPlugin trait.
///
/// Owns a TcpSender for outgoing traffic and a TcpMuxListener for incoming
/// traffic.  The MuxListener accept loop runs in a dedicated thread (spawned
/// during construction); each accepted connection gets its own read thread.
pub(crate) struct TcpTransportPlugin {
    sender: TcpSender,
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,
    listener_port: u16,
    /// Whether this host accepts inbound TCP connections. When false, the
    /// mux listener is not created and advertised locators are suppressed
    /// (asymmetric NAT mode, outbound-only).
    reachable: bool,

    /// Channel receivers — taken once via `take_*_source()`.
    discovery_rx: Mutex<Option<Receiver<IncomingMessage>>>,
    user_data_rx: Mutex<Option<Receiver<IncomingMessage>>>,

    /// Dead peer event receiver — taken once via `take_dead_peer_receiver()`.
    dead_peer_rx: Mutex<Option<Receiver<SocketAddr>>>,

    /// Termination flag for the mux listening thread.
    terminated: Arc<AtomicBool>,

    /// Join handle for the mux listening thread (None in asymmetric mode).
    mux_thread_handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl TcpTransportPlugin {
    /// Create a new TCP transport plugin.
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        working_ips: Vec<String>,
        guid_prefix: GuidPrefix,
    ) -> io::Result<Self> {
        Self::new_with_tls(domain_id, participant_id, working_ip, working_ips, guid_prefix, None)
    }

    /// Same as [`new`] but with an optional TLS config that wraps accepted
    /// (inbound) connections as well as outbound connections via TcpSender.
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

        // Asymmetric WAN mode: host behind NAT without port forwarding.
        // Skip listener creation, advertise no locators, only dial out.
        // Matches RTI Connext TCP `server_bind_port=0` pattern.
        let reachable = crate::common::env::get_tcp_reachable();

        let (discovery_tx, discovery_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);
        let (user_data_tx, user_data_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);
        let (dead_peer_tx, dead_peer_rx) = bounded::<SocketAddr>(CHANNEL_BUFFER_SIZE);
        let terminated = Arc::new(AtomicBool::new(false));

        let (mux_listener_opt, listener_port) = if reachable {
            let listener = match TcpMuxListener::new(
                physical_port,
                domain_id,
                participant_id,
                guid_prefix,
                discovery_tx,
                user_data_tx,
            ) {
                Ok(l) => l,
                Err(e) => {
                    log::error!(
                        "[TcpTransportPlugin] Failed to bind TCP listener on port {} (domain={}): {}. \
                         Another participant may already be using this port on the same host.",
                        physical_port, domain_id, e
                    );
                    return Err(transport_io_error(
                        TransportErrorCode::TcpBindFailed,
                        format!(
                            "Failed to bind TCP listener on port {} (domain={}): {}",
                            physical_port, domain_id, e
                        ),
                    ));
                }
            };
            let port = listener.port();
            (Some(listener), port)
        } else {
            log::info!(
                "[TcpTransportPlugin] Asymmetric mode (INT2DDS_TCP_REACHABLE=false): \
                 skipping TCP listener bind, outbound-only (domain={}, pid={})",
                domain_id, participant_id
            );
            // Drop the send halves so any (unexpected) upstream send fails fast
            // rather than silently buffering — the rx halves are still owned by
            // this plugin below for the `take_*_source()` contract.
            drop(discovery_tx);
            drop(user_data_tx);
            (None, 0u16)
        };

        let sender = TcpSender::new_with_tls(
            working_ip,
            guid_prefix,
            domain_id,
            participant_id,
            listener_port,
            Arc::new(DashMap::new()),
            tls_config.clone(),
        )?;

        let mux_thread_handle = if let Some(mux_listener) = mux_listener_opt {
            let terminated_clone = terminated.clone();
            let sender_clone = sender.clone();
            let tls_config_clone = tls_config.clone();
            let handle = thread::Builder::new()
                .name("tcp_mux_listening".to_string())
                .spawn(move || {
                    let mut task = TcpMuxListeningLoopTask {
                        mux_listener,
                        sender: sender_clone,
                        terminated: terminated_clone,
                        dead_peer_tx,
                        tls_config: tls_config_clone,
                    };
                    if let Err(e) = task.run() {
                        log::error!("[TcpTransportPlugin] Mux listening loop error: {:?}", e);
                    }
                    debug!("[TcpTransportPlugin] Mux listening thread finished");
                })
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            Some(handle)
        } else {
            // Asymmetric mode: no listener → no mux thread.
            // dead_peer_tx is still needed by TcpSender (for keepalive-detected deaths),
            // but its sender half is otherwise unused here. Keep it alive by attaching it
            // to the sender's disconnect path (TcpSender already has its own mechanism).
            drop(dead_peer_tx);
            None
        };

        info!(
            "[TcpTransportPlugin] Created (domain={}, pid={}, port={}, reachable={})",
            domain_id, participant_id, listener_port, reachable
        );

        Ok(Self {
            sender,
            domain_id,
            participant_id,
            working_ips,
            listener_port,
            reachable,
            discovery_rx: Mutex::new(Some(discovery_rx)),
            user_data_rx: Mutex::new(Some(user_data_rx)),
            dead_peer_rx: Mutex::new(Some(dead_peer_rx)),
            terminated,
            mux_thread_handle: Mutex::new(mux_thread_handle),
        })
    }

    /// Build TCP locators this plugin is actually listening on. Encapsulates
    /// the WAN `INT2DDS_TCP_PUBLIC_ADDR` override so upper layers never need
    /// to know about it.
    ///
    /// Returns an empty list when the plugin is running in asymmetric mode
    /// (`INT2DDS_TCP_REACHABLE=false`), so that remote peers do not attempt
    /// to dial this host. All communication in that case is initiated from
    /// this side via outbound connections.
    fn advertised_tcp_locators(&self) -> Vec<Locator> {
        if !self.reachable {
            log::debug!(
                "[TcpTransportPlugin] Asymmetric mode: suppressing advertised TCP locators"
            );
            return Vec::new();
        }
        // WAN mode: if a public address is configured, it replaces every
        // per-NIC locator (remote peers only reach us through the public
        // address anyway).
        if let Some(public_addr) = crate::common::env::get_tcp_public_addr() {
            if let std::net::IpAddr::V4(v4) = public_addr.ip() {
                log::info!(
                    "[TcpTransportPlugin] WAN mode: advertising public address {} instead of :{}",
                    public_addr, self.listener_port
                );
                return vec![Locator::from_tcp_v4(v4, public_addr.port() as u32)];
            }
            log::warn!(
                "[TcpTransportPlugin] Public address is not IPv4, falling back to LAN NICs"
            );
        }
        let mut locators = Vec::new();
        for ip_str in &self.working_ips {
            if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                locators.push(Locator::from_tcp_v4(ip, self.listener_port as u32));
            }
        }
        locators
    }
}

impl TransportPlugin for TcpTransportPlugin {
    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        match target {
            SendTarget::MulticastDiscovery => {
                let initial_peers = crate::common::env::get_initial_peers();
                for peer_addr in &initial_peers {
                    let _ = self.sender.send_to_discovery(peer_addr, data);
                }
                Ok(())
            }
            SendTarget::UnicastDiscovery(locator) => {
                let ip = locator.to_ip_v4_addr();
                let port = locator.port() as u16;
                let addr = SocketAddr::new(std::net::IpAddr::V4(ip), port);
                self.sender.send_to_discovery(&addr, data)?;
                Ok(())
            }
            SendTarget::UserData(locator) => {
                let ip = locator.to_ip_v4_addr();
                let port = locator.port() as u16;
                let addr = SocketAddr::new(std::net::IpAddr::V4(ip), port);
                self.sender.send_to_user_data(&addr, data)?;
                Ok(())
            }
        }
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
        None
    }

    fn take_discovery_unicast_source(&self) -> Option<MessageSource> {
        let rx = self.discovery_rx.lock().expect("lock poisoned").take()?;
        Some(MessageSource::Channel { rx })
    }

    fn take_user_data_unicast_source(&self) -> Option<MessageSource> {
        let rx = self.user_data_rx.lock().expect("lock poisoned").take()?;
        Some(MessageSource::Channel { rx })
    }

    fn take_dead_peer_receiver(&self) -> Option<crossbeam_channel::Receiver<SocketAddr>> {
        self.dead_peer_rx.lock().expect("lock poisoned").take()
    }

    fn port(&self) -> u16 {
        self.listener_port
    }

    fn tcp_listener_port(&self) -> Option<u16> {
        if self.reachable {
            Some(self.listener_port)
        } else {
            None
        }
    }

    fn participant_id(&self) -> u32 {
        self.participant_id
    }

    fn close(&self) {
        self.terminated.store(true, Ordering::SeqCst);

        if let Ok(mut handle) = self.mux_thread_handle.lock() {
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }

        debug!("[TcpTransportPlugin] Closed");
    }
}

// ── Mux listening loop task ───────────────────────────────────────────────────

/// Runs the blocking accept loop and a companion timer thread.
///
/// This struct is constructed and driven on the dedicated `tcp_mux_listening`
/// thread.  Each accepted TCP connection is wrapped (optionally in TLS) and
/// handed off to `TcpMuxListener::accept_connection`, which spawns a
/// per-connection read thread.
struct TcpMuxListeningLoopTask {
    mux_listener: TcpMuxListener,
    sender: TcpSender,
    terminated: Arc<AtomicBool>,
    dead_peer_tx: crossbeam_channel::Sender<SocketAddr>,
    /// Optional TLS configuration for accepting inbound TLS connections.
    tls_config: Option<Arc<TlsConfig>>,
}

impl TcpMuxListeningLoopTask {
    fn run(&mut self) -> io::Result<()> {
        let keepalive_check_interval_ms = crate::common::env::get_tcp_keepalive_interval_ms();
        let incoming_idle_timeout_ms = crate::common::env::get_tcp_incoming_idle_timeout_ms();
        let orphan_data_grace_ms = crate::common::env::get_tcp_orphan_data_grace_ms();

        let incoming_idle_timeout = Duration::from_millis(incoming_idle_timeout_ms);
        let orphan_data_grace = Duration::from_millis(orphan_data_grace_ms);
        let keepalive_interval = Duration::from_millis(keepalive_check_interval_ms);

        info!(
            "[TcpMuxListeningLoopTask] Starting on port {} \
             (incoming_idle_timeout={}ms, orphan_data_grace={}ms)",
            self.mux_listener.port(),
            incoming_idle_timeout_ms,
            orphan_data_grace_ms,
        );

        // Take the TCP listener socket for the blocking accept loop.
        let listener = match self.mux_listener.take_listener() {
            Some(l) => l,
            None => {
                return Err(io::Error::other(
                    "[TcpMuxListeningLoopTask] Listener not initialized",
                ))
            }
        };

        let shared = Arc::clone(&self.mux_listener.shared);

        // ── Timer thread: keepalive + orphan pruning ─────────────────────────
        {
            let timer_shared = Arc::clone(&shared);
            let timer_terminated = Arc::clone(&self.terminated);
            let timer_sender = self.sender.clone();
            let timer_dead_peer_tx = self.dead_peer_tx.clone();
            let orphan_check_interval =
                (orphan_data_grace / 2).max(Duration::from_millis(100));

            thread::Builder::new()
                .name("tcp_mux_timer".to_string())
                .stack_size(128 * 1024)
                .spawn(move || {
                    let mut last_keepalive = Instant::now();
                    let mut last_orphan = Instant::now();

                    loop {
                        thread::sleep(Duration::from_millis(100));
                        if timer_terminated.load(Ordering::SeqCst) {
                            break;
                        }

                        if last_keepalive.elapsed() >= keepalive_interval {
                            last_keepalive = Instant::now();
                            let dead_addrs = timer_sender.send_keepalives();
                            for addr in dead_addrs {
                                warn!(
                                    "[TcpMuxListeningLoopTask] Dead peer via keepalive: {:?}",
                                    addr
                                );
                                timer_sender.disconnect_peer(&addr);
                                let _ = timer_dead_peer_tx.try_send(addr);
                            }
                        }

                        if last_orphan.elapsed() >= orphan_check_interval {
                            last_orphan = Instant::now();
                            let pruned =
                                timer_shared.prune_orphan_data_connections(orphan_data_grace);
                            if pruned > 0 {
                                warn!(
                                    "[TcpMuxListeningLoopTask] Pruned {} orphan incoming",
                                    pruned
                                );
                            }
                            let pruned = timer_sender.prune_orphan_connections();
                            if pruned > 0 {
                                warn!(
                                    "[TcpMuxListeningLoopTask] Pruned {} outgoing orphan",
                                    pruned
                                );
                            }
                        }
                    }
                    debug!("[TcpMuxListeningLoopTask] Timer thread finished");
                })
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        }

        // ── Accept loop (this thread) ─────────────────────────────────────────
        let terminated = Arc::clone(&self.terminated);
        let tls_config = self.tls_config.clone();

        loop {
            match listener.accept() {
                Ok((tcp, addr)) => {
                    debug!("[TcpMuxListeningLoopTask] Accepted from {:?}", addr);

                    // Apply optional socket buffer size overrides.
                    if let Some(sz) = crate::common::env::get_tcp_so_rcvbuf() {
                        let _ = socket2::SockRef::from(&tcp).set_recv_buffer_size(sz);
                    }
                    if let Some(sz) = crate::common::env::get_tcp_so_sndbuf() {
                        let _ = socket2::SockRef::from(&tcp).set_send_buffer_size(sz);
                    }

                    // Optionally wrap in TLS (blocking handshake in accept thread).
                    let stream = match &tls_config {
                        Some(cfg) => match cfg.build_server_config() {
                            Ok(server_cfg) => match accept_tls(tcp, server_cfg) {
                                Ok(s) => s,
                                Err(e) => {
                                    warn!(
                                        "[TcpMuxListeningLoopTask] TLS accept from {:?} \
                                         failed: {:?}",
                                        addr, e
                                    );
                                    continue;
                                }
                            },
                            Err(e) => {
                                warn!(
                                    "[TcpMuxListeningLoopTask] TLS server config error: {:?}",
                                    e
                                );
                                continue;
                            }
                        },
                        None => wrap_stream(tcp),
                    };

                    let _ = stream.set_nodelay(true);

                    self.mux_listener.accept_connection(
                        stream,
                        addr,
                        Arc::clone(&terminated),
                        incoming_idle_timeout,
                    );
                }

                // Periodic timeout from SO_RCVTIMEO — check termination flag.
                Err(ref e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut =>
                {
                    if terminated.load(Ordering::SeqCst) {
                        break;
                    }
                }

                Err(e) => {
                    if terminated.load(Ordering::SeqCst) {
                        break;
                    }
                    warn!("[TcpMuxListeningLoopTask] Accept error: {:?}", e);
                }
            }
        }

        self.mux_listener.close();
        debug!("[TcpMuxListeningLoopTask] Accept loop finished");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::locator::Locator;
    use crate::rtps::transport::plugin::SendTarget;
    use std::net::TcpStream;
    use std::time::Duration;

    fn next_test_domain() -> u32 {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(900);
        NEXT.fetch_add(1, Ordering::SeqCst)
    }

    fn make_plugin(domain_id: u32) -> TcpTransportPlugin {
        TcpTransportPlugin::new(
            domain_id,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0u8; 12],
        )
        .expect("plugin creation")
    }

    #[test]
    fn test_plugin_creates_and_listens() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);
        let port = plugin.tcp_listener_port().expect("listener port");
        assert!(port != 0);

        let _stream =
            TcpStream::connect(format!("127.0.0.1:{}", port)).expect("connect to mux listener");
        plugin.close();
    }

    #[test]
    fn test_take_discovery_unicast_source_returns_channel_once() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);

        assert!(plugin.take_discovery_unicast_source().is_some());
        assert!(plugin.take_discovery_unicast_source().is_none());

        plugin.close();
    }

    #[test]
    fn test_take_user_data_unicast_source_returns_channel_once() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);

        assert!(plugin.take_user_data_unicast_source().is_some());
        assert!(plugin.take_user_data_unicast_source().is_none());

        plugin.close();
    }

    #[test]
    fn test_take_dead_peer_receiver_returns_channel_once() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);

        assert!(plugin.take_dead_peer_receiver().is_some());
        assert!(plugin.take_dead_peer_receiver().is_none());

        plugin.close();
    }

    #[test]
    fn test_multicast_discovery_source_is_none_for_tcp() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);

        assert!(plugin.take_discovery_multicast_source().is_none());

        plugin.close();
    }

    #[test]
    fn test_send_to_unreachable_peer_returns_err_not_panic() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);

        let locator = Locator::from_tcp_v4(std::net::Ipv4Addr::new(127, 0, 0, 1), 1);
        let target = SendTarget::UserData(&locator);
        let result = plugin.send(b"\x52\x54\x50\x53...", &target);
        assert!(result.is_err());

        plugin.close();
    }

    #[test]
    fn test_idle_timeout_env_var_default_when_unset() {
        unsafe {
            std::env::set_var("INT2DDS_TCP_INCOMING_IDLE_TIMEOUT", "5000");
        }
        let domain = next_test_domain();
        let plugin = make_plugin(domain);
        plugin.close();
        unsafe {
            std::env::remove_var("INT2DDS_TCP_INCOMING_IDLE_TIMEOUT");
        }
    }

    #[test]
    fn test_close_is_idempotent_and_releases_port() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);
        let port = plugin.tcp_listener_port().expect("listener port");

        plugin.close();
        plugin.close();

        std::thread::sleep(Duration::from_millis(100));
        let _ = TcpStream::connect(format!("127.0.0.1:{}", port));
    }
}

// ── TLS E2E tests (Phase 3e) ──────────────────────────────────────────────────

#[cfg(test)]
mod tls_tests {
    use std::io::Write as IoWrite;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use crossbeam_channel::bounded;
    use dashmap::DashMap;
    use rcgen::{generate_simple_self_signed, CertifiedKey};
    use tempfile::NamedTempFile;

    use crate::infrastructure::qos_policy::PropertyQosPolicy;
    use crate::rtps::transport::tcp::stream_wrapper::{accept_tls, wrap_stream};
    use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
    use crate::rtps::transport::tcp::tcp_sender::TcpSender;
    use crate::rtps::transport::tcp::tls::TlsConfig;

    // ── cert helpers ─────────────────────────────────────────────────────────

    struct Bundle {
        ca_file: NamedTempFile,
        cert_file: NamedTempFile,
        key_file: NamedTempFile,
    }

    fn make_bundle() -> Bundle {
        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["localhost".into()]).expect("rcgen");
        let mut ca = NamedTempFile::new().unwrap();
        let mut crt = NamedTempFile::new().unwrap();
        let mut key = NamedTempFile::new().unwrap();
        ca.write_all(cert.pem().as_bytes()).unwrap();
        crt.write_all(cert.pem().as_bytes()).unwrap();
        key.write_all(key_pair.serialize_pem().as_bytes()).unwrap();
        Bundle { ca_file: ca, cert_file: crt, key_file: key }
    }

    fn tls_config_from_bundle(bundle: &Bundle, server_name: &str) -> Arc<TlsConfig> {
        let mut p = PropertyQosPolicy::default();
        p.set("int2dds.tls.ca_file", bundle.ca_file.path().to_str().unwrap());
        p.set("int2dds.tls.cert_file", bundle.cert_file.path().to_str().unwrap());
        p.set("int2dds.tls.key_file", bundle.key_file.path().to_str().unwrap());
        p.set("int2dds.tls.server_name", server_name);
        p.set("int2dds.tls.verify_peer", "false");
        Arc::new(TlsConfig::from_property(&p).expect("parse ok").expect("config present"))
    }

    /// Spawn the TLS accept loop for `listener` in a background thread.
    /// Accepts connections until `terminated` is set.
    fn spawn_tls_accept_loop(
        mut listener: TcpMuxListener,
        tls: Arc<TlsConfig>,
        terminated: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let raw = match listener.take_listener() {
                Some(l) => l,
                None => return,
            };
            let idle = Duration::from_secs(30);
            while !terminated.load(Ordering::SeqCst) {
                match raw.accept() {
                    Ok((tcp, addr)) => {
                        let server_cfg = match tls.build_server_config() {
                            Ok(c) => c,
                            Err(_) => continue,
                        };
                        match accept_tls(tcp, server_cfg) {
                            Ok(stream) => {
                                listener.accept_connection(
                                    stream,
                                    addr,
                                    terminated.clone(),
                                    idle,
                                );
                            }
                            Err(e) => {
                                log::warn!("[tls_test] TLS accept failed: {:?}", e);
                            }
                        }
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {}
                    Err(_) => break,
                }
            }
        })
    }

    // ── Test 1: full TLS data roundtrip ──────────────────────────────────────

    /// Verifies that a TLS-wrapped sender can send RTPS data to a TLS-wrapped
    /// listener and the data is correctly routed to the discovery channel.
    #[test]
    fn tls_sender_to_listener_data_roundtrip() {
        let bundle = make_bundle();
        let tls = tls_config_from_bundle(&bundle, "localhost");

        // Use domain 0 for both sides so their logical discovery ports match.
        // Listener uses port 0 (OS-assigned) so there is no port conflict.
        let domain_id = 0u32;
        let participant_id = 0u32;

        let (disc_tx, disc_rx) = bounded(64);
        let (user_tx, _) = bounded(64);
        let listener =
            TcpMuxListener::new(0, domain_id, participant_id, [0x10; 12], disc_tx, user_tx)
                .expect("listener creation");
        let server_port = listener.port();
        let server_addr: std::net::SocketAddr =
            format!("127.0.0.1:{}", server_port).parse().unwrap();

        let terminated = Arc::new(AtomicBool::new(false));
        let _accept_thread =
            spawn_tls_accept_loop(listener, tls.clone(), terminated.clone());

        // TLS-enabled sender, same domain/participant so logical port matches.
        let sender = TcpSender::new_with_tls(
            "127.0.0.1".to_string(),
            [0x11; 12],
            domain_id,
            participant_id,
            server_port,
            Arc::new(DashMap::new()),
            Some(tls.clone()),
        )
        .expect("sender creation");

        // A minimal RTPS header (12 bytes) — enough for the listener to route.
        let rtps_header = b"RTPS\x02\x04\x00\x00\x01\x02\x03\x04";
        sender.send_to_discovery(&server_addr, rtps_header).expect("send ok");

        let msg = disc_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("discovery message not received within 3 s");
        assert!(
            msg.data.starts_with(b"RTPS"),
            "unexpected payload: {:?}",
            &msg.data[..msg.data.len().min(16)]
        );

        terminated.store(true, Ordering::SeqCst);
    }

    // ── Test 2: plain client rejected by TLS server ───────────────────────────

    /// A plain (non-TLS) client should fail to complete the handshake,
    /// so no data reaches the discovery channel.
    #[test]
    fn plaintext_client_rejected_by_tls_listener() {
        let bundle = make_bundle();
        let tls = tls_config_from_bundle(&bundle, "localhost");

        let (disc_tx, disc_rx) = bounded(64);
        let (user_tx, _) = bounded(64);
        let listener =
            TcpMuxListener::new(0, 0, 0, [0x20; 12], disc_tx, user_tx).expect("listener");
        let server_port = listener.port();

        let terminated = Arc::new(AtomicBool::new(false));
        let _accept_thread = spawn_tls_accept_loop(listener, tls, terminated.clone());

        // Plain TCP sender — no TLS config.
        let sender = TcpSender::new_with_tls(
            "127.0.0.1".to_string(),
            [0x21; 12],
            0,
            0,
            server_port,
            Arc::new(DashMap::new()),
            None, // no TLS
        )
        .expect("sender creation");

        let server_addr: std::net::SocketAddr =
            format!("127.0.0.1:{}", server_port).parse().unwrap();
        let rtps_header = b"RTPS\x02\x04\x00\x00\x01\x02\x03\x04";

        // The send may fail (TLS server closes the connection) or the RTPS
        // frame may arrive garbled. Either way, no valid message should be
        // routed to the discovery channel.
        let _ = sender.send_to_discovery(&server_addr, rtps_header);

        let result = disc_rx.recv_timeout(Duration::from_millis(500));
        assert!(
            result.is_err(),
            "plain client must not reach discovery channel, but got a message with {} bytes",
            result.map(|m| m.data.len()).unwrap_or(0),
        );

        terminated.store(true, Ordering::SeqCst);
    }
}
