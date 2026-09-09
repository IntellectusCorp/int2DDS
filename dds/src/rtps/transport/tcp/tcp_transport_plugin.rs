//! Facade bundling the two halves of the TCP transport.
//!
//! Inbound is a synchronous `TcpListener` handed to the stream listening task,
//! which polls it and classifies every frame. Outbound is the `TcpSender`,
//! which writes from whichever thread called it. Neither half owns a task
//! runtime.

use std::collections::HashSet;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use log::{debug, info};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::{is_same_host, Locator};
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::peer_spec::PeerSpec;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::connection_registry::{
    ConnectionRegistry, KeepaliveParams, TcpSocketTuning,
};
use crate::rtps::transport::tcp::framing::TcpFrameKind;
use crate::rtps::transport::tcp::peer_candidates::{AnnounceTargets, PEER_PRUNE_DELAY};
use crate::rtps::transport::tcp::tcp_listener::TcpListener;
use crate::rtps::transport::tcp::tcp_sender::TcpSender;
use crate::rtps::transport::tcp::tls::TlsConfig;
use crate::rtps::transport::{TcpConfig, TransportType};

// ── TcpTransportPlugin ──────────────────────────────────────────────────

/// Sync facade owning the runtime and forwarding trait calls to the async stack.
pub(crate) struct TcpTransportPlugin {
    domain_id: u32,
    participant_id: u32,
    working_ips: Vec<String>,
    listener_port: u16,

    /// The dial gate for addresses outside this domain's port block: a peer
    /// declared with a port may sit anywhere, so it is named exactly.
    dial_allowed: Vec<SocketAddr>,

    /// The hosts the configuration names, this one included. A participant on
    /// one of them is reachable anywhere in this domain's port block, whatever
    /// slot it settled on.
    dial_allowed_hosts: HashSet<IpAddr>,

    /// Where announcements go, and how that list settles as peers answer or
    /// fail to.
    announce_targets: Mutex<AnnounceTargets>,

    /// Public endpoint advertised in SPDP for WAN/NAT traversal (per participant).
    public_address: Option<SocketAddr>,

    /// Dial peers discovered at runtime that are not in `initial_peers`
    accept_undefined_peers: bool,

    /// Addresses already named by `report_unreachable_peer`, so one
    /// misconfigured peer is reported once instead of every announcement.
    reported_unreachable: Mutex<HashSet<SocketAddr>>,

    /// Shared with the sender, so the plugin can mark the transport shut down
    /// even when an outside `Arc` keeps the sender alive past it.
    shutdown: Arc<AtomicBool>,

    /// Outbound side. `Arc` because send paths and connect tasks hold clones.
    sender: Arc<TcpSender>,

    /// Inbound side, handed to the stream listening task by
    /// `take_stream_source()`.
    listener: Mutex<Option<TcpListener>>,

    /// Connection bookkeeping and the self-delivery queue, shared with the
    /// sender and with the stream listening task.
    shared: Arc<ConnectionRegistry>,
}

/// What TCP reports as the traffic it can absorb: the largest an SPDP
/// announcement can carry, since a stream has no point at which it drops.
const STREAM_UNBOUNDED_RECEIVE_BYTES: usize = u32::MAX as usize;

