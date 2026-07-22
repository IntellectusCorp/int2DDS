//! Outbound side of the TCP mux transport.
//!
//! `TcpSender` owns the outbound connection cache and reader/writer task lifecycles,
//! exposing sync `send_to_*` methods for the DDS layer.
//! A connect runs TCP + (optional) TLS + the 3-way handshake
//! (PEER_HELLO → PORT_RESERVE → PORT_BIND), then hands the stream to a
//! reader/writer task pair.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use log::{debug, warn};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::connection_registry::{apply_socket_tuning, ConnectionRegistry};
use crate::rtps::transport::tcp::connection_tasks::{
    inbox_capacity, spawn_connection_tasks, SharedWriteHalf,
};
use crate::rtps::transport::tcp::framing::{read_framed_message, write_framed_message};
use crate::rtps::transport::tcp::protocol::{encode_locator, ControlMsg};
use crate::rtps::transport::tcp::stream::{wrap_plain, AsyncConnStream};
use crate::rtps::transport::tcp::tls::{connect_tls_async, TlsConfig};
use crate::rtps::transport::TcpConfig;

/// Logical port 0 = control connection
pub(crate) const CONTROL_LOGICAL_PORT: u16 = 0;

// ── Cache entry types ───────────────────────────────────────────────────────

/// An entry in the sender's outbound cache.
struct OutboundEntry {
    /// Control inbox feeding the connection's `writer_task`. Used by the
    /// reader task and the PORT_RESERVE round-trip to enqueue protocol frames;
    /// not used by the user-data send path. A closed inbox means the task pair
    /// has exited, which the send path treats as a dead connection.
    writer_tx: mpsc::Sender<Vec<u8>>,
    /// User-data writes acquire this directly via
    /// `runtime.block_on(write_half.lock().await)` and perform the wire
    /// `writev` inline on the calling thread.
    write_half: SharedWriteHalf,
    /// Cancel token for this connection's actor pair (the same token stored in
    /// `ConnectionRegistry::ConnectionEntry::cancel`). `evict_connection` fires it to tear
    /// down just this connection on a write failure, without touching the peer.
    cancel: CancellationToken,
    /// `control` is `Some` only for
    /// control connections (logical_port == CONTROL_LOGICAL_PORT)
    control: Option<ControlExtras>,
}

#[derive(Clone)]
struct ControlExtras {
    /// Same `Arc` that lives in `ConnectionRegistry::ConnectionEntry::pending_ack`.
    /// dispatch installs the response; the awaiting connect_task takes it.
    pending_ack: Arc<StdMutex<Option<oneshot::Sender<ControlMsg>>>>,
    /// Lets only one PORT_RESERVE round-trip run at a time, so concurrent
    /// requests don't clobber each other's `pending_ack` slot.
    request_lock: Arc<TokioMutex<()>>,
}

/// Borrowed handle to the cache entry of a control connection. Holds clones
/// of the channels needed to issue + await a PORT_RESERVE request.
struct ControlConnHandle {
    writer_tx: mpsc::Sender<Vec<u8>>,
    extras: ControlExtras,
}

// ── In-flight connect guard ─────────────────────────────────────────────────

/// Guard that prevents duplicate concurrent connects to the same peer.
/// While it lives, the peer is marked "connecting" in `in_flight` so other
/// tasks wait instead of opening a second connection. On drop (success,
/// error, panic, or cancel) it clears the mark and wakes the waiters.
struct InFlightGuard {
    map: Arc<DashMap<(SocketAddr, u16), Arc<Notify>>>,
    key: (SocketAddr, u16),
    notify: Arc<Notify>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.map.remove(&self.key);
        self.notify.notify_waiters();
    }
}

/// Result of trying to claim the connect slot for a peer.
enum InFlightAcquisition {
    /// We claimed the slot — the caller must perform the connect.
    Acquired(InFlightGuard),
    /// Another task was already connecting (we waited for it) — the caller
    /// should just re-check the cache.
    AlreadyDone,
}

// ── TcpSender ────────────────────────────────────────────────────────────────

/// Outbound side of the TCP mux transport. See module-level docs.
pub(crate) struct TcpSender {
    domain_id: u32,
    participant_id: u32,
    working_ip: String,
    listener_port: u16,
    public_addr: Option<SocketAddr>,
    #[allow(dead_code)]
    local_guid_prefix: GuidPrefix,

    connect_timeout: Duration,
    handshake_timeout: Duration,

    tls_config: Option<Arc<TlsConfig>>,
    shared: Arc<ConnectionRegistry>,

    /// Outbound connection cache, keyed by (addr, logical_port).
    connections: Arc<DashMap<(SocketAddr, u16), OutboundEntry>>,

