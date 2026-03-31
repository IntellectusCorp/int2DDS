#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::SocketAddr;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::udp::udp_listener::UdpListener;

use super::TransportType;

/// Intent-based send target.
///
/// RTPS logic expresses *what* it wants to do, not *how*.
/// Each `TransportPlugin` implementation interprets these targets
/// according to its own transport semantics.
pub(crate) enum SendTarget<'a> {
    /// Announce this participant's presence to the network.
    ///
    /// - UDP: send_multicast to discovery multicast group
    /// - TCP: unicast to each initial_peer via discovery connection
    /// - Hybrid: UDP multicast (discovery always uses UDP)
    /// - SHM: UDP multicast (discovery always uses UDP)
    MulticastDiscovery,

    /// Send discovery data (SEDP) to a specific remote participant.
    ///
    /// - UDP: sendto(locator address)
    /// - TCP: BIND handshake + send on discovery logical port
    /// - Hybrid: route by locator kind (UDP or TCP)
    /// - SHM: sendto via UDP (discovery is always UDP)
    UnicastDiscovery(&'a Locator),

    /// Send user data to a specific remote endpoint.
    ///
    /// - UDP: sendto(locator address)
    /// - TCP: BIND handshake + send on user_data logical port
    /// - Hybrid: route by locator kind (UDP or TCP)
    /// - SHM: route by locator kind (SHM or UDP)
    UserData(&'a Locator),
}

/// Unified message received from any transport source.
pub(crate) struct IncomingMessage {
    pub data: Vec<u8>,
    pub source: SocketAddr,
}

/// Source of incoming messages for a ListeningTask.
///
/// The variant determines the I/O mechanism, not the transport type.
/// ListeningTask branches on I/O mechanism (2 branches),
/// not on transport type (which would be N branches).
pub(crate) enum MessageSource {
    /// Direct mio-based polling — zero channel overhead.
    /// Used when a single listener owns the receive path (e.g., UDP-only mode).
    MioPoll { listener: UdpListener },

    /// Channel-based receiving.
    /// Used when multiple sources must be merged (Hybrid, SHM)
    /// or when the transport internally demuxes (TCP mux listener).
    Channel { rx: crossbeam_channel::Receiver<IncomingMessage> },
}

/// Transport plugin trait — the only interface RTPS logic depends on.
///
/// Implementations encapsulate all transport-specific details:
/// connection management, framing, multiplexing, keep-alive, etc.
/// RTPS logic never branches on transport type.
pub(crate) trait TransportPlugin: Send + Sync {
    /// Send data with intent-based targeting.
    ///
    /// The caller expresses *what* to do (announce, discovery, user data).
    /// The implementation decides *how* (multicast, TCP BIND, SHM write, etc.).
    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()>;

    /// Return the locators this transport advertises to remote participants.
    ///
    /// Called during participant creation to build the locator list
    /// included in SPDP announcements.
    fn local_locators(&self, domain_id: u32, participant_id: u32) -> Vec<Locator>;

    /// Take ownership of the discovery multicast message source.
    ///
    /// Returns `None` if the transport does not support multicast (e.g., TCP).
    /// Called once during initialization. The returned `MessageSource`
    /// is moved into `DiscoveryMulticastListeningTask`.
    fn take_discovery_multicast_source(&self) -> Option<MessageSource>;

    /// Take ownership of the discovery unicast message source.
    ///
    /// Returns `None` if the transport has no discovery unicast source.
    /// Called once during initialization. The returned `MessageSource`
    /// is moved into `DiscoveryUnicastListeningTask`.
    fn take_discovery_unicast_source(&self) -> Option<MessageSource>;

    /// Take ownership of the user data unicast message source.
    ///
    /// Returns `None` if the transport has no user data unicast source.
    /// Called once during initialization. The returned `MessageSource`
    /// is moved into `UserUnicastListeningTask`.
    fn take_user_data_unicast_source(&self) -> Option<MessageSource>;

    /// Get the local port number used by this transport's sender.
    fn port(&self) -> u16;

    /// Get the TCP listener port for locator advertisement.
    /// Returns Some(port) for TCP/Hybrid, None for UDP/SHM.
    fn tcp_listener_port(&self) -> Option<u16> {
        None
    }

    /// Get the final participant_id (may differ from the initial value
    /// if unicast ports were already in use and participant_id was incremented).
    fn participant_id(&self) -> u32;

    /// Release all resources (sockets, connections, threads).
    fn close(&self);
}

/// Factory for creating transport plugin instances.
pub(crate) struct TransportPluginFactory;

impl TransportPluginFactory {
    /// Create a transport plugin based on the configured transport type.
    ///
    /// This is the single point where transport type branching occurs.
    /// After this call, all code uses `dyn TransportPlugin` —
    /// no further transport-type checks needed.
    pub(crate) fn create(
        transport_type: TransportType,
        domain_id: u32,
        participant_id: u32,
        bind_ip: String,
        multicast_if_ip: String,
        working_ips: Vec<String>,
        guid_prefix: GuidPrefix,
    ) -> io::Result<Box<dyn TransportPlugin>> {
        match transport_type {
            TransportType::UDP => {
                use crate::rtps::transport::udp::udp_transport_plugin::UdpTransportPlugin;
                let plugin = UdpTransportPlugin::new(
                    domain_id,
                    participant_id,
                    bind_ip,
                    multicast_if_ip,
                    working_ips,
                )?;
                Ok(Box::new(plugin))
            }
            TransportType::TCP => {
                use crate::rtps::transport::tcp::tcp_transport_plugin::TcpTransportPlugin;
                let plugin =
                    TcpTransportPlugin::new(domain_id, participant_id, bind_ip, guid_prefix)?;
                Ok(Box::new(plugin))
            }
            TransportType::Hybrid => {
                use crate::rtps::transport::hybrid_transport_plugin::HybridTransportPlugin;
                let plugin = HybridTransportPlugin::new(
                    domain_id,
                    participant_id,
                    bind_ip,
                    multicast_if_ip,
                    working_ips,
                    guid_prefix,
                )?;
                Ok(Box::new(plugin))
            }
            TransportType::SHM => {
                use crate::rtps::transport::shm::shm_transport_plugin::ShmTransportPlugin;
                let plugin = ShmTransportPlugin::new(
                    domain_id,
                    participant_id,
                    bind_ip,
                    multicast_if_ip,
                    working_ips,
                )?;
                Ok(Box::new(plugin))
            }
        }
    }
}
