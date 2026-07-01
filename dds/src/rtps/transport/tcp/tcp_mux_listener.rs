//! Inbound side of the TCP mux transport.
//!
//! `TcpMuxListener` owns the listener-side accept loop and exposes the shared
//! `MuxState` via `shared()`.
//!
//! The accept loop spawns a short-lived `handshake_and_register_task` per
//! connection so a slow TLS handshake does not stall new accepts; each
//! connection then gets its own conn_actor pair via `spawn_conn_actor`.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use log::{debug, error, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::rtps::transport::tcp::conn_actor::{inbox_capacity, spawn_conn_actor};
use crate::rtps::transport::tcp::stream::wrap_plain;
use crate::rtps::transport::tcp::tls::{accept_tls_async, TlsConfig};
use crate::rtps::{
    common::guid::GuidPrefix,
    transport::{
        plugin::IncomingMessage,
        tcp::mux_state::{apply_keepalive, apply_unacked_timeout, MuxState, TcpSocketTuning},
    },
};

/// TCP multiplexed listener — owns the listener-side tasks and the shared
/// `MuxState`.
///
/// Dropping or calling `shutdown()` cancels all tasks; cancellation
/// propagates into every conn_actor pair via child tokens.
pub(crate) struct TcpMuxListener {
    port: u16,
    pub(crate) shared: Arc<MuxState>,
    cancel: CancellationToken,
    task_handles: Vec<JoinHandle<()>>,
}

impl TcpMuxListener {
    /// Bind the listen socket and spawn all listener-side tasks on the
    /// current tokio runtime: accept loop.
    ///
    /// Must be called from within a tokio runtime context — the spawn calls
    /// require `Handle::current()` to be valid. From sync code, wrap the call
    /// in `runtime.block_on(async { ... })` or use `runtime.handle().enter()`.
    ///
    /// `port = 0` requests an OS-assigned ephemeral port; the actual port is
    /// captured into `port()` for advertising back to peers.
    pub(crate) fn bind_and_spawn(
        port: u16,
        domain_id: u32,
        participant_id: u32,
        local_guid_prefix: GuidPrefix,
        discovery_tx: crossbeam_channel::Sender<IncomingMessage>,
        user_data_tx: crossbeam_channel::Sender<IncomingMessage>,
        tls_config: Option<Arc<TlsConfig>>,
        tuning: TcpSocketTuning,
    ) -> io::Result<Self> {
        let std_listener = bind_listener(port)?;
        let actual_port = std_listener.local_addr()?.port();

        let shared = Arc::new(MuxState::new(
            domain_id,
            participant_id,
            local_guid_prefix,
            tuning,
            discovery_tx,
            user_data_tx,
        ));
        let cancel = CancellationToken::new();

        let mut handles = Vec::with_capacity(1);
        handles.push(tokio::spawn(accept_loop_task(
            std_listener,
            shared.clone(),
            tls_config,
            cancel.clone(),
        )));

        info!("TcpMuxListener: Listening on port {} (domain={})", actual_port, domain_id);
        Ok(Self { port: actual_port, shared, cancel, task_handles: handles })
    }

    /// Bound port. Will differ from the `port` argument of `bind_and_spawn`
    /// when `0` was passed (OS-assigned ephemeral).
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Shared mux state — connection map, peer groupings, dispatch logic.
    /// The sender uses this to look up writer channels for outbound frames;
    /// external consumers use it for metrics.
    pub(crate) fn shared(&self) -> &Arc<MuxState> {
        &self.shared
    }

    /// Graceful shutdown — cancels all tasks then awaits their completion.
    /// Consumes self so callers cannot accidentally use a half-shut listener.
    pub(crate) async fn shutdown(mut self) {
        self.cancel.cancel();
        for h in std::mem::take(&mut self.task_handles) {
            let _ = h.await;
        }
    }
}

impl Drop for TcpMuxListener {
    fn drop(&mut self) {
        // Best-effort cancellation — Drop can't await the task handles.
        // Callers that want a clean teardown should use `shutdown().await`.
        self.cancel.cancel();
    }
}

/// Sync bind. Returns a `std::net::TcpListener` configured for async use
/// (`set_nonblocking(true)`). Conversion to `tokio::net::TcpListener` is
/// deferred to the spawned accept task so this function can be called
/// outside a runtime context (e.g. straight from the plugin constructor).
fn bind_listener(port: u16) -> io::Result<std::net::TcpListener> {
    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)?;
    // SO_REUSEADDR semantics differ by OS. On Unix it only relaxes rebinding a
    // port left in TIME_WAIT — two live listeners on the same port still
    // conflict — so we keep it for clean restarts. On Windows it would instead
    // let a second listener share/hijack the same port, silently defeating the
    // per-participant bind-collision detection; leaving it off there preserves
    // the EADDRINUSE failure we rely on.
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(128)?;

    Ok(std::net::TcpListener::from(socket))
}