    /// Marks peers currently being connected, so duplicate concurrent
    /// connects to the same peer are prevented. See `InFlightGuard`.
    in_flight: Arc<DashMap<(SocketAddr, u16), Arc<Notify>>>,

    /// Handle to the runtime, saved when the sender is built.
    /// The sync `send_to_*` methods run outside the runtime, so they cannot
    /// use `tokio::spawn` (it would panic). They use this handle instead to
    /// run async work (`block_on`) and start connect tasks.
    runtime_handle: tokio::runtime::Handle,

    cancel: CancellationToken,
}

impl TcpSender {
    /// Build a sender. Must be called inside a tokio runtime context so the
    /// captured `Handle` is valid for the sync `send_to_*` block_on path.
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        listener_port: u16,
        local_guid_prefix: GuidPrefix,
        tls_config: Option<Arc<TlsConfig>>,
        shared: Arc<ConnectionRegistry>,
        tcp_config: &TcpConfig,
    ) -> Arc<Self> {
        let cancel = CancellationToken::new();
        let runtime_handle = tokio::runtime::Handle::current();

        // Prefer the configured public address (WAN/NAT).
        let public_addr = tcp_config.public_address;

        Arc::new(Self {
            domain_id,
            participant_id,
            working_ip,
            listener_port,
            public_addr,
            local_guid_prefix,
            connect_timeout: tcp_config.connect_timeout,
            handshake_timeout: tcp_config.bind_timeout,
            tls_config,
            shared,
            connections: Arc::new(DashMap::new()),
            in_flight: Arc::new(DashMap::new()),
            runtime_handle,
            cancel,
        })
    }

    /// Resolve the connection's shared write half, establishing it on cache
    /// miss. Blocks the caller (via `send_to`'s `block_on`) until the
    /// connection is ready.
    async fn ensure_connection(
        self: &Arc<Self>,
        addr: SocketAddr,
        logical_port: u16,
    ) -> io::Result<SharedWriteHalf> {
        let key = (addr, logical_port);

        // Reuse a live cached connection. A dead one — task pair gone, its
        // writer inbox closed — is evicted here so the connect below rebuilds it
        // on demand. A data connection always (re)establishes its control
        // connection first via `do_connect_data`, so no orphaned data link lingers.
        let cached_dead = match self.connections.get(&key) {
            Some(entry) if !entry.writer_tx.is_closed() => {
                return Ok(Arc::clone(&entry.write_half));
            }
            Some(_) => true,
            None => false,
        };
        if cached_dead {
            self.evict_connection(addr, logical_port);
        }

        // Reconnect backoff: while the peer is in its backoff window from a
        // recent failure, fail fast instead of attempting (and blocking on) a
        // fresh connect. This bounds reconnect churn during a long outage.
        if let Some(retry_in) = self.backoff_remaining(addr) {
            return Err(transport_io_error(
                TransportErrorCode::TcpReconnectBackoff,
                format!("peer {addr} in reconnect backoff ({retry_in:?} remaining)"),
            ));
        }

        let result = if logical_port == CONTROL_LOGICAL_PORT {
            do_connect_control(self, addr).await.map(|_| ())
        } else {
            do_connect_data(self, addr, logical_port).await.map(|_| ())
        };

        if let Err(e) = result {
            warn!(
                "TcpSender: outbound connect to {:?} (port={}) failed: {:?}",
                addr, logical_port, e
            );
            // If a connection fails to be established, the failure is reported
            // and backoff logic is triggered.
            self.note_connect_failure(addr);
            return Err(e);
        }

        // If the connection is successfully established, report success
        // and reset the backoff.
        self.note_connect_success(addr);

        self.connections.get(&key).map(|entry| Arc::clone(&entry.write_half)).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "connect succeeded but cache entry missing")
        })
    }

    /// Tear down a single cached connection `(addr, logical_port)` and its
    /// actor pair, leaving every other connection to the peer untouched.
    ///
    /// Used on a write failure: the write proves only that *this* connection is
    /// unusable. Firing the cancel token wakes the
    /// task pair, whose reader exit calls `ConnectionRegistry::remove_connection` to drop
    /// the shared-state entry.
    fn evict_connection(&self, addr: SocketAddr, logical_port: u16) {
        if let Some((_, entry)) = self.connections.remove(&(addr, logical_port)) {
            entry.cancel.cancel();
        }
    }

    /// Tear down every outbound connection to `addr` (all logical ports) and its
    /// actor pairs. Called when the DDS layer unmatches the peer,
    /// so its transport resources are released promptly.
    /// Cancelling each actor wakes its reader, which drops the matching `ConnectionRegistry` entry.
    pub(crate) fn disconnect_peer(&self, addr: SocketAddr) {
        self.connections.retain(|(peer_addr, _), entry| {
            let keep = *peer_addr != addr;
            if !keep {
                entry.cancel.cancel();
            }
            keep
        });
        self.shared.clear_backoff(addr);
        self.shared.remove_peer_by_addr(addr);
    }

    // Reconnect backoff lives in `ConnectionRegistry` (shared with the inbound
    // path); these thin wrappers keep the outbound call sites unchanged.

    fn backoff_remaining(&self, addr: SocketAddr) -> Option<Duration> {
        self.shared.backoff_remaining(addr)
    }

    fn note_connect_failure(&self, addr: SocketAddr) {
        self.shared.note_connect_failure(addr);
    }

    fn note_connect_success(&self, addr: SocketAddr) {
        self.shared.clear_backoff(addr);
    }

    /// Graceful shutdown — cancels the shared token, tearing down every
    /// connection actor via its child token.
    pub(crate) async fn shutdown(&self) {
        self.cancel.cancel();
    }

    /// Heuristic: is this address our own listener? Avoids loopback
    /// self-connections during SPDP fan-out.
    fn is_self_connection(&self, addr: &SocketAddr) -> bool {
        if addr.port() != self.listener_port {
            return false;
        }
        match addr.ip() {
            IpAddr::V4(v4) => v4.is_loopback() || self.working_ip == v4.to_string(),
            IpAddr::V6(_) => false,
        }
    }
}

