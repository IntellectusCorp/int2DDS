use std::sync::Arc;
use std::vec;

use crossbeam_channel::{bounded, Receiver};

use crate::common::env::{get_network_interface, get_network_ip};
use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::types::{DomainId, ParticipantId};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::shm::shm_listener::ShmListener;
use crate::rtps::transport::shm::shm_sender::ShmSender;
use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
use crate::rtps::transport::tcp::tcp_sender::TcpSender;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::{get_transport_type, Transport, TransportSender, TransportType};

use std::net::SocketAddr;

/// Channel capacity for TCP mux routing
const TCP_MUX_CHANNEL_CAPACITY: usize = 4096;

#[derive(Debug)]
pub(crate) struct Socket {
    //sender - primary sender (UDP for UDP/Hybrid, TCP for TCP-only)
    sender: Option<Arc<TransportSender>>,

    //tcp_sender - additional TCP sender for Hybrid mode
    tcp_sender: Option<Arc<TransportSender>>,

    //shm_sender - SHM sender for user data in SHM mode
    shm_sender: Option<Arc<TransportSender>>,

    //UDP listeners (used when transport = UDP or Hybrid or SHM for discovery)
    //discovery listener(spdp)
    discovery_traffic_multicast_listener: Option<UdpListener>,
    discovery_traffic_unicast_listener: Option<UdpListener>,

    //user_traffic(sedp + user_data)
    user_traffic_multicast_listener: Option<UdpListener>,
    user_traffic_unicast_listener: Option<UdpListener>,

    //TCP MuxListener (used when transport = TCP or Hybrid)
    tcp_mux_listener: Option<TcpMuxListener>,

    //TCP mux channel receivers (routed from MuxListener)
    tcp_discovery_rx: Option<Receiver<(Vec<u8>, SocketAddr)>>,
    tcp_user_data_rx: Option<Receiver<(Vec<u8>, SocketAddr)>>,

    //SHM listener (used when transport = SHM for user data)
    shm_listener: Option<ShmListener>,

    domain_id: DomainId,
    participant_id: ParticipantId,
    working_ips: WorkingIps,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkingIps {
    pub ips: Vec<String>,
    pub from_feature: bool,
}

pub const MAX_EVENTS: usize = 512;

impl Socket {
    pub(crate) fn new(domain_id: DomainId) -> Self {
        let working_ip = Self::get_new_working_ips().unwrap_or_else(|e| {
            log::error!("[socket] Failed to determine working IP: {}. Using fallback 127.0.0.1", e);
            WorkingIps { ips: vec!["127.0.0.1".to_string()], from_feature: false }
        });
        Self {
            //sender
            sender: None,
            tcp_sender: None,
            shm_sender: None,

            //UDP listeners
            discovery_traffic_multicast_listener: None,
            discovery_traffic_unicast_listener: None,
            user_traffic_multicast_listener: None,
            user_traffic_unicast_listener: None,

            //TCP mux listener + channel receivers
            tcp_mux_listener: None,
            tcp_discovery_rx: None,
            tcp_user_data_rx: None,

            //SHM listener
            shm_listener: None,

            domain_id,
            participant_id: 0,
            working_ips: working_ip,
        }
    }

    pub(crate) fn participant_id(&self) -> ParticipantId {
        self.participant_id
    }

    pub(crate) fn create_socket(&mut self) {
        self.create_sender();
        self.create_listener();
    }

    /// Create socket with guid_prefix for TCP mux mode.
    /// Must be called instead of create_socket() when TCP/Hybrid transport is used.
    pub(crate) fn create_socket_with_guid(&mut self, guid_prefix: GuidPrefix) {
        self.create_sender_with_guid(guid_prefix);
        self.create_listener_with_guid(guid_prefix);
    }

