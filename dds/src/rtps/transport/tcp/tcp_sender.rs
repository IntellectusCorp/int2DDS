//! Outbound TCP connections and persistent writers.
//!
//! A peer has at most two lazy outbound slots, keyed by the explicit frame
//! kind. Each established slot owns one persistent writer task and one generic
//! reader task. There is no application-level connection handshake.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use log::{debug, warn};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::tcp::connection_registry::{
    apply_socket_tuning, ConnectionDirection, ConnectionRegistry,
};
use crate::rtps::transport::tcp::connection_tasks::spawn_reader;
use crate::rtps::transport::tcp::framing::{
    validate_payload_size, write_framed_message, TcpFrameKind,
};
use crate::rtps::transport::tcp::stream::{wrap_plain, AsyncConnStream, AsyncConnWriteHalf};
use crate::rtps::transport::tcp::tls::{connect_tls_async, TlsConfig};
use crate::rtps::transport::tcp::write_state::{
    init_connection_state, install_writer, mark_failed, push_connecting, PendingFrame,
    SharedWriteState, WriteState, WriterCommand,
};
use crate::rtps::transport::TcpConfig;

struct ConnHealth {
    misses: AtomicU32,
    threshold: u32,
}

impl ConnHealth {
    fn new(threshold: u32) -> Self {
        Self { misses: AtomicU32::new(0), threshold }
    }

    fn is_congested(&self) -> bool {
        self.misses.load(Ordering::Relaxed) >= self.threshold
    }

    fn on_success(&self) {
        let _ = self.misses.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_sub(1))
        });
    }

    fn on_miss(&self) {
        let _ = self.misses.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_add(1))
        });
    }
}

#[derive(Default)]
struct DropCounter {
    count: AtomicU64,
    last_warn: StdMutex<Option<Instant>>,
}

impl DropCounter {
    fn record(&self, cause: &str, addr: SocketAddr, kind: TcpFrameKind) {
        let total = self.count.fetch_add(1, Ordering::Relaxed) + 1;
        let now = Instant::now();
        let mut last = match self.last_warn.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if last.map_or(true, |previous| now.duration_since(previous) >= Duration::from_secs(1)) {
            *last = Some(now);
            warn!("TcpSender: dropped {:?} frame to {}: {} ({} total)", kind, addr, cause, total);
        }
    }

    #[cfg(test)]
    fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

#[derive(Default)]
struct SendStats {
    send_deadline: DropCounter,
    connect_buffer_full: DropCounter,
    backoff: DropCounter,
    write_error: DropCounter,
}

struct OutboundEntry {
    write_state: SharedWriteState,
    cancel: CancellationToken,
    health: Arc<ConnHealth>,
}

impl OutboundEntry {
    fn is_dead(&self) -> bool {
        self.cancel.is_cancelled()
    }

    fn handle(&self) -> SendHandle {
        SendHandle {
            write_state: Arc::clone(&self.write_state),
            cancel: self.cancel.clone(),
            health: Arc::clone(&self.health),
        }
    }
}

#[derive(Clone)]
struct SendHandle {
    write_state: SharedWriteState,
    cancel: CancellationToken,
    health: Arc<ConnHealth>,
}

type ConnectionKey = (SocketAddr, TcpFrameKind);

pub(crate) struct TcpSender {
    working_ips: Vec<String>,
    listener_port: u16,
    connect_timeout: Duration,
    tls_handshake_timeout: Duration,

    send_deadline: Option<Duration>,
    send_probe_deadline: Duration,
    congestion_miss_threshold: u32,
    congestion_isolation: bool,

    stats: Arc<SendStats>,
    tls_config: Option<Arc<TlsConfig>>,
    shared: Arc<ConnectionRegistry>,
    connections: Arc<DashMap<ConnectionKey, OutboundEntry>>,
    peer_cancel: Arc<DashMap<SocketAddr, CancellationToken>>,
    runtime_handle: tokio::runtime::Handle,
    cancel: CancellationToken,
}

impl TcpSender {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        _domain_id: u32,
        _participant_id: u32,
        working_ips: Vec<String>,
        listener_port: u16,
        _local_guid_prefix: GuidPrefix,
        tls_config: Option<Arc<TlsConfig>>,
        shared: Arc<ConnectionRegistry>,
        tcp_config: &TcpConfig,
        cancel: CancellationToken,
    ) -> Arc<Self> {
        let send_probe_deadline =
            tcp_config.send_deadline.map_or(Duration::from_millis(1), |deadline| {
                (deadline / 20).max(Duration::from_millis(1))
            });
        let congestion_isolation = matches!(tcp_config.send_deadline, Some(d) if !d.is_zero());

        Arc::new(Self {
            working_ips,
            listener_port,
            connect_timeout: tcp_config.connect_timeout,
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
            runtime_handle: tokio::runtime::Handle::current(),
            cancel,
        })
    }