impl TcpTransportPlugin {
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        working_ips: Vec<String>,
        guid_prefix: GuidPrefix,
        tcp_config: TcpConfig,
    ) -> io::Result<Self> {
        Self::new_with_tls(
            domain_id,
            participant_id,
            working_ip,
            working_ips,
            guid_prefix,
            None,
            tcp_config,
        )
    }

    /// Build the plugin: bind the listener and create the sender. Neither half
    /// owns a task runtime, so nothing is spawned here.
    pub(crate) fn new_with_tls(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        working_ips: Vec<String>,
        guid_prefix: GuidPrefix,
        tls_config: Option<Arc<TlsConfig>>,
        tcp_config: TcpConfig,
    ) -> io::Result<Self> {
        let peers = Self::resolve_peers(&tcp_config.initial_peers);

        // TCP has no multicast: a peer is reached because it was named, never
        // because it was overheard. With the dial gate closed, an empty list is
        // a participant that can neither reach anyone nor be reached, so it is
        // refused here rather than left to look alive and stay silent.
        if tcp_config.transport_type == TransportType::TCP
            && !tcp_config.accept_undefined_peers
            && peers.is_empty()
        {
            log::error!(
                "[TcpTransportPlugin] No peers configured (domain={}). TCP has no multicast, so \
                 every peer has to be named: set the int2dds.initial_peers QoS property (or \
                 INT2DDS_INITIAL_PEERS) to '127.0.0.1:0' for this host, to '<ip>:0' to search \
                 another host, or to an exact ip:port.",
                domain_id
            );
            return Err(transport_io_error(
                TransportErrorCode::TcpBindFailed,
                format!(
                    "No initial peers configured for the TCP transport (domain={domain_id}); \
                     TCP cannot discover a peer it was not told about"
                ),
            ));
        }

        let dial_allowed = Self::allowed_dial_addresses(&peers);
        let dial_allowed_hosts = Self::allowed_dial_hosts(&peers);
        let announce_targets = Mutex::new(AnnounceTargets::new(
            Self::candidates(domain_id, &peers, tcp_config.peer_search_slots),
            Some(PEER_PRUNE_DELAY),
            Instant::now(),
        ));

        let tuning = TcpSocketTuning {
            nodelay: tcp_config.nodelay,
            so_rcvbuf: tcp_config.so_rcvbuf,
            so_sndbuf: tcp_config.so_sndbuf,
            unacked_timeout: tcp_config.unacked_timeout,
            keepalive: Some(KeepaliveParams {
                time: tcp_config.keepalive_interval,
                interval: tcp_config.keepalive_timeout,
                retries: tcp_config.keepalive_max_misses,
            }),
        };

        let shutdown = Arc::new(AtomicBool::new(false));

        let (listener, participant_id) = Self::bind_listener(
            domain_id,
            participant_id,
            tuning,
            tls_config.clone(),
            &tcp_config,
        )?;
        let listener_port = listener.port();

        let shared =
            Arc::new(ConnectionRegistry::new(domain_id, participant_id, guid_prefix, tuning));

        // Every address this participant answers on, so the sender can tell
        // a frame aimed at ourselves from one aimed at a peer.
        let mut local_ips = working_ips.clone();
        if !local_ips.contains(&working_ip) {
            local_ips.push(working_ip);
        }

        let sender = TcpSender::new(
            domain_id,
            participant_id,
            local_ips,
            listener_port,
            guid_prefix,
            tls_config,
            Arc::clone(&shared),
            &tcp_config,
            Arc::clone(&shutdown),
        );

        info!(
            "[TcpTransportPlugin] Created (domain={}, pid={}, port={})",
            domain_id, participant_id, listener_port
        );

        Ok(Self {
            domain_id,
            participant_id,
            working_ips,
            listener_port,
            dial_allowed,
            dial_allowed_hosts,
            announce_targets,
            public_address: tcp_config.public_address,
            accept_undefined_peers: tcp_config.accept_undefined_peers,
            reported_unreachable: Mutex::new(HashSet::new()),
            shutdown,
            sender,
            listener: Mutex::new(Some(listener)),
            shared,
        })
    }

    /// Bind the listener this participant answers on, and report which
    /// participant id it ended up owning.
    ///
    /// An explicit `bind_port` is taken as given and never searched around: the
    /// operator named that port, so a conflict has to surface rather than be
    /// worked around. Otherwise the domain formula is walked upwards until a free
    /// slot is found, which is what lets several participants share a host while
    /// every port stays computable by a peer. The walk stops at the end of this
    /// domain's own block — one step further would hand out the next domain's
    /// base port and break the isolation between domains.
    fn bind_listener(
        domain_id: u32,
        first_participant_id: u32,
        tuning: TcpSocketTuning,
        tls_config: Option<Arc<TlsConfig>>,
        tcp_config: &TcpConfig,
    ) -> io::Result<(TcpListener, u32)> {
        let open = |port: u16, tls_config: Option<Arc<TlsConfig>>| {
            TcpListener::new(
                port,
                tuning,
                tls_config,
                tcp_config.tls_handshake_timeout,
                tcp_config.first_frame_timeout,
            )
        };

        if let Some(port) = tcp_config.bind_port {
            return open(port, tls_config)
                .map(|listener| (listener, first_participant_id))
                .map_err(|e| {
                    log::error!(
                        "[TcpTransportPlugin] Failed to bind TCP listener on the configured \
                         port {} (domain={}): {}. int2dds.transport.TCPv4.bind_port is used \
                         as given, so give each participant its own port or drop the property \
                         to have one picked from the domain formula.",
                        port,
                        domain_id,
                        e
                    );
                    transport_io_error(
                        TransportErrorCode::TcpBindFailed,
                        format!(
                            "Failed to bind TCP listener on the configured port {} (domain={}): {}",
                            port, domain_id, e
                        ),
                    )
                });
        }

        let mut last_error = None;
        for participant_id in first_participant_id..=PortManager::MAX_TCP_PARTICIPANT_ID {
            let port = PortManager::get_tcp_physical_port(domain_id, participant_id);
            match open(port, tls_config.clone()) {
                Ok(listener) => return Ok((listener, participant_id)),
                Err(e) => {
                    debug!(
                        "[TcpTransportPlugin] Port {} taken, trying participant id {}",
                        port,
                        participant_id + 1
                    );
                    last_error = Some(e);
                }
            }
        }

        let last_port =
            PortManager::get_tcp_physical_port(domain_id, PortManager::MAX_TCP_PARTICIPANT_ID);
        let first_port = PortManager::get_tcp_physical_port(domain_id, first_participant_id);
        log::error!(
            "[TcpTransportPlugin] No free TCP listening port for domain {}: ports {}..={} are \
             all in use. A domain holds at most {} participants per host; the next port belongs \
             to domain {}.",
            domain_id,
            first_port,
            last_port,
            PortManager::MAX_TCP_PARTICIPANT_ID + 1,
            domain_id + 1
        );
        Err(transport_io_error(
            TransportErrorCode::TcpBindFailed,
            match last_error {
                Some(e) => format!(
                    "No free TCP listening port for domain {} (tried {}..={}): {}",
                    domain_id, first_port, last_port, e
                ),
                None => format!(
                    "No TCP listening port left for domain {}: participant id {} is past the \
                     domain's last slot {}",
                    domain_id,
                    first_participant_id,
                    PortManager::MAX_TCP_PARTICIPANT_ID
                ),
            },
        ))
    }

    /// Both discovery and user-data advertise the same standard TCP locator.
    /// Traffic separation is carried by frame kind, not by a logical port.
    fn advertised_tcp_locators(&self) -> Vec<Locator> {
        // 1. Explicit WAN/NAT public endpoint takes precedence.
        if let Some(public_addr) = self.public_address {
            if let std::net::IpAddr::V4(v4) = public_addr.ip() {
                log::info!(
                    "[TcpTransportPlugin] WAN mode: advertising public address \
                     {} instead of :{}",
                    public_addr,
                    self.listener_port
                );
                return vec![Locator::from_tcp_v4(v4, public_addr.port() as u32)];
            }
            log::warn!(
                "[TcpTransportPlugin] Public address is not IPv4, \
                 falling back to LAN NICs"
            );
        }

        // 2. Generic IP override (carried over from develop's `init_locators`).
        if let Some(ext_ip) = crate::common::env::get_external_address() {
            return vec![Locator::from_tcp_v4(ext_ip, self.listener_port as u32)];
        }

        // 3. All local NICs.
        self.working_ips
            .iter()
            .filter_map(|ip_str| ip_str.parse::<Ipv4Addr>().ok())
            .map(|ip| Locator::from_tcp_v4(ip, self.listener_port as u32))
            .collect()
    }

    /// The loopback address a peer on this host is reached under, and `None` for
    /// a peer anywhere else.
    fn loopback_form(ip: IpAddr) -> Option<IpAddr> {
        if !is_same_host(SocketAddr::new(ip, 0)) {
            return None;
        }
        Some(match ip {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        })
    }

    /// Every address an outbound dial is permitted to reach: the configured
    /// peers, plus the loopback address discovery substitutes for the ones that
    /// live on this host.
    fn allowed_dial_addresses(initial_peers: &[PeerSpec]) -> Vec<SocketAddr> {
        let mut allowed = Vec::new();
        for peer in initial_peers {
            let Some(port) = peer.port else {
                continue;
            };
            let forms = std::iter::once(peer.ip).chain(Self::loopback_form(peer.ip));
            for addr in forms.map(|ip| SocketAddr::new(ip, port)) {
                if !allowed.contains(&addr) {
                    allowed.push(addr);
                }
            }
        }
        allowed
    }

    /// Every host an outbound dial is permitted to reach: the configured hosts,
    /// plus the loopback address that stands for the ones living on this host.
    fn allowed_dial_hosts(initial_peers: &[PeerSpec]) -> HashSet<IpAddr> {
        initial_peers
            .iter()
            .flat_map(|peer| std::iter::once(peer.ip).chain(Self::loopback_form(peer.ip)))
            .collect()
    }

    /// Settle the configured list into the peers this participant announces to.
    ///
    /// A host given the wildcard port says its ports are unknown and stands for
    /// every participant slot of the domain. A host given a real port says the
    /// opposite and stands for that one address. Naming the same host both ways
    /// is a contradiction, and the wider reading wins: the wildcard is what the
    /// operator meant to search, and the ports written beside it would only
    /// narrow what they already asked to be searched in full.
    fn resolve_peers(initial_peers: &[PeerSpec]) -> Vec<PeerSpec> {
        let searched: HashSet<IpAddr> =
            initial_peers.iter().filter(|peer| peer.port.is_none()).map(|peer| peer.ip).collect();

        let mut peers: Vec<PeerSpec> = Vec::with_capacity(initial_peers.len());
        for peer in initial_peers {
            if peer.port.is_some() && searched.contains(&peer.ip) {
                log::debug!(
                    "[TcpTransportPlugin] {} also carries the wildcard port, searching its \
                     whole domain range instead",
                    peer.ip
                );
                continue;
            }
            if !peers.contains(peer) {
                peers.push(*peer);
            }
        }
        peers
    }

    /// The addresses announcements start out going to.
    ///
    /// A peer declared with a port is one address and is marked as such, so the
    /// list keeps it whatever happens. A peer named by host alone becomes one
    /// address per participant slot the configuration says a host may hold,
    /// which is guesswork the list is free to narrow down.
    fn candidates(domain_id: u32, peers: &[PeerSpec], slots: u32) -> Vec<(SocketAddr, bool)> {
        let slot = |index: u32| PortManager::get_tcp_physical_port(domain_id, index);
        peers
            .iter()
            .flat_map(|peer| {
                let declared = peer.port.is_some();
                peer.expand(slot, slots).into_iter().map(move |addr| (addr, declared))
            })
            .collect()
    }

    /// Whether `port` is one this domain hands out to its own participants.
    ///
    /// The block ends where the next domain's begins, so a port inside it can
    /// only ever belong to a participant of this domain.
    fn is_in_domain_block(&self, port: u16) -> bool {
        let first = PortManager::get_tcp_physical_port(self.domain_id, 0);
        let last =
            PortManager::get_tcp_physical_port(self.domain_id, PortManager::MAX_TCP_PARTICIPANT_ID);
        (first..=last).contains(&port)
    }

    /// Whether an outbound dial to `addr` is permitted. Restricted to the
    /// configured hosts unless `accept_undefined_peers` is set or no peer was
    /// configured at all (dial-all fallback).
    ///
    /// A configured host is trusted at any port of this domain's block, not only
    /// at the ports that were written down. Which slot a participant settles on
    /// depends on what else was running when it started, so pinning the gate to
    /// the listed ports would leave a peer that shifted by one slot discovered
    /// but unreachable. Narrowing inside a host buys nothing anyway — a host is
    /// one trust boundary, and separating them is what TLS is for.
    ///
    /// Our own listener is exempt: nobody lists themselves as an initial peer,
    /// and what a Reader and a Writer of this participant exchange is not a dial
    /// — the sender keeps it in-process.
    fn should_dial(&self, addr: &SocketAddr) -> bool {
        self.sender.is_self_connection(addr)
            || self.accept_undefined_peers
            || (self.dial_allowed.is_empty() && self.dial_allowed_hosts.is_empty())
            || self.dial_allowed.contains(addr)
            || (self.dial_allowed_hosts.contains(&addr.ip())
                && self.is_in_domain_block(addr.port()))
    }

    /// Report a peer this participant has discovered but is not allowed to
    /// answer, once per address.
    ///
    /// Record that a participant lives at `addr`, so announcements keep going
    /// there and the address is never given up on.
    fn note_participant_at(&self, addr: SocketAddr) {
        let first_sighting = match self.announce_targets.lock() {
            Ok(mut targets) => targets.confirm(addr),
            Err(poisoned) => poisoned.into_inner().confirm(addr),
        };
        if first_sighting {
            self.sender.forget_failures(addr);
        }
    }

    /// Only an address on a host this participant was meant to reach is
    /// reported — one already named in the configuration, or this host itself.
    /// A port outside the list there means a participant landed on a slot the
    /// list does not cover and the two will never match. A locator on any other
    /// host is the dial gate doing its job: a peer with several NICs advertises
    /// one locator per NIC, and skipping the ones not named is expected rather
    /// than a fault.
    fn report_unreachable_peer(&self, addr: &SocketAddr) {
        if !self.dial_allowed_hosts.contains(&addr.ip()) && !is_same_host(*addr) {
            return;
        }
        let first_report = match self.reported_unreachable.lock() {
            Ok(mut reported) => reported.insert(*addr),
            Err(_) => false,
        };
        if first_report {
            log::error!(
                "[TcpTransportPlugin] Discovered a participant at {} but it is not reachable \
                 through int2dds.initial_peers, so nothing will be sent to it and the two will \
                 not match. That host was meant to be reachable, so a participant landed on a \
                 port the list does not cover: add {} to the list.",
                addr,
                addr
            );
        }
    }
}

