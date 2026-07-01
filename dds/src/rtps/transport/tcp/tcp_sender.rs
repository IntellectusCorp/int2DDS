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

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use log::{debug, warn};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::conn_actor::{inbox_capacity, spawn_conn_actor, SharedWriteHalf};
use crate::rtps::transport::tcp::framing::{read_framed_message, write_framed_message};
use crate::rtps::transport::tcp::mux_state::{apply_keepalive, apply_unacked_timeout, MuxState};
use crate::rtps::transport::tcp::protocol::ControlMsg;
use crate::rtps::transport::tcp::stream::{wrap_plain, AsyncConnStream};
use crate::rtps::transport::tcp::tls::{connect_tls_async, TlsConfig};
use crate::rtps::transport::TcpConfig;

/// Logical port 0 = control connection — carries PEER_HELLO,
/// PORT_RESERVE; never RTPS data.
pub(crate) const CONTROL_LOGICAL_PORT: u16 = 0;

/// First reconnect-backoff delay after a connect failure.
const BACKOFF_BASE: Duration = Duration::from_millis(500);
/// Cap for the exponential reconnect-backoff growth.
const BACKOFF_MAX: Duration = Duration::from_secs(30);

// ── Cache entry types ───────────────────────────────────────────────────────

/// An entry in the sender's outbound cache. `control` is `Some` only for
/// control connections (logical_port == CONTROL_LOGICAL_PORT) — it carries
/// the bookkeeping needed to serialise PORT_RESERVE requests and read their
/// responses through the control connection's pending_ack slot.
struct OutboundEntry {
    /// Control inbox feeding the connection's `writer_task`. Used by the
    /// reader task and the PORT_RESERVE round-trip to enqueue protocol frames;
    /// not used by the user-data send path. A closed inbox means the conn_actor
    /// has exited, which the send path treats as a dead connection.
    writer_tx: mpsc::Sender<Vec<u8>>,
    /// User-data writes acquire this directly via
    /// `runtime.block_on(write_half.lock().await)` and perform the wire
    /// `writev` inline on the calling thread.
    write_half: SharedWriteHalf,
    /// Cancel token for this connection's actor pair (the same token stored in
    /// `MuxState::ConnectionEntry::cancel`). `evict_connection` fires it to tear
    /// down just this connection on a write failure, without touching the peer.
    cancel: CancellationToken,
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

/// Per-peer reconnect backoff. After a failed connect, no new connect to the
/// peer is attempted until `next_attempt`; `delay` doubles per consecutive
/// failure (capped at `BACKOFF_MAX`). A successful connect clears the entry.
struct BackoffState {
    next_attempt: Instant,
    delay: Duration,
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

    tls_config: Option<Arc<TlsConfig>>,
    shared: Arc<MuxState>,

    /// Outbound connection cache, keyed by (addr, logical_port).
    connections: Arc<DashMap<(SocketAddr, u16), OutboundEntry>>,

    /// Marks peers currently being connected, so duplicate concurrent
    /// connects to the same peer are prevented. See `InFlightGuard`.
    in_flight: Arc<DashMap<(SocketAddr, u16), Arc<Notify>>>,

    /// Per-peer reconnect backoff windows, keyed by peer address.
    backoff: DashMap<SocketAddr, BackoffState>,

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
        shared: Arc<MuxState>,
        tcp_config: &TcpConfig,
    ) -> Arc<Self> {
        let cancel = CancellationToken::new();
        // Capture the current runtime handle so sync `send_to_*` callers can
        // spawn connect tasks even though they are not in runtime context.
        let runtime_handle = tokio::runtime::Handle::current();

        Arc::new(Self {
            domain_id,
            participant_id,
            working_ip,
            listener_port,
            local_guid_prefix,
            connect_timeout: tcp_config.connect_timeout,
            handshake_timeout: tcp_config.bind_timeout,
            tls_config,
            shared,
            connections: Arc::new(DashMap::new()),
            in_flight: Arc::new(DashMap::new()),
            backoff: DashMap::new(),
            runtime_handle,
            cancel,
        })
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

        // Reuse a live cached connection. A dead one — conn_actor gone, its
        // writer inbox closed — is evicted here so the connect below rebuilds it.
        // This on-demand check replaces the periodic orphan-prune sweep; a data
        // connection always (re)establishes its control connection first via
        // `do_connect_data`, so no orphaned data link can linger.
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
            debug!(
                "TcpSender: send to {addr} deferred — reconnect backoff ({retry_in:?} remaining)"
            );
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
            self.note_connect_failure(addr);
            return Err(e);
        }
        self.note_connect_success(addr);