    //sender
    fn create_sender(&mut self) {
        let transport_type = get_transport_type();

        match transport_type {
            TransportType::UDP => {
                self.sender = match UdpSender::new(
                    self.get_sender_bind_addr(),
                    self.get_sender_multicast_if_addr(),
                ) {
                    Ok(udp_sender) => {
                        let transport_sender = TransportSender::Udp(udp_sender);
                        Some(Arc::new(transport_sender))
                    }
                    Err(e) => {
                        log::error!("UDP sender creation failed: {}", e);
                        panic!("UDP sender is not created");
                    }
                };
            }
            TransportType::TCP | TransportType::Hybrid => {
                // TCP/Hybrid sender requires guid_prefix — use create_socket_with_guid()
                log::warn!(
                    "[socket] TCP/Hybrid sender requires guid_prefix. \
                     Call create_socket_with_guid() instead of create_socket()."
                );
            }
            TransportType::SHM => {
                // SHM mode uses UDP for discovery (SPDP, SEDP)
                self.sender = match UdpSender::new(
                    self.get_sender_bind_addr(),
                    self.get_sender_multicast_if_addr(),
                ) {
                    Ok(udp_sender) => {
                        let transport_sender = TransportSender::Udp(udp_sender);
                        log::info!("[socket] SHM mode: UDP sender created for discovery");
                        Some(Arc::new(transport_sender))
                    }
                    Err(e) => {
                        log::error!("SHM mode: UDP sender creation failed: {}", e);
                        panic!("UDP sender is not created");
                    }
                };

                // Create SHM sender for user data (domain-wide shared segment)
                self.shm_sender = match ShmSender::new(self.domain_id) {
                    Ok(shm_sender) => {
                        let transport_sender = TransportSender::Shm(shm_sender);
                        log::info!(
                            "[socket] SHM mode: SHM sender created for domain {}",
                            self.domain_id
                        );
                        Some(Arc::new(transport_sender))
                    }
                    Err(e) => {
                        log::error!("SHM mode: SHM sender creation failed: {}", e);
                        panic!("SHM sender is not created");
                    }
                };
            }
        }
    }

    /// Create sender with guid_prefix for TCP mux mode
    fn create_sender_with_guid(&mut self, guid_prefix: GuidPrefix) {
        let transport_type = get_transport_type();

        match transport_type {
            TransportType::TCP => {
                let tcp_physical_port = PortManager::get_tcp_physical_port(self.domain_id);
                let tcp_sender_arc = match TcpSender::new(
                    self.get_sender_bind_addr(),
                    guid_prefix,
                    self.domain_id,
                    self.participant_id,
                    tcp_physical_port,
                ) {
                    Ok(tcp_sender) => {
                        let transport_sender = TransportSender::Tcp(tcp_sender);
                        log::info!("[socket] TCP sender created");
                        Arc::new(transport_sender)
                    }
                    Err(e) => {
                        log::error!("TCP sender creation failed: {}", e);
                        panic!("TCP sender is not created");
                    }
                };

                self.sender = Some(tcp_sender_arc.clone());
                self.tcp_sender = Some(tcp_sender_arc);
            }
            TransportType::Hybrid => {
                // Create UDP sender as primary
                self.sender = match UdpSender::new(
                    self.get_sender_bind_addr(),
                    self.get_sender_multicast_if_addr(),
                ) {
                    Ok(udp_sender) => {
                        let transport_sender = TransportSender::Udp(udp_sender);
                        log::info!("[socket] Hybrid mode: UDP sender created");
                        Some(Arc::new(transport_sender))
                    }
                    Err(e) => {
                        log::error!("Hybrid mode: UDP sender creation failed: {}", e);
                        panic!("UDP sender is not created");
                    }
                };

                // Create TCP sender as secondary
                let tcp_physical_port = PortManager::get_tcp_physical_port(self.domain_id);
                self.tcp_sender = match TcpSender::new(
                    self.get_sender_bind_addr(),
                    guid_prefix,
                    self.domain_id,
                    self.participant_id,
                    tcp_physical_port,
                ) {
                    Ok(tcp_sender) => {
                        log::info!("[socket] Hybrid mode: TCP sender created");
                        Some(Arc::new(TransportSender::Tcp(tcp_sender)))
                    }
                    Err(e) => {
                        log::error!("Hybrid mode: TCP sender creation failed: {}", e);
                        panic!("TCP sender is not created");
                    }
                };
            }
            _ => {
                // UDP/SHM don't need guid_prefix — delegate to regular create_sender
                self.create_sender();
            }
        }
    }

    pub(crate) fn sender(&self) -> Arc<TransportSender> {
        match &self.sender {
            Some(sender) => Arc::clone(sender),
            None => {
                log::error!("sender is not created");
                panic!("sender is not created");
            }
        }
    }

    pub(crate) fn tcp_sender(&self) -> Option<Arc<TransportSender>> {
        self.tcp_sender.as_ref().map(Arc::clone)
    }

    pub(crate) fn shm_sender(&self) -> Option<Arc<TransportSender>> {
        self.shm_sender.as_ref().map(Arc::clone)
    }