    pub(crate) fn send_to_discovery(
        self: &Arc<Self>,
        addr: &SocketAddr,
        data: &[u8],
    ) -> io::Result<()> {
        self.send_to(*addr, TcpFrameKind::Discovery, data)
    }

    pub(crate) fn send_to(
        self: &Arc<Self>,
        addr: SocketAddr,
        kind: TcpFrameKind,
        data: &[u8],
    ) -> io::Result<()> {
        validate_payload_size(data.len())?;

        if self.is_self_connection(&addr) {
            self.shared.deliver_to_self(addr, kind, data)?;
            return Ok(());
        }

        let key = (addr, kind);
        let live =
            self.connections.get(&key).filter(|entry| !entry.is_dead()).map(|entry| entry.handle());
        let handle = match live {
            Some(handle) => handle,
            None => self.ensure_connecting_entry(addr, kind)?,
        };
        self.admit_frame(addr, kind, &handle, data)
    }

    fn ensure_connecting_entry(
        self: &Arc<Self>,
        addr: SocketAddr,
        kind: TcpFrameKind,
    ) -> io::Result<SendHandle> {
        let key = (addr, kind);
        if matches!(self.connections.get(&key), Some(entry) if entry.is_dead()) {
            self.evict_connection(key);
        }

        if let Some(remaining) = self.shared.backoff_remaining(addr) {
            self.stats.backoff.record("reconnect backoff", addr, kind);
            return Err(transport_io_error(
                TransportErrorCode::TcpReconnectBackoff,
                format!("peer {addr} is in reconnect backoff for another {remaining:?}"),
            ));
        }

        match self.connections.entry(key) {
            Entry::Occupied(entry) => Ok(entry.get().handle()),
            Entry::Vacant(entry) => {
                let write_state = init_connection_state();
                let conn_cancel = self.peer_token(addr).child_token();
                let health = Arc::new(ConnHealth::new(self.congestion_miss_threshold));
                let handle = SendHandle {
                    write_state: Arc::clone(&write_state),
                    cancel: conn_cancel.clone(),
                    health: Arc::clone(&health),
                };
                entry.insert(OutboundEntry {
                    write_state: Arc::clone(&write_state),
                    cancel: conn_cancel.clone(),
                    health,
                });

                self.spawn_reaper(key, conn_cancel.clone());
                self.runtime_handle.spawn(background_connect(
                    Arc::clone(self),
                    key,
                    write_state,
                    Arc::clone(&handle.health),
                    conn_cancel,
                ));
                Ok(handle)
            }
        }
    }

    fn admission_deadline(&self, health: &ConnHealth) -> Option<Duration> {
        if self.congestion_isolation && health.is_congested() {
            Some(self.send_probe_deadline)
        } else {
            self.send_deadline
        }
    }

    fn state_lock(
        &self,
        state: SharedWriteState,
        deadline: Option<Duration>,
    ) -> Option<OwnedMutexGuard<WriteState>> {
        self.runtime_handle.block_on(async move {
            match deadline {
                Some(duration) if duration.is_zero() => state.try_lock_owned().ok(),
                Some(duration) => tokio::time::timeout(duration, state.lock_owned()).await.ok(),
                None => Some(state.lock_owned().await),
            }
        })
    }

    fn deadline_miss(
        &self,
        addr: SocketAddr,
        kind: TcpFrameKind,
        health: &ConnHealth,
    ) -> io::Error {
        if self.congestion_isolation {
            health.on_miss();
        }
        self.stats.send_deadline.record("send deadline", addr, kind);
        transport_io_error(
            TransportErrorCode::TcpSendDeadlineExpired,
            format!("previous {:?} frame to {addr} is still pending", kind),
        )
    }

