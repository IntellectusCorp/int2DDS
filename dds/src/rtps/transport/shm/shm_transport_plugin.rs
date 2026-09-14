#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;

use log::{debug, info};

use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::UdpConfig;

/// SHM transport plugin. Discovery and user data both travel on UDP.
pub(crate) struct ShmTransportPlugin {
    udp_sender: UdpSender,
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,

    // UDP listeners — discovery is always UDP
    discovery_multicast_listener: Mutex<Option<UdpListener>>,
    discovery_unicast_listener: Mutex<Option<UdpListener>>,

    user_multicast_listener: Mutex<Option<UdpListener>>,
    user_unicast_listener: Mutex<Option<UdpListener>>,
}

impl ShmTransportPlugin {
    pub(crate) fn new(
        domain_id: u32,
        mut participant_id: u32,
        bind_ip: String,
        multicast_if_ip: String,
        working_ips: Vec<String>,
        udp_config: UdpConfig,
    ) -> io::Result<Self> {
        let udp_sender = UdpSender::new(bind_ip, multicast_if_ip, udp_config)?;

        // Create UDP multicast listeners (shared ports, no per-pid collision).
        let discovery_mc_port = PortManager::get_discovery_traffic_multicast_port(domain_id);
        let user_mc_port = PortManager::get_user_traffic_multicast_port(domain_id);
        let discovery_mc = UdpListener::new_multicast(discovery_mc_port, &working_ips).ok();
        let user_mc = UdpListener::new_multicast(user_mc_port, &working_ips).ok();

        // Unicast listeners — both must bind at the same participant_id
        // (matches develop's Socket contract). If either fails, close any
        // partial bind, increment participant_id, and retry.
        let (discovery_uc, user_uc) = loop {
            let disc_port =
                PortManager::get_discovery_traffic_unicast_port(domain_id, participant_id);
            let user_port = PortManager::get_user_traffic_unicast_port(domain_id, participant_id);
            match UdpListener::new(disc_port) {
                Ok(disc_listener) => match UdpListener::new(user_port) {
                    Ok(user_listener) => break (Some(disc_listener), Some(user_listener)),
                    Err(_) => {
                        log::info!(
                            "[ShmTransportPlugin] User port {} in use, closing discovery and trying participant_id {}",
                            user_port,
                            participant_id + 1
                        );
                        let mut disc_listener = disc_listener;
                        disc_listener.close();
                        participant_id += 1;
                    }
                },
                Err(_) => {
                    log::info!(
                        "[ShmTransportPlugin] Discovery port {} in use, trying participant_id {}",
                        disc_port,
                        participant_id + 1
                    );
                    participant_id += 1;
                }
            }
        };
        info!("[ShmTransportPlugin] Created (domain={}, pid={})", domain_id, participant_id);

        Ok(Self {
            udp_sender,
            domain_id,
            participant_id,
            working_ips,
            discovery_multicast_listener: Mutex::new(discovery_mc),
            discovery_unicast_listener: Mutex::new(discovery_uc),
            user_multicast_listener: Mutex::new(user_mc),
            user_unicast_listener: Mutex::new(user_uc),
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

impl TransportPlugin for ShmTransportPlugin {
    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        match target {
            SendTarget::SPDPDiscovery { initial_peers } => {
                // Discovery always uses UDP multicast, plus fan-out to initial_peers.
                let _ = self.udp_sender.send_multicast(self.domain_id, data);
                for peer_addr in *initial_peers {
                    let _ = self.udp_sender.send(peer_addr, data);
                }
                Ok(())
            }
            SendTarget::SEDPDiscovery(locator) => {
                // SHM mode runs discovery over UDP only. Foreign kinds emit
                // Unsupported with the rejected kind's name so the RTPS layer
                // can format a "X locator found but no X sender" log without
                // per-kind branching.
                if !locator.is_udp() {
                    return Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()));
                }
                let ip = locator.to_ip_v4_addr();
                let port = locator.port() as u16;
                let addr = SocketAddr::new(IpAddr::V4(ip), port);
                self.udp_sender.send(&addr, data)?;
                Ok(())
            }
            SendTarget::UserData(locator) => {
                if !locator.is_udp() {
                    return Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()));
                }
                let ip = locator.to_ip_v4_addr();
                let port = locator.port() as u16;
                let addr = SocketAddr::new(IpAddr::V4(ip), port);
                self.udp_sender.send(&addr, data)?;
                Ok(())
            }
        }
    }

    fn can_handle(&self, locator: &Locator) -> bool {
        locator.is_udp()
    }

    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
        let port =
            PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id)
                as u32;
        self.udp_locators(port)
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        let port =
            PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id) as u32;
        self.udp_locators(port)
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
        self.udp_sender.port()
    }

    fn participant_id(&self) -> u32 {
        self.participant_id
    }

    fn close(&self) {
        self.udp_sender.force_close();

        for guard_arc in [
            &self.discovery_multicast_listener,
            &self.discovery_unicast_listener,
            &self.user_multicast_listener,
            &self.user_unicast_listener,
        ] {
            if let Ok(mut guard) = guard_arc.lock() {
                if let Some(mut listener) = guard.take() {
                    listener.close();
                }
            }
        }
        debug!("[ShmTransportPlugin] Closed");
    }
}