        self.connections.get(&key).map(|entry| Arc::clone(&entry.write_half)).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "connect succeeded but cache entry missing")
        })
    }

    /// Tear down a single cached connection `(addr, logical_port)` and its
    /// actor pair, leaving every other connection to the peer untouched.
    ///
    /// Used on a write failure: the write proves only that *this* connection is
    /// unusable, not that the peer is gone. Firing the cancel token wakes the
    /// conn_actor, whose reader exit calls `MuxState::remove_connection` to drop
    /// the shared-state entry. Peer liveness / unmatch stays with the DDS layer.
    fn evict_connection(&self, addr: SocketAddr, logical_port: u16) {
        if let Some((_, entry)) = self.connections.remove(&(addr, logical_port)) {
            entry.cancel.cancel();
        }
    }

    /// Tear down every outbound connection to `addr` (all logical ports) and its
    /// actor pairs, and clear the peer's reconnect backoff. Called when the DDS
    /// layer unmatches the peer, so its transport resources are released promptly
    /// instead of lingering until OS keepalive. Cancelling each actor wakes its
    /// reader, which drops the matching `MuxState` entry.
    pub(crate) fn disconnect_peer(&self, addr: SocketAddr) {
        self.connections.retain(|(peer_addr, _), entry| {
            let keep = *peer_addr != addr;
            if !keep {
                entry.cancel.cancel();
            }
            keep
        });
        self.backoff.remove(&addr);
    }

    /// Time left in `addr`'s reconnect-backoff window, or `None` if a connect
    /// may be attempted now.
    fn backoff_remaining(&self, addr: SocketAddr) -> Option<Duration> {
        let entry = self.backoff.get(&addr)?;
        let now = Instant::now();
        (entry.next_attempt > now).then(|| entry.next_attempt - now)
    }

    /// Record a connect failure, growing `addr`'s backoff window exponentially
    /// (BACKOFF_BASE, then doubling, capped at BACKOFF_MAX).
    fn note_connect_failure(&self, addr: SocketAddr) {
        let now = Instant::now();
        let mut entry = self
            .backoff
            .entry(addr)
            .or_insert(BackoffState { next_attempt: now, delay: Duration::ZERO });
        let delay =
            if entry.delay.is_zero() { BACKOFF_BASE } else { (entry.delay * 2).min(BACKOFF_MAX) };
        entry.delay = delay;
        entry.next_attempt = now + delay;
    }

    /// Clear `addr`'s backoff after a successful connect.
    fn note_connect_success(&self, addr: SocketAddr) {
        self.backoff.remove(&addr);
    }

    /// Graceful shutdown — cancels the shared token, tearing down every
    /// connection actor via its child token. Does not consume `self` so the
    /// plugin can hold an `Arc<TcpSender>`.
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
// kernel accepts the bytes. Connections, TLS, and the protocol handshake
// remain on the async task pair created by `spawn_conn_actor`.
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

        // Fast path: reuse a live cached connection. A dead entry (writer inbox
        // closed) falls through to ensure_connection, which evicts and rebuilds.
        // The cache guard is dropped before block_on so the rebuild can remove
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
                // Connection-only teardown: drop just this connection. The peer
                // and its other connections survive; the next send re-dials.
                // Declaring the peer dead is the DDS layer's job (lease). The raw
                // io::Error is returned transparently (like UDP); the RTPS layer
                // logs and continues, and its kind no longer drives any unmatch.
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
    let write_half = spawn_conn_actor(
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
    let write_half = spawn_conn_actor(
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
    apply_unacked_timeout(&tcp, sender.shared.tuning.unacked_timeout);
    apply_keepalive(&tcp, sender.shared.tuning.keepalive);

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
