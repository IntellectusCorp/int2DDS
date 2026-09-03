#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::Ipv4Addr;
use std::sync::Mutex;

use log::{debug, info};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::tcp_transport_plugin::TcpTransportPlugin;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::rtps::transport::udp::udp_sender::UdpSender;
use crate::rtps::transport::HybridConfig;

/// Hybrid transport plugin — UDP multicast discovery + TCP/UDP unicast.
///
/// Discovery multicast always uses UDP.
/// Unicast discovery and user data route by locator kind:
///   - UDP locator → UDP sender
///   - TCP locator → TCP sender (via embedded TcpTransportPlugin)
///
/// Incoming unicast messages from both UDP and TCP are merged
/// into a single channel-based `MessageSource`.
pub(crate) struct HybridTransportPlugin {
    udp_sender: UdpSender,
    tcp_plugin: TcpTransportPlugin,
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,

    // UDP listeners — multicast is always UDP
    discovery_multicast_listener: Mutex<Option<UdpListener>>,

    // Unicast reception is one stream source handed straight through from the
    // embedded TCP plugin; it carries both discovery and user data.
    stream_source: Mutex<Option<MessageSource>>,
}

impl HybridTransportPlugin {
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        bind_ip: String,
        multicast_if_ip: String,
        working_ips: Vec<String>,
        guid_prefix: GuidPrefix,
        hybrid_config: HybridConfig,
    ) -> io::Result<Self> {
        let egress_if = multicast_if_ip.parse::<Ipv4Addr>().ok();
        let udp_sender = UdpSender::new(bind_ip.clone(), multicast_if_ip, hybrid_config.udp)?;

        // Multicast first (domain-wide port, no per-participant collision).
        let discovery_mc_port = PortManager::get_discovery_traffic_multicast_port(domain_id);
        let discovery_mc =
            UdpListener::new_discovery_multicast(discovery_mc_port, &working_ips, egress_if).ok();

        // Create TCP plugin (handles its own mux listener thread). The TCP listen
        // port comes from the per-participant TcpConfig (bind_port property, else
        // the domain formula).
        // Hybrid discovers peers over UDP multicast, so the embedded TCP plugin
        // must dial any discovered peer's TCP locators — force accept-undefined-
        // peers regardless of the configured value.
        let mut tcp_config = hybrid_config.tcp;
        tcp_config.accept_undefined_peers = true;
        // Peers learn the TCP listen port from the SPDP locators carried over UDP
        // multicast, so it never has to be predictable. Binding an ephemeral port
        // by default lets several participants share one host without colliding.
        tcp_config.bind_port = Some(tcp_config.bind_port.unwrap_or(0));
        let tcp_plugin = TcpTransportPlugin::new(
            domain_id,
            participant_id,
            bind_ip,
            working_ips.clone(),
            guid_prefix,
            tcp_config,
        )?;

        let tcp_stream_source = tcp_plugin.take_stream_source();

        info!("[HybridTransportPlugin] Created (domain={}, pid={})", domain_id, participant_id);

        Ok(Self {
            udp_sender,
            tcp_plugin,
            domain_id,
            participant_id,
            working_ips,
            discovery_multicast_listener: Mutex::new(discovery_mc),
            stream_source: Mutex::new(tcp_stream_source),
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

impl TransportPlugin for HybridTransportPlugin {
    /// Unicast rides TCP, so the bound that applies is the TCP one.
    fn advertised_receive_buffer_size(&self) -> Option<usize> {
        self.tcp_plugin.advertised_receive_buffer_size()
    }

    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        match target {
            SendTarget::SPDPDiscovery { initial_peers } => {
                // SPDP is UDP only — multicast plus unicast to the
                // configured initial_peers. SEDP/liveliness/user data ride TCP.
                let _ = self.udp_sender.send_multicast(self.domain_id, data);
                for peer_addr in *initial_peers {
                    let _ = self.udp_sender.send(peer_addr, data);
                }
                Ok(())
            }
            SendTarget::SEDPDiscovery(locator) | SendTarget::UserData(locator) => {
                if locator.is_tcp() {
                    self.tcp_plugin.send(data, target)
                } else {
                    Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()))
                }
            }
        }
    }

    fn can_handle(&self, locator: &Locator) -> bool {
        locator.is_tcp()
    }

    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
        // unicast metatraffic (SEDP, liveliness) rides TCP. UDP carries
        // only multicast SPDP, so no UDP unicast locator is advertised.
        self.tcp_plugin.advertised_metatraffic_unicast_locators()
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        // user data rides TCP.
        self.tcp_plugin.advertised_default_unicast_locators()
    }

    fn advertised_default_multicast_locators(&self, _groups: Vec<Ipv4Addr>) -> Vec<Locator> {
        // User data rides TCP; the UDP side carries multicast SPDP only.
        Vec::new()
    }

    fn ensure_user_multicast_listener(&self, _group: Ipv4Addr) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "user data rides TCP, which cannot receive multicast",
        ))
    }

    fn take_discovery_multicast_source(&self) -> Option<MessageSource> {
        let listener = self.discovery_multicast_listener.lock().expect("lock poisoned").take()?;
        Some(MessageSource::Udp { listener })
    }

    fn take_discovery_unicast_source(&self) -> Option<MessageSource> {
        None
    }

    fn take_user_data_unicast_source(&self) -> Option<MessageSource> {
        None
    }

    fn take_stream_source(&self) -> Option<MessageSource> {
        self.stream_source.lock().expect("lock poisoned").take()
    }

    fn port(&self) -> u16 {
        self.udp_sender.port()
    }

    fn tcp_listener_port(&self) -> Option<u16> {
        self.tcp_plugin.tcp_listener_port()
    }

    fn disconnect_peer(&self, locators: &[Locator]) {
        // Only TCP holds per-peer connections; UDP is connectionless. Forward to
        // the embedded TCP plugin so a DDS-layer unmatch releases the peer's TCP
        // resources promptly instead of lingering until OS keepalive.
        self.tcp_plugin.disconnect_peer(locators);
    }

    fn participant_id(&self) -> u32 {
        self.participant_id
    }

    fn close(&self) {
        self.udp_sender.force_close();
        self.tcp_plugin.close();

        if let Ok(mut guard) = self.discovery_multicast_listener.lock() {
            if let Some(mut listener) = guard.take() {
                listener.close();
            }
        }

        debug!("[HybridTransportPlugin] Closed");
    }
}
