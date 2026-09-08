#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use log::{debug, info};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::shm::runtime::ShmRuntime;
use crate::rtps::transport::shm::shm_listener::ShmListener;
use crate::rtps::transport::shm::shm_sender::ShmSender;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::UdpConfig;

/// Spec §8: an SHM locator names a segment on the host that advertised it, so
/// only our own are reachable. Without this a peer on another machine that
/// also runs SHM is "reached" by writing into our own ring, and the sample
/// is lost -- `user_logic`'s SHM > TCP > UDP filter tries SHM first and
/// relies on `can_handle` to rule out what is not local.
///
/// `ours` must be real NIC addresses, never `INT2DDS_EXTERNAL_ADDRESS`: every
/// host behind the same NAT advertises that same external address, so
/// comparing against it would accept a remote peer's SHM locator as our own
/// and silently drop all user data between the two hosts. This still
/// misjudges when two hosts behind different NATs share a private IP (e.g.
/// both 192.168.1.10) -- closing that requires a host/boot id, not an IP
/// comparison.
///
/// A free function so it is testable without a plugin instance, and so it
/// stays independent of the zero-copy runtime: the legacy SHM data path
/// (`shm_sender`/`shm_listener`/`ring_buffer`) works whether or not
/// `ShmRuntime` started, so gating this on the runtime would turn
/// `INT2DDS_SHM_ZERO_COPY=0` into "no SHM at all" instead of "no zero-copy".
fn shm_locator_is_local(locator: &Locator, ours: &[Ipv4Addr]) -> bool {
    locator.is_shm() && ours.contains(&locator.to_ip_v4_addr())
}

/// SHM transport plugin — UDP for discovery, SHM for user data.
///
/// Discovery (both multicast and unicast) always uses UDP.
/// User data routes by locator kind:
///   - SHM locator → SHM sender (shared memory ring buffer)
///   - UDP locator → UDP sender (fallback for non-SHM peers)
///
/// User data unicast is handed out as `MessageSource::Shm`,
/// letting the listening task poll both UDP and SHM in the same loop
/// (mirroring the develop-branch single-task receive pattern; no extra
/// merge thread or inter-thread channel).
pub(crate) struct ShmTransportPlugin {
    udp_sender: UdpSender,
    shm_sender: ShmSender,
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,
    /// What we advertise as reachable from anywhere: the external address
    /// when one is configured, otherwise our own NIC addresses. UDP locators
    /// use it.
    advertised_ip_addrs: Vec<Ipv4Addr>,
    /// Our real NIC addresses. An SHM segment only exists on this machine,
    /// so these are the only addresses an SHM locator of ours may carry, and
    /// the only ones we accept in one. Never the external address: every
    /// host behind the same NAT would advertise it and we would take a
    /// remote peer's SHM locator for our own.
    host_ip_addrs: Vec<Ipv4Addr>,
    runtime: Option<Arc<ShmRuntime>>,

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
        guid_prefix: GuidPrefix,
        udp_config: UdpConfig,
    ) -> io::Result<Self> {
        let udp_sender = UdpSender::new(bind_ip, multicast_if_ip, udp_config)?;
        let shm_sender = ShmSender::new(domain_id)?;
        // UDP-facing set: external override, else each parsed working_ip.
        let advertised_ip_addrs: Vec<Ipv4Addr> = match crate::common::env::get_external_address() {
            Some(ext_ip) => vec![ext_ip],
            None => working_ips.iter().filter_map(|s| s.parse::<Ipv4Addr>().ok()).collect(),
        };
        // SHM-facing set: always the parsed working_ips, never the external
        // override. `can_handle` is on the send path and must not rebuild
        // this `Vec` per call, so it is computed once here.
        let host_ip_addrs: Vec<Ipv4Addr> =
            working_ips.iter().filter_map(|s| s.parse::<Ipv4Addr>().ok()).collect();
        let runtime = ShmRuntime::start(domain_id, guid_prefix);

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
            advertised_ip_addrs,
            host_ip_addrs,
            runtime,
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

    /// The IPs to advertise for UDP locators — `INT2DDS_EXTERNAL_ADDRESS`
    /// override, or the parsed working_ips. SHM locators use `host_ip_addrs`
    /// instead (see its doc): an external address does not identify this
    /// machine, so it must never appear in an SHM locator.
    fn advertised_ips(&self) -> Vec<Ipv4Addr> {
        self.advertised_ip_addrs.clone()
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
        if locator.is_shm() {
            // Independent of the zero-copy runtime: the legacy SHM path works
            // whether or not `ShmRuntime` started, so gating on it would turn
            // `INT2DDS_SHM_ZERO_COPY=0` into "no SHM at all".
            return shm_locator_is_local(locator, &self.host_ip_addrs);
        }
        locator.is_udp()
    }

    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
        let port =
            PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id)
                as u32;
        self.udp_locators(port)
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        let user_port =
            PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id) as u32;
        let mut locators = Vec::new();
        // SHM locators carry our NIC addresses, not the external one: a
        // segment name means nothing off this machine, and every host behind
        // the same NAT advertises the same external address. Order is not
        // load-bearing -- a peer picks by kind, not by position.
        for ip in &self.host_ip_addrs {
            locators.push(Locator::from_shm(ip, user_port));
        }
        for ip in self.advertised_ips() {
            locators.push(Locator::from_ip_v4_addr_and_port(&ip, user_port));
        }
        locators
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
        let shm = self.shm_listener.lock().expect("lock poisoned").take();
        Some(match shm {
            Some(shm) => MessageSource::Shm { listener, shm },
            None => MessageSource::Udp { listener },
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

    fn peer_lost(&self, prefix: GuidPrefix) {
        if let Some(rt) = &self.runtime {
            rt.peer_lost(prefix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shm_locator_is_local_accepts_our_own_ip_and_rejects_others() {
        let ip: Ipv4Addr = "127.0.0.1".parse().unwrap();
        let ours = vec![ip];

        let mine = Locator::from_shm(&ip, 7410);
        let theirs = Locator::from_shm(&"192.168.1.50".parse().unwrap(), 7410);
        let udp = Locator::from_ip_v4_addr_and_port(&ip, 7410);

        assert!(shm_locator_is_local(&mine, &ours));
        assert!(
            !shm_locator_is_local(&theirs, &ours),
            "a remote host's SHM locator must be rejected"
        );
        assert!(!shm_locator_is_local(&udp, &ours), "a UDP locator is not an SHM target");
    }
}