impl TransportPlugin for TcpTransportPlugin {
    /// A frame the socket will not take is queued rather than dropped, so no
    /// amount of traffic has to be held back to keep the peer from losing it.
    /// Reporting the largest value the announcement can carry is how that is
    /// said: a sender then never withholds bytes on this transport's account.
    ///
    /// The send queue's own budget still bounds memory, but exceeding it
    /// surfaces as an error the reliable path repairs, not as the silent loss
    /// a send window exists to avoid.
    fn advertised_receive_buffer_size(&self) -> Option<usize> {
        Some(STREAM_UNBOUNDED_RECEIVE_BYTES)
    }

    fn send(&self, data: &[u8], target: &SendTarget) -> io::Result<()> {
        match target {
            SendTarget::SPDPDiscovery { .. } => {
                // The list announcements go to is this transport's own: it
                // starts from the configuration and narrows to the peers that
                // answer, which the caller has no way to track.
                let targets = match self.announce_targets.lock() {
                    Ok(mut targets) => targets.due(Instant::now()),
                    Err(poisoned) => poisoned.into_inner().due(Instant::now()),
                };
                // Best-effort: a peer that fails must not abort the broadcast.
                for peer_addr in &targets {
                    let _ = self.sender.send_to_discovery(peer_addr, data);
                }
                Ok(())
            }
            SendTarget::SEDPDiscovery(locator) | SendTarget::UserData(locator) => {
                if !locator.is_tcp() {
                    return Err(io::Error::new(io::ErrorKind::Unsupported, locator.kind_name()));
                }
                let addr = SocketAddr::new(
                    std::net::IpAddr::V4(locator.to_ip_v4_addr()),
                    locator.port() as u16,
                );
                if !self.should_dial(&addr) {
                    debug!("[TcpTransportPlugin] skip non-initial-peer locator {}", addr);
                    self.report_unreachable_peer(&addr);
                    return Ok(());
                }
                // Reaching this point means a participant was discovered there,
                // which is the only evidence that settles a guessed address.
                self.note_participant_at(addr);
                let kind = match target {
                    SendTarget::SEDPDiscovery(_) => TcpFrameKind::Discovery,
                    SendTarget::UserData(_) => TcpFrameKind::UserData,
                    SendTarget::SPDPDiscovery { .. } => unreachable!(),
                };
                self.sender.send_to(addr, kind, data)
            }
        }
    }