// ── Send path ────────────────────────────────────────────────────────────────
//
// Sends a single RTPS frame inline on the user thread. The connection's
// `SharedWriteHalf` is locked for the duration of one `write_vectored`
// syscall; the calling thread waits via `runtime.block_on` until the
// kernel accepts the bytes.
impl TcpSender {
    /// SPDP bootstrap fan-out to an initial peer we have not discovered yet.
    /// The peer's participant id (hence its logical port) is unknown, so we
    /// reserve the well-known index-0 metatraffic port.
    /// Once SPDP completes, SEDP and user data reserve the
    /// peer's actual advertised logical port via `send_to`.
    pub(crate) fn send_to_discovery(
        self: &Arc<Self>,
        addr: &SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        let logical_port = PortManager::get_discovery_traffic_unicast_port(self.domain_id, 0);
        self.send_to(*addr, logical_port, data)
    }

    /// Resolve the connection's write half — fast path on cache hit,
    /// otherwise block until the connection (control + data handshake) is
    /// established — then perform the wire `writev` inline. `logical_port` is the
    /// destination's advertised RTPS port, reserved on the mux connection.
    pub(crate) fn send_to(
        self: &Arc<Self>,
        addr: SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<()> {
        let key = (addr, logical_port);

        // A dead entry (writer inbox closed) falls through to ensure_connection, which evicts and rebuilds.
        // The cache guard is dropped before sending so the rebuild can remove
        // the dead entry without a DashMap shard self-deadlock.
        let cached = match self.connections.get(&key) {
            Some(entry) if !entry.writer_tx.is_closed() => Some(Arc::clone(&entry.write_half)),
            _ => None,
        };
        let write_half = match cached {
            Some(wh) => wh,
            None => self.runtime_handle.block_on(self.ensure_connection(addr, logical_port))?,
        };

        let payload = data.to_vec();
        let res = self.runtime_handle.block_on(async move {
            let mut wh = write_half.lock().await;
            write_framed_message(&mut *wh, &payload).await
        });
        match res {
            Ok(()) => Ok(()),
            Err(e) => {
                warn!(
                    "TcpSender: write to {:?} (port={}) failed: {:?} — evicting connection",
                    addr, logical_port, e
                );
                self.evict_connection(addr, logical_port);
                Err(e)
            }
        }
    }
}

impl Drop for TcpSender {
    fn drop(&mut self) {
        // Cancel the shared token so every connection actor (child token) exits
        // at its next yield. Best-effort — Drop cannot await their teardown.
        self.cancel.cancel();
    }
}

/// Atomic acquire — returns `Acquired` if we got the slot, or `AlreadyDone`
/// after waiting for the existing in-flight task to finish.
async fn acquire_in_flight(sender: &Arc<TcpSender>, key: (SocketAddr, u16)) -> InFlightAcquisition {
    enum Slot {
        Existing(Arc<Notify>),
        New(Arc<Notify>),
    }

    // Atomic check-and-insert. Entry guard is dropped before any await.
    let slot = match sender.in_flight.entry(key) {
        Entry::Occupied(e) => Slot::Existing(e.get().clone()),
        Entry::Vacant(v) => {
            let n = Arc::new(Notify::new());
            v.insert(n.clone());
            Slot::New(n)
        }
    };

    match slot {
        Slot::Existing(notify) => {
            notify.notified().await;
            InFlightAcquisition::AlreadyDone
        }
        Slot::New(notify) => InFlightAcquisition::Acquired(InFlightGuard {
            map: Arc::clone(&sender.in_flight),
            key,
            notify,
        }),
    }
}

// ── do_connect_control ──────────────────────────────────────────────────────

async fn do_connect_control(
    sender: &Arc<TcpSender>,
    addr: SocketAddr,
) -> io::Result<mpsc::Sender<Vec<u8>>> {
    if sender.is_self_connection(&addr) {
        return Err(io::Error::new(io::ErrorKind::AddrInUse, "self connect"));
    }

    // 1. TCP (+ TLS).
    let mut stream = create_stream(sender, addr).await?;

    // 2. PEER_HELLO + PEER_HELLO_ACK (inline, before the tasks take the stream).
    let locator = if let Some(SocketAddr::V4(pub_v4)) = sender.public_addr {
        encode_locator(*pub_v4.ip(), pub_v4.port())
    } else if let Some(ext_ip) = crate::common::env::get_external_address() {
        encode_locator(ext_ip, sender.listener_port)
    } else {
        match stream.local_addr()?.ip() {
            IpAddr::V4(ip) => encode_locator(ip, sender.listener_port),
            IpAddr::V6(_) => [0u8; 16], // TCPv6: Unsupported
        }
    };
    peer_hello_handshake(&mut stream, locator, sender.handshake_timeout).await?;

    // 3. Channel + cancel + pending_ack mailbox.
    let (tx, rx) = mpsc::channel::<Vec<u8>>(inbox_capacity());
    let conn_cancel = sender.cancel.child_token();
    let pending_ack = Arc::new(StdMutex::new(None));
    let request_lock = Arc::new(TokioMutex::new(()));

    // 4. Register in connection_registry BEFORE spawning the tasks — otherwise the
    //    reader task could receive a frame and find no entry.
    let conn_id = sender.shared.register_outbound_control_connection(
        addr,
        tx.clone(),
        conn_cancel.clone(),
        Arc::clone(&pending_ack),
    );

    // 5. Spawn the actor pair.
    let write_half = spawn_connection_tasks(
        stream,
        conn_id,
        Arc::clone(&sender.shared),
        conn_cancel.clone(),
        tx.clone(),
        rx,
    );

    // 6. Cache for future sends + future PORT_RESERVE round-trips.
    sender.connections.insert(
        (addr, CONTROL_LOGICAL_PORT),
        OutboundEntry {
            writer_tx: tx.clone(),
            write_half,
            cancel: conn_cancel,
            control: Some(ControlExtras { pending_ack, request_lock }),
        },
    );

    debug!("TcpSender: control connection established to {:?} (conn={})", addr, conn_id);
    Ok(tx)
}

// ── do_connect_data ─────────────────────────────────────────────────────────

async fn do_connect_data(
    sender: &Arc<TcpSender>,
    addr: SocketAddr,
    logical_port: u16,
) -> io::Result<mpsc::Sender<Vec<u8>>> {
    if sender.is_self_connection(&addr) {
        return Err(io::Error::new(io::ErrorKind::AddrInUse, "self connect"));
    }

    // 1. Ensure we have a control connection.
    let control = ensure_control_connection(sender, addr).await?;

    // 2. PORT_RESERVE round-trip → cookie. A failure here means the control
    //    connection itself is unusable so evicts it.
    let cookie =
        match port_reserve_round_trip(&control, logical_port, sender.handshake_timeout).await {
            Ok(cookie) => cookie,
            Err(e) => {
                sender.evict_connection(addr, CONTROL_LOGICAL_PORT);
                return Err(e);
            }
        };

    // 3. Open a TCP for the data connection.
    let mut stream = create_stream(sender, addr).await?;

    // 4. PORT_BIND + PORT_BIND_ACK (inline, before the tasks).
    port_bind_handshake(&mut stream, cookie, sender.handshake_timeout).await?;

    // 5. Channel + cancel — no pending_ack needed for data connections.
    let (tx, rx) = mpsc::channel::<Vec<u8>>(inbox_capacity());
    let conn_cancel = sender.cancel.child_token();

    // 6. Register, then spawn (race-free order).
    let conn_id = sender.shared.register_outbound_data_connection(
        addr,
        logical_port,
        tx.clone(),
        conn_cancel.clone(),
    );
    let write_half = spawn_connection_tasks(
        stream,
        conn_id,
        Arc::clone(&sender.shared),
        conn_cancel.clone(),
        tx.clone(),
        rx,
    );

    // 7. Cache.
    sender.connections.insert(
        (addr, logical_port),
        OutboundEntry { writer_tx: tx.clone(), write_half, cancel: conn_cancel, control: None },
    );

    debug!(
        "TcpSender: data connection established to {:?} (port={}, conn={})",
        addr, logical_port, conn_id
    );
    Ok(tx)
}

// ── ensure_control_connection ───────────────────────────────────────────────

/// Look up the control connection in cache, creating it via the in-flight
/// gate if missing. Returns a handle the caller uses to issue PORT_RESERVE
/// round-trips.
async fn ensure_control_connection(
    sender: &Arc<TcpSender>,
    addr: SocketAddr,
) -> io::Result<ControlConnHandle> {
    let key = (addr, CONTROL_LOGICAL_PORT);

    if let Some(handle) = lookup_control_handle(sender, key) {
        return Ok(handle);
    }

    match acquire_in_flight(sender, key).await {
        InFlightAcquisition::Acquired(_guard) => {
            let _ = do_connect_control(sender, addr).await?;
            // _guard drops at end of scope → notify_waiters fires.
        }
        InFlightAcquisition::AlreadyDone => {
            // Concurrent task finished; cache may or may not have the entry.
        }
    }

    lookup_control_handle(sender, key).ok_or_else(|| {
        io::Error::new(io::ErrorKind::Other, "control connect failed (cache miss after wait)")
    })
}

fn lookup_control_handle(sender: &TcpSender, key: (SocketAddr, u16)) -> Option<ControlConnHandle> {
    if let Some(entry) = sender.connections.get(&key) {
        if !entry.writer_tx.is_closed() {
            return entry.control.as_ref().map(|extras| ControlConnHandle {
                writer_tx: entry.writer_tx.clone(),
                extras: extras.clone(),
            });
        }
    }

    if let Some((_, dead)) = sender.connections.remove_if(&key, |_, e| e.writer_tx.is_closed()) {
        dead.cancel.cancel();
    }
    None
}

// ── PORT_RESERVE round-trip via single-slot oneshot ─────────────────────────

async fn port_reserve_round_trip(
    control: &ControlConnHandle,
    logical_port: u16,
    timeout: Duration,
) -> io::Result<[u8; 16]> {
    // Serialise concurrent PORT_RESERVE on the same control connection.
    // Otherwise the second request would clobber the first's pending_ack slot.
    let _permit = control.extras.request_lock.lock().await;

    let (tx, rx) = oneshot::channel();
    *control.extras.pending_ack.lock().expect("pending_ack lock") = Some(tx);

    let _ = control.writer_tx.try_send(ControlMsg::PortReserve { logical_port }.to_bytes());

    let response = match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(msg)) => msg,
        Ok(Err(_)) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "PORT_RESERVE response channel closed",
            ));
        }
        Err(_) => {
            // Timeout — clean up our slot in case dispatch never ran.
            *control.extras.pending_ack.lock().expect("pending_ack lock") = None;
            return Err(transport_io_error(
                TransportErrorCode::TcpHandshakeReserveFailed,
                "PORT_RESERVE response timeout",
            ));
        }
    };

    match response {
        ControlMsg::PortReserveAck { cookie } => Ok(cookie),
        ControlMsg::Error { operation: _, code, message } => Err(transport_io_error(
            TransportErrorCode::TcpHandshakeReserveFailed,
            format!("PORT_RESERVE rejected: code={} message={}", code, message),
        )),
        other => Err(transport_io_error(
            TransportErrorCode::TcpHandshakeReserveFailed,
            format!("unexpected response to PORT_RESERVE: {}", other.type_name()),
        )),
    }
}

