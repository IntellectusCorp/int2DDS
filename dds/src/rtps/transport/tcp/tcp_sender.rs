//! Outbound side of the TCP mux transport.
//!
//! `TcpSender` owns the outbound connection cache and conn_actor lifecycles,
//! exposing sync `send_to_*` methods for the DDS layer. On cache miss a
//! connect runs TCP + (optional) TLS + the 3-way handshake
//! (PEER_HELLO → PORT_RESERVE → PORT_BIND), then hands the stream to a
//! conn_actor pair.
//!
//! PORT_RESERVE replies travel through the single-slot oneshot mailbox in
//! `MuxState::ConnectionEntry::pending_ack`: the sender registers the
//! `oneshot::Sender`, writes the request, then awaits the reply that the
//! reader task routes back via `MuxState::dispatch`.

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};
use std::{io, mem};

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use log::{debug, info, warn};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex, Notify};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::conn_actor::{inbox_capacity, spawn_conn_actor, SharedWriteHalf};
use crate::rtps::transport::tcp::framing::{read_framed_message, write_framed_message};
use crate::rtps::transport::tcp::mux_state::MuxState;
use crate::rtps::transport::tcp::protocol::ControlMsg;
use crate::rtps::transport::tcp::stream::{wrap_plain, AsyncConnStream};
use crate::rtps::transport::tcp::tls::{connect_tls_async, TlsConfig};
use crate::rtps::transport::TcpConfig;

/// Logical port 0 = control connection — carries PEER_HELLO,
/// PORT_RESERVE, KEEPALIVE; never RTPS data.
pub(crate) const CONTROL_LOGICAL_PORT: u16 = 0;

const ORPHAN_PRUNE_INTERVAL: Duration = Duration::from_millis(500);

// ── Cache entry types ───────────────────────────────────────────────────────

/// An entry in the sender's outbound cache. `control` is `Some` only for
/// control connections (logical_port == CONTROL_LOGICAL_PORT) — it carries
/// the bookkeeping needed to serialise PORT_RESERVE requests and read their
/// responses through the control connection's pending_ack slot.
struct OutboundEntry {
    /// Control inbox feeding the connection's `writer_task`. Used by the
    /// reader task and lifecycle tasks (keepalive, PORT_RESERVE) to enqueue
    /// protocol frames; not used by the user-data send path.
    writer_tx: mpsc::Sender<Vec<u8>>,
    /// User-data writes acquire this directly via
    /// `runtime.block_on(write_half.lock().await)` and perform the wire
    /// `writev` inline on the calling thread.
    write_half: SharedWriteHalf,
    control: Option<ControlExtras>,
}

#[derive(Clone)]
struct ControlExtras {
    /// Same `Arc` that lives in `MuxState::ConnectionEntry::pending_ack`.
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
    #[allow(dead_code)]
    local_guid_prefix: GuidPrefix,

    connect_timeout: Duration,
    handshake_timeout: Duration,
    keepalive_interval: Duration,
    keepalive_timeout: Duration,
    max_missed_keepalives: u32,

    tls_config: Option<Arc<TlsConfig>>,
    shared: Arc<MuxState>,

    /// Outbound connection cache, keyed by (addr, logical_port).
    connections: Arc<DashMap<(SocketAddr, u16), OutboundEntry>>,

    /// Marks peers currently being connected, so duplicate concurrent
    /// connects to the same peer are prevented. See `InFlightGuard`.
    in_flight: Arc<DashMap<(SocketAddr, u16), Arc<Notify>>>,

    /// Dead-peer notifier, installed once by the plugin during init.
    dead_peer_tx: OnceLock<crossbeam_channel::Sender<SocketAddr>>,

    /// Handle to the runtime, saved when the sender is built.
    /// The sync `send_to_*` methods run outside the runtime, so they cannot
    /// use `tokio::spawn` (it would panic). They use this handle instead to
    /// run async work (`block_on`) and start connect tasks.
    runtime_handle: tokio::runtime::Handle,

    cancel: CancellationToken,
    task_handles: StdMutex<Vec<JoinHandle<()>>>,
}

