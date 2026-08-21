//! Single TCP listener and accepted-connection lifecycle.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::plugin::IncomingMessage;
use crate::rtps::transport::tcp::connection_registry::{
    apply_socket_tuning, ConnectionDirection, ConnectionRegistry, TcpSocketTuning,
};
use crate::rtps::transport::tcp::connection_tasks::spawn_inbound_connection;
use crate::rtps::transport::tcp::stream::wrap_plain;
use crate::rtps::transport::tcp::tls::{accept_tls_async, TlsConfig};

pub(crate) struct TcpMuxListener {
    port: u16,
    shared: Arc<ConnectionRegistry>,
    cancel: CancellationToken,
    task_handles: Vec<JoinHandle<()>>,
}

impl TcpMuxListener {
    #[allow(clippy::too_many_arguments)]
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
        first_frame_timeout: Duration,
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

        let task_handles = vec![
            shared.spawn_self_delivery_task(cancel.clone()),
            tokio::spawn(accept_loop_task(
                std_listener,
                Arc::clone(&shared),
                tls_config,
                tls_handshake_timeout,
                first_frame_timeout,
                cancel.clone(),
            )),
        ];

        info!("TcpMuxListener: listening on port {} (domain={})", actual_port, domain_id);
        Ok(Self { port: actual_port, shared, cancel, task_handles })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn shared(&self) -> &Arc<ConnectionRegistry> {
        &self.shared
    }

    pub(crate) async fn shutdown(mut self) {
        self.cancel.cancel();
        for handle in std::mem::take(&mut self.task_handles) {
            let _ = handle.await;
        }
    }
}

impl Drop for TcpMuxListener {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn bind_listener(port: u16) -> io::Result<std::net::TcpListener> {
    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)?;
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(128)?;
    Ok(std::net::TcpListener::from(socket))
}

async fn accept_loop_task(
    std_listener: std::net::TcpListener,
    shared: Arc<ConnectionRegistry>,
    tls_config: Option<Arc<TlsConfig>>,
    tls_handshake_timeout: Duration,
    first_frame_timeout: Duration,
    cancel: CancellationToken,
) {
    let listener = match TcpListener::from_std(std_listener) {
        Ok(listener) => listener,
        Err(error) => {
            error!("failed to convert TCP listener to async: {}", error);
            return;
        }
    };

    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((tcp, addr)) => {
                    apply_socket_tuning(&tcp, &shared.tuning);
                    tokio::spawn(prepare_connection(
                        tcp,
                        addr,
                        Arc::clone(&shared),
                        tls_config.clone(),
                        tls_handshake_timeout,
                        first_frame_timeout,
                        cancel.clone(),
                    ));
                }
                Err(error) => warn!("TCP accept error: {}", error),
            },
            _ = cancel.cancelled() => {
                debug!("TCP accept loop cancelled");
                break;
            }
        }
    }
}

async fn prepare_connection(
    tcp: TcpStream,
    addr: SocketAddr,
    shared: Arc<ConnectionRegistry>,
    tls_config: Option<Arc<TlsConfig>>,
    tls_handshake_timeout: Duration,
    first_frame_timeout: Duration,
    parent_cancel: CancellationToken,
) {
    let stream = match tls_config {
        Some(config) => {
            let server_config = match config.build_server_config() {
                Ok(config) => config,
                Err(error) => {
                    warn!("TLS server config error: {}", error);
                    return;
                }
            };
            match tokio::time::timeout(tls_handshake_timeout, accept_tls_async(tcp, server_config))
                .await
            {
                Ok(Ok(stream)) => stream,
                Ok(Err(error)) => {
                    warn!("TLS handshake from {} failed: {}", addr, error);
                    return;
                }
                Err(_) => {
                    warn!("TLS handshake from {} timed out", addr);
                    return;
                }
            }
        }
        None => wrap_plain(tcp),
    };

    let (read_half, write_half) = stream.into_split();
    let conn_cancel = parent_cancel.child_token();
    let conn_id =
        shared.register_connection(addr, ConnectionDirection::Inbound, conn_cancel.clone());
    spawn_inbound_connection(
        read_half,
        write_half,
        conn_id,
        shared,
        conn_cancel,
        first_frame_timeout,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::tcp::framing::{write_framed_message, TcpFrameKind};

    fn listener(
        timeout: Duration,
    ) -> (TcpMuxListener, flume::Receiver<IncomingMessage>, flume::Receiver<IncomingMessage>) {
        let (discovery_tx, discovery_rx) = flume::bounded(1);
        let (user_tx, user_rx) = flume::bounded(1);
        let listener = TcpMuxListener::bind_and_spawn(
            0,
            0,
            0,
            [0; 12],
            discovery_tx,
            user_tx,
            None,
            TcpSocketTuning::default(),
            Duration::from_secs(1),
            timeout,
            CancellationToken::new(),
        )
        .unwrap();
        (listener, discovery_rx, user_rx)
    }

    async fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            if predicate() {
                return true;
            }
            tokio::task::yield_now().await;
        }
        predicate()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn one_reader_routes_interleaved_kinds() {
        let (listener, discovery_rx, user_rx) = listener(Duration::from_secs(1));
        let mut client = TcpStream::connect(("127.0.0.1", listener.port())).await.unwrap();
        write_framed_message(&mut client, TcpFrameKind::UserData, b"user").await.unwrap();
        write_framed_message(&mut client, TcpFrameKind::Discovery, b"discovery").await.unwrap();

        assert_eq!(user_rx.recv_async().await.unwrap().data.as_ref(), b"user");
        assert_eq!(discovery_rx.recv_async().await.unwrap().data.as_ref(), b"discovery");
        listener.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn backpressure_on_one_connection_does_not_park_another_reader() {
        let (listener, discovery_rx, user_rx) = listener(Duration::from_secs(1));
        let mut discovery = TcpStream::connect(("127.0.0.1", listener.port())).await.unwrap();
        let mut user = TcpStream::connect(("127.0.0.1", listener.port())).await.unwrap();

        write_framed_message(&mut discovery, TcpFrameKind::Discovery, b"first").await.unwrap();
        write_framed_message(&mut discovery, TcpFrameKind::Discovery, b"blocked").await.unwrap();
        write_framed_message(&mut user, TcpFrameKind::UserData, b"independent").await.unwrap();

        assert_eq!(user_rx.recv_async().await.unwrap().data.as_ref(), b"independent");
        assert_eq!(discovery_rx.recv_async().await.unwrap().data.as_ref(), b"first");
        assert_eq!(discovery_rx.recv_async().await.unwrap().data.as_ref(), b"blocked");
        listener.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn idle_accepted_connection_obeys_first_frame_timeout() {
        let (listener, _, _) = listener(Duration::from_millis(30));
        let _client = TcpStream::connect(("127.0.0.1", listener.port())).await.unwrap();
        assert!(
            wait_until(Duration::from_secs(1), || listener.shared().connection_count() == 1).await
        );
        assert!(
            wait_until(Duration::from_secs(1), || listener.shared().connection_count() == 0).await
        );
        listener.shutdown().await;
    }
}
