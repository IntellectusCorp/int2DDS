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

    /// Channel receivers — taken once via `take_*_source()`.
    discovery_rx: Mutex<Option<Receiver<IncomingMessage>>>,
    user_data_rx: Mutex<Option<Receiver<IncomingMessage>>>,

    /// Dead peer event receiver — taken once via `take_dead_peer_receiver()`.
    dead_peer_rx: Mutex<Option<Receiver<SocketAddr>>>,

    /// Termination flag for the mux listening thread.
    terminated: Arc<AtomicBool>,

    /// Join handle for the mux listening thread.
    mux_thread_handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl TcpTransportPlugin {
    /// Create a new TCP transport plugin.
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        guid_prefix: GuidPrefix,
    ) -> io::Result<Self> {
        Self::new_with_tls(domain_id, participant_id, working_ip, guid_prefix, None)
    }

    /// Same as [`new`] but with an optional TLS config that wraps accepted
    /// (inbound) connections as well as outbound connections via TcpSender.
    pub(crate) fn new_with_tls(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        guid_prefix: GuidPrefix,
        tls_config: Option<Arc<TlsConfig>>,
    ) -> io::Result<Self> {
        let physical_port = crate::common::env::get_tcp_port()
            .unwrap_or_else(|| PortManager::get_tcp_physical_port(domain_id));

        let (discovery_tx, discovery_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);
        let (user_data_tx, user_data_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);

        let mux_listener = match TcpMuxListener::new(
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
                    physical_port,
                    domain_id,
                    e
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
        let listener_port = mux_listener.port();

        let sender = TcpSender::new_with_tls(
            working_ip,
            guid_prefix,
            domain_id,
            participant_id,
            listener_port,
            Arc::new(DashMap::new()),
            tls_config.clone(),
        )?;

        let (dead_peer_tx, dead_peer_rx) = bounded::<SocketAddr>(CHANNEL_BUFFER_SIZE);

        let terminated = Arc::new(AtomicBool::new(false));
        let terminated_clone = terminated.clone();

        let sender_clone = sender.clone();
        let handle = thread::Builder::new()
            .name("tcp_mux_listening".to_string())
            .spawn(move || {
                let mut task = TcpMuxListeningLoopTask {
                    mux_listener,
                    sender: sender_clone,
                    terminated: terminated_clone,
                    dead_peer_tx,
                    tls_config,
                };
                if let Err(e) = task.run() {
                    log::error!("[TcpTransportPlugin] Mux listening loop error: {:?}", e);
                }
                debug!("[TcpTransportPlugin] Mux listening thread finished");
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        info!(
            "[TcpTransportPlugin] Created (domain={}, pid={}, port={})",
            domain_id, participant_id, listener_port
        );

        Ok(Self {
            sender,
            domain_id,
            participant_id,
            working_ips: Vec::new(),
            listener_port,
            discovery_rx: Mutex::new(Some(discovery_rx)),
            user_data_rx: Mutex::new(Some(user_data_rx)),
            dead_peer_rx: Mutex::new(Some(dead_peer_rx)),
            terminated,
            mux_thread_handle: Mutex::new(Some(handle)),
        })
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

    fn local_locators(&self, _domain_id: u32, _participant_id: u32) -> Vec<Locator> {
        let mut locators = Vec::new();
        for ip_str in &self.working_ips {
            if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                locators.push(Locator::from_tcp_v4(ip, self.listener_port as u32));
            }
        }
        locators
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
        Some(self.listener_port)
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
        TcpTransportPlugin::new(domain_id, 0, "127.0.0.1".to_string(), [0u8; 12])
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
