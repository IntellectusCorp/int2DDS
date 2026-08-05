//! Outbound side of the TCP mux transport.
//!
//! `TcpSender` owns the outbound connection cache and reader/writer task lifecycles,
//! exposing sync `send_to_*` methods for the DDS layer.
//! A connect runs TCP + (optional) TLS + the 3-way handshake
//! (PEER_HELLO → PORT_RESERVE → PORT_BIND), then hands the stream to a
//! reader/writer task pair.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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
use crate::rtps::transport::tcp::connection_registry::{apply_socket_tuning, ConnectionRegistry};
use crate::rtps::transport::tcp::connection_tasks::{
    inbox_capacity, spawn_reader_only, spawn_tasks,
};
use crate::rtps::transport::tcp::framing::{read_framed_message, write_framed_message};
use crate::rtps::transport::tcp::protocol::{encode_locator, ControlMsg};
use crate::rtps::transport::tcp::stream::{wrap_plain, AsyncConnStream};
use crate::rtps::transport::tcp::tls::{connect_tls_async, TlsConfig};
use crate::rtps::transport::tcp::write_state::{
    flush_and_ready, graceful_close, init_connection_state, SharedWriteState, WriteHandoff,
    WriteState,
};
use crate::rtps::transport::TcpConfig;

/// Logical port 0 = control connection
pub(crate) const CONTROL_LOGICAL_PORT: u16 = 0;

/// Bound on frames buffered per connection while it is still being established.
/// Steady-state sends never touch this buffer (they go straight to the wire),
/// so it only has to absorb a burst during the brief connect window.
const CONNECT_BUFFER_DEPTH: usize = 256;

/// Per-connection send health, driven by two complementary signals:
///
/// - `on_miss` — the send path could not take the write lock within the
///   deadline. Since the lock is held for exactly as long as the previous
///   frame's write, this says "the last frame still has not reached the wire".
///   It is a *leading* signal: it fires while the stall is happening.
/// - `on_success` — a write completed *within* `send_deadline`. Completion alone
///   is not evidence of health (a frame that took 20s also completes), so only
///   a genuinely fast write counts toward recovery.
///
/// Once `misses` reaches `threshold` the connection is congested and the send
/// path waits only the short probe deadline for its lock, so a slow or dead peer
/// stops delaying sends to healthy peers. A lower `threshold` isolates a
/// stalling peer sooner — less head-of-line delay for the peer serviced next in
/// the fan-out — while a higher one tolerates more transient backups on a
/// marginal link before dropping to the probe deadline. Recovery has hysteresis:
/// each fast write decrements by one, so a deeply congested peer must prove
/// itself repeatedly.
struct ConnHealth {
    /// Congestion is `misses >= threshold`, so there is no second field to keep
    /// in sync.
    misses: AtomicU32,
    /// Consecutive-miss bar for congestion (`PROP_TCP_CONGESTION_MISS_THRESHOLD`).
    threshold: u32,
}

impl ConnHealth {
    fn new(threshold: u32) -> Self {
        Self { misses: AtomicU32::new(0), threshold }
    }

    fn is_congested(&self) -> bool {
        self.misses.load(Ordering::Relaxed) >= self.threshold
    }

    /// A frame reached the socket buffer within the deadline.
    fn on_success(&self) {
        let _ = self
            .misses
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |m| Some(m.saturating_sub(1)));
    }

    /// The write lock could not be taken within the deadline.
    fn on_miss(&self) {
        // Saturating: a peer stalled for hours must not wrap back to healthy.
        let _ = self
            .misses
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |m| Some(m.saturating_add(1)));
    }
}

/// A drop counter paired with its own log rate limiter.
///
/// The two belong together: the limiter is what keeps a drop storm from
/// flooding the log, and the running total is what keeps the surviving line
/// honest about how many drops it stands for. One limiter per counter, so a
/// high-rate cause cannot starve a rare one out of the log.
#[derive(Default)]
pub(crate) struct DropCounter {
    count: AtomicU64,
    last_warn: StdMutex<Option<Instant>>,
}

impl DropCounter {
    /// Count a dropped frame, warning at most once per second.
    fn record(&self, cause: &str, addr: SocketAddr, logical_port: u16) {
        let total = self.count.fetch_add(1, Ordering::Relaxed) + 1;

        let now = Instant::now();
        let mut last = match self.last_warn.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if last.map_or(true, |t: Instant| now.duration_since(t) >= Duration::from_secs(1)) {
            *last = Some(now);
            drop(last);
            warn!(
                "TcpSender: dropped frame to {:?} (port={}): {} ({} total)",
                addr, logical_port, cause, total
            );
        }
    }

    #[cfg(test)]
    fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

/// Why frames failed to reach the wire. A frame refused before the handoff is
/// reported to the caller as well; a write that failed afterwards can only be
/// counted here. Either way the count is what makes a load test answerable:
/// which peer, which cause, how many.
#[derive(Default)]
pub(crate) struct SendStats {
    /// Could not take the write lock within the deadline — the previous frame
    /// is still on the wire.
    send_deadline: DropCounter,
    /// The connect-window buffer hit its depth or byte bound.
    connect_buffer_full: DropCounter,
    /// The peer is in its reconnect backoff window.
    backoff: DropCounter,
    /// The wire write itself failed; the frame never left.
    write_error: DropCounter,
}

// ── Cache entry types ───────────────────────────────────────────────────────

/// An entry in the sender's outbound cache.
struct OutboundEntry {
    /// Shared write side — `Some` only for a **data** connection, the only send
    /// target. A control connection is never a send target (its `writer_task`
    /// owns its half outright), so it has none.
    write_state: Option<SharedWriteState>,
    /// This connection's own token — the same one its actor pair and its
    /// `ConnectionRegistry::ConnectionEntry` hold, never a child of them.
    ///
    /// It is both directions at once: firing it tears the connection down
    /// (`evict_connection`, `disconnect_peer`), and *observing* it fired is how
    /// the send path knows the connection is dead, whoever noticed first — the
    /// reader at EOF, the write task on a wire error, or a `Drop`.
    cancel: CancellationToken,
    /// Send health, shared with the write path.
    health: Arc<ConnHealth>,
    /// `control` is `Some` only for
    /// control connections (logical_port == CONTROL_LOGICAL_PORT)
    control: Option<ControlExtras>,
}

impl OutboundEntry {
    fn is_dead(&self) -> bool {
        self.cancel.is_cancelled()
    }

    fn send_handle(&self) -> Option<SendHandle> {
        Some(SendHandle {
            write_state: Arc::clone(self.write_state.as_ref()?),
            health: Arc::clone(&self.health),
            cancel: self.cancel.clone(),
        })
    }
}

/// What the send path needs to write one frame to a cached connection, cloned
/// out of the cache so the (blocking) write never holds a cache shard lock.
struct SendHandle {
    write_state: SharedWriteState,
    health: Arc<ConnHealth>,
    cancel: CancellationToken,
}

/// Everything a control connection has that a data connection does not: what it
/// takes to issue a PORT_RESERVE round-trip and collect the answer.
///
/// It hangs off `OutboundEntry::control`, which is what makes "only a control
/// connection writes protocol frames" a shape rather than a convention — a data
/// connection has no inbox to send one through, because nothing ever does. It is
/// also the handle the connect path carries around, cloned out of the cache so
/// the round-trip never holds a cache shard lock across an await.
#[derive(Clone)]
struct ControlExtras {
    /// Inbox feeding this connection's `writer_task` (reader replies,
    /// PORT_RESERVE).
    writer_tx: mpsc::Sender<Vec<u8>>,
    /// Same `Arc` that lives in `ConnectionRegistry::ConnectionEntry::pending_ack`.
    /// dispatch installs the response; the awaiting connect_task takes it.
    pending_ack: Arc<StdMutex<Option<oneshot::Sender<ControlMsg>>>>,
    /// Lets only one PORT_RESERVE round-trip run at a time, so concurrent
    /// requests don't clobber each other's `pending_ack` slot.
    request_lock: Arc<TokioMutex<()>>,
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
    peer_handshake_timeout: Duration,
    tls_handshake_timeout: Duration,

    send_deadline: Option<Duration>,
    send_probe_deadline: Duration,
    congestion_miss_threshold: u32,
    congestion_isolation: bool,

    /// Drop counters. `Arc` so the off-thread write task can record its own
    /// failures without holding the sender alive.
    stats: Arc<SendStats>,

    tls_config: Option<Arc<TlsConfig>>,
    shared: Arc<ConnectionRegistry>,

    /// Outbound connection cache, keyed by (addr, logical_port).
    connections: Arc<DashMap<(SocketAddr, u16), OutboundEntry>>,

