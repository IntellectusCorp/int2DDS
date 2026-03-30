use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;

use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;

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
        participant_id: u32,
        bind_ip: String,
        multicast_if_ip: String,
        working_ips: Vec<String>,
    ) -> io::Result<Self> {
        let sender = UdpSender::new(bind_ip, multicast_if_ip)?;

        // Create all four listeners
        let discovery_mc_port = PortManager::get_discovery_traffic_multicast_port(domain_id);
        let discovery_uc_port =
            PortManager::get_discovery_traffic_unicast_port(domain_id, participant_id);
        let user_mc_port = PortManager::get_user_traffic_multicast_port(domain_id);
        let user_uc_port = PortManager::get_user_traffic_unicast_port(domain_id, participant_id);

        let discovery_mc = UdpListener::new_multicast(discovery_mc_port, &working_ips).ok();
        let discovery_uc = UdpListener::new(discovery_uc_port).ok();
        let user_mc = UdpListener::new_multicast(user_mc_port, &working_ips).ok();
        let user_uc = UdpListener::new(user_uc_port).ok();

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
    pub(crate) fn take_discovery_multicast_listener(&mut self) -> Option<UdpListener> {
        self.discovery_multicast_listener.lock().expect("lock poisoned").take()
    }

    /// Take the user data multicast listener (UDP-specific).
    ///
    /// Called once during initialization by DcpsBridge to create
    /// the UserMulticastListeningTask.
    pub(crate) fn take_user_multicast_listener(&mut self) -> Option<UdpListener> {
        self.user_multicast_listener.lock().expect("lock poisoned").take()
    }
}

impl TransportPlugin for UdpTransportPlugin {
    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        use crate::rtps::transport::Transport;

        match target {
            SendTarget::MulticastDiscovery => {
                self.sender.send_multicast(self.domain_id, data)?;
                Ok(())
            }
            SendTarget::UnicastDiscovery(locator) | SendTarget::UserData(locator) => {
                let ip = locator.to_ip_v4_addr();
                let port = locator.port() as u16;
                let addr = SocketAddr::new(IpAddr::V4(ip), port);
                self.sender.send(&addr, data)?;
                Ok(())
            }
        }
    }

    fn local_locators(&self, domain_id: u32, participant_id: u32) -> Vec<Locator> {
        let metatraffic_port =
            PortManager::get_discovery_traffic_unicast_port(domain_id, participant_id) as u32;
        let user_port =
            PortManager::get_user_traffic_unicast_port(domain_id, participant_id) as u32;

        let mut locators = Vec::new();
        for ip_str in &self.working_ips {
            if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                // metatraffic (discovery) locator
                locators.push(Locator::from_ip_v4_addr_and_port(&ip, metatraffic_port));
                // default (user data) locator
                locators.push(Locator::from_ip_v4_addr_and_port(&ip, user_port));
            }
        }
        locators
    }

    fn take_discovery_source(&mut self) -> MessageSource {
        let listener = self
            .discovery_unicast_listener
            .lock()
            .expect("lock poisoned")
            .take()
            .expect("discovery_unicast_listener already taken");
        MessageSource::MioPoll { listener }
    }

    fn take_user_data_source(&mut self) -> MessageSource {
        let listener = self
            .user_unicast_listener
            .lock()
            .expect("lock poisoned")
            .take()
            .expect("user_unicast_listener already taken");
        MessageSource::MioPoll { listener }
    }

    fn port(&self) -> u16 {
        self.sender.port()
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