// ── Stream open + inline handshakes ─────────────────────────────────────────

async fn create_stream(sender: &Arc<TcpSender>, addr: SocketAddr) -> io::Result<AsyncConnStream> {
    // 1. TCP connect (timeout-bounded).
    let tcp = tokio::time::timeout(sender.connect_timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| {
            transport_io_error(
                TransportErrorCode::TcpConnectionTimeout,
                format!("tcp connect timeout to {:?}", addr),
            )
        })??;

    apply_socket_tuning(&tcp, &sender.shared.tuning);

    // 2. Optional TLS handshake.
    if let Some(cfg) = &sender.tls_config {
        let client_cfg = cfg
            .build_client_config()
            .map_err(|e| transport_io_error(TransportErrorCode::TlsConfigError, e.to_string()))?;
        // TODO: thread the SNI name properly. For now use the configured
        // server_name or fall back to "localhost".
        let sni = cfg.server_name().to_string();

        let stream = tokio::time::timeout(
            sender.handshake_timeout,
            connect_tls_async(tcp, client_cfg, &sni),
        )
        .await
        .map_err(|_| {
            transport_io_error(TransportErrorCode::TlsHandshakeFailed, "tls handshake timeout")
        })??;

        Ok(stream)
    } else {
        Ok(wrap_plain(tcp))
    }
}