impl TcpSender {
    /// Build a sender and spawn its long-running tasks (keepalive, orphan
    /// prune). Must be called inside a tokio runtime context.
    pub(crate) fn new(
        domain_id: u32,
        participant_id: u32,
        working_ip: String,
        listener_port: u16,
        local_guid_prefix: GuidPrefix,
        tls_config: Option<Arc<TlsConfig>>,
        shared: Arc<MuxState>,
        tcp_config: &TcpConfig,
    ) -> Arc<Self> {
        let cancel = CancellationToken::new();
        // Capture the current runtime handle so sync `send_to_*` callers can
        // spawn connect tasks even though they are not in runtime context.
        let runtime_handle = tokio::runtime::Handle::current();

        let sender = Arc::new(Self {
            domain_id,
            participant_id,
            working_ip,
            listener_port,
            local_guid_prefix,
            connect_timeout: tcp_config.connect_timeout,
            handshake_timeout: tcp_config.bind_timeout,
            keepalive_interval: tcp_config.keepalive_interval,
            keepalive_timeout: tcp_config.keepalive_timeout,
            max_missed_keepalives: tcp_config.keepalive_max_misses,
            tls_config,
            shared,
            connections: Arc::new(DashMap::new()),
            in_flight: Arc::new(DashMap::new()),
            dead_peer_tx: OnceLock::new(),
            runtime_handle: runtime_handle.clone(),
            cancel: cancel.clone(),
            task_handles: StdMutex::new(Vec::new()),
        });

        // Spawn lifecycle tasks now that we have an Arc. Use the handle
        // explicitly so this is robust even if `new()` is somehow called
        // from a context where `tokio::spawn` would not be valid.
        let mut handles = sender.task_handles.lock().expect("task_handles lock");
        handles.push(
            runtime_handle.spawn(keepalive_interval_task(Arc::clone(&sender), cancel.clone())),
        );
        handles.push(
            runtime_handle.spawn(orphan_prune_interval_task(Arc::clone(&sender), cancel.clone())),
        );
        drop(handles);

        sender
    }

    /// Plug in the dead-peer notifier. Idempotent; subsequent calls are no-ops.
    pub(crate) fn set_dead_peer_tx(&self, tx: crossbeam_channel::Sender<SocketAddr>) {
        let _ = self.dead_peer_tx.set(tx);
    }

