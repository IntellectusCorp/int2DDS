#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::thread;

use crossbeam_channel::{bounded, Receiver};
use log::{debug, info, warn};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{IncomingMessage, MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::tcp_transport_plugin::TcpTransportPlugin;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::HybridConfig;

/// Channel buffer size for merged sources.
const CHANNEL_BUFFER_SIZE: usize = 256;

/// Hybrid transport plugin — UDP multicast discovery + TCP/UDP unicast.
///
/// Discovery multicast always uses UDP.
/// Unicast discovery and user data route by locator kind:
///   - UDP locator → UDP sender
///   - TCP locator → TCP sender (via embedded TcpTransportPlugin)
///
/// Incoming unicast messages from both UDP and TCP are merged
/// into a single channel-based `MessageSource`.
pub(crate) struct HybridTransportPlugin {
    udp_sender: UdpSender,
    tcp_plugin: TcpTransportPlugin,
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,

    // UDP listeners — multicast is always UDP
    discovery_multicast_listener: Mutex<Option<UdpListener>>,

    // Merged discovery unicast: UDP unicast + TCP discovery channel
    discovery_unicast_rx: Mutex<Option<Receiver<IncomingMessage>>>,

    // Merged user unicast: UDP unicast + TCP user data channel
    user_data_unicast_rx: Mutex<Option<Receiver<IncomingMessage>>>,
}

impl HybridTransportPlugin {
    pub(crate) fn new(
        domain_id: u32,
        mut participant_id: u32,
        bind_ip: String,
        multicast_if_ip: String,
        working_ips: Vec<String>,
        guid_prefix: GuidPrefix,
        hybrid_config: HybridConfig,
    ) -> io::Result<Self> {
        let udp_sender = UdpSender::new(bind_ip.clone(), multicast_if_ip, hybrid_config.udp)?;

        // Multicast first (domain-wide port, no per-participant collision).
        let discovery_mc_port = PortManager::get_discovery_traffic_multicast_port(domain_id);
        let discovery_mc = UdpListener::new_multicast(discovery_mc_port, &working_ips).ok();

        // UDP unicast: retry on AddrInUse with incremented participant_id, just
        // like UdpTransportPlugin and ShmTransportPlugin. Without this, two
        // Hybrid processes on the same host both bind (domain, pid=0)'s UDP
        // unicast ports, the second `.ok()` swallows the conflict, and
        // discovery silently fails. (The TCP listen port is independent of
        // participant_id; for several participants on one host, pin a distinct
        // int2dds.transport.TCPv4.bind_port per participant.)
        let (discovery_uc, user_uc) = loop {
            let disc_port =
                PortManager::get_discovery_traffic_unicast_port(domain_id, participant_id);
            let user_port = PortManager::get_user_traffic_unicast_port(domain_id, participant_id);
            match UdpListener::new(disc_port) {
                Ok(disc_listener) => {
                    let user_listener = UdpListener::new(user_port).ok();
                    break (Some(disc_listener), user_listener);
                }
                Err(_) => {
                    log::info!(
                        "[HybridTransportPlugin] UDP port {} in use, trying participant_id {}",
                        disc_port,
                        participant_id + 1
                    );
                    participant_id += 1;
                }
            }
        };

        // Create TCP plugin (handles its own mux listener thread).
        // Pass the final participant_id so TCP's identity matches UDP's. The TCP
        // listen port comes from the per-participant TcpConfig (bind_port
        // property, else the domain formula).
        let tcp_plugin = TcpTransportPlugin::new(
            domain_id,
            participant_id,
            bind_ip,
            working_ips.clone(),
            guid_prefix,
            hybrid_config.tcp,
        )?;

        // Create merged discovery unicast channel: UDP listener + TCP discovery rx
        let (disc_merged_tx, disc_merged_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);
        // Create merged user unicast channel: UDP listener + TCP user data rx
        let (user_merged_tx, user_merged_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);

        // Spawn merge thread for discovery unicast
        let tcp_disc_source = tcp_plugin.take_discovery_unicast_source();
        let disc_tx = disc_merged_tx.clone();
        if let Some(udp_listener) = discovery_uc {
            thread::Builder::new()
                .name("hybrid_discovery_merge".to_string())
                .spawn(move || {
                    merge_udp_and_channel(udp_listener, tcp_disc_source, disc_tx);
                })
                .expect("Failed to create hybrid discovery merge thread");
        } else if let Some(MessageSource::Channel { rx }) = tcp_disc_source {
            thread::Builder::new()
                .name("hybrid_discovery_forward".to_string())
                .spawn(move || {
                    forward_channel(rx, disc_merged_tx);
                })
                .expect("Failed to create hybrid discovery forward thread");
        }

        // Spawn merge thread for user unicast
        let tcp_user_source = tcp_plugin.take_user_data_unicast_source();
        if let Some(udp_listener) = user_uc {
            thread::Builder::new()
                .name("hybrid_user_merge".to_string())
                .spawn(move || {
                    merge_udp_and_channel(udp_listener, tcp_user_source, user_merged_tx);
                })
                .expect("Failed to create hybrid user merge thread");
        } else if let Some(MessageSource::Channel { rx }) = tcp_user_source {
            thread::Builder::new()
                .name("hybrid_user_forward".to_string())
                .spawn(move || {
                    forward_channel(rx, user_merged_tx);
                })
                .expect("Failed to create hybrid user forward thread");
        }

        info!("[HybridTransportPlugin] Created (domain={}, pid={})", domain_id, participant_id);

        Ok(Self {
            udp_sender,
            tcp_plugin,
            domain_id,
            participant_id,
            working_ips,
            discovery_multicast_listener: Mutex::new(discovery_mc),
            discovery_unicast_rx: Mutex::new(Some(disc_merged_rx)),
            user_data_unicast_rx: Mutex::new(Some(user_merged_rx)),
        })
    }

    /// Expand a single UDP port into per-NIC IPv4 locators using the
    /// plugin's `working_ips`. `INT2DDS_EXTERNAL_ADDRESS` (when set) replaces
    /// every NIC IP with a single advertised IP — matches develop's
    /// `init_locators` behavior so the env override remains effective.
    fn udp_locators(&self, port: u32) -> Vec<Locator> {
        let mut locators = Vec::new();
        if let Some(ext_ip) = crate::common::env::get_external_address() {
            locators.push(Locator::from_ip_v4_addr_and_port(&ext_ip, port));
            return locators;
        }
        for ip_str in &self.working_ips {
            if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                locators.push(Locator::from_ip_v4_addr_and_port(&ip, port));
            }
        }
        locators
    }
}

