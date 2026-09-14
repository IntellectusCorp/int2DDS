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
use crate::rtps::transport::shm::ring::{RingError, SPILL_NONE};
use crate::rtps::transport::shm::runtime::ShmRuntime;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::UdpConfig;

/// An SHM locator names a segment on the host that advertised it, so only
/// those carrying one of our own NIC addresses are reachable. `ours` is never
/// `INT2DDS_EXTERNAL_ADDRESS`: every host behind one NAT advertises the same
/// external address.
fn shm_locator_is_local(locator: &Locator, ours: &[Ipv4Addr]) -> bool {
    locator.is_shm() && ours.contains(&locator.to_ip_v4_addr())
}

/// SHM transport plugin: discovery on UDP; user data as a slot descriptor
/// through `send_to_peer` toward an SHM locator, as bytes over UDP otherwise.
pub(crate) struct ShmTransportPlugin {
    udp_sender: UdpSender,
    domain_id: u32,
    participant_id: u32,
    /// The external address when configured, else our NIC addresses. UDP
    /// locators carry these.
    advertised_ip_addrs: Vec<Ipv4Addr>,
    /// Our NIC addresses. SHM locators carry these, and only these are accepted.
    host_ip_addrs: Vec<Ipv4Addr>,
    runtime: Option<Arc<ShmRuntime>>,

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
        guid_prefix: GuidPrefix,
        udp_config: UdpConfig,
    ) -> io::Result<Self> {
        let udp_sender = UdpSender::new(bind_ip, multicast_if_ip, udp_config)?;
        let host_ip_addrs: Vec<Ipv4Addr> =
            working_ips.iter().filter_map(|s| s.parse::<Ipv4Addr>().ok()).collect();
        let advertised_ip_addrs = match crate::common::env::get_external_address() {
            Some(ext_ip) => vec![ext_ip],
            None => host_ip_addrs.clone(),
        };
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
        info!(
            "[ShmTransportPlugin] Created (domain={}, pid={}, zero_copy={})",
            domain_id,
            participant_id,
            runtime.is_some()
        );

        Ok(Self {
            udp_sender,
            domain_id,
            participant_id,
            advertised_ip_addrs,
            host_ip_addrs,
            runtime,
            discovery_multicast_listener: Mutex::new(discovery_mc),
            discovery_unicast_listener: Mutex::new(discovery_uc),
            user_multicast_listener: Mutex::new(user_mc),
            user_unicast_listener: Mutex::new(user_uc),
        })
    }

    fn udp_locators(&self, port: u32) -> Vec<Locator> {
        self.advertised_ip_addrs
            .iter()
            .map(|ip| Locator::from_ip_v4_addr_and_port(ip, port))
            .collect()
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
            SendTarget::SEDPDiscovery(locator) | SendTarget::UserData(locator) => {
                // Foreign kinds emit Unsupported with the rejected kind's name so
                // the RTPS layer can log it without per-kind branching.
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

    /// Reports why it failed through `ErrorKind` and counts nothing: only the
    /// caller knows when every locator has refused. `NotFound` is `PeerGone`,
    /// `WouldBlock` is `RingFull`, `Unsupported` means no zero-copy path.
    fn send_to_peer(
        &self,
        data: &[u8],
        locator: &Locator,
        dst_prefix: GuidPrefix,
    ) -> io::Result<()> {
        if !shm_locator_is_local(locator, &self.host_ip_addrs) {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "not a local shm locator"));
        }
        let Some(rt) = &self.runtime else {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "no zero-copy runtime"));
        };
        let Some(peer) = rt.peers().resolve(rt.own(), rt.registry(), dst_prefix) else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "peer gone"));
        };
        // A first `Preempted` means the payload was never published and one
        // more try publishes it; a second one means the ring is unusable.
        let mut pushed = peer.push_and_signal(data, SPILL_NONE);
        if pushed == Err(RingError::Preempted) {
            pushed = peer.push_and_signal(data, SPILL_NONE);
        }
        pushed.map_err(|e| io::Error::new(io::ErrorKind::WouldBlock, format!("{e:?}")))
    }

    fn can_handle(&self, locator: &Locator) -> bool {
        if locator.is_shm() {
            // Without the runtime the locator names a segment nothing reads.
            return self.runtime.is_some() && shm_locator_is_local(locator, &self.host_ip_addrs);
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
        // Advertised only with the runtime up: the locator promises a
        // descriptor will be read.
        if self.runtime.is_some() {
            for ip in &self.host_ip_addrs {
                locators.push(Locator::from_shm(ip, user_port));
            }
        }
        locators.extend(self.udp_locators(user_port));
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

    fn peer_lost(&self, prefix: GuidPrefix) {
        if let Some(rt) = &self.runtime {
            rt.peer_lost(prefix);
        }
    }

    fn shm_runtime(&self) -> Option<Arc<ShmRuntime>> {
        self.runtime.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::registry::{now_tick, unlink_registry};
    use crate::rtps::transport::shm::ring::RING_INLINE;
    use crate::rtps::transport::shm::runtime::{DEFAULT_CLASSES, DEFAULT_RING_ENTRIES};
    use crate::rtps::transport::shm::segment::{unlink_segment, OwnedSegment};

    /// Low enough that the UDP ports the plugin binds stay inside `u16`.
    const DOMAIN: u32 = 229;

    /// `INT2DDS_SHM_ZERO_COPY` is process-wide, so only one test at a time may move it.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn plugin(domain: u32, prefix: GuidPrefix) -> ShmTransportPlugin {
        ShmTransportPlugin::new(
            domain,
            0,
            "127.0.0.1".to_string(),
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            prefix,
            UdpConfig { multicast_ttl: 1 },
        )
        .unwrap()
    }

    /// Without a runtime the plugin must look like a UDP transport to both sides.
    #[test]
    fn without_a_zero_copy_runtime_the_plugin_neither_advertises_nor_accepts_shm() {
        const DISABLED_DOMAIN: u32 = 228;
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        unlink_registry(DISABLED_DOMAIN);
        unsafe { std::env::set_var("INT2DDS_SHM_ZERO_COPY", "0") };
        let plugin = plugin(DISABLED_DOMAIN, [78; 12]);
        unsafe { std::env::remove_var("INT2DDS_SHM_ZERO_COPY") };

        assert!(plugin.shm_runtime().is_none());
        let shm = Locator::from_shm(&Ipv4Addr::new(127, 0, 0, 1), 7411);
        assert!(!plugin.can_handle(&shm));
        assert!(plugin.advertised_default_unicast_locators().iter().all(|l| !l.is_shm()));
        unlink_registry(DISABLED_DOMAIN);
    }

    #[test]
    fn send_to_peer_lands_in_the_peers_ring_and_refuses_a_foreign_locator() {
        const OWN_PREFIX: GuidPrefix = [76; 12];
        const PEER_PREFIX: GuidPrefix = [77; 12];

        unlink_registry(DOMAIN);
        let plugin = plugin(DOMAIN, OWN_PREFIX);
        let rt = plugin.shm_runtime().expect("the zero-copy runtime must be up");
        let own_slot = rt.slot();

        let (peer_slot, peer_epoch) =
            rt.registry().claim(std::process::id(), PEER_PREFIX, now_tick()).unwrap();
        assert_ne!(peer_slot, own_slot);
        unlink_segment(DOMAIN, peer_slot);
        let peer = OwnedSegment::create(
            DOMAIN,
            peer_slot,
            peer_epoch,
            &DEFAULT_CLASSES,
            DEFAULT_RING_ENTRIES,
        )
        .unwrap();

        let locator = Locator::from_shm(&Ipv4Addr::new(127, 0, 0, 1), 7410);
        plugin.send_to_peer(b"descriptor", &locator, PEER_PREFIX).unwrap();
        let mut out = [0u8; RING_INLINE];
        let (len, _) = peer.ring_mut().pop(&mut out).expect("the peer's ring must carry it");
        assert_eq!(&out[..len as usize], b"descriptor");

        // Another host's SHM locator, and a UDP one, are not zero-copy targets.
        let theirs = Locator::from_shm(&Ipv4Addr::new(192, 168, 1, 50), 7410);
        let udp = Locator::from_ip_v4_addr_and_port(&Ipv4Addr::new(127, 0, 0, 1), 7410);
        for l in [&theirs, &udp] {
            let err = plugin.send_to_peer(b"x", l, PEER_PREFIX).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::Unsupported);
        }
        assert!(!plugin.can_handle(&theirs));
        assert!(plugin.can_handle(&udp));

        plugin.close();
        drop(peer);
        drop(rt);
        drop(plugin);
        unlink_registry(DOMAIN);
        unlink_segment(DOMAIN, own_slot);
        unlink_segment(DOMAIN, peer_slot);
    }
}
