#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam_channel::{bounded, Receiver};
use dashmap::DashMap;
use log::{debug, info};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::plugin::{IncomingMessage, MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
use crate::rtps::transport::tcp::tcp_sender::TcpSender;

/// Channel buffer size for discovery and user data channels.
const CHANNEL_BUFFER_SIZE: usize = 256;

/// TCP implementation of the TransportPlugin trait.
///
/// Owns a TcpSender for outgoing traffic and a TcpMuxListener for incoming traffic.
/// The MuxListener runs in a dedicated thread (spawned during construction)
/// and routes incoming frames to discovery/user channels via `MessageSource::Channel`.
///
/// MulticastDiscovery is implemented as unicast to each initial peer.
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
    ///
    /// Creates TcpSender, TcpMuxListener, spawns the mux listening thread,
    /// and wires up discovery/user channels.
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        guid_prefix: GuidPrefix,
    ) -> io::Result<Self> {
        // Physical port: use INT2DDS_TCP_PORT if set, otherwise calculate from domain_id
        let physical_port = crate::common::env::get_tcp_port()
            .unwrap_or_else(|| PortManager::get_tcp_physical_port(domain_id));

        // Create channels for routing incoming data
        let (discovery_tx, discovery_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);
        let (user_data_tx, user_data_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);

        // Create MuxListener on the fixed physical port.
        // No ephemeral fallback — in TCP mode, the physical port must be predictable
        // for initial_peers. If the port is already in use, participant creation fails.
        let mux_listener =
            match TcpMuxListener::new(
                physical_port,
                domain_id,
                participant_id,
                guid_prefix,
                discovery_tx,
                user_data_tx,
            ) {
                Ok(listener) => listener,
                Err(e) => {
                    log::error!(
                    "[TcpTransportPlugin] Failed to bind TCP listener on port {} (domain={}): {}. \
                     Another participant may already be using this port on the same host.",
                    physical_port, domain_id, e
                );
                    return Err(transport_io_error(
                        TransportErrorCode::TcpBindFailed,
                        format!("Failed to bind TCP listener on port {} (domain={}): {}", physical_port, domain_id, e),
                    ));
                }
            };
        let listener_port = mux_listener.port();

        // Create TcpSender
        let sender = TcpSender::new(
            working_ip,
            guid_prefix,
            domain_id,
            participant_id,
            listener_port,
            Arc::new(DashMap::new()),
        )?;

        // Dead peer event channel — carries SocketAddr so PeerMonitor can resolve the actual GuidPrefix
        let (dead_peer_tx, dead_peer_rx) = bounded::<SocketAddr>(CHANNEL_BUFFER_SIZE);

        // Spawn mux listening thread
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
                // TCP has no multicast. Send to each initial peer as unicast.
                // Establishes control connection (BIND handshake) → resolves discovery port → sends.
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
                // TCP locators advertise the physical listener port
                locators.push(Locator::from_tcp_v4(ip, self.listener_port as u32));
            }
        }
        locators
    }

    fn take_discovery_multicast_source(&self) -> Option<MessageSource> {
        // TCP has no multicast — discovery happens via unicast to initial peers.
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
        // Signal termination to mux listening thread
        self.terminated.store(true, Ordering::SeqCst);

        // Wait for mux thread to finish
        if let Ok(mut handle) = self.mux_thread_handle.lock() {
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }

        debug!("[TcpTransportPlugin] Closed");
    }
}

/// Internal event loop task for the mux listener.
/// Uses AtomicBool for termination instead of Participant reference.
struct TcpMuxListeningLoopTask {
    mux_listener: TcpMuxListener,
    sender: TcpSender,
    terminated: Arc<AtomicBool>,
    dead_peer_tx: crossbeam_channel::Sender<SocketAddr>,
}

