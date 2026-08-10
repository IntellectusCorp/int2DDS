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
    time::Duration,
};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use crate::rtps::common::locator::MULTICAST_IP;

use super::{
    config::{LinkRole, RelayConfig, ResolvedConfig},
    link::{self, Channel, Frame},
    peer_table::{PeerRoute, PeerTable},
    rtps_scan, spdp_rewrite,
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

        let mut threads = Vec::new();
        threads.push(spawn_receiver(
            "relay-lan-discovery",
            shared.clone(),
            shutdown.clone(),
            |shared| &shared.discovery_socket,
            |shared, datagram| shared.handle_lan_discovery(datagram),
        ));
        threads.push(spawn_receiver(
            "relay-lan-metatraffic",
            shared.clone(),
            shutdown.clone(),
            |shared| &shared.metatraffic_socket,
            |shared, datagram| shared.handle_lan_unicast(datagram, Channel::Metatraffic),
        ));
        threads.push(spawn_receiver(
            "relay-lan-user-data",
            shared.clone(),
            shutdown.clone(),
            |shared| &shared.user_data_socket,
            |shared, datagram| shared.handle_lan_unicast(datagram, Channel::UserData),
        ));

        threads.push(spawn_named("relay-link-writer", {
            let shared = shared.clone();
            let shutdown = shutdown.clone();
            move || run_link_writer(shared, link_rx, shutdown)
        }));
        threads.push(spawn_named("relay-link-reader", {
            let shared = shared.clone();
            let shutdown = shutdown.clone();
            move || run_link_reader(shared, shutdown)
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
    /// A local participant announcing itself. Its real addresses are recorded
    /// before the announcement is handed to the peer gateway untouched: the
    /// rewrite belongs to whichever gateway delivers it, because only that one
    /// knows the address its own network can reach.
    fn handle_lan_discovery(&self, datagram: &[u8]) {
        let Some(scanned) = rtps_scan::scan(datagram) else {
            return;
        };
        if !scanned.is_spdp {
            return;
        }
        // Announcements this gateway injected come straight back on the group.
        if self.table.is_remote(&scanned.source_prefix) {
            return;
        }

        if let Some(range) = scanned.spdp_payload {
            if let Some(endpoints) = spdp_rewrite::read_endpoints(&datagram[range]) {
                self.table.insert_local(scanned.source_prefix, endpoints);
            }
        }

        self.enqueue(Channel::Metatraffic, datagram);
    }

    /// Anything a local participant addressed to a relayed participant. The
    /// destination is the only thing read; the datagram stays opaque.
    fn handle_lan_unicast(&self, datagram: &[u8], channel: Channel) {
        let Some(scanned) = rtps_scan::scan(datagram) else {
            return;
        };
        let Some(destination) = scanned.dest_prefix else {
            return;
        };
        if self.table.is_remote(&destination) {
            self.enqueue(channel, datagram);
        }
    }

    fn handle_link_frame(&self, frame: Frame) {
        let mut datagram = frame.payload;
        let Some(scanned) = rtps_scan::scan(&datagram) else {
            return;
        };

        if scanned.is_spdp {
            if self.table.is_local(&scanned.source_prefix) {
                return;
            }
            self.table.insert_remote(scanned.source_prefix);

            if let Some(range) = scanned.spdp_payload {
                let rewritten = spdp_rewrite::rewrite(
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
            }

            if let Err(e) = self.discovery_socket.send_to(&datagram, self.multicast_target) {
                log::warn!("Relay failed to inject an announcement: {}", e);
            }
            return;
        }

        let Some(destination) = scanned.dest_prefix else {
            return;
        };
        let Some(PeerRoute::Local(endpoints)) = self.table.route(&destination) else {
            return;
        };
        let (socket, target) = match frame.channel {
            Channel::Metatraffic => (&self.metatraffic_socket, endpoints.metatraffic),
            Channel::UserData => (&self.user_data_socket, endpoints.user_data),
        };
        if let Err(e) = socket.send_to(&datagram, target) {
            log::warn!("Relay failed to deliver a datagram to {}: {}", target, e);
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

fn run_link_reader(shared: Arc<Shared>, shutdown: Arc<AtomicBool>) {
    let stream = match shared.config.link {
        LinkRole::Listen(address) => accept_peer(address, &shutdown),
        LinkRole::Connect(address) => dial_peer(address, &shutdown),
    };
    let Some(stream) = stream else {
        return;
    };

    match stream.try_clone() {
        Ok(clone) => *lock(&shared.link_stream) = Some(clone),
        Err(e) => {
            log::error!("Relay could not share the link stream: {}", e);
            return;
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
}

fn accept_peer(address: SocketAddr, shutdown: &AtomicBool) -> Option<TcpStream> {
    let listener = match TcpListener::bind(address) {
        Ok(listener) => listener,
        Err(e) => {
            log::error!("Relay could not listen on {}: {}", address, e);
            return None;
        }
    };
    if listener.set_nonblocking(true).is_err() {
        return None;
    }

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_nodelay(true);
                return Some(stream);
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(DIAL_RETRY_INTERVAL);
            }
            Err(e) => {
                log::error!("Relay accept failed: {}", e);
                return None;
            }
        }
    }
    None
}

fn dial_peer(address: SocketAddr, shutdown: &AtomicBool) -> Option<TcpStream> {
    while !shutdown.load(Ordering::Relaxed) {
        match TcpStream::connect_timeout(&address, POLL_INTERVAL) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Some(stream);
            }
            Err(_) => thread::sleep(DIAL_RETRY_INTERVAL),
        }
    }
    None
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