impl TransportPlugin for HybridTransportPlugin {
    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        match target {
            SendTarget::SPDPDiscovery { initial_peers } => {
                let _ = self.udp_sender.send_multicast(self.domain_id, data);
                // initial_peers fan-out: Hybrid reaches them over both UDP and
                // TCP so peers reachable on either transport get the SPDP.
                for peer_addr in *initial_peers {
                    let _ = self.udp_sender.send(peer_addr, data);
                }
                let _ = self.tcp_plugin.send(data, target);
                Ok(())
            }
            SendTarget::SEDPDiscovery(locator) => {
                if locator.is_tcp() {
                    self.tcp_plugin.send(data, target)
                } else if locator.is_udp() {
                    let ip = locator.to_ip_v4_addr();
                    let port = locator.port() as u16;
                    let addr = SocketAddr::new(IpAddr::V4(ip), port);
                    self.udp_sender.send(&addr, data)?;
                    Ok(())
                } else {
                    Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()))
                }
            }
            SendTarget::UserData(locator) => {
                if locator.is_tcp() {
                    self.tcp_plugin.send(data, target)
                } else if locator.is_udp() {
                    let ip = locator.to_ip_v4_addr();
                    let port = locator.port() as u16;
                    let addr = SocketAddr::new(IpAddr::V4(ip), port);
                    self.udp_sender.send(&addr, data)?;
                    Ok(())
                } else {
                    Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()))
                }
            }
        }
    }

    fn can_handle(&self, locator: &Locator) -> bool {
        // Hybrid carries both UDP and TCP senders.
        locator.is_udp() || locator.is_tcp()
    }

    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
        let udp_port =
            PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id)
                as u32;
        let mut locators = self.udp_locators(udp_port);
        // Hybrid advertises both UDP and TCP endpoints so peers on either
        // transport can reach us.
        locators.extend(self.tcp_plugin.advertised_metatraffic_unicast_locators());
        locators
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        let udp_port =
            PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id) as u32;
        let mut locators = self.udp_locators(udp_port);
        locators.extend(self.tcp_plugin.advertised_default_unicast_locators());
        locators
    }

    fn take_discovery_multicast_source(&self) -> Option<MessageSource> {
        let listener = self.discovery_multicast_listener.lock().expect("lock poisoned").take()?;
        Some(MessageSource::MioPoll { listener })
    }

    fn take_discovery_unicast_source(&self) -> Option<MessageSource> {
        let rx = self.discovery_unicast_rx.lock().expect("lock poisoned").take()?;
        Some(MessageSource::Channel { rx })
    }

    fn take_user_data_unicast_source(&self) -> Option<MessageSource> {
        let rx = self.user_data_unicast_rx.lock().expect("lock poisoned").take()?;
        Some(MessageSource::Channel { rx })
    }

    fn port(&self) -> u16 {
        self.udp_sender.port()
    }

    fn tcp_listener_port(&self) -> Option<u16> {
        self.tcp_plugin.tcp_listener_port()
    }

    fn participant_id(&self) -> u32 {
        self.participant_id
    }

    fn close(&self) {
        self.udp_sender.force_close();
        self.tcp_plugin.close();

        if let Ok(mut guard) = self.discovery_multicast_listener.lock() {
            if let Some(mut listener) = guard.take() {
                listener.close();
            }
        }

        debug!("[HybridTransportPlugin] Closed");
    }
}