    /// One token per peer, parent of that peer's connection tokens.
    peer_cancel: Arc<DashMap<SocketAddr, CancellationToken>>,

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
        cancel: CancellationToken,
    ) -> Arc<Self> {
        let runtime_handle = tokio::runtime::Handle::current();

        // Prefer the configured public address (WAN/NAT).
        let public_addr = tcp_config.public_address;

        // Congested probe deadline: a small fraction of the send deadline
        // (≥1ms). At the 1000ms default this is 50ms. Unused unless isolation
        // applies.
        let send_probe_deadline = tcp_config
            .send_deadline
            .map_or(Duration::from_millis(1), |d| (d / 20).max(Duration::from_millis(1)));

        // Health's only question is how long to wait for the lock. Both ends of
        // the dial answer it with a constant, leaving it nothing to decide.
        let congestion_isolation = matches!(tcp_config.send_deadline, Some(d) if !d.is_zero());

        Arc::new(Self {
            domain_id,
            participant_id,
            working_ip,
            listener_port,
            public_addr,
            local_guid_prefix,
            connect_timeout: tcp_config.connect_timeout,
            peer_handshake_timeout: tcp_config.peer_handshake_timeout,
            tls_handshake_timeout: tcp_config.tls_handshake_timeout,
            send_deadline: tcp_config.send_deadline,
            send_probe_deadline,
            congestion_miss_threshold: tcp_config.congestion_miss_threshold,
            congestion_isolation,
            stats: Arc::new(SendStats::default()),
            tls_config,
            shared,
            connections: Arc::new(DashMap::new()),
            peer_cancel: Arc::new(DashMap::new()),
            in_flight: Arc::new(DashMap::new()),
            runtime_handle,
            cancel,
        })
    }

    /// Return the live write state + health for `(addr, logical_port)`, creating
    /// a *connecting* entry — and spawning an background connect — on cache miss.
    fn ensure_connecting_entry(
        self: &Arc<Self>,
        addr: SocketAddr,
        logical_port: u16,
    ) -> io::Result<Option<SendHandle>> {
        // Control connections are established on demand by data connects; they
        // are never a direct send destination.
        if logical_port == CONTROL_LOGICAL_PORT {
            return Ok(None);
        }

        // Never connect to our own listener (SPDP self-loop): drop without
        // creating an entry or spawning a connect.
        if self.is_self_connection(&addr) {
            return Ok(None);
        }

        let key = (addr, logical_port);

        // Evict a torn-down entry the reaper has not removed yet, so a fresh
        // connect is started below instead of writing into a dead connection.
        let dead = matches!(self.connections.get(&key), Some(e) if e.is_dead());
        if dead {
            self.evict_connection(addr, logical_port);
        }

        if let Some(remaining) = self.backoff_remaining(addr) {
            self.stats.backoff.record("reconnect backoff", addr, logical_port);
            return Err(transport_io_error(
                TransportErrorCode::TcpReconnectBackoff,
                format!("peer {:?} is in reconnect backoff for another {:?}", addr, remaining),
            ));
        }

        // Create-or-get atomically so concurrent sends to a new peer start
        // exactly one background connect and all buffer into the same state.
        match self.connections.entry(key) {
            Entry::Occupied(o) => Ok(o.get().send_handle()),
            Entry::Vacant(v) => {
                let write_state = init_connection_state();
                let conn_cancel = self.peer_token(addr).child_token();

                let entry = OutboundEntry {
                    write_state: Some(Arc::clone(&write_state)),
                    cancel: conn_cancel.clone(),
                    health: Arc::new(ConnHealth::new(self.congestion_miss_threshold)),
                    control: None,
                };
                let handle = entry.send_handle();
                // Publish the entry, and release the shard, before anything is
                // spawned: `disconnect_peer` cancels what it can find, so a
                // connect started against an entry not yet visible would be a
                // connect nobody can stop.
                drop(v.insert(entry));
                // A data connection has no writer task, so the reaper carries its
                // write state and owns the graceful close.
                self.spawn_reaper(key, conn_cancel.clone(), Some(Arc::clone(&write_state)));

                // Off-thread connect: never blocks the send path. On success it
                // flushes frames buffered while connecting and goes Ready; on
                // failure it evicts the entry and arms backoff.
                self.runtime_handle.spawn(background_connect(
                    Arc::clone(self),
                    addr,
                    logical_port,
                    write_state,
                    conn_cancel,
                ));
                Ok(handle)
            }
        }
    }

    /// This peer's token, creating it if the peer is new. Connection tokens are
    /// children of it, which is what lets `disconnect_peer` cancel a connect
    /// that has not produced a connection yet.
    fn peer_token(&self, addr: SocketAddr) -> CancellationToken {
        self.peer_cancel.entry(addr).or_insert_with(|| self.cancel.child_token()).value().clone()
    }

    /// One reaper per outbound connection: it waits on the connection's token
    /// and reclaims the connection's resources once the token fires.
    fn spawn_reaper(
        self: &Arc<Self>,
        key: (SocketAddr, u16),
        cancel: CancellationToken,
        close: Option<SharedWriteState>,
    ) {
        // Weak, not Arc: an Arc would keep the sender alive, and `Drop for
        // TcpSender` firing `cancel` is one of the things this task waits for.
        let weak = Arc::downgrade(self);
        self.runtime_handle.spawn(async move {
            cancel.cancelled().await;

            // Drop the cache entry first — its token has fired — so a dead entry
            // does not linger while the close below waits on an in-flight send.
            // Remove only if the entry is itself dead: a reconnect may already
            // have replaced it with a live one, which has its own token+reaper.
            // Skipped if the sender is gone, or if another path removed it first.
            if let Some(sender) = weak.upgrade() {
                sender.connections.remove_if(&key, |_, e| e.is_dead());
            }

            // Graceful close for a writer-less (data) connection. Independent of
            // the removal above so it runs even when another path removed the
            // entry; `graceful_close` serialises against any in-flight send.
            if let Some(write_state) = close {
                graceful_close(&write_state).await;
            }
        });
    }

    /// Tear down a single cached connection `(addr, logical_port)` and its
    /// actor pair, leaving every other connection to the peer untouched.
    ///
    /// Used on a write failure: the write proves only that *this* connection is
    /// unusable. Firing the cancel token wakes the task pair, whose reader exit
    /// calls `ConnectionRegistry::remove_connection` to drop the shared-state entry.
    fn evict_connection(&self, addr: SocketAddr, logical_port: u16) {
        if let Some((_, entry)) = self.connections.remove(&(addr, logical_port)) {
            entry.cancel.cancel();
        }
    }

    /// Tear down every outbound connection to `addr` (all logical ports) and its
    /// actor pairs. Called when the DDS layer unmatches the peer, so its transport
    /// resources are released promptly. Cancelling each actor wakes its reader,
    /// which drops the matching `ConnectionRegistry` entry.
    pub(crate) fn disconnect_peer(&self, addr: SocketAddr) {
        // Fire the peer token first: it reaches every connection to the peer
        // *and* any connect still mid-handshake, which owns no cache entry to
        // find. Removing it lets a later send to the same peer start clean.
        if let Some((_, peer_token)) = self.peer_cancel.remove(&addr) {
            peer_token.cancel();
        }
        // The tokens above are already cancelled through their parent; this is
        // what makes the removal synchronous rather than waiting on the reapers.
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

    #[cfg(test)]
    pub(crate) fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
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
impl TcpSender {
    /// SPDP bootstrap fan-out to an initial peer we have not discovered yet.
    /// The peer's participant id (hence its logical port) is unknown, so we
    /// reserve the well-known index-0 metatraffic port. Best-effort: SPDP is
    /// re-announced periodically, so a dropped announcement is harmless.
    pub(crate) fn send_to_discovery(
        self: &Arc<Self>,
        addr: &SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        let logical_port = PortManager::get_discovery_traffic_unicast_port(self.domain_id, 0);
        self.send_to(*addr, logical_port, data)
    }

    /// Write one frame to `(addr, logical_port)`, establishing the connection
    /// off-thread on cache miss. Blocks only to take the connection's write
    /// lock, and only up to the send deadline; while still connecting the frame
    /// is buffered (bounded). See [`TcpSender::write_frame`].
    pub(crate) fn send_to(
        self: &Arc<Self>,
        addr: SocketAddr,
        logical_port: u16,
        data: &[u8],
    ) -> io::Result<()> {
        // A control connection is never a send target.
        if logical_port == CONTROL_LOGICAL_PORT {
            return Ok(());
        }

        let key = (addr, logical_port);

        // Fast path: a live cached connection. The cache guard is dropped
        // before the (blocking) write so it never spans a shard lock.
        let live = match self.connections.get(&key) {
            Some(e) if !e.is_dead() => e.send_handle(),
            _ => None,
        };
        let handle = match live {
            Some(h) => h,
            None => match self.ensure_connecting_entry(addr, logical_port)? {
                Some(h) => h,
                // Not a send target at all — nothing was meant to reach the wire.
                None => return Ok(()),
            },
        };

        self.write_frame(addr, logical_port, &handle, data)
    }

    /// Hand one frame to the connection, never blocking the caller for longer
    /// than the deadline.
    ///
    /// `Ok` means the frame was accepted: either buffered for the connect
    /// window, or staged under the write lock and handed to a task that runs the
    /// write to completion. `Err` means it was refused before any of that, so a
    /// caller whose bookkeeping depends on the frame having left — a piggyback
    /// heartbeat, for one — can tell the two apart.
    fn write_frame(
        &self,
        addr: SocketAddr,
        logical_port: u16,
        handle: &SendHandle,
        data: &[u8],
    ) -> io::Result<()> {
        let SendHandle { write_state, health, cancel } = handle;
        // `None` = block until the previous frame completes (no pre-wire drop);
        // `Some` = bounded wait; `Some(0)` = take the lock only if it is free.
        let deadline = if self.congestion_isolation && health.is_congested() {
            Some(self.send_probe_deadline)
        } else {
            self.send_deadline
        };

        // The lock is the only bounded wait. An expiry is clean by construction:
        // not a byte has left, so the stream stays well-formed and the frame is
        // refused rather than half-written.
        let guard = self.runtime_handle.block_on(async {
            match deadline {
                Some(d) => tokio::time::timeout(d, write_state.clone().lock_owned()).await.ok(),
                None => Some(write_state.clone().lock_owned().await),
            }
        });
        let mut guard = match guard {
            Some(g) => g,
            None => {
                // The miss counter only feeds congestion isolation, so it is
                // left alone where that does not apply. The refusal itself is
                // always recorded — at `0` refusing *is* the configured mode,
                // which is exactly when the counter matters most.
                if self.congestion_isolation {
                    health.on_miss();
                }
                self.stats.send_deadline.record("send deadline", addr, logical_port);
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "send deadline expired before the write lock was free",
                ));
            }
        };

        // Still connecting: buffer in order until the handshake completes. This
        // is the only path that needs an owned copy per frame.
        if let WriteState::Connecting(buf) = &mut *guard {
            if buf.len() >= CONNECT_BUFFER_DEPTH {
                self.stats.connect_buffer_full.record("connect buffer full", addr, logical_port);
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "connect-window buffer full",
                ));
            }
            buf.push_back(data.to_vec());
            return Ok(());
        }

        // Stage the frame while still holding the lock, then hand the guard off.
        if let WriteState::Ready(r) = &mut *guard {
            r.data.clear();
            r.data.extend_from_slice(data);
        }

        let health = Arc::clone(health);
        let send_deadline = self.send_deadline;
        let stats = Arc::clone(&self.stats);
        let cancel = cancel.clone();
        self.runtime_handle.spawn(async move {
            let started = Instant::now();
            let res = match &mut *guard {
                WriteState::Ready(WriteHandoff { half, data }) => {
                    write_framed_message(half, data).await
                }
                WriteState::Connecting(_) => Ok(()),
            };
            match res {
                // Congestion levels are also determined by comparing the time elapsed up
                // to the completion of transmission with the deadline.
                Ok(()) => {
                    if let Some(d) = send_deadline {
                        if started.elapsed() <= d {
                            health.on_success();
                        }
                    }
                }
                // A failed write proves this connection is unusable. Firing the token
                // tears the pair down and lets the reaper drop the cache entry.
                Err(e) => {
                    debug!(
                        "TcpSender: write to {:?} (port={}) failed: {:?}",
                        addr, logical_port, e
                    );
                    stats.write_error.record("write error", addr, logical_port);
                    cancel.cancel();
                }
            }
            // Guard drops here — after the frame is fully in the socket buffer.
        });

        Ok(())
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