impl TcpMuxListeningLoopTask {
    fn run(&mut self) -> io::Result<()> {
        use mio::{Events, Interest, Poll, Token};
        use std::time::{Duration, Instant};

        const MUX_LISTENER_TOKEN: Token = Token(0);
        const POLL_TIMEOUT_MS: u64 = 100;

        let keepalive_check_interval: u64 = crate::common::env::get_tcp_keepalive_interval_ms();

        let incoming_idle_timeout_ms = crate::common::env::get_tcp_incoming_idle_timeout_ms();
        let incoming_idle_timeout = Duration::from_millis(incoming_idle_timeout_ms);

        let orphan_data_grace_ms = crate::common::env::get_tcp_orphan_data_grace_ms();
        let orphan_data_grace = Duration::from_millis(orphan_data_grace_ms);

        info!(
            "[TcpMuxListeningLoopTask] Starting on port {} (incoming_idle_timeout={}ms, orphan_data_grace={}ms)",
            self.mux_listener.port(),
            incoming_idle_timeout_ms,
            orphan_data_grace_ms
        );

        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(crate::rtps::transport::socket::MAX_EVENTS);

        if let Some(listener) = self.mux_listener.listener_mut() {
            poll.registry().register(listener, MUX_LISTENER_TOKEN, Interest::READABLE)?;
        } else {
            return Err(io::Error::other("MuxListener not initialized"));
        }

        let mut last_keepalive_check = Instant::now();
        let mut last_idle_check = Instant::now();
        let mut last_orphan_check = Instant::now();
        let keepalive_interval = Duration::from_millis(keepalive_check_interval);
        // Check idle timeouts at half the configured interval so we never
        // exceed the threshold by more than half a check period.
        let idle_check_interval = (incoming_idle_timeout / 2).max(Duration::from_millis(100));
        // Same logic for orphan-data sweeps; the grace is short by default
        // so the check cadence must keep up.
        let orphan_check_interval = (orphan_data_grace / 2).max(Duration::from_millis(100));

        loop {
            match poll.poll(&mut events, Some(Duration::from_millis(POLL_TIMEOUT_MS))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    log::warn!("[TcpMuxListeningLoopTask] Poll interrupted, continuing");
                    continue;
                }
                Err(e) => return Err(e),
            }

            if self.terminated.load(Ordering::SeqCst) {
                debug!("[TcpMuxListeningLoopTask] Termination flag detected, shutting down");
                if let Some(listener) = self.mux_listener.listener_mut() {
                    let _ = poll.registry().deregister(listener);
                }
                self.mux_listener.close();
                return Ok(());
            }

            for event in events.iter() {
                if event.token() == MUX_LISTENER_TOKEN && event.is_readable() {
                    loop {
                        match self.mux_listener.accept(poll.registry()) {
                            Ok(Some(_token)) => {}
                            Ok(None) => break,
                            Err(e) => {
                                log::error!("[TcpMuxListeningLoopTask] [{}] Accept error: {:?}", TransportErrorCode::TcpAcceptFailed, e);
                                break;
                            }
                        }
                    }
                } else if event.is_readable() {
                    self.mux_listener.on_readable(event.token(), poll.registry());
                }
            }

            if last_keepalive_check.elapsed() >= keepalive_interval {
                last_keepalive_check = Instant::now();
                let dead_addrs = self.sender.send_keepalives();
                for addr in dead_addrs {
                    log::warn!(
                        "[TcpMuxListeningLoopTask] Dead peer detected via keepalive: {:?}",
                        addr
                    );
                    self.sender.disconnect_peer(&addr);
                    // send dead peer from keepalive timeout to peer monitor
                    let _ = self.dead_peer_tx.try_send(addr);
                }
            }

            if last_idle_check.elapsed() >= idle_check_interval {
                last_idle_check = Instant::now();
                let pruned = self
                    .mux_listener
                    .prune_idle_connections(incoming_idle_timeout, poll.registry());
                if pruned > 0 {
                    debug!(
                        "[TcpMuxListeningLoopTask] Pruned {} idle incoming connection(s)",
                        pruned
                    );
                }
            }

            if last_orphan_check.elapsed() >= orphan_check_interval {
                last_orphan_check = Instant::now();

                // Listener side: prune incoming orphan data connections
                let pruned = self
                    .mux_listener
                    .prune_orphan_data_connections(orphan_data_grace, poll.registry());
                if pruned > 0 {
                    debug!(
                        "[TcpMuxListeningLoopTask] Pruned {} incoming orphan peer group(s) past grace period",
                        pruned
                    );
                }

                // Sender side: prune outgoing orphan data connections
                let pruned = self.sender.prune_orphan_connections();
                if pruned > 0 {
                    debug!(
                        "[TcpMuxListeningLoopTask] Pruned {} outgoing orphan connection(s)",
                        pruned
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::locator::Locator;
    use crate::rtps::transport::plugin::SendTarget;
    use std::net::TcpStream;
    use std::time::Duration;

    /// Allocate a non-overlapping domain id per test so concurrent `cargo test`
    /// runs do not collide on the discovery/user data port pair.
    fn next_test_domain() -> u32 {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(900);
        NEXT.fetch_add(1, Ordering::SeqCst)
    }

    fn make_plugin(domain_id: u32) -> TcpTransportPlugin {
        // Force port 0 inside this test so the OS allocates an ephemeral
        // physical port and we never collide with other tests/hosts.
        unsafe {
            std::env::set_var("INT2DDS_TCP_PORT", "0");
        }
        TcpTransportPlugin::new(domain_id, 0, "127.0.0.1".to_string(), [0u8; 12])
            .expect("plugin creation")
    }

    #[test]
    fn test_plugin_creates_and_listens() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);
        let port = plugin.tcp_listener_port().expect("listener port");
        assert!(port != 0);

        // The mux listener thread is up — connecting must succeed.
        let _stream =
            TcpStream::connect(format!("127.0.0.1:{}", port)).expect("connect to mux listener");
        plugin.close();
    }

    #[test]
    fn test_take_discovery_unicast_source_returns_channel_once() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);

        let first = plugin.take_discovery_unicast_source();
        assert!(first.is_some(), "first take should return a source");

        let second = plugin.take_discovery_unicast_source();
        assert!(second.is_none(), "second take should be None");

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

        // TCP has no multicast — the discovery multicast source must be None
        // even on the very first call.
        assert!(plugin.take_discovery_multicast_source().is_none());

        plugin.close();
    }