/// Merge UDP listener and channel source into a single output channel.
fn merge_udp_and_channel(
    mut udp_listener: UdpListener,
    channel_source: Option<MessageSource>,
    tx: crossbeam_channel::Sender<IncomingMessage>,
) {
    use mio::{Events, Interest, Poll, Token};
    use std::time::Duration;

    let channel_rx = match channel_source {
        Some(MessageSource::Channel { rx }) => Some(rx),
        _ => None,
    };

    let mut poll = match Poll::new() {
        Ok(p) => p,
        Err(e) => {
            warn!("[HybridMerge] Failed to create poll: {:?}", e);
            return;
        }
    };
    let mut events = Events::with_capacity(64);
    let udp_token = Token(1);
    if let Err(e) = poll.registry().register(udp_listener.socket(), udp_token, Interest::READABLE) {
        warn!("[HybridMerge] Failed to register UDP listener: {:?}", e);
        return;
    }

    loop {
        let _ = poll.poll(&mut events, Some(Duration::from_millis(50)));

        // Drain UDP
        for event in events.iter() {
            if event.token() == udp_token && event.is_readable() {
                while let Some((buffer, from_addr)) = udp_listener.get_message() {
                    let msg = IncomingMessage { data: buffer.to_vec(), source: from_addr };
                    if tx.try_send(msg).is_err() {
                        return; // Channel closed
                    }
                }
            }
        }

        // Drain TCP channel
        if let Some(rx) = &channel_rx {
            while let Ok(msg) = rx.try_recv() {
                if tx.try_send(msg).is_err() {
                    return;
                }
            }
        }

        // Check if output channel is disconnected
        if tx.is_empty() && tx.len() == 0 {
            // Can't easily check disconnection; rely on try_send errors above
        }
    }
}

/// Forward messages from one channel to another.
fn forward_channel(
    rx: crossbeam_channel::Receiver<IncomingMessage>,
    tx: crossbeam_channel::Sender<IncomingMessage>,
) {
    while let Ok(msg) = rx.recv() {
        if tx.try_send(msg).is_err() {
            return;
        }
    }
}