    fn can_handle(&self, locator: &Locator) -> bool {
        locator.is_tcp()
    }

    fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
        self.advertised_tcp_locators()
    }

    fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
        self.advertised_tcp_locators()
    }

    fn advertised_default_multicast_locators(&self, _groups: Vec<Ipv4Addr>) -> Vec<Locator> {
        // TCP has no multicast — discovery uses unicast fan-out via SPDP.
        Vec::new()
    }

    fn ensure_user_multicast_listener(&self, _group: Ipv4Addr) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TCP cannot receive user data over multicast",
        ))
    }

    fn take_discovery_multicast_source(&self) -> Option<MessageSource> {
        // TCP has no multicast — discovery uses unicast fan-out via SPDP.
        None
    }

    fn take_discovery_unicast_source(&self) -> Option<MessageSource> {
        None
    }

    fn take_user_data_unicast_source(&self) -> Option<MessageSource> {
        None
    }

    fn take_stream_source(&self) -> Option<MessageSource> {
        let listener = self.listener.lock().expect("tcp listener lock").take()?;
        Some(MessageSource::Stream { listener, shared: Arc::clone(&self.shared) })
    }

    fn port(&self) -> u16 {
        self.listener_port
    }

    fn tcp_listener_port(&self) -> Option<u16> {
        Some(self.listener_port)
    }

    fn participant_id(&self) -> u32 {
        self.participant_id
    }

    fn close(&self) {
        // 1. One flag covers both halves, so neither side can be taken for
        //    live once close() has started.
        self.shutdown.store(true, Ordering::Release);

        // 2. The stream listening task owns the listener once it has taken it,
        //    and has already stopped by the time close() runs. Anything still
        //    in the slot never reached a task and is simply dropped.
        if let Ok(mut guard) = self.listener.lock() {
            drop(guard.take());
        }

        // 3. Drop every outbound connection.
        self.sender.shutdown();

        debug!("[TcpTransportPlugin] Closed");
    }

    fn disconnect_peer(&self, locators: &[Locator]) {
        let mut seen: Vec<SocketAddr> = Vec::new();
        for locator in locators {
            if !locator.is_tcp() {
                continue;
            }
            let addr = SocketAddr::new(
                std::net::IpAddr::V4(locator.to_ip_v4_addr()),
                locator.port() as u16,
            );
            if !seen.contains(&addr) {
                seen.push(addr);
                let now = Instant::now();
                match self.announce_targets.lock() {
                    Ok(mut targets) => targets.revoke(addr, now),
                    Err(poisoned) => poisoned.into_inner().revoke(addr, now),
                }
                self.sender.disconnect_peer(addr);
            }
        }
    }
}