    /// Resolve the connection's shared write half, establishing it on cache
    /// miss. Blocks the caller (via `send_to`'s `block_on`) until the
    /// connection is ready. `do_connect_data` internally ensures the shared
    /// control connection (PEER_HELLO + PORT_RESERVE) before PORT_BIND.
    async fn ensure_connection(
        self: &Arc<Self>,
        addr: SocketAddr,
        logical_port: u16,
    ) -> io::Result<SharedWriteHalf> {
        let key = (addr, logical_port);

        if let Some(entry) = self.connections.get(&key) {
            return Ok(Arc::clone(&entry.write_half));
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
            if let Some(tx) = self.dead_peer_tx.get() {
                let _ = tx.try_send(addr);
            }
            return Err(e);
        }

        self.connections.get(&key).map(|entry| Arc::clone(&entry.write_half)).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "connect succeeded but cache entry missing")
        })
    }

    /// Tear down every cached connection to `addr` (control + data) and
    /// notify the dead-peer channel. Used on keepalive failure / explicit
    /// peer eviction.
    pub(crate) fn disconnect_peer(&self, addr: SocketAddr) {
        // 1. Cancel + drop all sender-cached entries for this addr.
        self.connections.retain(|(peer_addr, _), _entry| *peer_addr != addr);

        // 2. Tear down the peer group on the shared mux state — this also
        //    cancels the corresponding conn_actors via their tokens.
        let synthetic_guid = addr_to_guid(addr);
        self.shared.remove_peer(synthetic_guid);

        // 3. Notify upper layer.
        if let Some(tx) = self.dead_peer_tx.get() {
            let _ = tx.try_send(addr);
        }

        info!("TcpSender: Disconnected peer {:?}", addr);
    }

    /// Graceful shutdown — cancels lifecycle tasks then awaits them. Does
    /// not consume `self` so the plugin can hold an `Arc<TcpSender>`.
    pub(crate) async fn shutdown(&self) {
        self.cancel.cancel();
        let handles = mem::take(&mut *self.task_handles.lock().expect("task_handles lock"));
        for h in handles {
            let _ = h.await;
        }
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
// kernel accepts the bytes. Connections, TLS, keepalive, and the protocol
// handshake remain on the async task pair created by `spawn_conn_actor`.
impl TcpSender {
    /// SPDP bootstrap fan-out to an initial peer we have not discovered yet.
    /// The peer's participant id (hence its logical port) is unknown, so we
    /// reserve the well-known index-0 metatraffic port — the conventional
    /// bootstrap port. Once SPDP completes, SEDP and user data reserve the
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
    /// established — then perform the wire `writev` inline. All user-data
    /// writes funnel through this single path, so a peer's fragments always
    /// reach the wire in the order the caller emits them. `logical_port` is the
    /// destination's advertised RTPS port, reserved on the mux connection.
    pub(crate) fn send_to(
        self: &Arc<Self>,
        addr: SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<()> {
        let key = (addr, logical_port);

        let write_half = match self.connections.get(&key) {
            Some(entry) => Arc::clone(&entry.write_half),
            None => self.runtime_handle.block_on(self.ensure_connection(addr, logical_port))?,
        };

        let payload = data.to_vec();
        self.runtime_handle.block_on(async move {
            let mut wh = write_half.lock().await;
            write_framed_message(&mut *wh, &payload).await
        })
    }
}

impl Drop for TcpSender {
    fn drop(&mut self) {
        // Best-effort — Drop can't await the lifecycle handles, but
        // cancelling the token tells them to exit at their next yield.
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
    let mut stream = open_stream(sender, addr).await?;

    // 2. PEER_HELLO + PEER_HELLO_ACK (inline, before conn_actor takes the stream).
    peer_hello_handshake(&mut stream, sender.handshake_timeout).await?;

    // 3. Channel + cancel + pending_ack mailbox.
    let (tx, rx) = mpsc::channel::<Vec<u8>>(inbox_capacity());
    let conn_cancel = sender.cancel.child_token();
    let pending_ack = Arc::new(StdMutex::new(None));
    let request_lock = Arc::new(TokioMutex::new(()));

    // 4. Register in mux_state BEFORE spawning conn_actor — otherwise the
    //    reader task could receive a frame and find no entry.
    let conn_id = sender.shared.register_outbound_control_connection(
        addr,
        tx.clone(),
        conn_cancel.clone(),
        Arc::clone(&pending_ack),
    );

    // 5. Spawn the actor pair.
    let write_half =
        spawn_conn_actor(stream, conn_id, Arc::clone(&sender.shared), conn_cancel, tx.clone(), rx);

    // 6. Seed liveness state: send first KEEPALIVE so that the
    //    interval task's first tick has not fail. (timeout)
    let _ = tx.try_send(ControlMsg::Keepalive.to_bytes());
    if let Some(e) = sender.shared.connections.get(&conn_id) {
        *e.last_keepalive_sent_at.lock().expect("...") = Some(Instant::now());
    }

    // 7. Cache for future sends + future PORT_RESERVE round-trips.
    sender.connections.insert(
        (addr, CONTROL_LOGICAL_PORT),
        OutboundEntry {
            writer_tx: tx.clone(),
            write_half,
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

    // 2. PORT_RESERVE round-trip → cookie.
    let cookie = port_reserve_round_trip(&control, logical_port, sender.handshake_timeout).await?;

    // 3. Open a fresh TCP for the data connection (+ TLS).
    let mut stream = open_stream(sender, addr).await?;

    // 4. PORT_BIND + PORT_BIND_ACK (inline, before conn_actor).
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
    let write_half =
        spawn_conn_actor(stream, conn_id, Arc::clone(&sender.shared), conn_cancel, tx.clone(), rx);

    // 7. Cache.
    sender.connections.insert(
        (addr, logical_port),
        OutboundEntry { writer_tx: tx.clone(), write_half, control: None },
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
    sender.connections.get(&key).and_then(|entry| {
        entry.control.as_ref().map(|extras| ControlConnHandle {
            writer_tx: entry.writer_tx.clone(),
            extras: extras.clone(),
        })
    })
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
            return Err(io::Error::new(io::ErrorKind::TimedOut, "PORT_RESERVE response timeout"));
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

async fn open_stream(sender: &Arc<TcpSender>, addr: SocketAddr) -> io::Result<AsyncConnStream> {
    // 1. TCP connect (timeout-bounded).
    let tcp = tokio::time::timeout(sender.connect_timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| {
            io::Error::new(io::ErrorKind::TimedOut, format!("tcp connect timeout to {:?}", addr))
        })??;

    let _ = tcp.set_nodelay(sender.shared.tuning.nodelay);

    // Mirror the accept path so outbound (send) sockets are bounded too.
    if let Some(sz) = sender.shared.tuning.so_rcvbuf {
        let _ = socket2::SockRef::from(&tcp).set_recv_buffer_size(sz);
    }
    if let Some(sz) = sender.shared.tuning.so_sndbuf {
        let _ = socket2::SockRef::from(&tcp).set_send_buffer_size(sz);
    }

    // 2. Optional TLS handshake (also timeout-bounded).
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
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "tls handshake timeout"))??;

        Ok(stream)
    } else {
        Ok(wrap_plain(tcp))
    }
}

async fn peer_hello_handshake(stream: &mut AsyncConnStream, timeout: Duration) -> io::Result<()> {
    // Locator field is opaque to the peer in this protocol; the sync version
    // sends a zero locator. TODO: thread the real local locator through once
    // the plugin wiring is in place.
    let hello = ControlMsg::PeerHello { locator: [0u8; 16] };
    write_framed_message(stream, &hello.to_bytes()).await?;

    let response_bytes = tokio::time::timeout(timeout, read_framed_message(stream))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "PEER_HELLO_ACK timeout"))??;

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

    let response_bytes = tokio::time::timeout(timeout, read_framed_message(stream))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "PORT_BIND_ACK timeout"))??;

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

// ── Lifecycle tasks ──────────────────────────────────────────────────────────

/// Per-tick keepalive cycle on every outbound control connection.
///
/// Each tick judges the previous round-trip from `last_keepalive_sent_at`
/// (set here) and `last_keepalive_ack_at` (set by `mux_state` on
/// KEEPALIVE_ACK):
/// - timely (`ack_at >= sent_at` and gap ≤ `keepalive_timeout`) → reset
///   `missed_keepalives`.
/// - otherwise → increment it; past `max_missed_keepalives` the peer is
///   declared dead and torn down via `disconnect_peer`.
///
/// Then a fresh KEEPALIVE is pushed and `last_keepalive_sent_at` set to `now`.
async fn keepalive_interval_task(sender: Arc<TcpSender>, cancel: CancellationToken) {
    let mut ticker = tokio::time::interval(sender.keepalive_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let now = Instant::now();
                let mut dead: Vec<SocketAddr> = Vec::new();

                for entry in sender.shared.connections.iter() {
                    // Filter to OUTBOUND CONTROL connections only. (skipping DATA conncections)
                    if entry.pending_ack.is_none() {
                        continue;
                    }

                    let key = (entry.remote_addr, CONTROL_LOGICAL_PORT);
                    if !sender.connections.contains_key(&key) {
                        continue; // sender cache may have evicted concurrently
                    }

                    // Read previous cycle's send/ack timestamps from the
                    // shared ConnectionEntry — mux_state::dispatch updates
                    // `last_keepalive_ack_at` on KEEPALIVE_ACK.
                    let sent_at = *entry
                        .last_keepalive_sent_at
                        .lock()
                        .expect("last_keepalive_sent_at lock");
                    let ack_at = *entry
                        .last_keepalive_ack_at
                        .lock()
                        .expect("last_keepalive_ack_at lock");

                    // Was the previous round-trip timely?
                    //  - sent_at == None: first-ever tick → no judgment yet
                    //  - ack_at < sent_at: latest ACK is from an older cycle
                    //  - gap > timeout: ACK arrived late
                    let timely = match (sent_at, ack_at) {
                        (Some(s), Some(a)) => {
                            a >= s && a.duration_since(s) <= sender.keepalive_timeout
                        }
                        _ => false,
                    };

                    if timely {
                        entry.missed_keepalives.store(0, Ordering::Relaxed);
                    } else {
                        // Previous keepalive went unacked or late.
                        let new_count = entry
                            .missed_keepalives
                            .fetch_add(1, Ordering::Relaxed)
                            + 1;
                        if new_count > sender.max_missed_keepalives {
                            dead.push(entry.remote_addr);
                            continue; // skip sending the next KEEPALIVE
                        }
                    }
                    // First-ever tick (sent_at None): just send below.

                    // Record send time, then push the KEEPALIVE frame.
                    *entry
                        .last_keepalive_sent_at
                        .lock()
                        .expect("last_keepalive_sent_at lock") = Some(now);
                    let _ = entry
                        .writer_tx
                        .try_send(ControlMsg::Keepalive.to_bytes());
                }

                for addr in dead {
                    warn!(
                        "TcpSender: peer {:?} missed > {} keepalives — disconnecting",
                        addr, sender.max_missed_keepalives
                    );
                    sender.disconnect_peer(addr);
                }
            }
            _ = cancel.cancelled() => break,
        }
    }
}

