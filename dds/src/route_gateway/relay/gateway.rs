//! The relay itself.
//!
//! The gateway is not a participant. It owns three UDP ports on its own network
//! and one link to the peer gateway, and it moves datagrams between them
//! without ever interpreting what they carry. The one datagram it does touch is
//! the SPDP announcement: rewriting the addresses inside it is what makes the
//! participants on both networks discover and match each other directly.

use std::{
    io::{self, BufReader},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket},
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
    config::{LinkRole, RelayConfig, ResolvedConfig},
    discovery_rewrite::{self, SpdpEndpoints},
    link::{self, Channel, Frame},
    peer_table::{PeerRoute, PeerTable, DEFAULT_LEASE},
    rtps_scan::{self, Announcement, ScannedMessage},
};

const RECV_BUFFER_LEN: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const DIAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);
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
    link_tx: Sender<Frame>,
    link_stream: Mutex<Option<TcpStream>>,
}

impl RelayGateway {
    pub fn start(config: RelayConfig) -> io::Result<Self> {
        let config = config.resolve()?;

        let discovery_socket = open_multicast_socket(&config)?;
        let metatraffic_socket = open_unicast_socket(config.metatraffic_port)?;
        let user_data_socket = open_unicast_socket(config.user_data_port)?;
        let multicast_target = SocketAddrV4::new(MULTICAST_IP, config.discovery_multicast_port);

        let (link_tx, link_rx) = bounded(LINK_QUEUE_LEN);
        let shared = Arc::new(Shared {
            config,
            table: PeerTable::default(),
            discovery_socket,
            metatraffic_socket,
            user_data_socket,
            multicast_target,
            link_tx,
            link_stream: Mutex::new(None),
        });
        let shutdown = Arc::new(AtomicBool::new(false));

        // Bound before any thread starts so that a port already in use is
        // reported to the caller instead of buried in a log line.
        let endpoint = LinkEndpoint::open(&shared.config.link)?;

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

        threads.push(spawn_named("relay-link-writer", {
            let shared = shared.clone();
            let shutdown = shutdown.clone();
            move || run_link_writer(shared, link_rx, shutdown)
        }));
        threads.push(spawn_named("relay-link-reader", {
            let shared = shared.clone();
            let shutdown = shutdown.clone();
            move || run_link_reader(shared, endpoint, shutdown)
        }));
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

    pub fn stop(self) {
        // Dropping performs the shutdown.
    }
}

impl Drop for RelayGateway {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);

        // The link reader blocks on the stream rather than polling, so it is
        // woken by tearing the stream down under it.
        if let Some(stream) = lock(&self.shared.link_stream).take() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
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

        let Some(destination) = scanned.dest_prefix else {
            return;
        };
        if self.table.is_remote(&destination) {
            self.enqueue(channel, datagram);
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
            let lease = discovery_rewrite::read_lease(payload).unwrap_or(DEFAULT_LEASE);
            self.table.insert_local(scanned.source_prefix, endpoints, lease, Instant::now());
        }

        self.enqueue(Channel::Metatraffic, datagram);
    }

    fn handle_link_frame(&self, frame: Frame) {
        let mut datagram = frame.payload;
        let Some(scanned) = rtps_scan::scan(&datagram) else {
            return;
        };

        if scanned.announcement == Some(Announcement::Participant) {
            self.inject_announcement(&mut datagram, &scanned);
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
    fn inject_announcement(&self, datagram: &mut [u8], scanned: &ScannedMessage) {
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
            self.table.insert_remote(scanned.source_prefix, lease, Instant::now());
        }

        if let Err(e) = self.discovery_socket.send_to(datagram, self.multicast_target) {
            log::warn!("Relay failed to inject an announcement: {}", e);
        }
    }

    /// True for the addresses this gateway writes into every announcement it
    /// forwards, which no participant of its own network can hold.
    fn advertises(&self, endpoints: &SpdpEndpoints) -> bool {
        *endpoints.metatraffic.ip() == self.config.lan_ip
            && endpoints.metatraffic.port() == self.config.metatraffic_port
    }

    /// Nothing behind the link is reachable while it is down, and the peer
    /// gateway announces all of it again once the link is back.
    fn handle_link_lost(&self) {
        *lock(&self.link_stream) = None;
        let forgotten = self.table.forget_remote();
        if forgotten > 0 {
            log::info!("Relay link lost, {} relayed participant(s) forgotten", forgotten);
        }
    }

    fn enqueue(&self, channel: Channel, datagram: &[u8]) {
        let frame = Frame { channel, payload: datagram.to_vec() };
        if self.link_tx.try_send(frame).is_err() {
            log::warn!("Relay link queue is full, datagram dropped");
        }
    }

    fn write_to_link(&self, frame: Frame) {
        let mut guard = lock(&self.link_stream);
        let Some(stream) = guard.as_ref() else {
            return;
        };
        if let Err(e) = link::write_frame(&mut &*stream, frame.channel, &frame.payload) {
            log::warn!("Relay link write failed: {}", e);
            // The reader is blocked on the same connection and would otherwise
            // sit there until TCP gives up, so it is torn down here.
            let _ = stream.shutdown(std::net::Shutdown::Both);
            *guard = None;
        }
    }
}

fn run_link_writer(shared: Arc<Shared>, link_rx: Receiver<Frame>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        match link_rx.recv_timeout(POLL_INTERVAL) {
            Ok(frame) => shared.write_to_link(frame),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// The end of the link this gateway owns. The listener is bound once and kept,
/// because a peer that goes away has to be able to come back.
enum LinkEndpoint {
    Listener(TcpListener),
    Dialer(SocketAddr),
}

impl LinkEndpoint {
    fn open(role: &LinkRole) -> io::Result<Self> {
        match role {
            LinkRole::Listen(address) => {
                let listener = TcpListener::bind(address)?;
                listener.set_nonblocking(true)?;
                Ok(LinkEndpoint::Listener(listener))
            }
            LinkRole::Connect(address) => Ok(LinkEndpoint::Dialer(*address)),
        }
    }

    fn connect(&self, shutdown: &AtomicBool) -> Option<TcpStream> {
        while !shutdown.load(Ordering::Relaxed) {
            let attempt = match self {
                LinkEndpoint::Listener(listener) => listener.accept().map(|(stream, _)| stream),
                LinkEndpoint::Dialer(address) => TcpStream::connect_timeout(address, POLL_INTERVAL),
            };
            match attempt {
                Ok(stream) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_nodelay(true);
                    return Some(stream);
                }
                Err(_) => thread::sleep(DIAL_RETRY_INTERVAL),
            }
        }
        None
    }
}

fn run_link_reader(shared: Arc<Shared>, endpoint: LinkEndpoint, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        let Some(stream) = endpoint.connect(&shutdown) else {
            break;
        };
        match stream.try_clone() {
            Ok(clone) => *lock(&shared.link_stream) = Some(clone),
            Err(e) => {
                log::error!("Relay could not share the link stream: {}", e);
                break;
            }
        }

        let mut reader = BufReader::new(stream);
        while !shutdown.load(Ordering::Relaxed) {
            match link::read_frame(&mut reader) {
                Ok(frame) => shared.handle_link_frame(frame),
                Err(e) => {
                    if !shutdown.load(Ordering::Relaxed) {
                        log::warn!("Relay link closed: {}", e);
                    }
                    break;
                }
            }
        }

        shared.handle_link_lost();
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
