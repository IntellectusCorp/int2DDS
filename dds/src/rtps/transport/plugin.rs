#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::SocketAddr;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::shm::shm_listener::ShmListener;
use crate::rtps::transport::udp::udp_listener::UdpListener;

use super::TransportType;

/// Intent-based send target, named after the RTPS protocol concept being
/// delivered rather than the transport mechanism used. Each `TransportPlugin`
/// picks the mechanism (multicast, unicast fan-out, BIND, SHM write, ...) that
/// realizes the intent on its own transport.
pub(crate) enum SendTarget<'a> {
    /// Announce this participant's presence (SPDP).
    ///
    /// The caller passes the configured `initial_peers`; the transport
    /// reaches them alongside its native discovery mechanism:
    /// - UDP: multicast to discovery group + unicast to each initial_peer
    /// - TCP: unicast to each initial_peer (no multicast on TCP)
    /// - Hybrid: UDP multicast + unicast fan-out to initial_peers
    /// - SHM: UDP multicast + unicast fan-out to initial_peers
    SPDPDiscovery { initial_peers: &'a [SocketAddr] },

    /// Send endpoint discovery data (SEDP) to a specific remote participant.
    ///
    /// - UDP: sendto(locator address)
    /// - TCP: BIND handshake + send on discovery logical port
    /// - Hybrid: route by locator kind (UDP or TCP)
    /// - SHM: sendto via UDP (discovery is always UDP)
    SEDPDiscovery(&'a Locator),

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

    /// Direct mio polling for a UDP listener combined with direct ring-buffer
    /// polling for an SHM listener in the same loop.
    /// SHM has no file descriptor and cannot register with mio, so it is polled
    /// alongside UDP (zero-timeout poll + brief CPU yield when both are idle).
    /// Used by SHM mode for user data unicast where UDP fallback and SHM are
    /// merged at the listening-task level (no inter-thread channel).
    MioPollWithShm { listener: UdpListener, shm: ShmListener },

    /// Channel-based receiving.
    /// Used when the transport internally demultiplexes a single byte stream
    /// into per-logical-port streams (TCP single-port mux).
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

    /// True iff this plugin can route to `locator`.
    ///
    /// `UserLogic` uses this when a peer advertises multiple locator kinds
    /// (e.g. SHM mode publishes both SHM and UDP) to filter to the highest-
    /// priority kind the local transport can actually reach. Without this
    /// check, a UDP-only local would attempt to send to a SHM-mode peer's
    /// SHM locator and fail. Equivalent to PR #234's `have_sender` test,
    /// expressed per-locator on the trait.
    fn can_handle(&self, locator: &Locator) -> bool;

    /// Metatraffic (discovery) unicast locators this transport advertises to
    /// remote participants. Populated into the SPDP announcement.
    ///
    /// Encapsulates transport-specific knowledge: port formulas, WAN
    /// public-address overrides, per-NIC IP expansion, locator kind, etc.
    /// RTPS layer never branches on transport type to build this list.
    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator>;

    /// User-data unicast locators this transport advertises to remote
    /// participants. Populated into the SPDP announcement.
    fn advertised_default_unicast_locators(&self) -> Vec<Locator>;

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

    /// Take ownership of the dead peer event receiver.
    ///
    /// Returns `None` if the transport does not support connection-level peer monitoring (e.g., UDP).
    /// TCP transports return a channel that emits the SocketAddr of peers whose connections are lost
    /// (detected via keepalive timeout). The upper layer resolves the SocketAddr to an RTPS
    /// GuidPrefix via SPDP participant data. Called once during initialization.
    fn take_dead_peer_receiver(&self) -> Option<crossbeam_channel::Receiver<SocketAddr>> {
        None
    }

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
        tls_config: Option<std::sync::Arc<crate::rtps::transport::tcp::tls::TlsConfig>>,
        transport_config: crate::rtps::transport::TransportConfig,
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
                    transport_config,
                )?;
                Ok(Box::new(plugin))
            }
            TransportType::TCP | TransportType::TCPAsync => {
                // Step 1 of tcp/ + tcp_async/ unification: both variants route
                // through the tcp_async plugin. The legacy synchronous tcp
                // plugin is no longer reachable and will be removed once the
                // module rename lands.
                use crate::rtps::transport::tcp_async::tcp_transport_plugin::TcpAsyncTransportPlugin;
                let plugin = TcpAsyncTransportPlugin::new_with_tls(
                    domain_id,
                    participant_id,
                    bind_ip,
                    working_ips,
                    guid_prefix,
                    tls_config,
                )?;
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
                    transport_config,
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
                    transport_config,
                )?;
                Ok(Box::new(plugin))
            }
        }
    }
}