/// Drop cache entries whose writer channel is closed (conn_actor exited),
/// so stale entries don't pile up on dead links.
async fn orphan_prune_interval_task(sender: Arc<TcpSender>, cancel: CancellationToken) {
    let mut ticker = tokio::time::interval(ORPHAN_PRUNE_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                sender.connections.retain(|_key, entry| !entry.writer_tx.is_closed());
            }
            _ = cancel.cancelled() => break,
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Mirror of `mux_state::addr_to_guid` — same synthetic-guid derivation so
/// `disconnect_peer` can target the right peer group.
fn addr_to_guid(addr: SocketAddr) -> GuidPrefix {
    let mut g = [0u8; 12];
    if let SocketAddr::V4(v4) = addr {
        g[0..4].copy_from_slice(&v4.ip().octets());
        g[4..6].copy_from_slice(&v4.port().to_be_bytes());
    }
    g
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::plugin::IncomingMessage;
    use crate::rtps::transport::tcp::mux_state::TcpSocketTuning;
    use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;
    use crossbeam_channel::bounded;
    use std::time::Instant;

    /// Helper: build the crossbeam channels needed by `MuxState::new` /
    /// `TcpMuxListener::bind_and_spawn`, returning the receivers so tests
    /// can observe routed RTPS frames.
    fn make_channels() -> (
        crossbeam_channel::Sender<IncomingMessage>,
        crossbeam_channel::Receiver<IncomingMessage>,
        crossbeam_channel::Sender<IncomingMessage>,
        crossbeam_channel::Receiver<IncomingMessage>,
    ) {
        let (d_tx, d_rx) = bounded(64);
        let (u_tx, u_rx) = bounded(64);
        (d_tx, d_rx, u_tx, u_rx)
    }

    /// Poll a crossbeam receiver from async code without blocking a worker.
    /// Returns `Some(msg)` if received before `deadline`, else `None`.
    async fn wait_for_recv<T>(rx: &crossbeam_channel::Receiver<T>, deadline: Instant) -> Option<T> {
        while Instant::now() < deadline {
            if let Ok(msg) = rx.try_recv() {
                return Some(msg);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        None
    }

    /// Build a bare sender with its own MuxState. The sender does NOT bind
    /// a listener — callers that need a target listener spawn one separately.
    fn make_sender(
        participant_id: u32,
        guid_prefix: GuidPrefix,
        listener_port: u16,
    ) -> (Arc<TcpSender>, crossbeam_channel::Receiver<IncomingMessage>) {
        let (d_tx, d_rx, u_tx, _u_rx) = make_channels();
        let shared = Arc::new(MuxState::new(
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

    /// `TcpSender::new` inside a tokio context succeeds and the lifecycle
    /// tasks (keepalive, orphan prune) shut down cleanly.
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
    /// agrees on both ends — the listener's `MuxState` only accepts
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
            Duration::from_secs(60),
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
        // round-trip → PORT_BIND + ACK → conn_actor spawn → first frame.
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
            Duration::from_secs(60),
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

    /// `disconnect_peer` evicts every cached connection for the given address.
    #[tokio::test(flavor = "multi_thread")]
    async fn disconnect_peer_evicts_cached_entries() {
        let (b_disc_tx, b_disc_rx, b_user_tx, _b_user_rx) = make_channels();
        let listener = TcpMuxListener::bind_and_spawn(
            0,
            0,
            0,
            [0u8; 12],
            b_disc_tx,
            b_user_tx,
            None,
            Duration::from_secs(60),
            TcpSocketTuning::default(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xAA; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00").await.expect("send");

        // Wait for cache to populate.
        let deadline = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, deadline).await.expect("send did not deliver within 5s");
        assert!(sender.connections.len() > 0);

        sender.disconnect_peer(target);
        assert_eq!(
            sender.connections.len(),
            0,
            "disconnect_peer should evict all cached entries to that peer",
        );

        sender.shutdown().await;
        listener.shutdown().await;
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
