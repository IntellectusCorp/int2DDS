#![allow(dead_code)]
#![allow(unused_variables)]

use std::net::SocketAddr;
use std::{io, net::Ipv4Addr};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::transport::shm::shm_listener::ShmListener;
use crate::rtps::transport::tcp::connection_registry::ConnectionRegistry;
use crate::rtps::transport::tcp::tcp_listener::TcpListener;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use bytes::Bytes;

use super::{HybridConfig, TcpConfig, TransportConfig, TransportType, UdpConfig};
use crate::dcps::infrastructure::qos_policy::PropertyQosPolicy;

/// Intent-based send target, named after the RTPS protocol concept being
/// delivered rather than the transport mechanism used. Each `TransportPlugin`
/// picks the mechanism (multicast, unicast fan-out, framed TCP, SHM write, ...) that
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
    /// - TCP: send a discovery-kind frame
    /// - Hybrid: route by locator kind (UDP or TCP)
    /// - SHM: sendto via UDP (discovery is always UDP)
    SEDPDiscovery(&'a Locator),

    /// Send user data to a specific remote endpoint.
    ///
    /// - UDP: sendto(locator address)
    /// - TCP: send a user-data-kind frame
    /// - Hybrid: route by locator kind (UDP or TCP)
    /// - SHM: route by locator kind (SHM or UDP)
    UserData(&'a Locator),
}

/// Unified message received from any transport source.
pub(crate) struct IncomingMessage {
    pub data: Bytes,
    pub source: SocketAddr,
}

/// Source of incoming messages for a ListeningTask.
///
/// The variant determines the I/O mechanism, not the transport type.
/// ListeningTask branches on I/O mechanism rather than transport type.
pub(crate) enum MessageSource {
    /// A single datagram listener owning the receive path.
    Udp { listener: UdpListener },

    /// A shared-memory listener polled in the same loop as its datagram
    /// fallback. SHM has no file descriptor and cannot register with mio, so
    /// both are polled directly (zero-timeout poll plus a brief yield when
    /// idle) instead of being merged over a channel.
    Shm { listener: UdpListener, shm: ShmListener },

    /// A listener that owns accepted connections and reassembles framed
    /// streams from them.
    Stream { listener: TcpListener, shared: std::sync::Arc<ConnectionRegistry> },
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
    /// The implementation decides *how* (multicast, framed TCP, SHM write, etc.).
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

    /// Group locators a DataReader advertises so remote writers can reach it
    /// over multicast. Empty for a transport that cannot carry user data over
    /// multicast.
    fn advertised_default_multicast_locators(&self, groups: Vec<Ipv4Addr>) -> Vec<Locator>;

    /// How many bytes may be in flight toward this participant before they
    /// start being dropped, for advertising in SPDP under the vendor PID. A
    /// datagram transport answers with the receive buffer the kernel granted
    /// it; a stream transport answers with the backlog it will hold. `None`
    /// when a transport has neither - a peer must then treat us as
    /// non-advertising, not as advertising zero.
    fn advertised_receive_buffer_size(&self) -> Option<usize> {
        None
    }

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

    fn take_stream_source(&self) -> Option<MessageSource> {
        None
    }

    /// Take ownership of one user data multicast source together with the group
    /// locator it receives.
    ///
    /// Returns `None` once every listener created so far has been handed out.
    /// Each returned `MessageSource` is moved into its own
    /// `UserMulticastListeningTask`, and the locator is the label that task
    /// stamps on everything it reads.
    fn take_user_data_multicast_source(&self) -> Option<(Locator, MessageSource)> {
        None
    }

    /// Create the user data multicast listener for `group` unless it already
    /// exists.
    ///
    /// Called once per DataReader that asks for multicast reception. Readers
    /// sharing a group share one listener, since two sockets on the same group
    /// would each read the same datagram. A transport that cannot carry user
    /// data over multicast rejects here, so the DataReader fails to be created
    /// instead of silently falling back to unicast.
    fn ensure_user_multicast_listener(&self, group: Ipv4Addr) -> io::Result<()>;

    /// Forget `group`, so a later reader on it is served by a freshly created
    /// listener.
    ///
    /// Called once the last DataReader on the group is gone. The socket itself
    /// belongs to the listening task by then, so the caller must have ended that
    /// task first; releasing while it still reads leaves two sockets on one
    /// group, each taking a copy of every datagram.
    fn release_user_multicast_listener(&self, group: Ipv4Addr) {
        let _ = group;
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

    /// Close every transport connection to a peer, identified by its advertised
    /// locators. Called when the DDS layer unmatches a remote participant, so
    /// per-peer resources (sockets, cache entries) are released promptly instead
    /// of lingering until OS keepalive. Connectionless transports have nothing
    /// to close — default no-op; only the TCP plugin overrides this.
    fn disconnect_peer(&self, _locators: &[Locator]) {}
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
        property: &PropertyQosPolicy,
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
                    UdpConfig::from_property(property),
                )?;
                Ok(Box::new(plugin))
            }
            TransportType::TCP => {
                use crate::rtps::transport::tcp::tcp_transport_plugin::TcpTransportPlugin;
                let plugin = TcpTransportPlugin::new_with_tls(
                    domain_id,
                    participant_id,
                    bind_ip,
                    working_ips,
                    guid_prefix,
                    tls_config,
                    TcpConfig::from_property(property),
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
                    HybridConfig::from_property(property),
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
                    UdpConfig::from_property(property),
                )?;
                Ok(Box::new(plugin))
            }
        }
    }
}