async fn peer_hello_handshake(
    stream: &mut AsyncConnStream,
    locator: [u8; 16],
    timeout: Duration,
) -> io::Result<()> {
    let hello = ControlMsg::PeerHello { locator };
    write_framed_message(stream, &hello.to_bytes()).await?;

    let response_bytes =
        tokio::time::timeout(timeout, read_framed_message(stream)).await.map_err(|_| {
            transport_io_error(
                TransportErrorCode::TcpHandshakeHelloFailed,
                "PEER_HELLO_ACK timeout",
            )
        })??;

    let response = ControlMsg::from_bytes(&response_bytes).map_err(|e| {
        transport_io_error(
            TransportErrorCode::TcpHandshakeHelloFailed,
            format!("malformed PEER_HELLO_ACK: {:?}", e),
        )
    })?;

    match response {
        ControlMsg::PeerHelloAck => Ok(()),
        ControlMsg::Error { code, message, .. } => Err(transport_io_error(
            TransportErrorCode::TcpHandshakeHelloFailed,
            format!("PEER_HELLO rejected: code={} message={}", code, message),
        )),
        other => Err(transport_io_error(
            TransportErrorCode::TcpHandshakeHelloFailed,
            format!("unexpected response to PEER_HELLO: {}", other.type_name()),
        )),
    }
}

