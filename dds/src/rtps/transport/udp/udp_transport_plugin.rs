#![allow(dead_code)]
#![allow(unused_variables)]

use socket2::Socket as Socket2;
use std::collections::HashSet;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;

use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::UdpConfig;

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
    multicast_if_ip: Ipv4Addr,

    // Listeners created during construction, taken once during init.
    discovery_multicast_listener: Mutex<Option<UdpListener>>,
    discovery_unicast_listener: Mutex<Option<UdpListener>>,
    user_multicast_listener: Mutex<Option<UdpListener>>,
    user_unicast_listener: Mutex<Option<UdpListener>>,

    user_multicast_group_handle: Mutex<Option<Socket2>>,
    joined_user_multicast_groups: Mutex<HashSet<Ipv4Addr>>,
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
        udp_config: UdpConfig,
    ) -> io::Result<Self> {
        let egress_if: Ipv4Addr = multicast_if_ip.parse().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid multicast interface address '{multicast_if_ip}': {e}"),
            )
        })?;
        let sender = UdpSender::new(bind_ip, multicast_if_ip, udp_config)?;

        let discovery_mc_port = PortManager::get_discovery_traffic_multicast_port(domain_id);
        let discovery_mc =
            UdpListener::new_discovery_multicast(discovery_mc_port, &working_ips, Some(egress_if))
                .ok();

        // Create unicast listeners — both discovery_uc and user_uc must bind at
        // the same participant_id (matches develop's Socket contract). If either
        // fails, close any partial bind, increment participant_id, and retry.
        // Listener stays bound (no probe-and-release race).
        let (discovery_uc, user_uc) = loop {
            let disc_port =
                PortManager::get_discovery_traffic_unicast_port(domain_id, participant_id);
            let user_port = PortManager::get_user_traffic_unicast_port(domain_id, participant_id);

            match UdpListener::new(disc_port) {
                Ok(disc_listener) => match UdpListener::new(user_port) {
                    Ok(user_listener) => break (Some(disc_listener), Some(user_listener)),
                    Err(_) => {
                        log::info!(
                            "[UdpTransportPlugin] User port {} in use, closing discovery and trying participant_id {}",
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
                        "[UdpTransportPlugin] Discovery port {} in use, trying participant_id {}",
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
            multicast_if_ip: egress_if,
            discovery_multicast_listener: Mutex::new(discovery_mc),
            discovery_unicast_listener: Mutex::new(discovery_uc),
            user_multicast_listener: Mutex::new(None),
            user_unicast_listener: Mutex::new(user_uc),
            user_multicast_group_handle: Mutex::new(None),
            joined_user_multicast_groups: Mutex::new(HashSet::new()),
        })
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

    fn advertised_default_multicast_locators(&self, groups: Vec<Ipv4Addr>) -> Vec<Locator> {
        let port = PortManager::get_user_traffic_multicast_port(self.domain_id) as u32;
        groups.iter().map(|group| Locator::from_ip_v4_addr_and_port(group, port)).collect()
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

    fn take_user_data_multicast_source(&self) -> Option<MessageSource> {
        let listener = self.user_multicast_listener.lock().expect("lock poisoned").take()?;
        Some(MessageSource::MioPoll { listener })
    }

    fn ensure_user_multicast_listener(&self, group: Ipv4Addr) -> io::Result<()> {
        let mut handle_guard = self.user_multicast_group_handle.lock().expect("lock poisoned");
        if handle_guard.is_none() {
            let user_mc_port = PortManager::get_user_traffic_multicast_port(self.domain_id);
            let mut listener = UdpListener::new_user_multicast(user_mc_port, &self.working_ips)?;
            *handle_guard = listener.take_multicast_group_handle();
            *self.user_multicast_listener.lock().expect("lock poisoned") = Some(listener);
            log::info!(
                "[UdpTransportPlugin] user data multicast listener bound on port {user_mc_port}"
            );
        }

        let mut joined = self.joined_user_multicast_groups.lock().expect("lock poisoned");
        if !joined.insert(group) {
            return Ok(());
        }

        let handle = handle_guard
            .as_ref()
            .ok_or_else(|| io::Error::other("user data multicast listener has no group handle"))?;
        // A group nobody can receive on is worse than a refused DataReader, so a
        // failed join on the sending interface is fatal here.
        if let Err(e) = UdpListener::join_multicast_group(
            handle,
            &group,
            &self.working_ips,
            Some(self.multicast_if_ip),
            true,
        ) {
            joined.remove(&group);
            return Err(e);
        }

        log::info!("[UdpTransportPlugin] joined user data multicast group {group}");
        Ok(())
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