    //listener
    fn create_listener(&mut self) {
        let transport_type = get_transport_type();
        match transport_type {
            TransportType::TCP | TransportType::Hybrid => {
                panic!(
                    "[socket] TCP/Hybrid listener requires guid_prefix. \
                     Call create_socket_with_guid() instead of create_socket()."
                );
            }
            TransportType::UDP => {
                self.create_multicast_listener();
                self.create_unicast_listener();
                log::info!("[socket] UDP listeners created");
            }
            TransportType::SHM => {
                self.create_multicast_listener();
                self.create_unicast_listener();
                log::info!("[socket] SHM mode: UDP listeners created for discovery");
                self.create_shm_listener();
            }
        }
    }

    /// Create listener with guid_prefix for TCP mux mode
    fn create_listener_with_guid(&mut self, guid_prefix: GuidPrefix) {
        let transport_type = get_transport_type();
        match transport_type {
            TransportType::TCP => {
                self.create_tcp_mux_listener(guid_prefix);
                log::info!("[socket] TCP MuxListener created");
            }
            TransportType::Hybrid => {
                self.create_multicast_listener();
                self.create_unicast_listener();
                self.create_tcp_mux_listener(guid_prefix);
                log::info!("[socket] Hybrid mode: UDP and TCP MuxListener created");
            }
            _ => {
                // UDP/SHM don't need guid_prefix — delegate to regular create_listener
                self.create_listener();
            }
        }
    }

    fn create_multicast_listener(&mut self) {
        // If port acquisition fails, listener = None
        self.create_discovery_multicast_listener(self.domain_id);
        self.create_user_traffic_multicast_listener(self.domain_id);
    }

    fn create_unicast_listener(&mut self) {
        // If port acquisition fails, change participant and retry
        self.create_discovery_unicast_listener(self.domain_id, self.participant_id);
        self.create_user_traffic_unicast_listener(self.domain_id, self.participant_id);
    }

    //discovery multicast listener
    fn create_discovery_multicast_listener(&mut self, domain_id: u32) {
        let udp_listener: Option<UdpListener> = UdpListener::new_multicast(
            PortManager::get_discovery_traffic_multicast_port(domain_id),
            &self.working_ips.ips,
        )
        .ok();
        self.discovery_traffic_multicast_listener = udp_listener;
    }
    pub(crate) fn discovery_multicast_listener(&mut self) -> Option<UdpListener> {
        self.discovery_traffic_multicast_listener.take()
    }

    //discovery unicast listener
    fn create_discovery_unicast_listener(&mut self, domain_id: u32, participant_id: u32) {
        let udp_listener: Option<UdpListener> = UdpListener::new(
            PortManager::get_discovery_traffic_unicast_port(domain_id, participant_id),
        )
        .ok();
        self.discovery_traffic_unicast_listener = udp_listener;
        if self.discovery_traffic_unicast_listener.is_none() {
            self.participant_id += 1;
            self.create_unicast_listener();
        }
    }
    pub(crate) fn discovery_unicast_listener(&mut self) -> Option<UdpListener> {
        self.discovery_traffic_unicast_listener.take()
    }

    //user traffic multicast listener
    fn create_user_traffic_multicast_listener(&mut self, domain_id: u32) {
        let udp_listener: Option<UdpListener> = UdpListener::new_multicast(
            PortManager::get_user_traffic_multicast_port(domain_id),
            &self.working_ips.ips,
        )
        .ok();
        self.user_traffic_multicast_listener = udp_listener;
    }
    pub(crate) fn user_traffic_multicast_listener(&mut self) -> Option<UdpListener> {
        self.user_traffic_multicast_listener.take()
    }

    //user traffic unicast listener
    fn create_user_traffic_unicast_listener(&mut self, domain_id: u32, participant_id: u32) {
        let udp_listener: Option<UdpListener> =
            UdpListener::new(PortManager::get_user_traffic_unicast_port(domain_id, participant_id))
                .ok();
        self.user_traffic_unicast_listener = udp_listener;
        if self.user_traffic_unicast_listener.is_none()
            && self.discovery_traffic_unicast_listener.is_some()
        {
            if let Some(mut listener) = self.discovery_traffic_unicast_listener.take() {
                listener.close();
            }
            self.participant_id += 1;
            self.create_unicast_listener();
        }
    }

    pub(crate) fn user_traffic_unicast_listener(&mut self) -> Option<UdpListener> {
        self.user_traffic_unicast_listener.take()
    }