impl Drop for TcpTransportPlugin {
    fn drop(&mut self) {
        // Best-effort fallback when close() was not called explicitly. The
        // shared flag reaches the sender without depending on the last sender
        // Arc dying here.
        self.shutdown.store(true, Ordering::Release);

        if let Ok(mut guard) = self.listener.lock() {
            drop(guard.take());
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    use crate::rtps::transport::peer_spec::DEFAULT_PARTICIPANTS_PER_HOST;
    use crate::rtps::transport::tcp::framing::{test_framed, test_message};

    fn take_listener(plugin: &TcpTransportPlugin) -> TcpListener {
        match plugin.take_stream_source().expect("stream source") {
            MessageSource::Stream { listener, .. } => listener,
            _ => panic!("expected a stream source"),
        }
    }

    fn pump(listener: &mut TcpListener, count: usize) -> Vec<(TcpFrameKind, bytes::Bytes)> {
        let mut poll = mio::Poll::new().expect("poll");
        let mut events = mio::Events::with_capacity(64);
        let listener_token = listener.register(poll.registry()).expect("register");
        let mut collected = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);

        while collected.len() < count && std::time::Instant::now() < deadline {
            poll.poll(&mut events, Some(std::time::Duration::from_millis(50))).expect("poll");
            for event in &events {
                let token = event.token();
                if token == listener_token {
                    listener.accept_ready(poll.registry()).expect("accept");
                    continue;
                }
                while let Ok(Some(message)) = listener.get_message(token, poll.registry()) {
                    collected.push((message.kind, message.data));
                }
            }
        }
        collected
    }

    const BUILTIN_WRITER: u8 = 0xC2;
    const USER_WRITER: u8 = 0x02;

    /// Keep domain IDs unique across tests to avoid port collisions when
    /// the test suite runs in parallel.
    fn next_test_domain() -> u32 {
        static NEXT: AtomicU32 = AtomicU32::new(900);
        NEXT.fetch_add(1, Ordering::SeqCst)
    }

    fn make_plugin(domain: u32) -> TcpTransportPlugin {
        // These tests exercise the listener/runtime mechanics, not discovery, so
        // satisfy the pure-TCP initial-peers requirement with a dummy peer.
        let cfg = TcpConfig {
            initial_peers: vec!["127.0.0.1:7400".parse().unwrap()],
            bind_port: Some(0),
            ..TcpConfig::default()
        };
        // Bind an ephemeral port rather than the domain-derived fixed port: the
        // latter lingers in TIME_WAIT and makes back-to-back suite runs fail with
        // AddrInUse. The tests below check against the *actual* listener port, so
        // the ephemeral choice is transparent to them.
        TcpTransportPlugin::new(
            domain,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0u8; 12],
            cfg,
        )
        .expect("plugin creation")
    }

