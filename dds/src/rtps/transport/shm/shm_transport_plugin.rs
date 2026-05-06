#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::thread;

use crossbeam_channel::{bounded, Receiver};
use log::{debug, info, warn};

use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{IncomingMessage, MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::shm::shm_listener::ShmListener;
use crate::rtps::transport::shm::shm_sender::ShmSender;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;

/// Channel buffer size for merged sources.
const CHANNEL_BUFFER_SIZE: usize = 256;

/// SHM transport plugin — UDP for discovery, SHM for user data.
///
/// Discovery (both multicast and unicast) always uses UDP.
/// User data routes by locator kind:
///   - SHM locator → SHM sender (shared memory ring buffer)
///   - UDP locator → UDP sender (fallback for non-SHM peers)
///
/// Incoming user data from both UDP and SHM are merged
/// into a single channel-based `MessageSource`.
pub(crate) struct ShmTransportPlugin {
    udp_sender: UdpSender,
    shm_sender: ShmSender,
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,

    // UDP listeners — discovery is always UDP
    discovery_multicast_listener: Mutex<Option<UdpListener>>,
    discovery_unicast_listener: Mutex<Option<UdpListener>>,

    // Merged user unicast: UDP unicast + SHM listener
    user_data_unicast_rx: Mutex<Option<Receiver<IncomingMessage>>>,
}

impl ShmTransportPlugin {
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        bind_ip: String,
        multicast_if_ip: String,
        working_ips: Vec<String>,
    ) -> io::Result<Self> {
        let udp_sender = UdpSender::new(bind_ip, multicast_if_ip)?;
        let shm_sender = ShmSender::new(domain_id)?;

        // Create UDP listeners for discovery
        let discovery_mc_port = PortManager::get_discovery_traffic_multicast_port(domain_id);
        let discovery_uc_port =
            PortManager::get_discovery_traffic_unicast_port(domain_id, participant_id);
        let user_uc_port = PortManager::get_user_traffic_unicast_port(domain_id, participant_id);

        let discovery_mc = UdpListener::new_multicast(discovery_mc_port, &working_ips).ok();
        let discovery_uc = UdpListener::new(discovery_uc_port).ok();
        let user_uc = UdpListener::new(user_uc_port).ok();
        let shm_listener = ShmListener::new(domain_id).ok();

        // Create merged user unicast channel: UDP + SHM
        let (user_merged_tx, user_merged_rx) = bounded::<IncomingMessage>(CHANNEL_BUFFER_SIZE);

        // Spawn merge thread for user unicast (UDP + SHM)
        thread::Builder::new()
            .name("shm_user_merge".to_string())
            .spawn(move || {
                merge_udp_and_shm(user_uc, shm_listener, user_merged_tx);
            })
            .expect("Failed to create SHM user merge thread");

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
            user_data_unicast_rx: Mutex::new(Some(user_merged_rx)),
        })
    }

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

impl TransportPlugin for ShmTransportPlugin {
    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        match target {
            SendTarget::SPDPDiscovery { initial_peers } => {
                // Discovery always uses UDP multicast, plus fan-out to initial_peers.
                self.udp_sender.send_multicast(self.domain_id, data)?;
                for peer_addr in *initial_peers {
                    let _ = self.udp_sender.send(peer_addr, data);
                }
                Ok(())
            }
            SendTarget::SEDPDiscovery(locator) => {
                // Discovery always uses UDP
                let ip = locator.to_ip_v4_addr();
                let port = locator.port() as u16;
                let addr = SocketAddr::new(IpAddr::V4(ip), port);
                self.udp_sender.send(&addr, data)?;
                Ok(())
            }
            SendTarget::UserData(locator) => {
                if locator.is_shm() {
                    // SHM locator — send via shared memory
                    let dummy_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
                    self.shm_sender.send(&dummy_addr, data)?;
                    Ok(())
                } else {
                    // UDP locator — fallback for non-SHM peers
                    let ip = locator.to_ip_v4_addr();
                    let port = locator.port() as u16;
                    let addr = SocketAddr::new(IpAddr::V4(ip), port);
                    self.udp_sender.send(&addr, data)?;
                    Ok(())
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
        let port = PortManager::get_discovery_traffic_unicast_port(
            self.domain_id,
            self.participant_id,
        ) as u32;
        self.udp_locators(port)
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        let udp_port =
            PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id) as u32;
        let mut locators = self.udp_locators(udp_port);
        if self.shm_sender.is_available() {
            locators.push(Locator::from_shm(&Ipv4Addr::new(127, 0, 0, 1), self.domain_id));
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
        let rx = self.user_data_unicast_rx.lock().expect("lock poisoned").take()?;
        Some(MessageSource::Channel { rx })
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

        debug!("[ShmTransportPlugin] Closed");
    }
}

/// Merge UDP listener and SHM listener into a single output channel.
fn merge_udp_and_shm(
    mut udp_listener: Option<UdpListener>,
    mut shm_listener: Option<ShmListener>,
    tx: crossbeam_channel::Sender<IncomingMessage>,
) {
    use mio::{Events, Interest, Poll, Token};
    use std::time::Duration;

    let has_shm = shm_listener.is_some();

    let mut poll = match Poll::new() {
        Ok(p) => p,
        Err(e) => {
            warn!("[ShmMerge] Failed to create poll: {:?}", e);
            return;
        }
    };
    let mut events = Events::with_capacity(64);
    let udp_token = Token(1);

    if let Some(ref mut listener) = udp_listener {
        if let Err(e) = poll.registry().register(listener.socket(), udp_token, Interest::READABLE) {
            warn!("[ShmMerge] Failed to register UDP listener: {:?}", e);
        }
    }

    // Use short timeout when SHM is active for low-latency polling
    let poll_timeout = if has_shm { Duration::from_nanos(0) } else { Duration::from_millis(100) };

    loop {
        let _ = poll.poll(&mut events, Some(poll_timeout));

        // Drain UDP
        for event in events.iter() {
            if event.token() == udp_token && event.is_readable() {
                if let Some(ref mut listener) = udp_listener {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        let msg = IncomingMessage { data: buffer.to_vec(), source: from_addr };
                        if tx.try_send(msg).is_err() {
                            return;
                        }
                    }
                }
            }
        }

        // Poll SHM (non-mio, busy poll)
        let mut shm_had_data = false;
        if let Some(ref mut listener) = shm_listener {
            while let Some((buffer, from_addr)) = listener.get_message() {
                let msg = IncomingMessage { data: buffer.to_vec(), source: from_addr };
                if tx.try_send(msg).is_err() {
                    return;
                }
                shm_had_data = true;
            }
        }

        // Yield CPU briefly if SHM enabled but no data
        if has_shm && !shm_had_data && events.is_empty() {
            std::thread::sleep(Duration::from_micros(10));
        }
    }
}
