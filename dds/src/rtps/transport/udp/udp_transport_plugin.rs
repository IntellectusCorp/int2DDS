#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;

use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::TransportConfig;

/// UDP implementation of the TransportPlugin trait.
///
/// Owns one UdpSender (for all outgoing traffic) and four UdpListeners
/// (discovery multicast/unicast, user multicast/unicast).
/// Each listener is handed out exactly once via `take_*_source()`,
/// wrapped in `MessageSource::MioPoll` for zero-channel-overhead polling.
pub(crate) struct UdpTransportPlugin {
    sender: UdpSender,
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,

    // Listeners created during construction, taken once during init.
    discovery_multicast_listener: Mutex<Option<UdpListener>>,
    discovery_unicast_listener: Mutex<Option<UdpListener>>,
    user_multicast_listener: Mutex<Option<UdpListener>>,
    user_unicast_listener: Mutex<Option<UdpListener>>,
}

impl UdpTransportPlugin {
    /// Create a new UDP transport plugin.
    ///
    /// Creates one sender and four listeners (discovery mc/uc, user mc/uc).
    /// Listeners are created with ports calculated from domain_id and participant_id.
    pub(crate) fn new(
        domain_id: u32,
        mut participant_id: u32,
        bind_ip: String,
        multicast_if_ip: String,
        working_ips: Vec<String>,
        transport_config: TransportConfig,
    ) -> io::Result<Self> {
        let sender = UdpSender::new(bind_ip, multicast_if_ip, transport_config)?;

        // Create multicast listeners (shared ports, no conflict)
        let discovery_mc_port = PortManager::get_discovery_traffic_multicast_port(domain_id);
        let user_mc_port = PortManager::get_user_traffic_multicast_port(domain_id);
        let discovery_mc = UdpListener::new_multicast(discovery_mc_port, &working_ips).ok();
        let user_mc = UdpListener::new_multicast(user_mc_port, &working_ips).ok();

        // Create unicast listeners — if port is in use, increment participant_id and retry.
        // Listener stays bound (no probe-and-release race). Matches old Socket behavior.
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
                        "[UdpTransportPlugin] Port {} in use, trying participant_id {}",
                        disc_port,
                        participant_id + 1
                    );
                    participant_id += 1;
                }
            }
        };

        Ok(Self {
            sender,
            domain_id,
            participant_id,
            working_ips,
            discovery_multicast_listener: Mutex::new(discovery_mc),
            discovery_unicast_listener: Mutex::new(discovery_uc),
            user_multicast_listener: Mutex::new(user_mc),
            user_unicast_listener: Mutex::new(user_uc),
        })
    }

    /// Take the discovery multicast listener (UDP-specific).
    ///
    /// Multicast listening is a UDP concept — not part of TransportPlugin trait.
    /// Called once during initialization by DcpsBridge to create
    /// the DiscoveryMulticastListeningTask.
    pub(crate) fn take_discovery_multicast_listener(&self) -> Option<UdpListener> {
        self.discovery_multicast_listener.lock().expect("lock poisoned").take()
    }

    /// Take the user data multicast listener (UDP-specific).
    ///
    /// Called once during initialization by DcpsBridge to create
    /// the UserMulticastListeningTask.
    pub(crate) fn take_user_multicast_listener(&self) -> Option<UdpListener> {
        self.user_multicast_listener.lock().expect("lock poisoned").take()
    }

    /// Expand a single UDP port into per-NIC IPv4 locators using the
    /// plugin's `working_ips`.
    fn udp_locators(&self, port: u32) -> Vec<Locator> {
        let mut locators = Vec::new();
        for ip_str in &self.working_ips {
            if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                locators.push(Locator::from_ip_v4_addr_and_port(&ip, port));
            }
        }
        locators
    }
}

impl TransportPlugin for UdpTransportPlugin {
    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        match target {
            SendTarget::SPDPDiscovery { initial_peers } => {
                self.sender.send_multicast(self.domain_id, data)?;
                for peer_addr in *initial_peers {
                    let _ = self.sender.send(peer_addr, data);
                }
                Ok(())
            }
            SendTarget::SEDPDiscovery(locator) | SendTarget::UserData(locator) => {
                let ip = locator.to_ip_v4_addr();
                let port = locator.port() as u16;
                let addr = SocketAddr::new(IpAddr::V4(ip), port);
                self.sender.send(&addr, data)?;
                Ok(())
            }
        }
    }

    fn can_handle(&self, locator: &Locator) -> bool {
        locator.is_udp()
    }

    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
        let port = PortManager::get_discovery_traffic_unicast_port(
            self.domain_id,
            self.participant_id,
        ) as u32;
        self.udp_locators(port)
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        let port =
            PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id) as u32;
        self.udp_locators(port)
    }

    fn take_discovery_multicast_source(&self) -> Option<MessageSource> {
        let listener = self.discovery_multicast_listener.lock().expect("lock poisoned").take()?;
        Some(MessageSource::MioPoll { listener })
    }

    fn take_discovery_unicast_source(&self) -> Option<MessageSource> {
        let listener = self.discovery_unicast_listener.lock().expect("lock poisoned").take()?;
        Some(MessageSource::MioPoll { listener })
    }

    fn take_user_data_unicast_source(&self) -> Option<MessageSource> {
        let listener = self.user_unicast_listener.lock().expect("lock poisoned").take()?;
        Some(MessageSource::MioPoll { listener })
    }

    fn port(&self) -> u16 {
        self.sender.port()
    }

    fn participant_id(&self) -> u32 {
        self.participant_id
    }

    fn close(&self) {
        self.sender.force_close();

        if let Ok(mut guard) = self.discovery_multicast_listener.lock() {
            if let Some(mut listener) = guard.take() {
                listener.close();
            }
        }
        if let Ok(mut guard) = self.discovery_unicast_listener.lock() {
            if let Some(mut listener) = guard.take() {
                listener.close();
            }
        }
        if let Ok(mut guard) = self.user_multicast_listener.lock() {
            if let Some(mut listener) = guard.take() {
                listener.close();
            }
        }
        if let Ok(mut guard) = self.user_unicast_listener.lock() {
            if let Some(mut listener) = guard.take() {
                listener.close();
            }
        }
    }
}