    fn admit_frame(
        &self,
        addr: SocketAddr,
        kind: TcpFrameKind,
        handle: &SendHandle,
        data: &[u8],
    ) -> io::Result<()> {
        if handle.cancel.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::NotConnected, "TCP connection is closed"));
        }

        let deadline = self.admission_deadline(&handle.health);
        let started = Instant::now();
        let mut state = self
            .state_lock(Arc::clone(&handle.write_state), deadline)
            .ok_or_else(|| self.deadline_miss(addr, kind, &handle.health))?;

        match &mut *state {
            WriteState::Connecting { backlog, backlog_bytes } => {
                let result = push_connecting(
                    backlog,
                    backlog_bytes,
                    PendingFrame { kind, payload: self.shared.buffer_pool().copy_from_slice(data) },
                );
                if result.is_err() {
                    self.stats.connect_buffer_full.record("connect buffer full", addr, kind);
                }
                result
            }
            WriteState::Failed => {
                Err(io::Error::new(io::ErrorKind::NotConnected, "TCP connection failed"))
            }
            WriteState::Ready(ready) => {
                let ready = ready.clone();
                drop(state);

                let remaining = deadline.map(|total| total.saturating_sub(started.elapsed()));
                let admission = Arc::clone(&ready.admission);
                let cancel = handle.cancel.clone();
                let permit = self.runtime_handle.block_on(async move {
                    match remaining {
                        Some(duration) if duration.is_zero() => admission.try_acquire_owned().ok(),
                        Some(duration) => {
                            tokio::select! {
                                permit = tokio::time::timeout(duration, admission.acquire_owned()) => {
                                    permit.ok().and_then(Result::ok)
                                }
                                _ = cancel.cancelled() => None,
                            }
                        }
                        None => {
                            tokio::select! {
                                permit = admission.acquire_owned() => permit.ok(),
                                _ = cancel.cancelled() => None,
                            }
                        }
                    }
                });
                let permit = match permit {
                    Some(permit) => permit,
                    None if handle.cancel.is_cancelled() => {
                        return Err(io::Error::new(
                            io::ErrorKind::NotConnected,
                            "TCP connection closed during admission",
                        ));
                    }
                    None => return Err(self.deadline_miss(addr, kind, &handle.health)),
                };
                if handle.cancel.is_cancelled() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotConnected,
                        "TCP connection closed during admission",
                    ));
                }

                let command = WriterCommand {
                    frame: PendingFrame {
                        kind,
                        payload: self.shared.buffer_pool().copy_from_slice(data),
                    },
                    _permit: permit,
                };
                ready.tx.try_send(command).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        format!("persistent TCP writer unavailable: {error}"),
                    )
                })
            }
        }
    }

    fn peer_token(&self, addr: SocketAddr) -> CancellationToken {
        self.peer_cancel.entry(addr).or_insert_with(|| self.cancel.child_token()).clone()
    }

    fn spawn_reaper(self: &Arc<Self>, key: ConnectionKey, cancel: CancellationToken) {
        let weak = Arc::downgrade(self);
        self.runtime_handle.spawn(async move {
            cancel.cancelled().await;
            if let Some(sender) = weak.upgrade() {
                sender.connections.remove_if(&key, |_, entry| entry.is_dead());
            }
        });
    }

    fn evict_connection(&self, key: ConnectionKey) {
        if let Some((_, entry)) = self.connections.remove(&key) {
            entry.cancel.cancel();
        }
    }

    pub(crate) fn disconnect_peer(&self, addr: SocketAddr) {
        if let Some((_, token)) = self.peer_cancel.remove(&addr) {
            token.cancel();
        }
        self.connections.retain(|(peer, _), entry| {
            let keep = *peer != addr;
            if !keep {
                entry.cancel.cancel();
            }
            keep
        });
        self.shared.clear_backoff(addr);
        self.shared.remove_peer_by_addr(addr);
    }

    pub(crate) async fn shutdown(&self) {
        self.cancel.cancel();
    }

    pub(crate) fn is_self_connection(&self, addr: &SocketAddr) -> bool {
        if addr.port() != self.listener_port {
            return false;
        }
        match addr.ip() {
            IpAddr::V4(ip) => {
                ip.is_loopback()
                    || self.working_ips.iter().any(|candidate| *candidate == ip.to_string())
            }
            IpAddr::V6(_) => false,
        }
    }

    #[cfg(test)]
    pub(crate) fn connection_count(&self) -> usize {
        self.connections.len()
    }

    #[cfg(test)]
    pub(crate) fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }
}