    // TCP MuxListener (single-port multiplexed)
    fn create_tcp_mux_listener(&mut self, guid_prefix: GuidPrefix) {
        let (discovery_tx, discovery_rx) = bounded(TCP_MUX_CHANNEL_CAPACITY);
        let (user_data_tx, user_data_rx) = bounded(TCP_MUX_CHANNEL_CAPACITY);

        let physical_port = PortManager::get_tcp_physical_port(self.domain_id);

        match TcpMuxListener::new(
            physical_port,
            self.domain_id,
            self.participant_id,
            guid_prefix,
            discovery_tx,
            user_data_tx,
        ) {
            Ok(listener) => {
                log::info!(
                    "[socket] TCP MuxListener created on port {} (domain={}, pid={})",
                    listener.port(),
                    self.domain_id,
                    self.participant_id,
                );
                self.tcp_mux_listener = Some(listener);
                self.tcp_discovery_rx = Some(discovery_rx);
                self.tcp_user_data_rx = Some(user_data_rx);
            }
            Err(e) => {
                log::warn!(
                    "[socket] Failed to bind TCP MuxListener on port {}: {}. \
                     Falling back to ephemeral port.",
                    physical_port,
                    e,
                );
                // Fallback: bind to OS-assigned ephemeral port (port 0)
                // Avoids collision with logical ports in the RTPS port range
                let (dtx, drx) = bounded(TCP_MUX_CHANNEL_CAPACITY);
                let (utx, urx) = bounded(TCP_MUX_CHANNEL_CAPACITY);
                match TcpMuxListener::new(
                    0,
                    self.domain_id,
                    self.participant_id,
                    guid_prefix,
                    dtx,
                    utx,
                ) {
                    Ok(listener) => {
                        log::info!(
                            "[socket] TCP MuxListener fallback on ephemeral port {}",
                            listener.port()
                        );
                        self.tcp_mux_listener = Some(listener);
                        self.tcp_discovery_rx = Some(drx);
                        self.tcp_user_data_rx = Some(urx);
                    }
                    Err(e2) => {
                        log::error!(
                            "[socket] TCP MuxListener ephemeral fallback also failed: {}",
                            e2
                        );
                    }
                }
            }
        }
    }

    /// Take the TcpMuxListener (for the mux listening task thread)
    pub(crate) fn tcp_mux_listener(&mut self) -> Option<TcpMuxListener> {
        self.tcp_mux_listener.take()
    }

    /// Take the discovery channel receiver (for discovery listening task)
    pub(crate) fn tcp_discovery_rx(&mut self) -> Option<Receiver<(Vec<u8>, SocketAddr)>> {
        self.tcp_discovery_rx.take()
    }

    /// Take the user data channel receiver (for user listening task)
    pub(crate) fn tcp_user_data_rx(&mut self) -> Option<Receiver<(Vec<u8>, SocketAddr)>> {
        self.tcp_user_data_rx.take()
    }

    //SHM listener
    fn create_shm_listener(&mut self) {
        match ShmListener::new(self.domain_id) {
            Ok(listener) => {
                log::info!("[socket] SHM listener created for domain {}", self.domain_id);
                self.shm_listener = Some(listener);
            }
            Err(e) => {
                log::error!("[socket] Failed to create SHM listener: {}", e);
            }
        }
    }

    pub(crate) fn shm_listener(&mut self) -> Option<ShmListener> {
        self.shm_listener.take()
    }

    pub(crate) fn close(&mut self) {
        // Close senders
        if let Some(sender) = self.sender.take() {
            let ref_count = Arc::strong_count(&sender);
            match Arc::try_unwrap(sender) {
                Ok(s) => s.close(),
                Err(arc) => {
                    log::warn!(
                        "[socket] sender has other references, force closing, ref_count:{}",
                        ref_count
                    );
                    arc.force_close();
                }
            }
        }
        if let Some(tcp_sender) = self.tcp_sender.take() {
            let ref_count = Arc::strong_count(&tcp_sender);
            match Arc::try_unwrap(tcp_sender) {
                Ok(s) => s.close(),
                Err(arc) => {
                    log::warn!(
                        "[socket] tcp_sender has other references, force closing, ref_count: {}",
                        ref_count
                    );
                    arc.force_close();
                }
            }
        }
        if let Some(shm_sender) = self.shm_sender.take() {
            let ref_count = Arc::strong_count(&shm_sender);
            match Arc::try_unwrap(shm_sender) {
                Ok(s) => s.close(),
                Err(arc) => {
                    log::warn!(
                        "[socket] shm_sender has other references, force closing, ref_count: {}",
                        ref_count
                    );
                    arc.force_close();
                }
            }
        }

        // Close UDP listeners
        if let Some(mut listener) = self.discovery_traffic_multicast_listener.take() {
            listener.close();
        }
        if let Some(mut listener) = self.discovery_traffic_unicast_listener.take() {
            listener.close();
        }
        if let Some(mut listener) = self.user_traffic_multicast_listener.take() {
            listener.close();
        }
        if let Some(mut listener) = self.user_traffic_unicast_listener.take() {
            listener.close();
        }

        // Close TCP MuxListener
        if let Some(mut listener) = self.tcp_mux_listener.take() {
            listener.close();
        }

        // Drop channel receivers
        self.tcp_discovery_rx.take();
        self.tcp_user_data_rx.take();

        // Close SHM listener
        if let Some(mut listener) = self.shm_listener.take() {
            listener.close();
        }

        log::info!("[socket] all listeners and senders closed");
    }

