//! The relay itself.
//!
//! The gateway is not a participant. It owns three UDP ports on its own network
//! and one link per peer gateway, and it moves datagrams between them without
//! ever interpreting what they carry. The one datagram it does touch is the
//! SPDP announcement: rewriting the addresses inside it is what makes the
//! participants on both networks discover and match each other directly.
//!
//! Links never feed each other. A datagram that arrives on one link is either
//! delivered to this network or dropped, so several peers form a star around
//! each network and no datagram can circle between gateways.

use std::{
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use crate::rtps::common::locator::MULTICAST_IP;

use super::{
    config::{RelayConfig, ResolvedConfig},
    discovery_rewrite::{self, SpdpEndpoints},
    link::{self, Channel, Frame, LinkConnection, LinkEndpoint, LinkId},
    peer_policy::Verdict,
    peer_table::{PeerRoute, PeerTable, DEFAULT_LEASE},
    rtps_scan::{self, Announcement, GuidPrefix, ScannedMessage},
    stats::{LinkCounters, LinkStats},
};

const RECV_BUFFER_LEN: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const LINK_QUEUE_LEN: usize = 1024;

pub struct RelayGateway {
    shared: Arc<Shared>,
    shutdown: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

struct Shared {
    config: ResolvedConfig,
    table: PeerTable,
    discovery_socket: UdpSocket,
    metatraffic_socket: UdpSocket,
    user_data_socket: UdpSocket,
    multicast_target: SocketAddrV4,
    links: Vec<Link>,
    counters: LinkCounters,
}

/// One peer gateway: the queue feeding it and the connection carrying it,
/// which is absent whenever that peer is unreachable.
struct Link {
    tx: Sender<Frame>,
    connection: Mutex<Option<Arc<dyn LinkConnection>>>,
}

impl RelayGateway {
    pub fn start(config: RelayConfig) -> io::Result<Self> {
        let config = config.resolve()?;

        let discovery_socket = open_multicast_socket(&config)?;
        let metatraffic_socket = open_unicast_socket(config.metatraffic_port)?;
        let user_data_socket = open_unicast_socket(config.user_data_port)?;
        let multicast_target = SocketAddrV4::new(MULTICAST_IP, config.discovery_multicast_port);

        // Bound before any thread starts so that a port already in use is
        // reported to the caller instead of buried in a log line.
        let endpoints = config
            .links
            .iter()
            .map(link::open_endpoint)
            .collect::<io::Result<Vec<Box<dyn LinkEndpoint>>>>()?;

        let mut links = Vec::with_capacity(endpoints.len());
        let mut queues = Vec::with_capacity(endpoints.len());
        for _ in &endpoints {
            let (tx, rx) = bounded(LINK_QUEUE_LEN);
            links.push(Link { tx, connection: Mutex::new(None) });
            queues.push(rx);
        }

        let shared = Arc::new(Shared {
            config,
            table: PeerTable::default(),
            discovery_socket,
            metatraffic_socket,
            user_data_socket,
            multicast_target,
            links,
            counters: LinkCounters::default(),
        });
        let shutdown = Arc::new(AtomicBool::new(false));

        let mut threads = Vec::new();
        threads.push(spawn_receiver(
            "relay-lan-discovery",
            shared.clone(),
            shutdown.clone(),
            |shared| &shared.discovery_socket,
            |shared, datagram| shared.handle_lan(datagram, Channel::Metatraffic),
        ));
        threads.push(spawn_receiver(
            "relay-lan-metatraffic",
            shared.clone(),
            shutdown.clone(),
            |shared| &shared.metatraffic_socket,
            |shared, datagram| shared.handle_lan(datagram, Channel::Metatraffic),
        ));
        threads.push(spawn_receiver(
            "relay-lan-user-data",
            shared.clone(),
            shutdown.clone(),
            |shared| &shared.user_data_socket,
            |shared, datagram| shared.handle_lan(datagram, Channel::UserData),
        ));

        for (index, (queue, endpoint)) in queues.into_iter().zip(endpoints).enumerate() {
            let id = LinkId(index);
            threads.push(spawn_named(&format!("relay-link-writer-{}", index), {
                let shared = shared.clone();
                let shutdown = shutdown.clone();
                move || run_link_writer(shared, id, queue, shutdown)
            }));
            threads.push(spawn_named(&format!("relay-link-reader-{}", index), {
                let shared = shared.clone();
                let shutdown = shutdown.clone();
                move || run_link_reader(shared, id, endpoint, shutdown)
            }));
        }
        threads.push(spawn_named("relay-reaper", {
            let shared = shared.clone();
            let shutdown = shutdown.clone();
            move || run_reaper(shared, shutdown)
        }));

        Ok(Self { shared, shutdown, threads })
    }

    /// Address this gateway advertises on behalf of every relayed participant.
    pub fn advertised_address(&self) -> Ipv4Addr {
        self.shared.config.lan_ip
    }

    /// What has crossed the link so far, split by direction and channel.
    pub fn link_stats(&self) -> LinkStats {
        self.shared.counters.read()
    }

    pub fn stop(self) {
        // Dropping performs the shutdown.
    }
}

impl Drop for RelayGateway {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);

        // Each link reader blocks on its connection rather than polling, so it
        // is woken by tearing that connection down under it.
        for link in &self.shared.links {
            if let Some(connection) = lock(&link.connection).take() {
                connection.close();
            }
        }

        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Shared {
    /// Anything a local participant sent. An announcement is routed by what it
    /// says rather than by the port it arrived on, because a participant says
    /// goodbye on both the discovery group and the metatraffic port of every
    /// peer it knows.
    fn handle_lan(&self, datagram: &[u8], channel: Channel) {
        let Some(scanned) = rtps_scan::scan(datagram) else {
            return;
        };
        if scanned.announcement == Some(Announcement::Participant) {
            self.handle_lan_announcement(datagram, &scanned);
            return;
        }

        // A participant the policy turned away still hears the announcements
        // this gateway injects, and would address the far network directly. It
        // is held back here rather than at the announcement alone, because only
        // participants the table admitted may put anything on a link.
        if !self.table.is_local(&scanned.source_prefix) {
            return;
        }

        let Some(destination) = scanned.dest_prefix else {
            return;
        };
        if let Some(link) = self.table.remote_link(&destination) {
            self.enqueue(link, channel, datagram);
        }
    }

    /// A local participant announcing itself. Its real addresses are recorded
    /// before the announcement is handed to the peer gateway untouched: the
    /// rewrite belongs to whichever gateway delivers it, because only that one
    /// knows the address its own network can reach.
    fn handle_lan_announcement(&self, datagram: &[u8], scanned: &ScannedMessage) {
        // Announcements this gateway injected come straight back on the group.
        if self.table.is_remote(&scanned.source_prefix) {
            return;
        }

        if scanned.is_departure {
            self.table.remove(&scanned.source_prefix);
        } else if let Some(range) = scanned.payload.clone() {
            let payload = &datagram[range];
            let Some(endpoints) = discovery_rewrite::read_endpoints(payload) else {
                return;
            };
            // The table may have been cleared since the injection, which leaves
            // the advertised address as the only proof of where it came from.
            if self.advertises(&endpoints) {
                return;
            }
            if !self.admits(payload, &scanned.source_prefix) {
                return;
            }
            let lease = discovery_rewrite::read_lease(payload).unwrap_or(DEFAULT_LEASE);
            self.table.insert_local(scanned.source_prefix, endpoints, lease, Instant::now());
        }

        self.enqueue_everywhere(Channel::Metatraffic, datagram);
    }

    fn handle_link_frame(&self, link: LinkId, frame: Frame) {
        self.counters.count_received(frame.channel, frame.payload.len());

        let mut datagram = frame.payload;
        let Some(scanned) = rtps_scan::scan(&datagram) else {
            return;
        };

        if scanned.announcement == Some(Announcement::Participant) {
            self.inject_announcement(link, &mut datagram, &scanned);
            return;
        }

        let Some(destination) = scanned.dest_prefix else {
            return;
        };
        let Some(PeerRoute::Local(endpoints)) = self.table.route(&destination) else {
            return;
        };

        // An endpoint from the far network advertises addresses of its own that
        // this network cannot reach, so they are taken out before delivery.
        if scanned.announcement == Some(Announcement::Endpoint) {
            if let Some(range) = scanned.payload.clone() {
                discovery_rewrite::strip_endpoint_locators(&mut datagram[range]);
            }
        }

        let (socket, target) = match frame.channel {
            Channel::Metatraffic => (&self.metatraffic_socket, endpoints.metatraffic),
            Channel::UserData => (&self.user_data_socket, endpoints.user_data),
        };
        if let Err(e) = socket.send_to(&datagram, target) {
            log::warn!("Relay failed to deliver a datagram to {}: {}", target, e);
        }
    }

    /// A participant on the far network, seen through the link. It is given the
    /// gateway's own addresses so that this network can reach it at all.
    fn inject_announcement(&self, link: LinkId, datagram: &mut [u8], scanned: &ScannedMessage) {
        // Whatever the peer gateway says about this network came from here.
        if self.table.is_local(&scanned.source_prefix) {
            return;
        }

        if scanned.is_departure {
            self.table.remove(&scanned.source_prefix);
        } else if let Some(range) = scanned.payload.clone() {
            let lease =
                discovery_rewrite::read_lease(&datagram[range.clone()]).unwrap_or(DEFAULT_LEASE);
            let rewritten = discovery_rewrite::rewrite(
                &mut datagram[range],
                self.config.lan_ip,
                self.config.metatraffic_port,
                self.config.user_data_port,
                self.config.lan_domain_id,
            );
            if !rewritten {
                log::warn!("Relay dropped an announcement it could not rewrite");
                return;
            }
            self.table.insert_remote(scanned.source_prefix, link, lease, Instant::now());
        }

        if let Err(e) = self.discovery_socket.send_to(datagram, self.multicast_target) {
            log::warn!("Relay failed to inject an announcement: {}", e);
        }
    }

    /// Weighs a local participant against the configured policy. A refusal is
    /// logged once per announcement rather than kept, because an operator
    /// reading the log is the only one who can act on it.
    fn admits(&self, payload: &[u8], prefix: &GuidPrefix) -> bool {
        let addresses = discovery_rewrite::read_unicast_addresses(payload);
        let verdict = self.config.policy.judge(
            &addresses,
            self.table.local_count(),
            self.table.is_local(prefix),
        );
        match verdict {
            Verdict::Admit => true,
            Verdict::Denied => {
                log::debug!("Relay policy turned away a participant at {:?}", addresses);
                false
            }
            Verdict::OverCapacity => {
                log::warn!(
                    "Relay is already carrying {} participant(s), turning away {:?}",
                    self.table.local_count(),
                    addresses
                );
                false
            }
        }
    }

    /// True for the addresses this gateway writes into every announcement it
    /// forwards, which no participant of its own network can hold.
    fn advertises(&self, endpoints: &SpdpEndpoints) -> bool {
        *endpoints.metatraffic.ip() == self.config.lan_ip
            && endpoints.metatraffic.port() == self.config.metatraffic_port
    }

    /// Nothing behind a link is reachable while it is down, and that peer
    /// gateway announces all of it again once the link is back. The other
    /// links keep their participants.
    fn handle_link_lost(&self, link: LinkId) {
        *lock(&self.links[link.0].connection) = None;
        let forgotten = self.table.forget_remote(link);
        if forgotten > 0 {
            log::info!(
                "Relay link {} lost, {} relayed participant(s) forgotten",
                link.0,
                forgotten
            );
        }
    }

    /// Announcements go to every peer, because which of them holds a matching
    /// participant is exactly what is not known yet.
    fn enqueue_everywhere(&self, channel: Channel, datagram: &[u8]) {
        for index in 0..self.links.len() {
            self.enqueue(LinkId(index), channel, datagram);
        }
    }

    fn enqueue(&self, link: LinkId, channel: Channel, datagram: &[u8]) {
        let frame = Frame { channel, payload: datagram.to_vec() };
        if self.links[link.0].tx.try_send(frame).is_err() {
            log::warn!("Relay queue for link {} is full, datagram dropped", link.0);
        }
    }

    fn write_to_link(&self, link: LinkId, frame: Frame) {
        let mut guard = lock(&self.links[link.0].connection);
        let Some(connection) = guard.as_ref() else {
            return;
        };
        match connection.send(frame.channel, &frame.payload) {
            Ok(()) => self.counters.count_sent(frame.channel, frame.payload.len()),
            Err(e) => {
                log::warn!("Relay link write failed: {}", e);
                // The reader is blocked on the same connection and would
                // otherwise sit there until the transport gives up, so it is
                // torn down here.
                connection.close();
                *guard = None;
            }
        }
    }
}

fn run_link_writer(
    shared: Arc<Shared>,
    link: LinkId,
    queue: Receiver<Frame>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match queue.recv_timeout(POLL_INTERVAL) {
            Ok(frame) => shared.write_to_link(link, frame),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn run_link_reader(
    shared: Arc<Shared>,
    link: LinkId,
    endpoint: Box<dyn LinkEndpoint>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        let Some(connection) = endpoint.connect(&shutdown) else {
            break;
        };
        *lock(&shared.links[link.0].connection) = Some(connection.clone());

        while !shutdown.load(Ordering::Relaxed) {
            match connection.recv() {
                Ok(frame) => shared.handle_link_frame(link, frame),
                Err(e) => {
                    if !shutdown.load(Ordering::Relaxed) {
                        log::warn!("Relay link {} closed: {}", link.0, e);
                    }
                    break;
                }
            }
        }

        shared.handle_link_lost(link);
    }
}

fn run_reaper(shared: Arc<Shared>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        thread::sleep(POLL_INTERVAL);
        let expired = shared.table.purge_expired(Instant::now());
        if expired > 0 {
            log::debug!("Relay forgot {} participant(s) that stopped announcing", expired);
        }
    }
}

fn spawn_receiver<S, H>(
    name: &str,
    shared: Arc<Shared>,
    shutdown: Arc<AtomicBool>,
    select: S,
    handle: H,
) -> JoinHandle<()>
where
    S: Fn(&Shared) -> &UdpSocket + Send + 'static,
    H: Fn(&Shared, &[u8]) + Send + 'static,
{
    spawn_named(name, move || {
        let mut buffer = vec![0u8; RECV_BUFFER_LEN];
        while !shutdown.load(Ordering::Relaxed) {
            match select(&shared).recv_from(&mut buffer) {
                Ok((length, _)) => handle(&shared, &buffer[..length]),
                Err(ref e) if is_timeout(e) => continue,
                Err(e) => {
                    if !shutdown.load(Ordering::Relaxed) {
                        log::warn!("Relay receive failed: {}", e);
                    }
                }
            }
        }
    })
}

fn spawn_named<F: FnOnce() + Send + 'static>(name: &str, body: F) -> JoinHandle<()> {
    thread::Builder::new().name(name.to_string()).spawn(body).expect("failed to spawn relay thread")
}

fn open_multicast_socket(config: &ResolvedConfig) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.join_multicast_v4(&MULTICAST_IP, &config.lan_ip)?;
    socket.set_multicast_if_v4(&config.lan_ip)?;
    socket.set_multicast_loop_v4(true)?;
    socket.bind(&SockAddr::from(SocketAddr::from((
        Ipv4Addr::UNSPECIFIED,
        config.discovery_multicast_port,
    ))))?;

    let socket: UdpSocket = socket.into();
    socket.set_read_timeout(Some(POLL_INTERVAL))?;
    Ok(socket)
}

fn open_unicast_socket(port: u16) -> io::Result<UdpSocket> {
    let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))?;
    socket.set_read_timeout(Some(POLL_INTERVAL))?;
    Ok(socket)
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
