//! Outbound side of the TCP mux transport.
//!
//! `TcpSender` owns the outbound connection cache and the lifecycle of
//! the conn_actor pairs created via outbound connects. It exposes sync
//! `send_to_*` methods for the DDS layer; on cache miss the call spawns
//! an `outbound_connect_task` that performs TCP + (optional) TLS + the
//! 3-way protocol handshake (PEER_HELLO → PORT_RESERVE → PORT_BIND) before
//! handing the resulting stream to a conn_actor pair.
//!
//! The PORT_RESERVE_ACK round-trip uses the single-slot oneshot mailbox
//! installed in `MuxState::ConnectionEntry::pending_ack`: the sender
//! registers a `oneshot::Sender`, dispatches the request through the
//! control connection's writer, then awaits the receiver. The reader
//! task hands incoming responses to `MuxState::dispatch`, which routes
//! them to the slot via `handle_control_frame`.

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

use crate::common::env::{get_tcp_send_mode, TcpSendMode};
use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::port_manager::PortManager;
use crate::rtps::transport::tcp::tls::TlsConfig;
use crate::rtps::transport::tcp_async::conn_actor::{outbound_inbox_capacity, spawn_conn_actor};
use crate::rtps::transport::tcp_async::framing::{read_framed_message, write_framed_message};
use crate::rtps::transport::tcp_async::mux_state::MuxState;
use crate::rtps::transport::tcp_async::protocol::ControlMsg;
use crate::rtps::transport::tcp_async::stream::{connect_tls_async, wrap_plain, AsyncConnStream};

/// Logical port 0 = control connection — carries PEER_HELLO,
/// PORT_RESERVE, KEEPALIVE; never RTPS data.
pub(crate) const CONTROL_LOGICAL_PORT: u16 = 0;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
/// Per-keepalive ACK deadline — must be < `DEFAULT_KEEPALIVE_INTERVAL`,
/// since the next tick is when we evaluate whether the previous round-trip
/// met the deadline.
const DEFAULT_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_MAX_MISSED_KEEPALIVES: u32 = 3;
const ORPHAN_PRUNE_INTERVAL: Duration = Duration::from_millis(500);

// ── Cache entry types ───────────────────────────────────────────────────────

/// An entry in the sender's outbound cache. `control` is `Some` only for
/// control connections (logical_port == CONTROL_LOGICAL_PORT) — it carries
/// the bookkeeping needed to serialise PORT_RESERVE requests and read their
/// responses through the control connection's pending_ack slot.
struct OutboundEntry {
    writer_tx: mpsc::Sender<Vec<u8>>,
    control: Option<ControlExtras>,
}

#[derive(Clone)]
struct ControlExtras {
    /// Same `Arc` that lives in `MuxState::ConnectionEntry::pending_ack`.
    /// dispatch installs the response; the awaiting connect_task takes it.
    pending_ack: Arc<StdMutex<Option<oneshot::Sender<ControlMsg>>>>,
    /// Async lock that serialises one PORT_RESERVE round-trip at a time so
    /// concurrent requests don't clobber each other's `pending_ack` slot.
    request_lock: Arc<TokioMutex<()>>,
}

/// Borrowed handle to the cache entry of a control connection. Holds clones
/// of the channels needed to issue + await a PORT_RESERVE request.
struct ControlConnHandle {
    writer_tx: mpsc::Sender<Vec<u8>>,
    extras: ControlExtras,
}

// ── In-flight connect guard ─────────────────────────────────────────────────

/// Drop-guard for the in-flight connect entry. Removes the entry from
/// `in_flight` and notifies waiters when the connect_task ends — works
/// even on panic / cancel.
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

enum InFlightAcquisition {
    /// We won the race — caller must do the connect.
    Acquired(InFlightGuard),
    /// Another task already finished. Caller should re-check the cache.
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
    /// Per-keepalive ACK deadline. The previous keepalive is considered
    /// "missed" if `last_keepalive_ack_at - last_keepalive_sent_at` exceeds
    /// this on the next tick. Must be ≤ `keepalive_interval` for the per-tick
    /// evaluation to make sense.
    keepalive_timeout: Duration,
    max_missed_keepalives: u32,

    tls_config: Option<Arc<TlsConfig>>,

    /// Producer-side send-method selection (Try vs Blocking). Captured
    /// once at construction from `INT2DDS_TCP_SEND_MODE` so a single run
    /// has consistent semantics. See [`TcpSendMode`].
    send_mode: TcpSendMode,

    /// Shared mux state — outbound connections also live in `shared.connections`.
    shared: Arc<MuxState>,

    /// Outbound cache: (peer_addr, logical_port) → writer + control extras.
    connections: Arc<DashMap<(SocketAddr, u16), OutboundEntry>>,

    /// In-flight connect guard — prevents duplicate concurrent connects to
    /// the same key.
    in_flight: Arc<DashMap<(SocketAddr, u16), Arc<Notify>>>,

