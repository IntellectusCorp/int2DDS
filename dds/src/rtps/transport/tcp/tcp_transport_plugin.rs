#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam_channel::{bounded, Receiver};
use log::{debug, info};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
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
        // Calculate physical port for TCP listener
        let physical_port = PortManager::get_tcp_physical_port(domain_id);

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
                    return Err(e);
                }
            };
        let listener_port = mux_listener.port();

        // Create TcpSender
        let sender =
            TcpSender::new(working_ip, guid_prefix, domain_id, participant_id, listener_port)?;

        // Spawn mux listening thread
        let terminated = Arc::new(AtomicBool::new(false));
        let terminated_clone = terminated.clone();

        let handle = thread::Builder::new()
            .name("tcp_mux_listening".to_string())
            .spawn(move || {
                let mut task =
                    TcpMuxListeningLoopTask { mux_listener, terminated: terminated_clone };
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
    terminated: Arc<AtomicBool>,
}

impl TcpMuxListeningLoopTask {
    fn run(&mut self) -> io::Result<()> {
        use mio::{Events, Interest, Poll, Token};
        use std::time::{Duration, Instant};

        const MUX_LISTENER_TOKEN: Token = Token(0);
        const POLL_TIMEOUT_MS: u64 = 100;
        const KEEPALIVE_CHECK_INTERVAL_SECS: u64 = 10;

        info!("[TcpMuxListeningLoopTask] Starting on port {}", self.mux_listener.port());

        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(crate::rtps::transport::socket::MAX_EVENTS);

        if let Some(listener) = self.mux_listener.listener_mut() {
            poll.registry().register(listener, MUX_LISTENER_TOKEN, Interest::READABLE)?;
        } else {
            return Err(io::Error::other("MuxListener not initialized"));
        }

        let mut last_keepalive_check = Instant::now();
        let keepalive_interval = Duration::from_secs(KEEPALIVE_CHECK_INTERVAL_SECS);

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
                                log::error!("[TcpMuxListeningLoopTask] Accept error: {:?}", e);
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
                let dead_peers = self.mux_listener.send_keepalives();
                for guid in dead_peers {
                    log::warn!("[TcpMuxListeningLoopTask] Removing dead peer {:?}", guid);
                    self.mux_listener.remove_peer(guid, poll.registry());
                }
            }
        }
    }
}