async fn do_connect_control(sender: &Arc<TcpSender>, addr: SocketAddr) -> io::Result<()> {
    if sender.is_self_connection(&addr) {
        return Err(io::Error::new(io::ErrorKind::AddrInUse, "self connect"));
    }

    let conn_cancel = sender.peer_token(addr).child_token();

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
    peer_hello_handshake(&mut stream, locator, sender.peer_handshake_timeout).await?;

    // 3. Control inbox + cancel + pending_ack mailbox. The handshake is done and
    //    a control connection carries only protocol frames, so its `writer_task`
    //    owns the write half outright — no connect window, no shared write state.
    let (writer_tx, control_rx) = mpsc::channel::<Vec<u8>>(inbox_capacity());
    let (read_half, write_half) = stream.into_split();
    let pending_ack = Arc::new(StdMutex::new(None));
    let request_lock = Arc::new(TokioMutex::new(()));

    // 4. Register BEFORE spawning the tasks — otherwise the reader task could
    //    receive a frame and find no entry.
    let conn_id = sender.shared.register_outbound_control_connection(
        addr,
        conn_cancel.clone(),
        Arc::clone(&pending_ack),
    );

    // 5. Spawn the actor pair.
    spawn_tasks(
        read_half,
        conn_id,
        Arc::clone(&sender.shared),
        conn_cancel.clone(),
        writer_tx.clone(),
        control_rx,
        write_half,
    );

    // 6. Cache for future sends + future PORT_RESERVE round-trips. The reaper
    //    goes last, so that an already-cancelled token removes the entry rather
    //    than racing ahead of the insert and leaving it behind. A control
    //    connection is never a send target and its writer task owns its close, so
    //    it has no write state and the reaper's is `None` — it only drops the
    //    cache entry.
    let key = (addr, CONTROL_LOGICAL_PORT);
    sender.connections.insert(
        key,
        OutboundEntry {
            write_state: None,
            cancel: conn_cancel.clone(),
            health: Arc::new(ConnHealth::new(sender.congestion_miss_threshold)),
            control: Some(ControlExtras { writer_tx, pending_ack, request_lock }),
        },
    );
    sender.spawn_reaper(key, conn_cancel, None);

    debug!("TcpSender: control connection established to {:?} (conn={})", addr, conn_id);
    Ok(())
}

// ── background_connect / do_connect_data ────────────────────────────────────

/// Off-thread connect for a connecting data entry created by
/// `ensure_connecting_entry`. On success it flushes the frames buffered while
/// connecting and goes Ready; on failure the entry is evicted and reconnect
/// backoff armed.
async fn background_connect(
    sender: Arc<TcpSender>,
    addr: SocketAddr,
    logical_port: u16,
    write_state: SharedWriteState,
    conn_cancel: CancellationToken,
) {
    let connect = do_connect_data(&sender, addr, logical_port, write_state, conn_cancel.clone());

    let result = tokio::select! {
        r = connect => r,

        _ = conn_cancel.cancelled() => {
            debug!(
                "TcpSender: connect to {:?} (port={}) abandoned mid-flight (cancelled)",
                addr, logical_port
            );
            return;
        }
    };

    match result {
        Ok(()) => sender.note_connect_success(addr),
        Err(e) => {
            warn!(
                "TcpSender: background connect to {:?} (port={}) failed: {:?}",
                addr, logical_port, e
            );
            sender.evict_connection(addr, logical_port);
            sender.note_connect_failure(addr);
        }
    }
}

async fn do_connect_data(
    sender: &Arc<TcpSender>,
    addr: SocketAddr,
    logical_port: u16,
    write_state: SharedWriteState,
    conn_cancel: CancellationToken,
) -> io::Result<()> {
    if sender.is_self_connection(&addr) {
        return Err(io::Error::new(io::ErrorKind::AddrInUse, "self connect"));
    }

    // 1. Ensure we have a control connection.
    let control = ensure_control_connection(sender, addr).await?;

    // 2. PORT_RESERVE round-trip → cookie. A failure here means the control
    //    connection itself is unusable, so it is evicted.
    let cookie = match port_reserve_round_trip(
        &control,
        logical_port,
        sender.peer_handshake_timeout,
    )
    .await
    {
        Ok(cookie) => cookie,
        Err(e) => {
            sender.evict_connection(addr, CONTROL_LOGICAL_PORT);
            return Err(e);
        }
    };

    // 3. Open a TCP for the data connection.
    let mut stream = create_stream(sender, addr).await?;

    // 4. PORT_BIND + PORT_BIND_ACK (inline, before the tasks).
    port_bind_handshake(&mut stream, cookie, sender.peer_handshake_timeout).await?;

    // 5. Flush the connect-window buffer + go Ready, then register (before the
    //    reader starts) and spawn the reader task. No writer task: a data
    //    connection is `Active`, so it never sends a protocol reply, and its
    //    graceful close is the reaper's job. `reader_task` still needs a sender
    //    for `dispatch`'s signature, so hand it one whose receiver is dropped.
    let read_half = flush_and_ready(stream, &write_state).await?;
    let conn_id =
        sender.shared.register_outbound_data_connection(addr, logical_port, conn_cancel.clone());
    let (writer_tx, _rx) = mpsc::channel::<Vec<u8>>(1);
    spawn_reader_only(read_half, conn_id, Arc::clone(&sender.shared), conn_cancel, writer_tx);

    debug!(
        "TcpSender: data connection established to {:?} (port={}, conn={})",
        addr, logical_port, conn_id
    );
    Ok(())
}

// ── ensure_control_connection ───────────────────────────────────────────────