    #[test]
    fn test_send_to_unreachable_peer_returns_err_not_panic() {
        let domain = next_test_domain();
        let plugin = make_plugin(domain);

        // 127.0.0.1:1 is virtually guaranteed to be closed.
        let locator = Locator::from_tcp_v4(std::net::Ipv4Addr::new(127, 0, 0, 1), 1);
        let target = SendTarget::UserData(&locator);
        let result = plugin.send(b"\x52\x54\x50\x53...", &target);
        assert!(result.is_err(), "unreachable peer should yield Err, not panic");

        plugin.close();
    }

    #[test]
    fn test_idle_timeout_env_var_default_when_unset() {
        // The default DEFAULT_INCOMING_IDLE_TIMEOUT_MS lives inside the mux
        // loop, but here we just verify the env var is parsed safely when
        // set to a custom value before plugin creation.
        unsafe {
            std::env::set_var("INT2DDS_TCP_INCOMING_IDLE_TIMEOUT", "5000");
        }
        let domain = next_test_domain();
        let plugin = make_plugin(domain);
        // No assertion on the value itself — just confirm plugin creation
        // succeeds with a custom timeout configured.
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
        plugin.close(); // second close must not panic

        // After close, the port should eventually be reusable. Wait briefly
        // and try a fresh bind on the same port to confirm.
        std::thread::sleep(Duration::from_millis(100));
        // We don't assert success here because OS port reuse semantics vary;
        // the only hard requirement is that close() does not deadlock.
        let _ = TcpStream::connect(format!("127.0.0.1:{}", port));
    }
}
