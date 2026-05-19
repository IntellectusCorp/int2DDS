#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;

use log::{debug, info};

use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::shm::shm_listener::ShmListener;
use crate::rtps::transport::shm::shm_sender::ShmSender;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::TransportConfig;

/// SHM transport plugin — UDP for discovery, SHM for user data.
///
/// Discovery (both multicast and unicast) always uses UDP.
/// User data routes by locator kind:
///   - SHM locator → SHM sender (shared memory ring buffer)
///   - UDP locator → UDP sender (fallback for non-SHM peers)
///
/// User data unicast is handed out as `MessageSource::MioPollWithShm`,
/// letting the listening task poll both UDP and SHM in the same loop
/// (mirroring the develop-branch single-task receive pattern; no extra
/// merge thread or inter-thread channel).
pub(crate) struct ShmTransportPlugin {
    udp_sender: UdpSender,
    shm_sender: ShmSender,
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,

    // UDP listeners — discovery is always UDP
    discovery_multicast_listener: Mutex<Option<UdpListener>>,
    discovery_unicast_listener: Mutex<Option<UdpListener>>,

    // User-traffic listeners (UDP fallback + SHM ring buffer)
    user_multicast_listener: Mutex<Option<UdpListener>>,
    user_unicast_listener: Mutex<Option<UdpListener>>,
    shm_listener: Mutex<Option<ShmListener>>,
}

impl ShmTransportPlugin {
    pub(crate) fn new(
        domain_id: u32,
        mut participant_id: u32,
        bind_ip: String,
        multicast_if_ip: String,
        working_ips: Vec<String>,
        transport_config: TransportConfig,
    ) -> io::Result<Self> {
        let udp_sender = UdpSender::new(bind_ip, multicast_if_ip, transport_config)?;
        let shm_sender = ShmSender::new(domain_id)?;

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
        let shm_listener = ShmListener::new(domain_id).ok();

        info!(
            "[ShmTransportPlugin] Created (domain={}, pid={}, shm_available={})",
            domain_id,
            participant_id,
            shm_sender.is_available()
        );

        Ok(Self {
            udp_sender,
            shm_sender,
            domain_id,
            participant_id,
            working_ips,
            discovery_multicast_listener: Mutex::new(discovery_mc),
            discovery_unicast_listener: Mutex::new(discovery_uc),
            user_multicast_listener: Mutex::new(user_mc),
            user_unicast_listener: Mutex::new(user_uc),
            shm_listener: Mutex::new(shm_listener),
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

    /// Resolve the IPs to advertise — `INT2DDS_EXTERNAL_ADDRESS` override, or
    /// the parsed working_ips. Used by `advertised_default_unicast_locators`
    /// to emit per-NIC SHM + UDP locators in develop's order.
    fn advertised_ips(&self) -> Vec<Ipv4Addr> {
        if let Some(ext_ip) = crate::common::env::get_external_address() {
            return vec![ext_ip];
        }
        self.working_ips.iter().filter_map(|s| s.parse::<Ipv4Addr>().ok()).collect()
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
                if locator.is_shm() {
                    let dummy_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
                    self.shm_sender.send(&dummy_addr, data)?;
                    Ok(())
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
        // SHM plugin owns both a SHM ring buffer and a UDP fallback for
        // non-SHM peers, so it claims both kinds.
        locator.is_shm() || locator.is_udp()
    }

    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
        let port =
            PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id)
                as u32;
        self.udp_locators(port)
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        // Mirrors develop's `add_locators_for_ip` for SHM mode: for every
        // advertised IP (env override or each working_ip), emit an SHM
        // locator and a UDP locator at user_port — SHM first so peers'
        // SHM > UDP priority filter picks SHM when reachable.
        let user_port =
            PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id) as u32;
        let mut locators = Vec::new();
        for ip in self.advertised_ips() {
            locators.push(Locator::from_shm(&ip, user_port));
            locators.push(Locator::from_ip_v4_addr_and_port(&ip, user_port));
        }
        locators
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
        let shm = self.shm_listener.lock().expect("lock poisoned").take();
        Some(match shm {
            Some(shm) => MessageSource::MioPollWithShm { listener, shm },
            None => MessageSource::MioPoll { listener },
        })
    }

    fn port(&self) -> u16 {
        self.udp_sender.port()
    }

    fn participant_id(&self) -> u32 {
        self.participant_id
    }

    fn close(&self) {
        self.udp_sender.force_close();
        self.shm_sender.force_close();

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
        if let Ok(mut guard) = self.shm_listener.lock() {
            if let Some(mut listener) = guard.take() {
                listener.close();
            }
        }

        debug!("[ShmTransportPlugin] Closed");
    }
}