    fn get_new_working_ips() -> std::io::Result<WorkingIps> {
        let mut ips: Vec<String> = Vec::new();
        let mut from_feature = false;

        // Check if user specified which network to use via env variable
        let is_network_specified = get_network_interface().is_some() || get_network_ip().is_some();

        if let Ok(Some(ip)) = crate::common::int2dds_feature_ffi::get_working_ip() {
            // int2DDS-feature enabled
            if is_network_specified {
                ips.push(ip);
                from_feature = true;
            } else {
                log::warn!(
                    "int2DDS-feature is enabled but no network interface specified. \
                            Falling back to default (auto-detection)"
                );
            }
        } else if is_network_specified {
            // These variables can only be used with int2DDS-feature
            log::warn!("Env variable INT2DDS_NETWORK_INTERFACE or INT2DDS_NETWORK_IP is set but int2DDS-feature is not enabled. \
                        Ignoring the value");
        }

        // If no specific IP was selected, use all available NICs
        if ips.is_empty() {
            if let Ok(ifaces) = get_if_addrs::get_if_addrs() {
                for iface in ifaces {
                    if !iface.ip().is_loopback() {
                        ips.push(iface.ip().to_string());
                    }
                }
            }
        }

        let use_loopback = crate::common::env::get_use_loopback_interface();
        let should_add_loopback =
            !from_feature && !ips.contains(&"127.0.0.1".to_string()) && use_loopback;

        // If no NIC available or loopback is set to use, add localhost IP to the list
        // also skipped when from_feature — feature-specified NIC takes full control
        if ips.is_empty() || should_add_loopback {
            ips.push("127.0.0.1".to_string());
        }

        Ok(WorkingIps { ips, from_feature })
    }

    pub(crate) fn working_ips(&self) -> Vec<String> {
        self.working_ips.ips.clone()
    }

    pub(crate) fn is_working_ips_from_feature(&self) -> bool {
        self.working_ips.from_feature
    }

    fn get_sender_bind_addr(&self) -> String {
        // From int2DDS-feature: bind to the feature-specified IP directly
        if self.working_ips.from_feature {
            return self.working_ips.ips[0].clone();
        }
        // This happens when no physical NIC exists
        let only_loopback =
            self.working_ips.ips.len() == 1 && self.working_ips.ips[0] == "127.0.0.1";
        if only_loopback {
            // No physical NIC available, 0.0.0.0 has no interface to route through
            "127.0.0.1".to_string()
        } else {
            // 0.0.0.0 allows unicast to reach any subnet via OS routing table
            "0.0.0.0".to_string()
        }
    }

    // TODO: Ideally, create one multicast sender per NIC with set_multicast_if_v4(ip) + bind(ip:0)
    // to send multicast out of all NICs simultaneously.
    // 0.0.0.0 relies on default route, which doesn't exist in gateway-less environments,
    // and multicast addresses (e.g. 239.x) don't match any subnet route.
    fn get_sender_multicast_if_addr(&self) -> String {
        // From int2DDS-feature: use the feature-specified IP directly
        if self.working_ips.from_feature {
            return self.working_ips.ips[0].clone();
        }

        // Probe the OS routing table by connecting to a public address.
        // Resolve the default outgoing IP via 0.0.0.0 bind & connect
        if let Ok(addr) = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("8.8.8.8:80").map(|_| s))
            .and_then(|s| s.local_addr())
        {
            let ip = addr.ip().to_string();
            if ip != "127.0.0.1" && ip != "0.0.0.0" {
                return ip;
            }
        }

        // No default route (e.g. direct Ethernet without gateway):
        // pick the first non-loopback IP from working_ips
        self.working_ips
            .ips
            .iter()
            .find(|ip| ip.as_str() != "127.0.0.1")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string()) // Fallback to loopback if no other IPs are available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_listener() {
        let mut socket = Socket::new(0);
        socket.create_listener();
        assert!(socket.discovery_multicast_listener().is_some());
        assert!(socket.discovery_unicast_listener().is_some());
        assert!(socket.user_traffic_multicast_listener().is_some());
        assert!(socket.user_traffic_unicast_listener().is_some());
    }
}