    fn make_scanning_plugin(domain: u32, bind_port: Option<u16>) -> io::Result<TcpTransportPlugin> {
        let cfg = TcpConfig {
            initial_peers: vec!["127.0.0.1:7400".parse().unwrap()],
            bind_port,
            ..TcpConfig::default()
        };
        TcpTransportPlugin::new(
            domain,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0u8; 12],
            cfg,
        )
    }

    /// A peer named by an address this host owns is also reachable at the
    /// loopback address discovery substitutes for it.
    #[test]
    fn a_same_host_peer_is_also_allowed_at_its_loopback_form() {
        let configured: SocketAddr = "127.0.0.2:8650".parse().unwrap();

        let allowed = TcpTransportPlugin::allowed_dial_addresses(&[configured.into()]);

        assert!(allowed.contains(&configured));
        assert!(allowed.contains(&"127.0.0.1:8650".parse().unwrap()));
    }

    /// A host named with a port stands for that address alone.
    #[test]
    fn a_host_named_with_a_port_is_one_address() {
        let declared: PeerSpec = "192.168.0.5:7400".parse().unwrap();

        let peers = TcpTransportPlugin::resolve_peers(&[declared]);

        assert_eq!(peers, vec![declared]);
        assert_eq!(
            TcpTransportPlugin::candidates(0, &peers, DEFAULT_PARTICIPANTS_PER_HOST),
            vec![("192.168.0.5:7400".parse().unwrap(), true)]
        );
    }

    /// A host given the wildcard port stands for every slot of the domain.
    #[test]
    fn a_host_given_the_wildcard_is_the_whole_domain_range() {
        let named: PeerSpec = "192.168.0.5:0".parse().unwrap();

        let candidates = TcpTransportPlugin::candidates(
            0,
            &TcpTransportPlugin::resolve_peers(&[named]),
            DEFAULT_PARTICIPANTS_PER_HOST,
        );

        assert_eq!(candidates.len(), DEFAULT_PARTICIPANTS_PER_HOST as usize);
        assert!(candidates.iter().all(|(_, declared)| !declared), "a guess is not declared");
        assert_eq!(
            candidates[0].0,
            format!("192.168.0.5:{}", PortManager::get_tcp_physical_port(0, 0)).parse().unwrap()
        );
    }

    /// Naming one host both ways is a contradiction, and the search wins.
    #[test]
    fn a_host_named_both_ways_is_searched_in_full() {
        let named: PeerSpec = "192.168.0.5:0".parse().unwrap();
        let declared: PeerSpec = "192.168.0.5:7400".parse().unwrap();
        let elsewhere: PeerSpec = "192.168.0.6:7400".parse().unwrap();

        let peers = TcpTransportPlugin::resolve_peers(&[declared, named, elsewhere]);

        assert_eq!(peers, vec![named, elsewhere], "the port beside the searched host is dropped");
    }

    /// Nothing is added behind the operator's back: the list is what they wrote.
    #[test]
    fn no_host_is_taken_in_that_was_not_named() {
        let named: PeerSpec = "192.168.0.5:0".parse().unwrap();

        assert_eq!(TcpTransportPlugin::resolve_peers(&[named]), vec![named]);
        assert!(TcpTransportPlugin::resolve_peers(&[]).is_empty());
    }

    /// With the dial gate closed and no peer named, the participant could
    /// neither reach anyone nor be reached, so it must not come up at all.
    #[test]
    fn a_tcp_participant_without_peers_is_refused() {
        let cfg = TcpConfig {
            bind_port: Some(0),
            transport_type: TransportType::TCP,
            accept_undefined_peers: false,
            initial_peers: Vec::new(),
            ..TcpConfig::default()
        };

        let result = TcpTransportPlugin::new(
            next_test_domain(),
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0u8; 12],
            cfg,
        );

        assert!(result.is_err());
    }

    /// A configured host is reachable at any slot of this domain's block, so a
    /// participant that shifted by a slot is still spoken to.
    #[test]
    fn a_configured_host_is_dialable_across_the_whole_domain_block() {
        const DOMAIN: u32 = 3;
        let first_slot = PortManager::get_tcp_physical_port(DOMAIN, 0);
        let cfg = TcpConfig {
            bind_port: Some(0),
            initial_peers: vec![format!("127.0.0.1:{first_slot}").parse().unwrap()],
            ..TcpConfig::default()
        };
        let plugin = TcpTransportPlugin::new(
            DOMAIN,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0u8; 12],
            cfg,
        )
        .expect("plugin creation");

        let other_slot = PortManager::get_tcp_physical_port(DOMAIN, 7);
        assert!(plugin.should_dial(&format!("127.0.0.1:{other_slot}").parse().unwrap()));

        let next_domain_slot = PortManager::get_tcp_physical_port(DOMAIN + 1, 0);
        assert!(
            !plugin.should_dial(&format!("127.0.0.1:{next_domain_slot}").parse().unwrap()),
            "a port of the next domain is not this domain's to dial"
        );
        assert!(
            !plugin.should_dial(&format!("203.0.113.7:{other_slot}").parse().unwrap()),
            "an unconfigured host stays out regardless of the port"
        );

        plugin.close();
    }

    /// A configuration written only with the wildcard port carries no address at
    /// all, and the gate has to read that as a host list rather than as nothing
    /// configured.
    #[test]
    fn a_host_named_only_by_the_wildcard_still_closes_the_gate() {
        const DOMAIN: u32 = 4;
        let cfg = TcpConfig {
            bind_port: Some(0),
            initial_peers: vec!["127.0.0.1:0".parse().unwrap()],
            ..TcpConfig::default()
        };
        let plugin = TcpTransportPlugin::new(
            DOMAIN,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0u8; 12],
            cfg,
        )
        .expect("plugin creation");

        let slot = PortManager::get_tcp_physical_port(DOMAIN, 5);
        assert!(plugin.should_dial(&format!("127.0.0.1:{slot}").parse().unwrap()));
        assert!(
            !plugin.should_dial(&format!("203.0.113.7:{slot}").parse().unwrap()),
            "a host nobody named stays out"
        );

        plugin.close();
    }

    /// A host given the wildcard carries no port, so the gate has to admit it by
    /// host alone — including under the loopback address its locators are
    /// rewritten to once the peer turns out to live here.
    #[test]
    fn a_same_host_peer_named_by_the_wildcard_is_dialable_at_its_loopback_form() {
        const DOMAIN: u32 = 9;
        let own_ip: IpAddr = "127.0.0.2".parse().unwrap();
        let cfg = TcpConfig {
            bind_port: Some(0),
            initial_peers: vec![PeerSpec::searched(own_ip)],
            ..TcpConfig::default()
        };
        let plugin = TcpTransportPlugin::new(
            DOMAIN,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0u8; 12],
            cfg,
        )
        .expect("plugin creation");

        let slot = PortManager::get_tcp_physical_port(DOMAIN, 5);
        assert!(plugin.should_dial(&SocketAddr::new(own_ip, slot)), "the declared form");
        assert!(
            plugin.should_dial(&format!("127.0.0.1:{slot}").parse().unwrap()),
            "the loopback form discovery substitutes for it"
        );
        assert!(
            !plugin.should_dial(&format!("203.0.113.7:{slot}").parse().unwrap()),
            "a host nobody named stays out"
        );

        plugin.close();
    }

    /// A peer on another host is never rewritten, so the gate stays exactly as
    /// the operator declared it.
    #[test]
    fn a_peer_on_another_host_is_left_as_configured() {
        let configured: SocketAddr = "203.0.113.7:8650".parse().unwrap();

        assert_eq!(
            TcpTransportPlugin::allowed_dial_addresses(&[configured.into()]),
            vec![configured]
        );
    }

    /// Two participants on one host and domain both come up, on adjacent slots
    /// of the same domain block.
    #[test]
    fn a_second_participant_takes_the_next_slot_in_the_domain_block() {
        let domain = next_test_domain();
        let first = make_scanning_plugin(domain, None).expect("first plugin");
        let second = make_scanning_plugin(domain, None).expect("second plugin");

        assert_eq!(first.participant_id(), 0);
        assert_eq!(second.participant_id(), 1);
        assert_eq!(
            second.tcp_listener_port().expect("second port"),
            first.tcp_listener_port().expect("first port") + 2
        );

        first.close();
        second.close();
    }

    /// A pinned port is used as given: the walk that finds a free slot must not
    /// silently move a participant off the port an operator named.
    #[test]
    fn a_pinned_bind_port_is_never_searched_around() {
        let occupied = std::net::TcpListener::bind("0.0.0.0:0").expect("occupy a port");
        let port = occupied.local_addr().expect("local addr").port();

        let result = make_scanning_plugin(next_test_domain(), Some(port));

        assert!(result.is_err());
        drop(occupied);
    }

    /// Plugin construction succeeds and the OS accepts TCP connections on
    /// the reported listener port.
    #[test]
    fn plugin_creates_and_listens() {
        let plugin = make_plugin(next_test_domain());
        let port = plugin.tcp_listener_port().expect("listener port");
        assert!(port != 0);

        let _stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .expect("connect to listener");

        plugin.close();
    }

    /// One stream source carries both kinds, and it is handed out exactly once.
    #[test]
    fn take_sources_are_one_shot() {
        let plugin = make_plugin(next_test_domain());

        assert!(plugin.take_discovery_unicast_source().is_none());
        assert!(plugin.take_user_data_unicast_source().is_none());

        assert!(plugin.take_stream_source().is_some());
        assert!(plugin.take_stream_source().is_none());

        plugin.close();
    }

    /// Dropping the plugin marks the outbound side shut down even when the
    /// sender's own `Drop` cannot: an outside `Arc` keeps the sender alive
    /// past the plugin.
    #[test]
    fn drop_shuts_down_the_outbound_side_through_the_shared_flag() {
        let plugin = make_plugin(next_test_domain());
        let port = plugin.tcp_listener_port().expect("listener port");

        let client =
            std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).expect("client connect");

        let sender = Arc::clone(&plugin.sender);
        assert!(!sender.is_shut_down());

        drop(plugin);

        assert!(
            sender.is_shut_down(),
            "plugin Drop must shut the outbound side down via the shared flag"
        );

        drop(client);
    }

    /// `close()` is idempotent and does not hang on the second call.
    #[test]
    fn close_is_idempotent() {
        let plugin = make_plugin(next_test_domain());
        plugin.close();
        plugin.close();
    }

    /// Discovery and user-data advertise the same standard physical locator.
    #[test]
    fn advertised_locators_share_the_physical_port() {
        let plugin = make_plugin(next_test_domain());
        let port = plugin.tcp_listener_port().expect("listener port");

        let meta = plugin.advertised_metatraffic_unicast_locators();
        assert!(!meta.is_empty(), "expected at least one advertised locator");
        for loc in &meta {
            assert!(loc.is_tcp());
            assert_eq!(loc.port(), port as u32);
        }

        let def = plugin.advertised_default_unicast_locators();
        for loc in &def {
            assert_eq!(loc.port(), port as u32);
        }
        assert_eq!(meta, def);

        plugin.close();
    }

    /// A sender at participant_id 0 reaches a listener at participant_id 1
    /// through the shared physical locator.
    #[test]
    fn send_reaches_listener_with_mismatched_participant_id() {
        let domain = next_test_domain();
        let recv_port: u16 = 17601;
        let send_port: u16 = 17602;

        // Receiver at participant_id 1.
        let recv_cfg = TcpConfig {
            bind_port: Some(recv_port),
            initial_peers: vec![format!("127.0.0.1:{recv_port}").parse().unwrap()],
            ..TcpConfig::default()
        };
        let receiver = TcpTransportPlugin::new(
            domain,
            1,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0x11u8; 12],
            recv_cfg,
        )
        .expect("receiver");

        // Sender at participant_id 0, allowed to dial the receiver.
        let send_cfg = TcpConfig {
            bind_port: Some(send_port),
            initial_peers: vec![format!("127.0.0.1:{recv_port}").parse().unwrap()],
            ..TcpConfig::default()
        };
        let sender = TcpTransportPlugin::new(
            domain,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0x00u8; 12],
            send_cfg,
        )
        .expect("sender");

        let mut listener = take_listener(&receiver);

        // Participant ids do not affect the shared physical TCP locator.
        let loc =
            receiver.advertised_default_unicast_locators().into_iter().next().expect("locator");
        assert_eq!(loc.port(), recv_port as u32);

        let rtps = test_message(USER_WRITER, b"payload");
        sender.send(&rtps, &SendTarget::UserData(&loc)).expect("send");

        let received = pump(&mut listener, 1);
        assert_eq!(received.len(), 1, "receiver got no frame despite pid mismatch");
        assert_eq!(received[0].0, TcpFrameKind::UserData);
        assert_eq!(received[0].1.as_ref(), test_framed(&rtps).as_slice());

        sender.close();
        receiver.close();
    }

    #[test]
    fn discovery_and_user_data_open_two_role_connections() {
        let domain = next_test_domain();
        let recv_cfg = TcpConfig {
            bind_port: Some(0),
            initial_peers: vec!["127.0.0.1:1".parse().unwrap()],
            ..TcpConfig::default()
        };
        let receiver = TcpTransportPlugin::new(
            domain,
            1,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0x22; 12],
            recv_cfg,
        )
        .unwrap();
        let receiver_addr: SocketAddr =
            format!("127.0.0.1:{}", receiver.listener_port).parse().unwrap();

        let send_cfg = TcpConfig {
            bind_port: Some(0),
            initial_peers: vec![receiver_addr.into()],
            ..TcpConfig::default()
        };
        let sender = TcpTransportPlugin::new(
            domain,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0x11; 12],
            send_cfg,
        )
        .unwrap();

        let mut listener = take_listener(&receiver);
        let locator = Locator::from_tcp_v4(Ipv4Addr::LOCALHOST, receiver.listener_port as u32);

        let discovery = test_message(BUILTIN_WRITER, b"discovery");
        let user = test_message(USER_WRITER, b"user");
        sender.send(&discovery, &SendTarget::SEDPDiscovery(&locator)).unwrap();
        sender.send(&user, &SendTarget::UserData(&locator)).unwrap();
        let received = pump(&mut listener, 2);
        assert_eq!(received.len(), 2, "both frames must arrive");
        let discovery_frame = received
            .iter()
            .find(|(kind, _)| *kind == TcpFrameKind::Discovery)
            .expect("discovery frame");
        let user_frame =
            received.iter().find(|(kind, _)| *kind == TcpFrameKind::UserData).expect("user frame");
        assert_eq!(discovery_frame.1.as_ref(), test_framed(&discovery).as_slice());
        assert_eq!(user_frame.1.as_ref(), test_framed(&user).as_slice());
        assert_eq!(sender.sender.connection_count(), 2);
        assert_eq!(listener.connection_count(), 2);

        sender.close();
        receiver.close();
    }

    /// The fan-out hands its dials to a worker, so the announcement that
    /// triggered one still has to reach the peer, and a peer nobody answers must
    /// not hold up the peers that follow it in the list.
    #[test]
    fn an_announcement_survives_the_handoff_to_the_dial_worker() {
        let domain = next_test_domain();
        let recv_cfg = TcpConfig {
            bind_port: Some(0),
            initial_peers: vec!["127.0.0.1:1".parse().unwrap()],
            ..TcpConfig::default()
        };
        let receiver = TcpTransportPlugin::new(
            domain,
            1,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0x44; 12],
            recv_cfg,
        )
        .unwrap();
        let receiver_addr: SocketAddr =
            format!("127.0.0.1:{}", receiver.listener_port).parse().unwrap();

        let dead: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let send_cfg = TcpConfig {
            bind_port: Some(0),
            initial_peers: vec![dead.into(), receiver_addr.into()],
            ..TcpConfig::default()
        };
        let sender = TcpTransportPlugin::new(
            domain,
            0,
            "127.0.0.1".to_string(),
            vec!["127.0.0.1".to_string()],
            [0x33; 12],
            send_cfg,
        )
        .unwrap();

        let mut listener = take_listener(&receiver);
        let announcement = test_message(BUILTIN_WRITER, b"spdp");
        sender.send(&announcement, &SendTarget::SPDPDiscovery { initial_peers: &[] }).unwrap();

        let received = pump(&mut listener, 1);
        assert_eq!(received.len(), 1, "the announcement must reach the peer that answers");
        assert_eq!(received[0].0, TcpFrameKind::Discovery);
        assert_eq!(received[0].1.as_ref(), test_framed(&announcement).as_slice());

        sender.close();
        receiver.close();
    }
}