async fn port_bind_handshake(
    stream: &mut AsyncConnStream,
    cookie: [u8; 16],
    timeout: Duration,
) -> io::Result<()> {
    let bind = ControlMsg::PortBind { cookie };
    write_framed_message(stream, &bind.to_bytes()).await?;

    let response_bytes =
        tokio::time::timeout(timeout, read_framed_message(stream)).await.map_err(|_| {
            transport_io_error(TransportErrorCode::TcpHandshakeBindFailed, "PORT_BIND_ACK timeout")
        })??;

    let response = ControlMsg::from_bytes(&response_bytes).map_err(|e| {
        transport_io_error(
            TransportErrorCode::TcpHandshakeBindFailed,
            format!("malformed PORT_BIND_ACK: {:?}", e),
        )
    })?;

    match response {
        ControlMsg::PortBindAck => Ok(()),
        ControlMsg::Error { code, message, .. } => Err(transport_io_error(
            TransportErrorCode::TcpHandshakeBindFailed,
            format!("PORT_BIND rejected: code={} message={}", code, message),
        )),
        other => Err(transport_io_error(
            TransportErrorCode::TcpHandshakeBindFailed,
            format!("unexpected response to PORT_BIND: {}", other.type_name()),
        )),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::plugin::IncomingMessage;
    use crate::rtps::transport::tcp::connection_registry::{TcpSocketTuning, BACKOFF_BASE};
    use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
    use flume::bounded;
    use std::time::Instant;

    /// Helper: build the crossbeam channels needed by `ConnectionRegistry::new` /
    /// `TcpMuxListener::bind_and_spawn`, returning the receivers so tests
    /// can observe routed RTPS frames.
    fn make_channels() -> (
        flume::Sender<IncomingMessage>,
        flume::Receiver<IncomingMessage>,
        flume::Sender<IncomingMessage>,
        flume::Receiver<IncomingMessage>,
    ) {
        let (d_tx, d_rx) = bounded(64);
        let (u_tx, u_rx) = bounded(64);
        (d_tx, d_rx, u_tx, u_rx)
    }

    /// Poll a crossbeam receiver from async code without blocking a worker.
    /// Returns `Some(msg)` if received before `deadline`, else `None`.
    async fn wait_for_recv<T>(rx: &flume::Receiver<T>, deadline: Instant) -> Option<T> {
        while Instant::now() < deadline {
            if let Ok(msg) = rx.try_recv() {
                return Some(msg);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        None
    }

    /// Build a bare sender with its own ConnectionRegistry. The sender does NOT bind
    /// a listener — callers that need a target listener spawn one separately.
    fn make_sender(
        participant_id: u32,
        guid_prefix: GuidPrefix,
        listener_port: u16,
    ) -> (Arc<TcpSender>, flume::Receiver<IncomingMessage>) {
        let (d_tx, d_rx, u_tx, _u_rx) = make_channels();
        let shared = Arc::new(ConnectionRegistry::new(
            0,
            participant_id,
            guid_prefix,
            TcpSocketTuning::default(),
            d_tx,
            u_tx,
        ));
        let sender = TcpSender::new(
            0,
            participant_id,
            "127.0.0.1".to_string(),
            listener_port,
            guid_prefix,
            None,
            shared,
            &TcpConfig::default(),
        );
        (sender, d_rx)
    }

    /// `send_to_*` blocks via `runtime_handle.block_on`, which panics if
    /// invoked from inside a tokio runtime worker. Production callers are the
    /// sync DDS write path (outside any runtime); tests run under
    /// `#[tokio::test]`, so they must drive the send from a blocking thread.
    async fn blocking_send_discovery(
        sender: &Arc<TcpSender>,
        target: SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        let sender = Arc::clone(sender);
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || sender.send_to_discovery(&target, &data))
            .await
            .expect("spawn_blocking join")
    }

    // ── construction smoke ───────────────────────────────────────────────────

    /// `TcpSender::new` inside a tokio context succeeds and `shutdown()`
    /// completes cleanly (it just cancels the shared token).
    #[tokio::test(flavor = "multi_thread")]
    async fn new_in_runtime_succeeds_and_shuts_down() {
        let (sender, _disc_rx) = make_sender(0, [0xAA; 12], 12345);
        // No connections yet.
        assert_eq!(sender.connections.len(), 0);
        // Shutdown must complete within a small timeout — guard against deadlock.
        tokio::time::timeout(Duration::from_secs(2), sender.shutdown())
            .await
            .expect("shutdown did not complete within 2s");
    }

    // ── private heuristics ───────────────────────────────────────────────────

    /// `is_self_connection` flags loopback + matching listener port.
    #[tokio::test(flavor = "multi_thread")]
    async fn self_connection_detection() {
        let listener_port: u16 = 7000;
        let (sender, _) = make_sender(0, [0xAA; 12], listener_port);

        let self_addr: SocketAddr = format!("127.0.0.1:{}", listener_port).parse().unwrap();
        assert!(sender.is_self_connection(&self_addr));

        let wrong_port: SocketAddr = format!("127.0.0.1:{}", listener_port + 1).parse().unwrap();
        assert!(!sender.is_self_connection(&wrong_port));

        let wrong_ip: SocketAddr = format!("192.0.2.1:{}", listener_port).parse().unwrap();
        assert!(!sender.is_self_connection(&wrong_ip));

        sender.shutdown().await;
    }

    // ── end-to-end: sender ──▶ separate listener (same process) ──────────────

    /// Full 3-way handshake (PEER_HELLO + PORT_RESERVE + PORT_BIND) plus an
    /// RTPS frame round-trip. Sender on side A reaches the listener on side B,
    /// and the RTPS frame is routed to side B's discovery crossbeam channel.
    ///
    /// Both sides use `participant_id = 0` so the logical port computation
    /// agrees on both ends — the listener's `ConnectionRegistry` only accepts
    /// PORT_RESERVE for its own discovery/user ports.
    #[tokio::test(flavor = "multi_thread")]
    async fn end_to_end_send_to_discovery_roundtrip() {
        // Side B (listener) — ephemeral port.
        let (b_disc_tx, b_disc_rx, b_user_tx, _b_user_rx) = make_channels();
        let listener = TcpMuxListener::bind_and_spawn(
            0,
            0,
            0,
            [0u8; 12],
            b_disc_tx,
            b_user_tx,
            None,
            TcpSocketTuning::default(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        // Side A (sender) — different GUID, dummy local listener port.
        let (sender, _a_disc_rx) = make_sender(0, [0xAA; 12], 12345);

        // Send.
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        let rtps_data: &[u8] = b"RTPS\x02\x04\x00\x00\x01\x02\x03\x04\x05\x06\x07\x08";
        blocking_send_discovery(&sender, target, rtps_data).await.expect("send");

        // Listener should eventually receive the RTPS frame on its discovery channel.
        // 5s allows for the full chain: connect → PEER_HELLO + ACK → PORT_RESERVE
        // round-trip → PORT_BIND + ACK → task spawn → first frame.
        let deadline = Instant::now() + Duration::from_secs(5);
        let received = wait_for_recv(&b_disc_rx, deadline).await;
        let msg = received.expect("listener did not receive RTPS data within 5s");

        assert_eq!(&msg.data[..rtps_data.len()], rtps_data);

        sender.shutdown().await;
        listener.shutdown().await;
    }

    /// A second send to the same peer reuses the cached data connection
    /// instead of triggering a new connect_task.
    #[tokio::test(flavor = "multi_thread")]
    async fn cached_send_reuses_existing_data_connection() {
        let (b_disc_tx, b_disc_rx, b_user_tx, _b_user_rx) = make_channels();
        let listener = TcpMuxListener::bind_and_spawn(
            0,
            0,
            0,
            [0u8; 12],
            b_disc_tx,
            b_user_tx,
            None,
            TcpSocketTuning::default(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xAA; 12], 12345);

        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00")
            .await
            .expect("first send");

        // Wait until the first frame lands on the listener (proves the cache is populated).
        let deadline = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, deadline).await.expect("first send did not deliver within 5s");

        // Cache should now contain at least control + data entries.
        let after_first = sender.connections.len();
        assert!(after_first >= 2, "expected control + data entries in cache, got {}", after_first);

        // Second send reuses the cached connection.
        blocking_send_discovery(&sender, target, b"RTPS\x11\x11\x11\x11")
            .await
            .expect("second send");

        let deadline = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, deadline).await.expect("second send did not deliver within 5s");

        assert_eq!(
            sender.connections.len(),
            after_first,
            "second send should reuse cache, not spawn new connect",
        );

        sender.shutdown().await;
        listener.shutdown().await;
    }

    /// `evict_connection` tears down only the targeted connection, leaving the
    /// peer's other connections (here, the control connection) in the cache.
    #[tokio::test(flavor = "multi_thread")]
    async fn evict_connection_removes_only_that_connection() {
        let (b_disc_tx, b_disc_rx, b_user_tx, _b_user_rx) = make_channels();
        let listener = TcpMuxListener::bind_and_spawn(
            0,
            0,
            0,
            [0u8; 12],
            b_disc_tx,
            b_user_tx,
            None,
            TcpSocketTuning::default(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xAA; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00").await.expect("send");

        // Wait for cache to populate (control + discovery-data entries).
        let deadline = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, deadline).await.expect("send did not deliver within 5s");
        assert!(sender.connections.len() >= 2, "expected control + data entries in cache");

        let disc_port = PortManager::get_discovery_traffic_unicast_port(0, 0);
        sender.evict_connection(target, disc_port);

        assert!(
            sender.connections.get(&(target, disc_port)).is_none(),
            "evicted data connection must be gone",
        );
        assert!(
            sender.connections.get(&(target, CONTROL_LOGICAL_PORT)).is_some(),
            "control connection must survive single-connection eviction",
        );

        sender.shutdown().await;
        listener.shutdown().await;
    }

    /// `disconnect_peer` evicts every cached connection to the addr (all logical
    /// ports) and clears its backoff — the DDS-unmatch cleanup path.
    #[tokio::test(flavor = "multi_thread")]
    async fn disconnect_peer_evicts_all_connections_and_backoff() {
        let (b_disc_tx, b_disc_rx, b_user_tx, _b_user_rx) = make_channels();
        let listener = TcpMuxListener::bind_and_spawn(
            0,
            0,
            0,
            [0u8; 12],
            b_disc_tx,
            b_user_tx,
            None,
            TcpSocketTuning::default(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xAA; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00").await.expect("send");

        let deadline = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, deadline).await.expect("send did not deliver within 5s");
        assert!(sender.connections.len() >= 2, "expected control + data entries");

        // Seed a backoff entry to confirm it is cleared too.
        sender.note_connect_failure(target);
        assert!(sender.backoff_remaining(target).is_some());

        sender.disconnect_peer(target);
        assert_eq!(
            sender.connections.iter().filter(|e| e.key().0 == target).count(),
            0,
            "disconnect_peer must evict every connection to the addr",
        );
        assert!(sender.backoff_remaining(target).is_none(), "backoff must be cleared");

        sender.shutdown().await;
        listener.shutdown().await;
    }

    /// Reconnect backoff grows exponentially per consecutive connect failure
    /// and is cleared by a success.
    #[tokio::test(flavor = "multi_thread")]
    async fn reconnect_backoff_grows_and_resets() {
        let (sender, _) = make_sender(0, [0xAA; 12], 12345);
        let addr: SocketAddr = "192.0.2.1:7400".parse().unwrap();

        assert!(sender.backoff_remaining(addr).is_none(), "no backoff initially");

        sender.note_connect_failure(addr);
        let first = sender.backoff_remaining(addr).expect("backoff after first failure");
        assert!(first > Duration::ZERO && first <= BACKOFF_BASE);

        sender.note_connect_failure(addr);
        let second = sender.backoff_remaining(addr).expect("backoff after second failure");
        assert!(second > BACKOFF_BASE, "delay must grow on a repeat failure");
        assert!(second <= BACKOFF_BASE * 2);

        sender.note_connect_success(addr);
        assert!(sender.backoff_remaining(addr).is_none(), "success clears backoff");

        sender.shutdown().await;
    }

    /// A send to our own listener port is refused without spawning a connect
    /// (self-loop heuristic). The send itself returns Ok (fire-and-forget)
    /// but no connection is added to the cache.
    #[tokio::test(flavor = "multi_thread")]
    async fn send_to_self_does_not_populate_cache() {
        let listener_port = 7777;
        let (sender, _) = make_sender(0, [0xAA; 12], listener_port);

        let self_addr: SocketAddr = format!("127.0.0.1:{}", listener_port).parse().unwrap();
        // `ensure_connection` rejects the self-connect (AddrInUse) before any
        // cache insert, so the send returns Err and nothing is cached.
        let _ = blocking_send_discovery(&sender, self_addr, b"RTPS\x00\x00\x00\x00").await;

        assert_eq!(sender.connections.len(), 0, "self-connection must not be cached",);

        sender.shutdown().await;
    }
}