/// Look up the control connection in cache, creating it via the in-flight
/// gate if missing. Returns a handle the caller uses to issue PORT_RESERVE
/// round-trips.
async fn ensure_control_connection(
    sender: &Arc<TcpSender>,
    addr: SocketAddr,
) -> io::Result<ControlExtras> {
    let key = (addr, CONTROL_LOGICAL_PORT);

    if let Some(handle) = lookup_control_handle(sender, key) {
        return Ok(handle);
    }

    match acquire_in_flight(sender, key).await {
        InFlightAcquisition::Acquired(_guard) => {
            do_connect_control(sender, addr).await?;
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

fn lookup_control_handle(sender: &TcpSender, key: (SocketAddr, u16)) -> Option<ControlExtras> {
    if let Some(entry) = sender.connections.get(&key) {
        if !entry.is_dead() {
            return entry.control.clone();
        }
    }

    // Torn down but not reaped yet: drop it here so the caller reconnects
    // instead of waiting on a control connection that will never answer.
    sender.connections.remove_if(&key, |_, e| e.is_dead());
    None
}

// ── PORT_RESERVE round-trip via single-slot oneshot ─────────────────────────

async fn port_reserve_round_trip(
    control: &ControlExtras,
    logical_port: u16,
    timeout: Duration,
) -> io::Result<[u8; 16]> {
    // Serialise concurrent PORT_RESERVE on the same control connection.
    // Otherwise the second request would clobber the first's pending_ack slot.
    let _permit = control.request_lock.lock().await;

    let (tx, rx) = oneshot::channel();
    *control.pending_ack.lock().expect("pending_ack lock") = Some(tx);

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
            *control.pending_ack.lock().expect("pending_ack lock") = None;
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
            sender.tls_handshake_timeout,
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

    const CONGESTION_MISS_THRESHOLD: u32 = 3;

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

    /// Poll a channel receiver from async code without blocking a worker.
    /// Returns `Some(msg)` if received before `timeout`, else `None`.
    async fn wait_for_recv<T>(rx: &flume::Receiver<T>, timeout: Instant) -> Option<T> {
        while Instant::now() < timeout {
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
        make_sender_cfg(participant_id, guid_prefix, listener_port, &TcpConfig::default())
    }

    /// `make_sender` with an explicit `TcpConfig` (send deadline, queue depth,
    /// non-blocking mode, …).
    fn make_sender_cfg(
        participant_id: u32,
        guid_prefix: GuidPrefix,
        listener_port: u16,
        cfg: &TcpConfig,
    ) -> (Arc<TcpSender>, flume::Receiver<IncomingMessage>) {
        make_sender_tuned(
            participant_id,
            guid_prefix,
            listener_port,
            cfg,
            TcpSocketTuning::default(),
        )
    }

    /// `make_sender_cfg` with explicit socket tuning. The default leaves
    /// keepalive and the unacked timeout off, so a test that needs the kernel to
    /// give up on a connection has to ask for it.
    fn make_sender_tuned(
        participant_id: u32,
        guid_prefix: GuidPrefix,
        listener_port: u16,
        cfg: &TcpConfig,
        tuning: TcpSocketTuning,
    ) -> (Arc<TcpSender>, flume::Receiver<IncomingMessage>) {
        let (d_tx, d_rx, u_tx, _u_rx) = make_channels();
        let shared =
            Arc::new(ConnectionRegistry::new(0, participant_id, guid_prefix, tuning, d_tx, u_tx));
        let sender = TcpSender::new(
            0,
            participant_id,
            "127.0.0.1".to_string(),
            listener_port,
            guid_prefix,
            None,
            shared,
            cfg,
            CancellationToken::new(),
        );
        (sender, d_rx)
    }

    /// A `SendHandle` over a write state the test drives itself, for the tests
    /// that exercise `write_frame` directly instead of going through the
    /// connect + handshake path.
    fn test_handle(
        write_state: &SharedWriteState,
        health: &Arc<ConnHealth>,
        cancel: &CancellationToken,
    ) -> SendHandle {
        SendHandle {
            write_state: Arc::clone(write_state),
            health: Arc::clone(health),
            cancel: cancel.clone(),
        }
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
    /// and the RTPS frame is routed to side B's discovery channel.
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
            Duration::from_secs(5),
            Duration::from_secs(5),
            CancellationToken::new(),
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
        let timeout = Instant::now() + Duration::from_secs(5);
        let received = wait_for_recv(&b_disc_rx, timeout).await;
        let msg = received.expect("listener did not receive RTPS data within 5s");

        assert_eq!(&msg.data[..rtps_data.len()], rtps_data);

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
            Duration::from_secs(5),
            Duration::from_secs(5),
            CancellationToken::new(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xAA; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00").await.expect("send");

        // Wait for cache to populate (control + discovery-data entries).
        let timeout = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, timeout).await.expect("send did not deliver within 5s");
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
        // `ensure_connecting_entry` rejects the self-connect before inserting an
        // entry or spawning a connect, so nothing is cached.
        let _ = blocking_send_discovery(&sender, self_addr, b"RTPS\x00\x00\x00\x00").await;

        assert_eq!(sender.connections.len(), 0, "self-connection must not be cached",);

        sender.shutdown().await;
    }

    // ── health state machine ─────────────────────────────────────────────────

    /// `ConnHealth` congests once `CONGESTION_MISS_THRESHOLD` misses accumulate,
    /// then recovers with hysteresis: each success decrements the miss counter
    /// and the connection stays congested until the counter falls back below the
    /// threshold, so a deeply-congested peer needs several successes to recover.
    #[test]
    fn conn_health_congests_at_threshold_and_recovers_with_hysteresis() {
        let h = ConnHealth::new(CONGESTION_MISS_THRESHOLD);
        assert!(!h.is_congested());

        // Misses below the threshold do not congest; the threshold-th one does.
        for _ in 0..CONGESTION_MISS_THRESHOLD - 1 {
            h.on_miss();
            assert!(!h.is_congested(), "must not congest before the threshold");
        }
        h.on_miss();
        assert!(h.is_congested(), "must congest at the threshold");

        // From exactly the threshold, one success drops the counter below it.
        h.on_success();
        assert!(!h.is_congested(), "a success from the threshold recovers to healthy");

        // Hysteresis: drive the counter well past the threshold — now a single
        // success is not enough; recovery needs the counter back below it.
        let h = ConnHealth::new(CONGESTION_MISS_THRESHOLD);
        for _ in 0..CONGESTION_MISS_THRESHOLD + 2 {
            h.on_miss();
        }
        assert!(h.is_congested());
        h.on_success();
        assert!(h.is_congested(), "still congested while misses >= threshold");
        h.on_success();
        assert!(h.is_congested(), "still congested while misses >= threshold");
        h.on_success();
        assert!(!h.is_congested(), "recovers once misses fall below the threshold");
    }

    /// A send targeting the control logical port must not spin up a connection:
    /// control connections are established only as a side-effect of data
    /// connects, never as a direct destination.
    #[tokio::test(flavor = "multi_thread")]
    async fn ensure_connecting_entry_rejects_control_port() {
        let (sender, _rx) = make_sender(0, [0xAF; 12], 7040);
        let peer: SocketAddr = "192.0.2.20:7400".parse().unwrap();

        assert!(
            sender
                .ensure_connecting_entry(peer, CONTROL_LOGICAL_PORT)
                .expect("the control port is not a send target, not a send failure")
                .is_none(),
            "control-port send must not create a connecting entry"
        );
        assert_eq!(sender.connections.len(), 0, "control-port send must not cache a connection");

        sender.shutdown().await;
    }

    /// A control connection is never a send target: it carries no write state and
    /// yields no send handle. This is the structural guarantee — a stray send to
    /// the control port finds nothing to write to, so the frame is dropped rather
    /// than injected onto the control wire (and the `send_to` port-0 guard drops
    /// it even sooner). Its writer task owns the write half outright.
    #[tokio::test(flavor = "multi_thread")]
    async fn control_connection_is_never_a_send_target() {
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
            Duration::from_secs(5),
            Duration::from_secs(5),
            CancellationToken::new(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xAE; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        // A data send establishes the control connection as a side effect.
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00").await.expect("send");
        let timeout = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, timeout).await.expect("send did not deliver within 5s");

        let control =
            sender.connections.get(&(target, CONTROL_LOGICAL_PORT)).expect("control cached");
        assert!(control.write_state.is_none(), "a control connection carries no write state");
        assert!(control.send_handle().is_none(), "a control connection is never a send target");
        drop(control);

        // The data connection, by contrast, is a send target.
        let disc_port = PortManager::get_discovery_traffic_unicast_port(0, 0);
        let data = sender.connections.get(&(target, disc_port)).expect("data cached");
        assert!(data.write_state.is_some(), "a data connection carries write state");
        assert!(data.send_handle().is_some(), "a data connection is a send target");
        drop(data);

        sender.shutdown().await;
        listener.shutdown().await;
    }

    // ── head-of-line-blocking isolation ──────────────────────────────────────

    /// A first send to a brand-new peer must not wait for the TCP connect: the
    /// connect runs off-thread and the frame is buffered, so the call returns
    /// far sooner than `connect_timeout` (5s default) even for an unroutable peer.
    #[tokio::test(flavor = "multi_thread")]
    async fn connect_to_unreachable_peer_does_not_block_send() {
        let (sender, _rx) = make_sender(0, [0xAB; 12], 7010);
        // RFC 5737 TEST-NET-1 — guaranteed unroutable.
        let unreachable: SocketAddr = "192.0.2.1:7400".parse().unwrap();

        let s = Arc::clone(&sender);
        let elapsed = tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            let _ = s.send_to(unreachable, 7400, b"RTPS\x00\x00\x00\x00");
            start.elapsed()
        })
        .await
        .unwrap();

        assert!(
            elapsed < Duration::from_secs(1),
            "send blocked on connect ({elapsed:?}); expected immediate return"
        );
        sender.shutdown().await;
    }

    /// The connect window absorbs a burst, not an unbounded backlog. Once the
    /// buffer is at depth the frame is refused, and the refusal reaches the
    /// caller — buffering is a promise to deliver, so a frame that never made it
    /// into the buffer must not read as one that did.
    #[tokio::test(flavor = "multi_thread")]
    async fn connect_buffer_full_refuses_the_frame() {
        const PAST_DEPTH: usize = 8;

        let (sender, _rx) = make_sender(0, [0xAE; 12], 7012);
        // RFC 5737 TEST-NET-1 — the connect never completes, so the entry stays
        // in its connect window for the whole test.
        let unreachable: SocketAddr = "192.0.2.2:7400".parse().unwrap();

        let s = Arc::clone(&sender);
        let (buffered, refused) = tokio::task::spawn_blocking(move || {
            let mut buffered = 0usize;
            let mut refused = 0usize;
            for _ in 0..CONNECT_BUFFER_DEPTH + PAST_DEPTH {
                match s.send_to(unreachable, 7400, b"RTPS\x00\x00\x00\x00") {
                    Ok(()) => buffered += 1,
                    Err(e) => {
                        assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
                        refused += 1;
                    }
                }
            }
            (buffered, refused)
        })
        .await
        .unwrap();

        assert_eq!(buffered, CONNECT_BUFFER_DEPTH, "the connect window must buffer its full depth");
        assert_eq!(refused, PAST_DEPTH, "every frame past the depth must be refused");
        assert_eq!(sender.stats.connect_buffer_full.count(), PAST_DEPTH as u64);

        sender.shutdown().await;
    }

    /// A write whose write-state lock is already held times out *acquiring the
    /// lock* and refuses the frame, without ever touching the stream. Repeated
    /// expiries congest the connection. (Recovery is covered by
    /// `conn_health_congests_at_threshold_and_recovers_with_hysteresis`.)
    #[tokio::test(flavor = "multi_thread")]
    async fn write_times_out_on_lock_and_congests() {
        let cfg =
            TcpConfig { send_deadline: Some(Duration::from_millis(30)), ..TcpConfig::default() };
        let (sender, _rx) = make_sender_cfg(0, [0xAC; 12], 7020, &cfg);
        let addr: SocketAddr = "192.0.2.9:7400".parse().unwrap();

        let write_state = init_connection_state();
        let health = Arc::new(ConnHealth::new(CONGESTION_MISS_THRESHOLD));

        // Hold the lock, as an in-flight write to a stalled wire would.
        let guard = write_state.lock().await;
        {
            let s = Arc::clone(&sender);
            let handle = test_handle(&write_state, &health, &CancellationToken::new());
            tokio::task::spawn_blocking(move || {
                for _ in 0..CONGESTION_MISS_THRESHOLD {
                    let err = s
                        .write_frame(addr, 7400, &handle, b"x")
                        .expect_err("a frame refused at the lock must be reported to the caller");
                    assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
                }
            })
            .await
            .unwrap();
        }
        assert!(health.is_congested(), "repeated lock timeouts must congest the connection");
        assert_eq!(sender.stats.send_deadline.count(), CONGESTION_MISS_THRESHOLD as u64);

        drop(guard);
        sender.shutdown().await;
    }

    /// With `send_deadline: None` (property `-1`) a send blocks on the lock
    /// instead of dropping: the frame is never lost before the wire and the
    /// connection never congests. Reverting to a bounded deadline drops the
    /// frame and records a miss, failing this test.
    #[tokio::test(flavor = "multi_thread")]
    async fn block_mode_waits_for_the_lock_instead_of_dropping() {
        let cfg = TcpConfig { send_deadline: None, ..TcpConfig::default() };
        let (sender, _rx) = make_sender_cfg(0, [0xB0; 12], 7040, &cfg);
        let addr: SocketAddr = "192.0.2.10:7400".parse().unwrap();

        let write_state = init_connection_state();
        let health = Arc::new(ConnHealth::new(CONGESTION_MISS_THRESHOLD));

        // Hold the lock, as an in-flight write to a stalled wire would.
        let guard = write_state.lock().await;

        let s = Arc::clone(&sender);
        let handle = test_handle(&write_state, &health, &CancellationToken::new());
        // Parks on the lock; a bounded deadline would have dropped by now.
        let sender_task = tokio::task::spawn_blocking(move || {
            s.write_frame(addr, 7400, &handle, b"x").unwrap();
        });

        // Let the send park well past any bounded deadline, then release.
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(guard);
        sender_task.await.unwrap();

        assert_eq!(sender.stats.send_deadline.count(), 0, "block mode must not drop");
        assert!(!health.is_congested(), "block mode must never congest");

        sender.shutdown().await;
    }

    /// With `send_deadline: Some(0)` (property `0`) a held lock drops the frame
    /// at once. The drop is still counted, but health stays inert: there is no
    /// wait left to shorten, and the probe deadline's 1ms floor would only make
    /// it longer. Reverting the `congestion_isolation` gate congests the
    /// connection on the first miss and fails this test.
    #[tokio::test(flavor = "multi_thread")]
    async fn zero_deadline_never_congests_but_still_counts_drops() {
        const SENDS: u64 = 5;
        let cfg = TcpConfig { send_deadline: Some(Duration::ZERO), ..TcpConfig::default() };
        let (sender, _rx) = make_sender_cfg(0, [0xB1; 12], 7042, &cfg);
        let addr: SocketAddr = "192.0.2.11:7400".parse().unwrap();

        let write_state = init_connection_state();
        // Threshold 1 is the production default and the harshest case here: a
        // single unguarded miss would be enough to congest.
        let health = Arc::new(ConnHealth::new(1));

        // Hold the lock, as an in-flight write to a stalled wire would.
        let guard = write_state.lock().await;
        {
            let s = Arc::clone(&sender);
            let handle = test_handle(&write_state, &health, &CancellationToken::new());
            tokio::task::spawn_blocking(move || {
                for _ in 0..SENDS {
                    s.write_frame(addr, 7400, &handle, b"x")
                        .expect_err("a held lock must refuse the frame");
                }
            })
            .await
            .unwrap();
        }

        assert!(!health.is_congested(), "a zero deadline must never congest");
        assert_eq!(sender.stats.send_deadline.count(), SENDS, "drops must still be counted");

        drop(guard);
        sender.shutdown().await;
    }

    /// A zero deadline is a try-lock, not an unconditional drop: the `timeout`
    /// wrapper polls the inner future before the (already elapsed) delay, so a
    /// free lock is taken and nothing is dropped.
    #[tokio::test(flavor = "multi_thread")]
    async fn zero_deadline_takes_a_free_lock() {
        let cfg = TcpConfig { send_deadline: Some(Duration::ZERO), ..TcpConfig::default() };
        let (sender, _rx) = make_sender_cfg(0, [0xB2; 12], 7044, &cfg);
        let addr: SocketAddr = "192.0.2.12:7400".parse().unwrap();

        let write_state = init_connection_state();
        let health = Arc::new(ConnHealth::new(1));

        let s = Arc::clone(&sender);
        let handle = test_handle(&write_state, &health, &CancellationToken::new());
        tokio::task::spawn_blocking(move || {
            s.write_frame(addr, 7400, &handle, b"x").unwrap();
        })
        .await
        .unwrap();

        assert_eq!(sender.stats.send_deadline.count(), 0, "a free lock must be taken, not dropped");

        sender.shutdown().await;
    }

    /// `unacked_timeout` is what turns a peer that stopped acknowledging into a
    /// write error — the only thing that detects a peer which is gone but never
    /// closed. Without it such a write parks on the OS default (~15 min) and the
    /// connection is never torn down.
    ///
    /// Gated to Linux/Android. TODO: verify and implement the `unacked_timeout`
    /// behavior on macOS and Windows, then re-enable this test there.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[tokio::test(flavor = "multi_thread")]
    async fn unacked_timeout_turns_a_silent_peer_into_a_write_error() {
        const FRAME: usize = 64 * 1024;
        const UNACKED: Duration = Duration::from_secs(2);

        let tuning = TcpSocketTuning {
            unacked_timeout: Some(UNACKED),
            so_sndbuf: Some(4 * 1024),
            ..TcpSocketTuning::default()
        };
        let cfg =
            TcpConfig { send_deadline: Some(Duration::from_millis(50)), ..TcpConfig::default() };
        let (sender, _rx) = make_sender_tuned(0, [0xBA; 12], 7100, &cfg, tuning);

        // Accept, then never read and never close: the peer's window shuts and
        // our data stays unacknowledged, with no FIN or RST to notice instead.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = listener.local_addr().unwrap();
        let accepted = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
            drop(stream);
        });

        let tcp = TcpStream::connect(peer_addr).await.unwrap();
        let sock = socket2::SockRef::from(&tcp);
        let _ = sock.set_recv_buffer_size(4 * 1024);
        apply_socket_tuning(&tcp, &sender.shared.tuning);

        let write_state = init_connection_state();
        let _read_half = flush_and_ready(wrap_plain(tcp), &write_state).await.unwrap();
        let health = Arc::new(ConnHealth::new(CONGESTION_MISS_THRESHOLD));
        let cancel = CancellationToken::new();
        let handle = test_handle(&write_state, &health, &cancel);

        let s = Arc::clone(&sender);
        let c = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let payload = vec![0xEE; FRAME];
            let deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < deadline && !c.is_cancelled() {
                // Refusals at the lock are expected while the wire is stalled;
                // what is under test is the write task's own failure.
                let _ = s.write_frame(peer_addr, 7400, &handle, &payload);
                std::thread::sleep(Duration::from_millis(50));
            }
        })
        .await
        .unwrap();

        assert!(
            cancel.is_cancelled(),
            "a peer that stops acknowledging must surface as a write error within \
             unacked_timeout ({UNACKED:?}) and tear the connection down",
        );
        assert!(sender.stats.write_error.count() > 0, "the failed write must be counted");

        accepted.abort();
        sender.shutdown().await;
    }

    /// Cancellation does **not** interrupt a write already on the wire — the one
    /// deliberate exception to "a cancel tears the connection down at once".
    ///
    /// A write that stopped mid-frame would leave the peer's stream misframed
    /// for good, and every later frame on it garbage. So the write task's only
    /// exit is the write returning, and it is exactly that guarantee which makes
    /// releasing the lock mean "this frame is wholly in the socket buffer".
    /// The cost is bounded and accepted: the write half survives the cancel
    /// until the OS gives up on the frame (`unacked_timeout`).
    ///
    /// The stalled write here holds the state lock for as long as it runs, so
    /// the lock still being held after the cancel is the observable form of
    /// "the write was not abandoned".
    #[tokio::test(flavor = "multi_thread")]
    async fn cancel_does_not_interrupt_a_write_in_flight() {
        const FRAME: usize = 64 * 1024;

        let cfg =
            TcpConfig { send_deadline: Some(Duration::from_millis(50)), ..TcpConfig::default() };
        let (sender, _rx) = make_sender_cfg(0, [0xB9; 12], 7090, &cfg);

        // A peer that accepts and never reads: the write cannot finish.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = listener.local_addr().unwrap();
        // Pin the peer's recv buffer small (accepted sockets inherit it) so an
        // unread frame stays parked. An OS that autotunes the recv buffer up
        // would keep swallowing bytes until the frame completes, which reads
        // here as an abandoned write rather than one still in flight.
        let _ = socket2::SockRef::from(&listener).set_recv_buffer_size(4 * 1024);
        let accepted = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
            drop(stream);
        });

        let tcp = TcpStream::connect(peer_addr).await.unwrap();
        let sock = socket2::SockRef::from(&tcp);
        let _ = sock.set_send_buffer_size(4 * 1024);
        let _ = sock.set_recv_buffer_size(4 * 1024);

        let write_state = init_connection_state();
        let _read_half = flush_and_ready(wrap_plain(tcp), &write_state).await.unwrap();
        let health = Arc::new(ConnHealth::new(CONGESTION_MISS_THRESHOLD));
        let cancel = CancellationToken::new();
        let handle = test_handle(&write_state, &health, &cancel);

        // Fill the wire, so a write is parked holding the lock.
        let s = Arc::clone(&sender);
        let ws = Arc::clone(&write_state);
        tokio::task::spawn_blocking(move || {
            let payload = vec![0xEE; FRAME];
            for _ in 0..64 {
                let _ = s.write_frame(peer_addr, 7400, &handle, &payload);
            }
            // The lock is held by the stalled write; anything else times out.
            assert!(ws.try_lock().is_err(), "expected a write to be stuck holding the lock");
        })
        .await
        .unwrap();

        cancel.cancel();
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert!(
            write_state.try_lock().is_err(),
            "the in-flight write let go of the lock after a cancel — it must run to \
             completion instead, or the peer is left with a truncated frame",
        );

        accepted.abort();
        sender.shutdown().await;
    }

    /// Head-of-line blocking regression.
    ///
    /// A peer whose socket send buffer is full — the wire genuinely stalled, not
    /// a lock held by a test — must not capture the publishing thread. The DDS
    /// send path fans out to every matched reader from one thread
    /// (`UserLogic::send_rtps_message_to_locators`), so a send that blocks for
    /// the duration of the write starves every peer behind it in that loop.
    ///
    /// Here the peer accepts the connection and then never reads, so once the
    /// send buffer and the peer's receive buffer fill, no write can complete
    /// until the OS gives up. Every `write_frame` must still return promptly:
    /// the first hands the stalled write to a task, and the rest time out taking
    /// the lock.
    #[tokio::test(flavor = "multi_thread")]
    async fn stalled_wire_does_not_block_the_publishing_thread() {
        const SEND_DEADLINE: Duration = Duration::from_millis(50);
        const FRAME: usize = 64 * 1024;
        const FRAMES: usize = 256; // 16MB — far past any socket buffer.

        let cfg = TcpConfig { send_deadline: Some(SEND_DEADLINE), ..TcpConfig::default() };
        let (sender, _rx) = make_sender_cfg(0, [0xAD; 12], 7030, &cfg);

        // A peer that accepts and then never reads a byte.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = listener.local_addr().unwrap();
        let accepted = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            // Hold the connection open, reading nothing, until the test ends.
            std::future::pending::<()>().await;
            drop(stream);
        });

        let tcp = TcpStream::connect(peer_addr).await.unwrap();
        // Shrink both buffers so the wire stalls after a few frames rather than
        // absorbing megabytes.
        let sock = socket2::SockRef::from(&tcp);
        let _ = sock.set_send_buffer_size(4 * 1024);
        let _ = sock.set_recv_buffer_size(4 * 1024);

        // Go Ready directly: this exercises the write path, not the handshake.
        let write_state = init_connection_state();
        let _read_half = flush_and_ready(wrap_plain(tcp), &write_state).await.unwrap();
        let health = Arc::new(ConnHealth::new(CONGESTION_MISS_THRESHOLD));

        let s = Arc::clone(&sender);
        let handle = test_handle(&write_state, &health, &CancellationToken::new());
        let worst = tokio::task::spawn_blocking(move || {
            let payload = vec![0xEE; FRAME];
            let mut worst = Duration::ZERO;
            for _ in 0..FRAMES {
                let t = Instant::now();
                let _ = s.write_frame(peer_addr, 7400, &handle, &payload);
                worst = worst.max(t.elapsed());
            }
            worst
        })
        .await
        .unwrap();

        // The publishing thread is bounded by the deadline, not by the write.
        // Generous multiplier: this asserts "not captured", not scheduler timing.
        assert!(
            worst < SEND_DEADLINE * 4,
            "a stalled peer captured the publishing thread for {worst:?} \
             (send_deadline={SEND_DEADLINE:?}); sends to other peers would starve"
        );
        assert!(
            health.is_congested(),
            "a stalled wire must be detected via lock-wait misses and congest"
        );
        assert!(sender.stats.send_deadline.count() > 0, "deadline drops must be counted");

        accepted.abort();
        sender.shutdown().await;
    }

    // ── self-sufficient teardown ─────────────────────────────────────────────

    /// Poll `cond` until it holds or `timeout` elapses. Teardown runs through
    /// tasks (reader exit → cancel → reaper), so the observable end state is
    /// eventual rather than immediate.
    async fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if cond() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        cond()
    }

    /// The transport reclaims a dead connection entirely on its own — the rule
    /// being that detecting a disconnect and releasing what it holds is the
    /// transport's job, with no DDS involvement.
    ///
    /// Here only the peer's sockets go away. `disconnect_peer` — the DDS-unmatch
    /// path — is never called, and neither is any other DDS-side cleanup, so
    /// nothing but the transport noticing can drive this. Both maps must drain:
    /// the sender's outbound cache, which owns the write halves, and the
    /// registry. If the cache entry survived, so would its fd, until some later
    /// send happened to notice or a DDS lease expiry called `disconnect_peer` —
    /// leaning on the liveliness contract to do the transport's cleanup.
    #[tokio::test(flavor = "multi_thread")]
    async fn peer_loss_reclaims_every_connection_without_disconnect_peer() {
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
            Duration::from_secs(5),
            Duration::from_secs(5),
            CancellationToken::new(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xB0; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00").await.expect("send");

        let timeout = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, timeout).await.expect("send did not deliver within 5s");
        assert!(sender.connections.len() >= 2, "expected control + data entries");

        // The peer goes away — nothing else. This is the only stimulus.
        listener.shutdown().await;

        assert!(
            wait_until(Duration::from_secs(5), || sender.connections.is_empty()).await,
            "outbound cache still holds {} entries after the peer vanished; \
             the transport must reap them itself, not wait for disconnect_peer",
            sender.connections.len(),
        );
        assert!(
            wait_until(Duration::from_secs(5), || sender.shared.connection_count() == 0).await,
            "registry still holds {} connections after the peer vanished",
            sender.shared.connection_count(),
        );

        sender.shutdown().await;
    }

    /// `disconnect_peer` is a hard kill, so a connect still mid-handshake ends
    /// with it — it does not get to run out its timeouts first.
    ///
    /// The peer here never answers, so the connect is parked in its TCP connect
    /// for the full `connect_timeout`. That connect owns no cache entry to be
    /// found by address, which is why cancelling it takes the peer token rather
    /// than anything in `connections`.
    #[tokio::test(flavor = "multi_thread")]
    async fn disconnect_peer_kills_an_in_flight_connect() {
        // Long enough that finishing the connect and reacting to the cancel are
        // clearly distinguishable outcomes.
        let cfg = TcpConfig { connect_timeout: Duration::from_secs(30), ..TcpConfig::default() };
        let (sender, _rx) = make_sender_cfg(0, [0xB4; 12], 7060, &cfg);
        // RFC 5737 TEST-NET-1 — guaranteed unroutable, so the connect hangs.
        let unreachable: SocketAddr = "192.0.2.1:7400".parse().unwrap();

        blocking_send_discovery(&sender, unreachable, b"RTPS\x00\x00\x00\x00").await.expect("send");
        let disc_port = PortManager::get_discovery_traffic_unicast_port(0, 0);

        // The in-flight marker is held by the connect future itself, so it is
        // the one thing that says whether that future is still alive. It clears
        // only when the future is dropped — which is what is being asserted.
        assert!(
            wait_until(Duration::from_secs(5), || !sender.in_flight.is_empty()).await,
            "the connect never reached its handshake; nothing is in flight to kill",
        );

        sender.disconnect_peer(unreachable);

        assert!(
            wait_until(Duration::from_secs(2), || sender.in_flight.is_empty()).await,
            "the connect outlived disconnect_peer; it is running out its \
             {:?} connect timeout instead of being abandoned",
            cfg.connect_timeout,
        );
        assert!(
            sender.connections.get(&(unreachable, disc_port)).is_none(),
            "disconnect_peer must drop the connecting entry",
        );
        assert!(
            sender.peer_cancel.get(&unreachable).is_none(),
            "the peer token must be dropped, or the peer could never reconnect",
        );

        // A later send to the same peer must build a *fresh*, uncancelled token:
        // disconnect_peer ends the current work, it does not blacklist the peer.
        blocking_send_discovery(&sender, unreachable, b"RTPS\x11\x11\x11\x11")
            .await
            .expect("send after disconnect");
        assert!(
            !sender
                .connections
                .get(&(unreachable, disc_port))
                .expect("reconnect entry cached")
                .is_dead(),
            "a send after disconnect_peer must start a live connect, not inherit the kill",
        );

        sender.shutdown().await;
    }

    /// Every connection to a peer hangs off that peer's token — the control
    /// connection included, though it mints its own token deep inside the
    /// connect rather than being handed one.
    ///
    /// That parentage is what makes `disconnect_peer` a hard kill rather than a
    /// sweep of whatever the cache happens to hold. The control connect takes
    /// its token before its handshakes and registers afterwards with no await in
    /// between: a disconnect landing in that gap cannot interrupt it, and there
    /// is no control entry to find yet, so the token it already holds is the
    /// only way to reach it. Fired here on its own, without the cache sweep
    /// `disconnect_peer` also does, so only the parentage is under test.
    #[tokio::test(flavor = "multi_thread")]
    async fn every_connection_hangs_off_the_peer_token() {
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
            Duration::from_secs(5),
            Duration::from_secs(5),
            CancellationToken::new(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xB5; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00").await.expect("send");

        let timeout = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, timeout).await.expect("send did not deliver within 5s");

        let disc_port = PortManager::get_discovery_traffic_unicast_port(0, 0);
        let control = sender
            .connections
            .get(&(target, CONTROL_LOGICAL_PORT))
            .expect("control cached")
            .cancel
            .clone();
        let data =
            sender.connections.get(&(target, disc_port)).expect("data cached").cancel.clone();

        sender.peer_cancel.get(&target).expect("peer token").cancel();

        assert!(
            control.is_cancelled(),
            "the control connection must hang off the peer token — it is built \
             mid-connect, where the cache cannot reach it",
        );
        assert!(data.is_cancelled(), "the data connection must hang off the peer token");

        sender.shutdown().await;
        listener.shutdown().await;
    }

    /// `disconnect_peer` — the DDS-unmatch cleanup path — releases the sockets,
    /// not just the bookkeeping.
    ///
    /// The two maps emptying only proves entries were dropped; it says nothing
    /// about whether the connections are actually gone. So the peer is asked:
    /// its own connection count falling to zero means our tasks really did exit
    /// and close their halves, which is the part that returns fds to the OS.
    /// The backoff goes with them, or the peer could not be redialled promptly
    /// after a rematch.
    #[tokio::test(flavor = "multi_thread")]
    async fn disconnect_peer_closes_the_sockets_and_clears_both_maps() {
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
            Duration::from_secs(5),
            Duration::from_secs(5),
            CancellationToken::new(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xB6; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00").await.expect("send");

        let timeout = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, timeout).await.expect("send did not deliver within 5s");
        assert!(
            wait_until(Duration::from_secs(5), || listener.shared.connection_count() >= 2).await,
            "peer should have accepted control + data connections",
        );

        // Seed a backoff entry so its clearing is observable.
        sender.note_connect_failure(target);
        assert!(sender.backoff_remaining(target).is_some());

        sender.disconnect_peer(target);

        assert!(sender.connections.is_empty(), "outbound cache must be cleared synchronously");
        assert_eq!(sender.shared.connection_count(), 0, "registry must be cleared");
        assert!(sender.backoff_remaining(target).is_none(), "backoff must be cleared");
        assert!(
            wait_until(Duration::from_secs(5), || listener.shared.connection_count() == 0).await,
            "the peer still sees {} open connections; disconnect_peer dropped the \
             entries but the tasks never exited to close their sockets",
            listener.shared.connection_count(),
        );

        sender.shutdown().await;
        listener.shutdown().await;
    }

    /// A peer that resets the connection is detected through the read error and
    /// reclaimed — the abrupt counterpart to the graceful close in
    /// `peer_loss_reclaims_every_connection_without_disconnect_peer`.
    #[tokio::test(flavor = "multi_thread")]
    async fn peer_reset_reclaims_the_connection() {
        let (d_tx, _d_rx, u_tx, _u_rx) = make_channels();
        let shared = Arc::new(ConnectionRegistry::new(
            0,
            0,
            [0xB7; 12],
            TcpSocketTuning::default(),
            d_tx,
            u_tx,
        ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let dial = tokio::spawn(async move { TcpStream::connect(addr).await.unwrap() });
        let (under_test, _) = listener.accept().await.unwrap();
        let peer = dial.await.unwrap();

        // A data connection is reader-only in production; hold the write half so
        // the socket is not closed early.
        let (read_half, _write_half) = wrap_plain(under_test).into_split();
        let (writer_tx, _rx) = mpsc::channel::<Vec<u8>>(1);
        let cancel = CancellationToken::new();
        let disc_port = PortManager::get_discovery_traffic_unicast_port(0, 0);
        let conn_id = shared.register_outbound_data_connection(addr, disc_port, cancel.clone());
        spawn_reader_only(read_half, conn_id, Arc::clone(&shared), cancel.clone(), writer_tx);

        // SO_LINGER 0 turns the close into an RST rather than a FIN, so the
        // reader sees ECONNRESET instead of a clean EOF.
        socket2::SockRef::from(&peer).set_linger(Some(Duration::ZERO)).unwrap();
        drop(peer);

        assert!(
            wait_until(Duration::from_secs(5), || cancel.is_cancelled()).await,
            "a peer reset must be detected through the read error and cancel the connection",
        );
        assert!(
            wait_until(Duration::from_secs(5), || shared.connection_count() == 0).await,
            "the registry entry must be reclaimed after a reset",
        );
    }

    /// The reaper sends a TLS `close_notify` when a data connection is torn down.
    ///
    /// A data connection has no writer task, so the reaper owns its graceful
    /// close — and this is where that earns its keep over merely dropping the
    /// write half. tokio-rustls emits `close_notify` only from `poll_shutdown`
    /// and never on drop, so without the reaper's `graceful_close` a TLS peer
    /// would see the socket vanish mid-stream (an unclean shutdown) instead of a
    /// clean close. rustls surfaces the difference: a real `close_notify` reads
    /// as `Ok(0)`, a bare TCP FIN as an `UnexpectedEof` error.
    #[tokio::test(flavor = "multi_thread")]
    async fn reaper_sends_tls_close_notify_on_a_data_connection() {
        use crate::rtps::transport::tcp::tls::{accept_tls_async, connect_tls_async};
        use rcgen::{generate_simple_self_signed, CertifiedKey};
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        use rustls::{ClientConfig, ServerConfig};
        use tokio::io::AsyncReadExt;

        // Self-signed cert the client is configured to trust.
        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der = PrivateKeyDer::try_from(key_pair.serialize_der()).unwrap();
        let s_cfg = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert_der.clone()], key_der)
                .unwrap(),
        );
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der).unwrap();
        let c_cfg =
            Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());

        // TLS handshake: our side is the server, the peer is the client.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            accept_tls_async(tcp, s_cfg).await.unwrap()
        });
        let tcp = TcpStream::connect(addr).await.unwrap();
        let mut peer = connect_tls_async(tcp, c_cfg, "localhost").await.unwrap();
        let our_stream = accept.await.unwrap();

        // Build a reader-only data connection with a reaper owning the close,
        // exactly as `do_connect_data` does: a shared write state (now Ready) that
        // only the reaper holds.
        let (sender, _disc) = make_sender(0, [0xBB; 12], 7110);
        let write_state = init_connection_state();
        let read_half = flush_and_ready(our_stream, &write_state).await.unwrap();
        let cancel = sender.cancel.child_token();
        let (writer_tx, _wr) = mpsc::channel::<Vec<u8>>(1);
        spawn_reader_only(read_half, 0, Arc::clone(&sender.shared), cancel.clone(), writer_tx);
        sender.spawn_reaper((addr, 7400), cancel.clone(), Some(write_state));

        // Tear the connection down — only the reaper can close the write half.
        cancel.cancel();

        let mut buf = [0u8; 16];
        let read = tokio::time::timeout(Duration::from_secs(5), peer.read(&mut buf)).await;
        assert!(
            matches!(read, Ok(Ok(0))),
            "peer must receive a clean TLS close_notify (Ok(0)), got {:?}; without the \
             reaper's graceful_close a TLS peer sees an unclean shutdown",
            read.map(|r| r.map_err(|e| e.kind())),
        );

        sender.shutdown().await;
    }

    /// A connect that fails drops its entry and arms the reconnect backoff.
    ///
    /// Nothing is listening, so the connect is refused outright — the entry the
    /// send left behind must not survive as a connection that never connects.
    #[tokio::test(flavor = "multi_thread")]
    async fn failed_connect_evicts_the_entry_and_arms_backoff() {
        let (sender, _rx) = make_sender(0, [0xB8; 12], 7080);

        // Bind, read the port, drop: a port nothing is listening on, without
        // guessing at one that might be in use.
        let closed_port = {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().port()
        };
        let refused: SocketAddr = format!("127.0.0.1:{}", closed_port).parse().unwrap();

        let disc_port = PortManager::get_discovery_traffic_unicast_port(0, 0);
        blocking_send_discovery(&sender, refused, b"RTPS\x00\x00\x00\x00").await.expect("send");

        assert!(
            wait_until(Duration::from_secs(5), || sender
                .connections
                .get(&(refused, disc_port))
                .is_none())
            .await,
            "a failed connect must evict the entry it was building",
        );
        assert!(
            sender.backoff_remaining(refused).is_some(),
            "a failed connect must arm the backoff, or the next send retries immediately",
        );

        // A send inside the backoff window never reaches the wire, and the
        // caller has to be able to tell — a reliable writer settles its
        // heartbeat bookkeeping on this answer.
        let err = blocking_send_discovery(&sender, refused, b"RTPS\x11\x11\x11\x11")
            .await
            .expect_err("a send deferred by backoff must be reported to the caller");
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);

        sender.shutdown().await;
    }

    /// A torn-down connection is reaped from the cache, and the next send
    /// rebuilds it and delivers.
    ///
    /// The token is fired directly, which is exactly what a reader at EOF or a
    /// failed write does, and the entry is deliberately left in place: removing
    /// it is the reaper's job, and a send that raced the reaper must not write
    /// into the dead entry either.
    #[tokio::test(flavor = "multi_thread")]
    async fn torn_down_connection_is_reaped_then_reconnects() {
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
            Duration::from_secs(5),
            Duration::from_secs(5),
            CancellationToken::new(),
        )
        .expect("listener bind_and_spawn");
        let b_port = listener.port();

        let (sender, _) = make_sender(0, [0xB1; 12], 12345);
        let target: SocketAddr = format!("127.0.0.1:{}", b_port).parse().unwrap();
        blocking_send_discovery(&sender, target, b"RTPS\x00\x00\x00\x00").await.expect("send");

        let timeout = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, timeout).await.expect("first send did not deliver within 5s");

        let disc_port = PortManager::get_discovery_traffic_unicast_port(0, 0);
        let key = (target, disc_port);
        sender.connections.get(&key).expect("data connection cached").cancel.cancel();

        assert!(
            wait_until(Duration::from_secs(5), || sender.connections.get(&key).is_none()).await,
            "the reaper must drop the cache entry of a cancelled connection",
        );

        // Same peer, fresh connection: this only works if the dead entry is
        // really gone and a new connect is started in its place.
        blocking_send_discovery(&sender, target, b"RTPS\x11\x11\x11\x11")
            .await
            .expect("send after teardown");
        let timeout = Instant::now() + Duration::from_secs(5);
        wait_for_recv(&b_disc_rx, timeout).await.expect("reconnect did not deliver within 5s");
        assert!(sender.connections.get(&key).is_some(), "reconnect must repopulate the cache");

        sender.shutdown().await;
        listener.shutdown().await;
    }

    /// A failed write tears the connection down by itself.
    ///
    /// The write task is the first to know here, and for a peer that stops
    /// reading it may be the only one for a long while: the reader learns of the
    /// break only when its own read fails. So the write task fires the token,
    /// and the reaper drops the entry — no send has to come along and notice.
    #[tokio::test(flavor = "multi_thread")]
    async fn write_error_tears_down_the_connection() {
        let (sender, _rx) = make_sender(0, [0xB2; 12], 7050);

        // A peer that accepts, then vanishes: the first write may still land in
        // the socket buffer, but the RST it triggers fails the next one.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = listener.local_addr().unwrap();
        let accepted = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream);
        });
        let tcp = TcpStream::connect(peer_addr).await.unwrap();
        accepted.await.unwrap();

        let write_state = init_connection_state();
        let _read_half = flush_and_ready(wrap_plain(tcp), &write_state).await.unwrap();
        let cancel = CancellationToken::new();
        let handle = test_handle(
            &write_state,
            &Arc::new(ConnHealth::new(CONGESTION_MISS_THRESHOLD)),
            &cancel,
        );

        // Keep writing until one fails — the exact write that trips is up to the
        // kernel, and each has to reach the wire before the next is judged.
        let s = Arc::clone(&sender);
        let writer = tokio::task::spawn_blocking(move || {
            for _ in 0..50 {
                if cancel.is_cancelled() {
                    return true;
                }
                let _ = s.write_frame(peer_addr, 7400, &handle, b"RTPS\x00\x00\x00\x00");
                std::thread::sleep(Duration::from_millis(20));
            }
            cancel.is_cancelled()
        });

        assert!(
            writer.await.unwrap(),
            "a failed write must cancel the connection, so it is torn down \
             without waiting for the reader to notice",
        );
        assert!(sender.stats.write_error.count() > 0, "write errors must be counted");

        sender.shutdown().await;
    }

    /// A reader parked on DDS backpressure still observes cancellation.
    ///
    /// User data is routed with an awaiting send, so a slow consumer parks the
    /// reader for as long as it stays slow. If that await did not race
    /// cancellation, tearing the connection down would be hostage to the DDS
    /// consumer draining first — the transport could not reclaim its own
    /// connection on its own schedule.
    #[tokio::test(flavor = "multi_thread")]
    async fn cancel_frees_a_reader_parked_on_dds_backpressure() {
        // Neither receiver is ever drained, so the user channel fills and stays
        // full — the reader parks in dispatch and cannot come back on its own.
        let (d_tx, _d_rx, u_tx, _u_rx) = make_channels();
        let shared = Arc::new(ConnectionRegistry::new(
            0,
            0,
            [0xB3; 12],
            TcpSocketTuning::default(),
            d_tx,
            u_tx,
        ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let dial = tokio::spawn(async move { TcpStream::connect(addr).await.unwrap() });
        let (under_test, _) = listener.accept().await.unwrap();
        let peer = dial.await.unwrap();

        // A data connection is reader-only in production; hold the write half so
        // the socket is not closed early.
        let (read_half, _write_half) = wrap_plain(under_test).into_split();
        let (writer_tx, _rx) = mpsc::channel::<Vec<u8>>(1);
        let cancel = CancellationToken::new();
        let user_port = PortManager::get_user_traffic_unicast_port(0, 0);
        let conn_id = shared.register_outbound_data_connection(addr, user_port, cancel.clone());
        spawn_reader_only(read_half, conn_id, Arc::clone(&shared), cancel.clone(), writer_tx);

        // Flood well past the channel's depth so the reader is parked, not just
        // busy. The peer write is spawned: it stalls once the reader stops
        // draining the socket, which is the state under test.
        let flood = tokio::spawn(async move {
            let mut peer = wrap_plain(peer);
            for _ in 0..512 {
                if write_framed_message(&mut peer, b"RTPS\x02\x04\x00\x00frame").await.is_err() {
                    break;
                }
            }
        });
        assert!(
            wait_until(Duration::from_secs(5), || _u_rx.is_full()).await,
            "user channel never filled; the reader was never parked on backpressure",
        );

        cancel.cancel();
        assert!(
            wait_until(Duration::from_secs(5), || shared.connection_count() == 0).await,
            "a reader parked on a full DDS channel never saw the cancel; \
             transport teardown must not wait on the DDS consumer",
        );

        flood.abort();
    }
}