impl Drop for TcpSender {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

async fn background_connect(
    sender: Arc<TcpSender>,
    key: ConnectionKey,
    write_state: SharedWriteState,
    health: Arc<ConnHealth>,
    conn_cancel: CancellationToken,
) {
    let result = tokio::select! {
        result = establish_connection(
            &sender,
            key,
            Arc::clone(&write_state),
            health,
            conn_cancel.clone(),
        ) => result,
        _ = conn_cancel.cancelled() => {
            mark_failed(&write_state).await;
            return;
        }
    };

    match result {
        Ok(()) => sender.shared.clear_backoff(key.0),
        Err(error) => {
            mark_failed(&write_state).await;
            if conn_cancel.is_cancelled() {
                return;
            }
            warn!("TcpSender: connect {:?} to {} failed: {}", key.1, key.0, error);
            sender.evict_connection(key);
            sender.shared.note_connect_failure(key.0, &error);
        }
    }
}

async fn establish_connection(
    sender: &Arc<TcpSender>,
    key: ConnectionKey,
    write_state: SharedWriteState,
    health: Arc<ConnHealth>,
    conn_cancel: CancellationToken,
) -> io::Result<()> {
    let stream = create_stream(sender, key.0).await?;
    let (read_half, write_half) = stream.into_split();
    let (backlog, writer_rx) = install_writer(&write_state).await?;

    let conn_id = sender.shared.register_connection(
        key.0,
        ConnectionDirection::Outbound,
        conn_cancel.clone(),
    );
    spawn_reader(read_half, conn_id, Arc::clone(&sender.shared), conn_cancel.clone(), None);
    tokio::spawn(writer_task(
        write_half,
        backlog,
        writer_rx,
        Arc::clone(&write_state),
        health,
        Arc::clone(&sender.stats),
        key,
        sender.send_deadline,
        conn_cancel,
    ));
    debug!("TcpSender: established {:?} connection to {}", key.1, key.0);
    Ok(())
}

async fn create_stream(sender: &Arc<TcpSender>, addr: SocketAddr) -> io::Result<AsyncConnStream> {
    let tcp = tokio::time::timeout(sender.connect_timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| {
            transport_io_error(
                TransportErrorCode::TcpConnectionTimeout,
                format!("TCP connect timeout to {addr}"),
            )
        })??;
    apply_socket_tuning(&tcp, &sender.shared.tuning);

    if let Some(config) = &sender.tls_config {
        let client_config = config.build_client_config().map_err(|error| {
            transport_io_error(TransportErrorCode::TlsConfigError, error.to_string())
        })?;
        tokio::time::timeout(
            sender.tls_handshake_timeout,
            connect_tls_async(tcp, client_config, config.server_name()),
        )
        .await
        .map_err(|_| {
            transport_io_error(TransportErrorCode::TlsHandshakeFailed, "TLS handshake timeout")
        })?
    } else {
        Ok(wrap_plain(tcp))
    }
}

#[allow(clippy::too_many_arguments)]
async fn writer_task(
    mut write_half: AsyncConnWriteHalf,
    mut backlog: std::collections::VecDeque<PendingFrame>,
    mut receiver: mpsc::Receiver<WriterCommand>,
    write_state: SharedWriteState,
    health: Arc<ConnHealth>,
    stats: Arc<SendStats>,
    key: ConnectionKey,
    send_deadline: Option<Duration>,
    cancel: CancellationToken,
) {
    let mut failed = false;

    while let Some(frame) = backlog.pop_front() {
        if cancel.is_cancelled() {
            break;
        }
        if let Err(error) = write_framed_message(&mut write_half, frame.kind, &frame.payload).await
        {
            debug!("TcpSender: connect-backlog write to {} failed: {}", key.0, error);
            stats.write_error.record("write error", key.0, key.1);
            failed = true;
            break;
        }
    }

    while !failed {
        let command = tokio::select! {
            command = receiver.recv() => match command {
                Some(command) => command,
                None => break,
            },
            _ = cancel.cancelled() => break,
        };

        let started = Instant::now();
        let result =
            write_framed_message(&mut write_half, command.frame.kind, &command.frame.payload).await;
        // command (and therefore its admission permit) drops after this block.
        match result {
            Ok(()) => {
                if send_deadline.is_some_and(|deadline| started.elapsed() <= deadline) {
                    health.on_success();
                }
            }
            Err(error) => {
                debug!("TcpSender: write {:?} to {} failed: {}", key.1, key.0, error);
                stats.write_error.record("write error", key.0, key.1);
                failed = true;
            }
        }
    }

    let _ = write_half.shutdown().await;
    mark_failed(&write_state).await;
    cancel.cancel();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_health_uses_threshold_and_hysteresis() {
        let health = ConnHealth::new(2);
        health.on_miss();
        assert!(!health.is_congested());
        health.on_miss();
        assert!(health.is_congested());
        health.on_success();
        assert!(!health.is_congested());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cache_has_at_most_two_roles_per_peer() {
        let (discovery_tx, _discovery_rx) = flume::bounded(1);
        let (user_tx, _user_rx) = flume::bounded(1);
        let registry = Arc::new(ConnectionRegistry::new(
            0,
            0,
            [0; 12],
            Default::default(),
            discovery_tx,
            user_tx,
        ));
        let config =
            TcpConfig { connect_timeout: Duration::from_millis(50), ..TcpConfig::default() };
        let sender = TcpSender::new(
            0,
            0,
            vec!["192.0.2.1".into()],
            7400,
            [0; 12],
            None,
            registry,
            &config,
            CancellationToken::new(),
        );
        let peer: SocketAddr = "192.0.2.2:7400".parse().unwrap();
        let _ = sender.ensure_connecting_entry(peer, TcpFrameKind::Discovery);
        let _ = sender.ensure_connecting_entry(peer, TcpFrameKind::UserData);
        assert_eq!(sender.connection_count(), 2);
        sender.cancel.cancel();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn send_deadline_waits_on_writer_completion_permit() {
        let (discovery_tx, _discovery_rx) = flume::bounded(1);
        let (user_tx, _user_rx) = flume::bounded(1);
        let registry = Arc::new(ConnectionRegistry::new(
            0,
            0,
            [0; 12],
            Default::default(),
            discovery_tx,
            user_tx,
        ));
        let config = TcpConfig {
            send_deadline: Some(Duration::from_millis(30)),
            congestion_miss_threshold: 1,
            ..TcpConfig::default()
        };
        let sender = TcpSender::new(
            0,
            0,
            vec!["192.0.2.1".into()],
            7400,
            [0; 12],
            None,
            registry,
            &config,
            CancellationToken::new(),
        );

        let state = init_connection_state();
        let (_backlog, mut receiver) = install_writer(&state).await.unwrap();
        let handle = SendHandle {
            write_state: state,
            cancel: CancellationToken::new(),
            health: Arc::new(ConnHealth::new(1)),
        };
        let peer: SocketAddr = "192.0.2.2:7400".parse().unwrap();

        let first_sender = Arc::clone(&sender);
        let first_handle = handle.clone();
        tokio::task::spawn_blocking(move || {
            first_sender
                .admit_frame(peer, TcpFrameKind::UserData, &first_handle, b"first")
                .unwrap();
        })
        .await
        .unwrap();
        let held = receiver.try_recv().expect("writer command");

        let second_sender = Arc::clone(&sender);
        let second_handle = handle.clone();
        let error = tokio::task::spawn_blocking(move || {
            second_sender
                .admit_frame(peer, TcpFrameKind::UserData, &second_handle, b"second")
                .unwrap_err()
        })
        .await
        .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(handle.health.is_congested());
        assert_eq!(sender.stats.send_deadline.count(), 1);

        drop(held); // models writer completion
        let third_sender = Arc::clone(&sender);
        let third_handle = handle.clone();
        tokio::task::spawn_blocking(move || {
            third_sender
                .admit_frame(peer, TcpFrameKind::UserData, &third_handle, b"third")
                .unwrap();
        })
        .await
        .unwrap();
        drop(receiver.try_recv().expect("writer command after permit release"));
        sender.cancel.cancel();
    }
}
