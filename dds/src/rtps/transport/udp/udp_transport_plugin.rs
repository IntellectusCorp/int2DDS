#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::{HashMap, HashSet};
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
    user_unicast_listener: Mutex<Option<UdpListener>>,

    // One listener per joined group, created on demand and handed out once.
    // `joined_user_multicast_groups` outlives the handout, so a group that
    // already has a listening thread is not opened a second time.
    user_multicast_listeners: Mutex<HashMap<Ipv4Addr, UdpListener>>,
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
            user_unicast_listener: Mutex::new(user_uc),
            user_multicast_listeners: Mutex::new(HashMap::new()),
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

    fn take_user_data_multicast_source(&self) -> Option<(Locator, MessageSource)> {
        let mut listeners = self.user_multicast_listeners.lock().expect("lock poisoned");
        let group = *listeners.keys().next()?;
        let listener = listeners.remove(&group)?;
        let port = PortManager::get_user_traffic_multicast_port(self.domain_id) as u32;
        Some((Locator::from_ip_v4_addr_and_port(&group, port), MessageSource::MioPoll { listener }))
    }

    fn ensure_user_multicast_listener(&self, group: Ipv4Addr) -> io::Result<()> {
        let mut joined = self.joined_user_multicast_groups.lock().expect("lock poisoned");
        if joined.contains(&group) {
            return Ok(());
        }

        // A group nobody can receive on is worse than a refused DataReader, so a
        // failed join on the sending interface is fatal here.
        let user_mc_port = PortManager::get_user_traffic_multicast_port(self.domain_id);
        let listener = UdpListener::new_user_multicast(
            user_mc_port,
            &group,
            &self.working_ips,
            self.multicast_if_ip,
        )?;

        self.user_multicast_listeners.lock().expect("lock poisoned").insert(group, listener);
        joined.insert(group);

        log::info!(
            "[UdpTransportPlugin] user data multicast group {group} listening on port {user_mc_port}"
        );
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
        if let Ok(mut guard) = self.user_multicast_listeners.lock() {
            for (_, mut listener) in guard.drain() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcps::infrastructure::qos_policy::PropertyQosPolicy;
    use crate::rtps::transport::socket::Socket;
    use crate::rtps::transport::transport_config::TransportConfig;
    use crate::test_utils::unique_domain_id;

    fn plugin(domain_id: u32) -> UdpTransportPlugin {
        let socket = Socket::new(domain_id);
        UdpTransportPlugin::new(
            domain_id,
            socket.participant_id(),
            socket.get_sender_bind_addr(),
            socket.get_sender_multicast_if_addr(None),
            socket.working_ips(),
            UdpConfig::from_property(&PropertyQosPolicy::default()),
        )
        .unwrap()
    }

    #[test]
    fn each_group_gets_its_own_labelled_listener() {
        let domain_id = unique_domain_id() as u32;
        let plugin = plugin(domain_id);
        let port = PortManager::get_user_traffic_multicast_port(domain_id) as u32;
        let first = Ipv4Addr::new(239, 255, 13, 1);
        let second = Ipv4Addr::new(239, 255, 13, 2);

        plugin.ensure_user_multicast_listener(first).unwrap();
        plugin.ensure_user_multicast_listener(second).unwrap();

        let mut handed_out: Vec<Locator> = Vec::new();
        while let Some((locator, _source)) = plugin.take_user_data_multicast_source() {
            handed_out.push(locator);
        }
        handed_out.sort_by_key(|locator| locator.to_ip_v4_addr().octets());

        assert_eq!(
            handed_out,
            vec![
                Locator::from_ip_v4_addr_and_port(&first, port),
                Locator::from_ip_v4_addr_and_port(&second, port),
            ],
            "Every group must come back as its own source, labelled with the group it receives"
        );

        plugin.close();
    }

    #[test]
    fn readers_sharing_a_group_share_one_listener() {
        let domain_id = unique_domain_id() as u32;
        let plugin = plugin(domain_id);
        let group = Ipv4Addr::new(239, 255, 13, 3);

        plugin.ensure_user_multicast_listener(group).unwrap();
        assert!(plugin.take_user_data_multicast_source().is_some());

        // Two sockets on one group would each read the same datagram, so a
        // second reader on a group already served must open nothing.
        plugin.ensure_user_multicast_listener(group).unwrap();
        assert!(
            plugin.take_user_data_multicast_source().is_none(),
            "A group that is already listened to must not open a second socket"
        );

        plugin.close();
    }

    #[test]
    fn a_group_that_cannot_be_joined_is_refused() {
        let domain_id = unique_domain_id() as u32;
        let plugin = plugin(domain_id);
        let not_a_group = Ipv4Addr::new(10, 0, 0, 1);

        assert!(plugin.ensure_user_multicast_listener(not_a_group).is_err());
        assert!(
            plugin.take_user_data_multicast_source().is_none(),
            "A refused group must leave no listener behind"
        );

        plugin.close();
    }
}