/// Long-running accept task.
///
/// Converts the sync listener to async then loops on `tokio::select!` between
/// `listener.accept()` and `cancel.cancelled()`. Each accepted connection is
/// handed off to a spawned `handshake_and_register_task` so a slow TLS
/// handshake does not stall new accepts.
async fn accept_loop_task(
    std_listener: std::net::TcpListener,
    shared: Arc<MuxState>,
    tls_config: Option<Arc<TlsConfig>>,
    cancel: CancellationToken,
) {
    let listener = match TcpListener::from_std(std_listener) {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to convert listener to async: {:?}", e);
            return;
        }
    };

    loop {
        tokio::select! {
            res = listener.accept() => {
                match res {
                    Ok((tcp, addr)) => {
                        debug!("Accepted from {:?}", addr);

                        // Optional socket buffer overrides (per-participant config).
                        if let Some(sz) = shared.tuning.so_rcvbuf {
                            let _ = socket2::SockRef::from(&tcp).set_recv_buffer_size(sz);
                        }
                        if let Some(sz) = shared.tuning.so_sndbuf {
                            let _ = socket2::SockRef::from(&tcp).set_send_buffer_size(sz);
                        }
                        apply_unacked_timeout(&tcp, shared.tuning.unacked_timeout);
                        apply_keepalive(&tcp, shared.tuning.keepalive);

                        // Spawn off — do NOT await handshake in the accept loop.
                        let shared = shared.clone();
                        let tls = tls_config.clone();
                        let parent_cancel = cancel.clone();
                        tokio::spawn(handshake_and_register_task(
                            tcp, addr, shared, tls, parent_cancel,
                        ));
                    }
                    Err(e) => {
                        warn!("accept error: {:?}", e);
                    }
                }
            }
            _ = cancel.cancelled() => {
                debug!("accept loop cancelled");
                break;
            }
        }
    }
}

