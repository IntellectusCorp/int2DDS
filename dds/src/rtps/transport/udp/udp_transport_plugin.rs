#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;

use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::UdpConfig;

/// UDP implementation of the TransportPlugin trait.
///
/// Owns one UdpSender (for all outgoing traffic) and four UdpListeners
/// (discovery multicast/unicast, user multicast/unicast).
/// Each listener is handed out exactly once via `take_*_source()`,
/// wrapped in `MessageSource::Udp` for zero-channel-overhead polling.
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
    discovery_unicast_port: u16,
    user_unicast_port: u16,

    // Captured before the unicast listeners are taken (see advertised_receive_buffer_size).
    advertised_receive_buffer_size: Option<usize>,
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
        udp_config: UdpConfig,
    ) -> io::Result<Self> {
        let sender = UdpSender::new(bind_ip, multicast_if_ip, udp_config)?;
        let port_manager = udp_config.port_manager;

        // Create multicast listeners (shared ports, no conflict)
        let discovery_mc_port =
            port_manager.calculate_discovery_traffic_multicast_port(domain_id)?;
        let user_mc_port = port_manager.calculate_user_traffic_multicast_port(domain_id)?;
        let discovery_mc = Some(UdpListener::new_multicast(discovery_mc_port, &working_ips)?);
        let user_mc = Some(UdpListener::new_multicast(user_mc_port, &working_ips)?);

        // Create unicast listeners — both discovery_uc and user_uc must bind at
        // the same participant_id (matches develop's Socket contract).
        // Listener stays bound (no probe-and-release race).
        let (first_participant_id, is_participant_id_fixed) = match udp_config.participant_id {
            // int2dds.participant_id property: only this id is tried
            Some(fixed_participant_id) => (fixed_participant_id, true),
            // No property: search upwards from the id the caller passed in (0 from Socket)
            None => (participant_id, false),
        };
        let (discovery_uc, user_uc, participant_id) =
            UdpListener::bind_discovery_and_user_unicast_listeners(
                &port_manager,
                domain_id,
                first_participant_id,
                is_participant_id_fixed,
            )?;
        let discovery_unicast_port = discovery_uc.port();
        let user_unicast_port = user_uc.port();

        // The two unicast sockets are bound moments apart under the same sysctl,
        // so they normally agree; take the minimum in case a port-conflict retry
        // left them different.
        let advertised_receive_buffer_size =
            [&discovery_uc, &user_uc].into_iter().filter_map(UdpListener::recv_buffer_size).min();

        log::info!(
            "[UdpTransportPlugin] Created (domain={}, pid={}, discovery_port={}, user_port={})",
            domain_id,
            participant_id,
            discovery_unicast_port,
            user_unicast_port
        );

        Ok(Self {
            sender,
            domain_id,
            participant_id,
            working_ips,
            discovery_multicast_listener: Mutex::new(discovery_mc),
            discovery_unicast_listener: Mutex::new(Some(discovery_uc)),
            user_multicast_listener: Mutex::new(user_mc),
            user_unicast_listener: Mutex::new(Some(user_uc)),
            discovery_unicast_port,
            user_unicast_port,
            advertised_receive_buffer_size,
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
    ///
    /// `INT2DDS_EXTERNAL_ADDRESS` (when set) replaces every NIC IP with a
    /// single advertised IP. Matches develop's `init_locators` behavior so
    /// the env override remains effective in UDP mode.
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

impl TransportPlugin for UdpTransportPlugin {
    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        match target {
            SendTarget::SPDPDiscovery { initial_peers } => {
                // Discovery always uses UDP multicast, plus fan-out to initial_peers.
                let _ = self.sender.send_multicast(self.domain_id, data);
                for peer_addr in *initial_peers {
                    let _ = self.sender.send(peer_addr, data);
                }
                Ok(())
            }
            SendTarget::SEDPDiscovery(locator) | SendTarget::UserData(locator) => {
                // Encode the rejected locator's kind in the error message so
                // the RTPS layer can format "X locator found but no X sender
                // available" without needing to branch on the kind itself.
                if !locator.is_udp() {
                    return Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()));
                }
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
        self.udp_locators(self.discovery_unicast_port as u32)
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        self.udp_locators(self.user_unicast_port as u32)
    }

    fn advertised_receive_buffer_size(&self) -> Option<usize> {
        self.advertised_receive_buffer_size
    }

    fn take_discovery_multicast_source(&self) -> Option<MessageSource> {
        let listener = self.discovery_multicast_listener.lock().expect("lock poisoned").take()?;
        Some(MessageSource::Udp { listener })
    }

    fn take_discovery_unicast_source(&self) -> Option<MessageSource> {
        let listener = self.discovery_unicast_listener.lock().expect("lock poisoned").take()?;
        Some(MessageSource::Udp { listener })
    }

    fn take_user_data_unicast_source(&self) -> Option<MessageSource> {
        let listener = self.user_unicast_listener.lock().expect("lock poisoned").take()?;
        Some(MessageSource::Udp { listener })
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
