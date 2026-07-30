//! Inbound side of the TCP mux transport.
//!
//! `TcpMuxListener` owns the listener-side accept loop and exposes the shared
//! `ConnectionRegistry` via `shared()`.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::rtps::transport::tcp::connection_tasks::{inbox_capacity, spawn_tasks};
use crate::rtps::transport::tcp::stream::wrap_plain;
use crate::rtps::transport::tcp::tls::{accept_tls_async, TlsConfig};
use crate::rtps::{
    common::guid::GuidPrefix,
    transport::{
        error::TransportErrorCode,
        plugin::IncomingMessage,
        tcp::connection_registry::{apply_socket_tuning, ConnectionRegistry, TcpSocketTuning},
    },
};

/// TCP multiplexed listener — owns the listener-side tasks and the shared
/// `ConnectionRegistry`.
///
/// Dropping or calling `shutdown()` cancels all tasks; cancellation
/// propagates into every reader/writer task pair via child tokens.
pub(crate) struct TcpMuxListener {
    port: u16,
    pub(crate) shared: Arc<ConnectionRegistry>,
    cancel: CancellationToken,
    task_handles: Vec<JoinHandle<()>>,
}

impl TcpMuxListener {
    pub(crate) fn bind_and_spawn(
        port: u16,
        domain_id: u32,
        participant_id: u32,
        local_guid_prefix: GuidPrefix,
        discovery_tx: flume::Sender<IncomingMessage>,
        user_data_tx: flume::Sender<IncomingMessage>,
        tls_config: Option<Arc<TlsConfig>>,
        tuning: TcpSocketTuning,
        tls_handshake_timeout: Duration,
        peer_handshake_timeout: Duration,
        cancel: CancellationToken,
    ) -> io::Result<Self> {
        let std_listener = bind_listener(port)?;
        let actual_port = std_listener.local_addr()?.port();

        let shared = Arc::new(ConnectionRegistry::new(
            domain_id,
            participant_id,
            local_guid_prefix,
            tuning,
            discovery_tx,
            user_data_tx,
        ));

        let mut handles = Vec::with_capacity(1);
        handles.push(tokio::spawn(accept_loop_task(
            std_listener,
            shared.clone(),
            tls_config,
            tls_handshake_timeout,
            peer_handshake_timeout,
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
    pub(crate) fn shared(&self) -> &Arc<ConnectionRegistry> {
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
/// outside a runtime context.
fn bind_listener(port: u16) -> io::Result<std::net::TcpListener> {
    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)?;
    // SO_REUSEADDR semantics differ by OS. On Unix it only relaxes rebinding a
    // port left in TIME_WAIT. On Windows it would instead
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
    shared: Arc<ConnectionRegistry>,
    tls_config: Option<Arc<TlsConfig>>,
    tls_handshake_timeout: Duration,
    peer_handshake_timeout: Duration,
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

                        apply_socket_tuning(&tcp, &shared.tuning);

                        let shared = shared.clone();
                        let tls = tls_config.clone();
                        let parent_cancel = cancel.clone();
                        tokio::spawn(handshake_and_register_task(
                            tcp,
                            addr,
                            shared,
                            tls,
                            tls_handshake_timeout,
                            peer_handshake_timeout,
                            parent_cancel,
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

/// Registration happens **before** spawning the tasks — this guarantees
/// the reader task finds its `ConnectionEntry` in `ConnectionRegistry.connections` when
/// the first inbound frame arrives.
async fn handshake_and_register_task(
    tcp: TcpStream,
    addr: SocketAddr,
    shared: Arc<ConnectionRegistry>,
    tls_config: Option<Arc<TlsConfig>>,
    tls_handshake_timeout: Duration,
    peer_handshake_timeout: Duration,
    parent_cancel: CancellationToken,
) {
    // Optional TLS — wrap or pass through plain.
    let stream = match &tls_config {
        Some(cfg) => match cfg.build_server_config() {
            Ok(server_cfg) => {
                match tokio::time::timeout(tls_handshake_timeout, accept_tls_async(tcp, server_cfg))
                    .await
                {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        warn!("TLS handshake failed from {:?}: {:?}", addr, e);
                        return;
                    }
                    Err(_) => {
                        warn!(
                            "TLS handshake from {:?} did not complete within {:?} — closing",
                            addr, tls_handshake_timeout
                        );
                        return;
                    }
                }
            }
            Err(e) => {
                warn!("TLS server config error: {:?}", e);
                return;
            }
        },
        None => wrap_plain(tcp),
    };

    // An inbound connection is already established, and its writer task owns the
    // write half outright (it writes handshake/control acks; the send path never
    // targets an inbound connection). Split the stream straight into halves.
    let (read_half, write_half) = stream.into_split();

    // Register the entry before the reader task starts polling.
    let (tx, rx) = mpsc::channel::<Vec<u8>>(inbox_capacity());
    let conn_cancel = parent_cancel.child_token();
    let conn_id = shared.register_inbound_connection(addr, conn_cancel.clone());
    spawn_tasks(read_half, conn_id, shared.clone(), conn_cancel.clone(), tx, rx, write_half);

    // Check for success after waiting for the handshake timeout
    // period (clean up connections stalled at the handshake).
    tokio::select! {
        _ = tokio::time::sleep(peer_handshake_timeout) => {
            if !shared.inbound_handshake_complete(conn_id) {
                warn!(
                    "TcpMuxListener [{}]: conn {} from {:?} did not finish the handshake \
                     within {:?} — closing",
                    TransportErrorCode::TcpHandshakeHelloFailed,
                    conn_id,
                    addr,
                    peer_handshake_timeout
                );
                conn_cancel.cancel();
            }
        }
        _ = conn_cancel.cancelled() => {}
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::port_manager::PortManager;
    use crate::rtps::transport::tcp::connection_registry::ConnectionState;
    use crate::rtps::transport::tcp::framing::{read_framed_message, write_framed_message};
    use crate::rtps::transport::tcp::protocol::{
        encode_locator, ControlMsg, ERR_CODE_MISSING_LOCATOR, MSG_ERROR, MSG_PEER_HELLO,
        MSG_PEER_HELLO_ACK,
    };
    use flume::bounded;
    use socket2::{Domain, SockAddr, Socket, Type};
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::oneshot;

    /// Helper: build the four channels needed by `bind_and_spawn`, returning
    /// the receivers so tests can assert on routed messages if needed.
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

    /// Helper: standard listener with no TLS.
    fn make_listener() -> TcpMuxListener {
        make_listener_with_cancel(CancellationToken::new())
    }

    /// Helper: standard listener whose cancel token the caller keeps a handle on.
    fn make_listener_with_cancel(cancel: CancellationToken) -> TcpMuxListener {
        make_listener_with(Duration::from_secs(5), cancel)
    }

    /// Helper: listener with an explicit handshake deadline, for the tests that
    /// need it to fire (or to stay clear) within the test's own budget.
    fn make_listener_with(
        peer_handshake_timeout: Duration,
        cancel: CancellationToken,
    ) -> TcpMuxListener {
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
            Duration::from_secs(5),
            peer_handshake_timeout,
            cancel,
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

    /// Dropping the listener (without explicit shutdown) cancels the token it
    /// was handed — and nothing above it, so a component sharing the same root
    /// keeps running.
    #[tokio::test(flavor = "multi_thread")]
    async fn drop_cancels_tasks() {
        let root = CancellationToken::new();
        let cancel_clone = root.child_token();
        let listener = make_listener_with_cancel(cancel_clone.clone());
        drop(listener);

        // The token must be cancelled even though we did not await shutdown.
        // (Underlying task handles are leaked here — that is acceptable for
        // best-effort Drop; tests using `shutdown()` get clean teardown.)
        assert!(cancel_clone.is_cancelled(), "Drop should fire cancel");
        assert!(!root.is_cancelled(), "Drop must not reach past the listener's own token");
    }

    // ── accept + register ────────────────────────────────────────────────────

    /// Connecting to the bound port causes `accept_loop_task` to accept and
    /// `handshake_and_register_task` to register the entry in `ConnectionRegistry`.
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

    // ── inbound handshake deadline ───────────────────────────────────────────

    /// Deadline short enough that a test can observe it firing, but long enough
    /// that a real handshake completes first.
    const TEST_DEADLINE: Duration = Duration::from_millis(700);

    fn make_deadline_listener() -> TcpMuxListener {
        make_listener_with(TEST_DEADLINE, CancellationToken::new())
    }

    /// A peer that opens a socket and sends nothing is invisible to keepalive —
    /// its kernel answers probes — so only the handshake deadline releases it.
    #[tokio::test(flavor = "multi_thread")]
    async fn silent_connection_is_closed_after_handshake_deadline() {
        let listener = make_deadline_listener();
        let port = listener.port();
        let shared = listener.shared().clone();

        let client = tokio::spawn(async move {
            let _stream = TcpStream::connect(("127.0.0.1", port)).await.expect("client connect");
            // Outlive the observation window so only the deadline can end this.
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        // Registered first, then reaped — proves the deadline did the work.
        let up =
            wait_until(Instant::now() + Duration::from_secs(2), || shared.connection_count() == 1)
                .await;
        assert!(up, "connection was never registered");

        let gone =
            wait_until(Instant::now() + Duration::from_secs(3), || shared.connection_count() == 0)
                .await;
        assert!(gone, "silent connection outlived the handshake deadline");

        listener.shutdown().await;
        client.abort();
    }

    /// A length header with no payload behind it parks the reader mid-frame.
    /// The deadline has to cut through a read already in progress.
    #[tokio::test(flavor = "multi_thread")]
    async fn partial_frame_is_closed_after_handshake_deadline() {
        let listener = make_deadline_listener();
        let port = listener.port();
        let shared = listener.shared().clone();

        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).await.expect("client connect");
            // Announce a large frame, then never send its body.
            stream.write_all(&(1024u32 * 1024).to_be_bytes()).await.expect("write length");
            // Outlive the observation window so only the deadline can end this.
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        let up =
            wait_until(Instant::now() + Duration::from_secs(2), || shared.connection_count() == 1)
                .await;
        assert!(up, "connection was never registered");

        let gone =
            wait_until(Instant::now() + Duration::from_secs(3), || shared.connection_count() == 0)
                .await;
        assert!(gone, "half-sent frame outlived the handshake deadline");

        listener.shutdown().await;
        client.abort();
    }

    /// PEER_HELLO alone is not a finished handshake: a control connection has to
    /// be followed by the peer's data connection. Without it the peer holds a
    /// control connection it never uses.
    #[tokio::test(flavor = "multi_thread")]
    async fn peer_hello_without_a_data_connection_is_closed_after_deadline() {
        let listener = make_deadline_listener();
        let port = listener.port();
        let shared = listener.shared().clone();

        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).await.expect("client connect");
            let hello = ControlMsg::PeerHello {
                locator: encode_locator(Ipv4Addr::new(127, 0, 0, 1), 40100),
            };
            write_framed_message(&mut stream, &hello.to_bytes()).await.expect("write PEER_HELLO");
            let _ = read_framed_message(&mut stream).await; // PEER_HELLO_ACK
                                                            // Outlive the observation window so only the deadline can end this.
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        let reached_control = wait_until(Instant::now() + Duration::from_secs(2), || {
            shared.connections.iter().any(|e| e.state == ConnectionState::Control)
        })
        .await;
        assert!(reached_control, "PEER_HELLO did not advance the connection to Control");

        let gone =
            wait_until(Instant::now() + Duration::from_secs(3), || shared.connection_count() == 0)
                .await;
        assert!(gone, "control connection with no data connection outlived the deadline");

        listener.shutdown().await;
        client.abort();
    }

    /// The regression that matters: a peer that completes all three steps must
    /// keep both connections well past the deadline. A control connection stops
    /// at `Control` for good, so a naive "must reach Active" rule would cut a
    /// healthy peer here.
    #[tokio::test(flavor = "multi_thread")]
    async fn completed_handshake_survives_the_deadline() {
        let listener = make_deadline_listener();
        let port = listener.port();
        let shared = listener.shared().clone();

        // domain 0 / participant 0 — matches the listener built by the helper.
        let logical_port = PortManager::get_discovery_traffic_unicast_port(0, 0);

        let (done_tx, done_rx) = oneshot::channel::<()>();
        let client = tokio::spawn(async move {
            // 1. Control connection: PEER_HELLO.
            let mut control =
                TcpStream::connect(("127.0.0.1", port)).await.expect("control connect");
            let hello = ControlMsg::PeerHello {
                locator: encode_locator(Ipv4Addr::new(127, 0, 0, 1), 40200),
            };
            write_framed_message(&mut control, &hello.to_bytes()).await.expect("write PEER_HELLO");
            read_framed_message(&mut control).await.expect("PEER_HELLO_ACK");

            // 2. PORT_RESERVE on the control connection → cookie.
            let reserve = ControlMsg::PortReserve { logical_port };
            write_framed_message(&mut control, &reserve.to_bytes())
                .await
                .expect("write PORT_RESERVE");
            let ack = read_framed_message(&mut control).await.expect("PORT_RESERVE_ACK");
            let cookie = match ControlMsg::from_bytes(&ack).expect("parse ack") {
                ControlMsg::PortReserveAck { cookie } => cookie,
                other => panic!("expected PORT_RESERVE_ACK, got {}", other.type_name()),
            };

            // 3. Data connection: PORT_BIND with that cookie.
            let mut data = TcpStream::connect(("127.0.0.1", port)).await.expect("data connect");
            let bind = ControlMsg::PortBind { cookie };
            write_framed_message(&mut data, &bind.to_bytes()).await.expect("write PORT_BIND");
            read_framed_message(&mut data).await.expect("PORT_BIND_ACK");

            let _ = done_rx.await;
            drop((control, data));
        });

        let established =
            wait_until(Instant::now() + Duration::from_secs(2), || shared.connection_count() == 2)
                .await;
        assert!(established, "the three-step handshake did not establish both connections");

        // Outlast the deadline by a wide margin, then confirm nothing was reaped.
        tokio::time::sleep(TEST_DEADLINE * 3).await;
        assert_eq!(
            shared.connection_count(),
            2,
            "the deadline closed connections belonging to a completed handshake"
        );

        let _ = done_tx.send(());
        listener.shutdown().await;
        let _ = client.await;
    }

    // ── connection cleanup on RST / FIN ──────────────────────────────────────

    /// A client that resets the connection (`linger=0` + drop) causes the
    /// reader task to detect the RST and remove the connection entry.
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

    /// A client that issues a graceful FIN causes the reader task to
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