/// Short-lived per-accept task: completes the optional TLS handshake,
/// registers the connection in `MuxState`, then spawns the conn_actor pair.
///
/// Registration happens **before** spawning the conn_actor — this guarantees
/// the reader task finds its `ConnectionEntry` in `MuxState.connections` when
/// the first inbound frame arrives. Reversing the order opens a race window
/// where the first frame's dispatch lookup returns `None` and the connection
/// gets stuck in `AwaitingFirstMessage` forever.
async fn handshake_and_register_task(
    tcp: TcpStream,
    addr: SocketAddr,
    shared: Arc<MuxState>,
    tls_config: Option<Arc<TlsConfig>>,
    parent_cancel: CancellationToken,
) {
    // Optional TLS — wrap or pass through plain.
    let stream = match &tls_config {
        Some(cfg) => match cfg.build_server_config() {
            Ok(server_cfg) => match accept_tls_async(tcp, server_cfg).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("TLS handshake failed from {:?}: {:?}", addr, e);
                    return;
                }
            },
            Err(e) => {
                warn!("TLS server config error: {:?}", e);
                return;
            }
        },
        None => wrap_plain(tcp),
    };

    let _ = stream.set_nodelay(shared.tuning.nodelay);

    // Channel created here, NOT inside spawn_conn_actor — so we can register
    // the entry (with tx) before the reader task starts polling.
    let (tx, rx) = mpsc::channel::<Vec<u8>>(inbox_capacity());
    let conn_cancel = parent_cancel.child_token();

    let conn_id = shared.register_inbound_connection(addr, tx.clone(), conn_cancel.clone());

    // Inbound connections only receive from the wire and write protocol
    // acks via the writer_tx inbox. The user-data send path never targets
    // an inbound connection, so the returned `SharedWriteHalf` is dropped
    // here — only the writer_task uses it.
    let _ = spawn_conn_actor(stream, conn_id, shared.clone(), conn_cancel, tx, rx);
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::tcp::framing::write_framed_message;
    use crate::rtps::transport::tcp::mux_state::ConnectionState;
    use crate::rtps::transport::tcp::protocol::{
        encode_locator, ControlMsg, ERR_CODE_MISSING_LOCATOR, MSG_ERROR, MSG_PEER_HELLO,
        MSG_PEER_HELLO_ACK,
    };
    use crossbeam_channel::bounded;
    use socket2::{Domain, SockAddr, Socket, Type};
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::oneshot;

    /// Helper: build the four channels needed by `bind_and_spawn`, returning
    /// the receivers so tests can assert on routed messages if needed.
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

    /// Helper: standard listener with no TLS.
    fn make_listener() -> TcpMuxListener {
        let (d_tx, _d_rx, u_tx, _u_rx) = make_channels();
        TcpMuxListener::bind_and_spawn(
            0,
            0,
            0,
            [0u8; 12],
            d_tx,
            u_tx,
            None,
            TcpSocketTuning::default(),
        )
        .expect("bind_and_spawn")
    }

    /// Spin-wait (cooperatively yielding) until `pred` is true or deadline expires.
    async fn wait_until(deadline: Instant, mut pred: impl FnMut() -> bool) -> bool {
        while Instant::now() < deadline {
            if pred() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    // ── bind_listener ────────────────────────────────────────────────────────

    /// `bind_listener(0)` should bind successfully and the OS should assign a
    /// non-zero ephemeral port.
    #[test]
    fn bind_listener_assigns_ephemeral_port() {
        let listener = bind_listener(0).expect("bind");
        let port = listener.local_addr().unwrap().port();
        assert!(port != 0, "OS should assign a port");
    }

    // ── TcpMuxListener lifecycle ─────────────────────────────────────────────

    /// `bind_and_spawn(0, ...)` returns a listener with a non-zero port and
    /// zero active connections.
    #[tokio::test(flavor = "multi_thread")]
    async fn bind_and_spawn_reports_port_and_empty_state() {
        let listener = make_listener();
        assert!(listener.port() != 0);
        assert_eq!(listener.shared().connection_count(), 0);
        listener.shutdown().await;
    }

    /// `shutdown()` cancels all tasks and awaits their JoinHandles without hanging.
    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_completes_without_hanging() {
        let listener = make_listener();

        // Cap the shutdown wait so a deadlock would fail the test loudly.
        tokio::time::timeout(Duration::from_secs(2), listener.shutdown())
            .await
            .expect("shutdown did not complete within 2s");
    }

    /// Dropping the listener (without explicit shutdown) cancels the cancel
    /// token so any clones observe cancellation.
    #[tokio::test(flavor = "multi_thread")]
    async fn drop_cancels_tasks() {
        let listener = make_listener();
        let cancel_clone = listener.cancel.clone();
        drop(listener);

        // The token must be cancelled even though we did not await shutdown.
        // (Underlying task handles are leaked here — that is acceptable for
        // best-effort Drop; tests using `shutdown()` get clean teardown.)
        assert!(cancel_clone.is_cancelled(), "Drop should fire cancel");
    }

    // ── accept + register ────────────────────────────────────────────────────

    /// Connecting to the bound port causes `accept_loop_task` to accept and
    /// `handshake_and_register_task` to register the entry in `MuxState`.
    #[tokio::test(flavor = "multi_thread")]
    async fn accept_registers_inbound_connection() {
        let listener = make_listener();
        let port = listener.port();

        // Client connects and holds the connection open long enough for the
        // accept + register path to run.
        let client = tokio::spawn(async move {
            let _stream = TcpStream::connect(("127.0.0.1", port)).await.expect("client connect");
            tokio::time::sleep(Duration::from_millis(500)).await;
        });

        let deadline = Instant::now() + Duration::from_secs(2);
        let ok = wait_until(deadline, || listener.shared().connection_count() == 1).await;
        assert!(ok, "connection was not registered within deadline");

        listener.shutdown().await;
        let _ = client.await;
    }

    // ── handshake state machine ──────────────────────────────────────────────

    /// Sending PEER_HELLO advances the connection state to `Control` and
    /// the peer receives a PEER_HELLO_ACK reply.
    #[tokio::test(flavor = "multi_thread")]
    async fn handshake_first_message_advances_state() {
        let listener = make_listener();
        let port = listener.port();
        let shared = listener.shared().clone();

        // Hold-open signal — main task tells client when state check is done
        // so the connection stays in Control long enough to be observed.
        let (done_tx, done_rx) = oneshot::channel::<()>();

        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).await.expect("client connect");

            let hello = ControlMsg::PeerHello {
                locator: encode_locator(Ipv4Addr::new(127, 0, 0, 1), 40000),
            };
            write_framed_message(&mut stream, &hello.to_bytes()).await.expect("write PEER_HELLO");

            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await.expect("read ack length");
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut data = vec![0u8; len];
            stream.read_exact(&mut data).await.expect("read ack payload");

            let _ = done_rx.await;
            data
        });

        let deadline = Instant::now() + Duration::from_secs(3);
        let advanced = wait_until(deadline, || {
            shared.connections.iter().any(|e| e.state == ConnectionState::Control)
        })
        .await;

        let _ = done_tx.send(());
        assert!(advanced, "state did not advance to Control within deadline");

        let received = client.await.expect("client task");
        assert_eq!(&received[..4], b"INT2");
        assert_eq!(received[4], MSG_PEER_HELLO_ACK);

        listener.shutdown().await;
    }

    /// A PEER_HELLO with a zero (missing) locator is rejected with an ERROR
    /// carrying `ERR_CODE_MISSING_LOCATOR` — a peer must advertise its locator.
    #[tokio::test(flavor = "multi_thread")]
    async fn peer_hello_without_locator_is_rejected() {
        let listener = make_listener();
        let port = listener.port();

        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).await.expect("client connect");
            let hello = ControlMsg::PeerHello { locator: [0u8; 16] };
            write_framed_message(&mut stream, &hello.to_bytes()).await.expect("write PEER_HELLO");

            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await.expect("read length");
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut data = vec![0u8; len];
            stream.read_exact(&mut data).await.expect("read payload");
            data
        });

        let received = client.await.expect("client task");
        // Frame: [4B "INT2"][MSG_ERROR][operation][2B code]...
        assert_eq!(&received[..4], b"INT2");
        assert_eq!(received[4], MSG_ERROR, "expected an ERROR response");
        assert_eq!(received[5], MSG_PEER_HELLO, "operation should be PEER_HELLO");
        let code = u16::from_be_bytes([received[6], received[7]]);
        assert_eq!(code, ERR_CODE_MISSING_LOCATOR);

        listener.shutdown().await;
    }

    // ── connection cleanup on RST / FIN ──────────────────────────────────────

    /// A client that resets the connection (`linger=0` + drop) causes the
    /// conn_actor reader to detect the RST and remove the connection entry.
    #[tokio::test(flavor = "multi_thread")]
    async fn rst_close_cleans_up_connection_and_token() {
        let listener = make_listener();
        let port = listener.port();

        let client = tokio::spawn(async move {
            let sock = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
            sock.connect(&SockAddr::from(addr)).unwrap();
            sock.set_linger(Some(Duration::from_secs(0))).unwrap();
            drop(sock); // RST
        });
        let _ = client.await;

        let shared = listener.shared().clone();
        let deadline = Instant::now() + Duration::from_secs(3);
        let gone = wait_until(deadline, || shared.connection_count() == 0).await;
        assert!(gone, "connection not removed after RST");

        listener.shutdown().await;
    }

    /// A client that issues a graceful FIN causes the conn_actor reader to
    /// detect EOF and remove the connection entry.
    #[tokio::test(flavor = "multi_thread")]
    async fn fin_close_cleans_up_connection_and_token() {
        let listener = make_listener();
        let port = listener.port();

        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).await.expect("client connect");
            stream.shutdown().await.ok(); // FIN
        });
        let _ = client.await;

        let shared = listener.shared().clone();
        let deadline = Instant::now() + Duration::from_secs(3);
        let gone = wait_until(deadline, || shared.connection_count() == 0).await;
        assert!(gone, "connection not removed after FIN");

        listener.shutdown().await;
    }

    /// A burst of RST connections must not leak entries in either the
    /// connections map or the peer_connections map.
    #[tokio::test(flavor = "multi_thread")]
    async fn rst_burst_does_not_leak_tokens() {
        const ROUNDS: usize = 8;

        let listener = make_listener();
        let port = listener.port();

        let mut clients = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let h = tokio::spawn(async move {
                let sock = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
                let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
                sock.connect(&SockAddr::from(addr)).unwrap();
                sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                drop(sock);
            });
            clients.push(h);
        }
        for c in clients {
            let _ = c.await;
        }

        let shared = listener.shared().clone();
        let deadline = Instant::now() + Duration::from_secs(5);
        let clean = wait_until(deadline, || shared.connection_count() == 0).await;
        assert!(clean, "RST burst leaked connection tokens");
        assert_eq!(shared.peer_count(), 0, "RST burst leaked peer groups");

        listener.shutdown().await;
    }
}