    /// Set once by the plugin during init to receive dead-peer events.
    dead_peer_tx: OnceLock<crossbeam_channel::Sender<SocketAddr>>,

    /// Handle to the tokio runtime we live on. Captured at construction
    /// time (which always happens inside a runtime context). Used to spawn
    /// `outbound_connect_task` from sync `send_to_*` calls — `tokio::spawn`
    /// would panic there because the sync caller is not in runtime context,
    /// but `Handle::spawn` carries the runtime reference with it.
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
    ) -> Arc<Self> {
        let cancel = CancellationToken::new();
        // Capture the current runtime handle so sync `send_to_*` callers can
        // spawn connect tasks even though they are not in runtime context.
        let runtime_handle = tokio::runtime::Handle::current();

        let send_mode = get_tcp_send_mode();
        info!("[TcpSender] send_mode = {:?}", send_mode);

        let sender = Arc::new(Self {
            domain_id,
            participant_id,
            working_ip,
            listener_port,
            local_guid_prefix,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
            keepalive_interval: DEFAULT_KEEPALIVE_INTERVAL,
            keepalive_timeout: DEFAULT_KEEPALIVE_TIMEOUT,
            max_missed_keepalives: DEFAULT_MAX_MISSED_KEEPALIVES,
            tls_config,
            send_mode,
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

    /// Send an RTPS frame to a peer's discovery port. Sync, fire-and-forget;
    /// on cache miss spawns a connect task and queues the frame for delivery
    /// once the data connection is established.
    pub(crate) fn send_to_discovery(
        self: &Arc<Self>,
        addr: &SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        let logical_port =
            PortManager::get_discovery_traffic_unicast_port(self.domain_id, self.participant_id);
        self.send_to(*addr, logical_port, data)
    }

    /// Send an RTPS frame to a peer's user-data port. Same semantics as
    /// `send_to_discovery`.
    pub(crate) fn send_to_user_data(
        self: &Arc<Self>,
        addr: &SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        let logical_port =
            PortManager::get_user_traffic_unicast_port(self.domain_id, self.participant_id);
        self.send_to(*addr, logical_port, data)
    }

    /// Internal: cache lookup → push, or spawn connect with the data queued.
    fn send_to(
        self: &Arc<Self>,
        addr: SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<()> {
        let key = (addr, logical_port);

        if let Some(entry) = self.connections.get(&key) {
            // Producer-side send method is toggleable via INT2DDS_TCP_SEND_MODE
            // (captured into `self.send_mode` at construction):
            //   - Try      → non-blocking try_send; drops on full inbox (lossy).
            //   - Blocking → blocking_send; parks the caller until the writer
            //                task drains a slot (lossless, RTI-like).
            //
            // Runtime-safety override: `blocking_send` panics when invoked from
            // inside a tokio runtime worker. Production DDS write path is a
            // sync thread (outside any runtime) so safe; tests using
            // `#[tokio::test]` are not — when `Handle::try_current().is_ok()`
            // we silently fall back to `try_send` even if Blocking is configured.
            let use_blocking = self.send_mode == TcpSendMode::Blocking
                && tokio::runtime::Handle::try_current().is_err();
            let result = if use_blocking {
                entry.writer_tx.blocking_send(data.to_vec())
            } else {
                match entry.writer_tx.try_send(data.to_vec()) {
                    Ok(()) => Ok(()),
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        warn!("send to {:?} dropped (writer inbox full)", addr);
                        return Err(io::Error::new(io::ErrorKind::WouldBlock, "writer inbox full"));
                    }
                    Err(mpsc::error::TrySendError::Closed(returned)) => {
                        Err(mpsc::error::SendError(returned))
                    }
                }
            };

            match result {
                Ok(()) => return Ok(()),
                Err(mpsc::error::SendError(returned)) => {
                    // Connection dead — drop guard, evict, then spawn fresh.
                    drop(entry);
                    self.connections.remove(&key);
                    self.spawn_outbound_connect(addr, logical_port, Some(returned));
                    return Ok(());
                }
            }
        }

        self.spawn_outbound_connect(addr, logical_port, Some(data.to_vec()));
        Ok(())
    }

    fn spawn_outbound_connect(
        self: &Arc<Self>,
        addr: SocketAddr,
        logical_port: u16,
        initial_data: Option<Vec<u8>>,
    ) {
        let me = Arc::clone(self);
        // `Handle::spawn` instead of `tokio::spawn` — `send_to_*` may be
        // called from a sync thread that is NOT inside the runtime context
        // (e.g. the DDS write path or a test thread).
        self.runtime_handle.spawn(outbound_connect_task(me, addr, logical_port, initial_data));
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

impl Drop for TcpSender {
    fn drop(&mut self) {
        // Best-effort — Drop can't await the lifecycle handles, but
        // cancelling the token tells them to exit at their next yield.
        self.cancel.cancel();
    }
}

// ── outbound_connect_task — single connect, including in-flight guard ────────

async fn outbound_connect_task(
    sender: Arc<TcpSender>,
    addr: SocketAddr,
    logical_port: u16,
    initial_data: Option<Vec<u8>>,
) {
    let key = (addr, logical_port);

    // ── In-flight gate ──────────────────────────────────────────────────
    let _guard = match acquire_in_flight(&sender, key).await {
        InFlightAcquisition::Acquired(g) => g,
        InFlightAcquisition::AlreadyDone => {
            // Someone else completed (or failed). Re-check cache and push
            // initial_data if a writer is now available.
            if let Some(data) = initial_data {
                if let Some(entry) = sender.connections.get(&key) {
                    let _ = entry.writer_tx.try_send(data);
                }
            }
            return;
        }
    };

    // ── Actual connect ──────────────────────────────────────────────────
    let result = if logical_port == CONTROL_LOGICAL_PORT {
        do_connect_control(&sender, addr).await
    } else {
        do_connect_data(&sender, addr, logical_port).await
    };

    match result {
        Ok(writer_tx) => {
            if let Some(data) = initial_data {
                let _ = writer_tx.try_send(data);
            }
        }
        Err(e) => {
            warn!(
                "TcpSender: outbound connect to {:?} (port={}) failed: {:?}",
                addr, logical_port, e
            );
            if let Some(tx) = sender.dead_peer_tx.get() {
                let _ = tx.try_send(addr);
            }
        }
    }
    // _guard drops → in_flight removed, notify_waiters fires.
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
    let (tx, rx) = mpsc::channel::<Vec<u8>>(outbound_inbox_capacity());
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
    let (tx, rx) = mpsc::channel::<Vec<u8>>(outbound_inbox_capacity());
    let conn_cancel = sender.cancel.child_token();

    // 6. Register, then spawn (race-free order).
    let conn_id = sender.shared.register_outbound_data_connection(
        addr,
        logical_port,
        tx.clone(),
        conn_cancel.clone(),
    );
    spawn_conn_actor(stream, conn_id, Arc::clone(&sender.shared), conn_cancel, tx.clone(), rx);

    // 7. Cache.
    sender
        .connections
        .insert((addr, logical_port), OutboundEntry { writer_tx: tx.clone(), control: None });

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

    let _ = tcp.set_nodelay(true);

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
/// Liveness model: each `ConnectionEntry` carries `last_keepalive_sent_at`
/// (written here after each push) and `last_keepalive_ack_at` (written by
/// `mux_state::handle_control_frame` when a KEEPALIVE_ACK arrives). On every
/// tick we compare the two:
///
/// - If `ack_at >= sent_at` AND the gap ≤ `keepalive_timeout` → previous
///   round-trip was timely → reset `missed_keepalives` to 0.
/// - Else (ACK never arrived, arrived late, or arrived before our latest send)
///   → increment `missed_keepalives`. If it exceeds `max_missed_keepalives`
///   the peer is declared dead and torn down via `disconnect_peer`.
///
/// Then a fresh KEEPALIVE is pushed and `last_keepalive_sent_at` is set to
/// `now`, starting the next cycle.
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

/// Sweep cache entries whose mpsc channel has been closed (typically because
/// the conn_actor exited). Avoids stale writer_tx accumulating on dead links.
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
    use crate::rtps::transport::tcp_async::tcp_mux_listener::TcpMuxListener;
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
        let shared = Arc::new(MuxState::new(0, participant_id, guid_prefix, d_tx, u_tx));
        let sender = TcpSender::new(
            0,
            participant_id,
            "127.0.0.1".to_string(),
            listener_port,
            guid_prefix,
            None,
            shared,
        );
        (sender, d_rx)
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
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        // Side A (sender) — different GUID, dummy local listener port.
        let (sender, _a_disc_rx) = make_sender(0, [0xAA; 12], 12345);

        // Send.
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        let rtps_data: &[u8] = b"RTPS\x02\x04\x00\x00\x01\x02\x03\x04\x05\x06\x07\x08";
        sender.send_to_discovery(&target, rtps_data).expect("send");

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
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xAA; 12], 12345);

        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        sender.send_to_discovery(&target, b"RTPS\x00\x00\x00\x00").expect("first send");

        // Wait until the first frame lands on the listener (proves the cache is populated).
        let deadline = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, deadline).await.expect("first send did not deliver within 5s");

        // Cache should now contain at least control + data entries.
        let after_first = sender.connections.len();
        assert!(after_first >= 2, "expected control + data entries in cache, got {}", after_first);

        // Second send — must not grow the cache.
        sender.send_to_discovery(&target, b"RTPS\x11\x11\x11\x11").expect("second send");

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
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xAA; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        sender.send_to_discovery(&target, b"RTPS\x00\x00\x00\x00").expect("send");

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
        // send_to is fire-and-forget — spawned task will reject internally.
        let _ = sender.send_to_discovery(&self_addr, b"RTPS\x00\x00\x00\x00");

        // Give the spawned task a moment to run and reject.
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(sender.connections.len(), 0, "self-connection must not be cached",);

        sender.shutdown().await;
    }
}
