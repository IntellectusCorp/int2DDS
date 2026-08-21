//! Long-lived per-connection tasks.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use log::debug;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::rtps::transport::tcp::connection_registry::{ConnectionId, ConnectionRegistry};
use crate::rtps::transport::tcp::framing::{read_framed_message_pooled, TcpBufferPool, TcpFrame};
use crate::rtps::transport::tcp::stream::{AsyncConnReadHalf, AsyncConnWriteHalf};

/// Spawn the generic reader used by both accepted and outbound connections.
pub(crate) fn spawn_reader(
    read_half: AsyncConnReadHalf,
    conn_id: ConnectionId,
    shared: Arc<ConnectionRegistry>,
    cancel: CancellationToken,
    first_frame_timeout: Option<Duration>,
) {
    tokio::spawn(reader_task(read_half, conn_id, shared, cancel, first_frame_timeout));
}

/// Accepted connections are receive-only in the directional connection model.
/// Keep their write half alive so dropping it does not emit a premature FIN;
/// it performs the graceful plain/TLS shutdown when the reader exits.
pub(crate) fn spawn_inbound_connection(
    read_half: AsyncConnReadHalf,
    write_half: AsyncConnWriteHalf,
    conn_id: ConnectionId,
    shared: Arc<ConnectionRegistry>,
    cancel: CancellationToken,
    first_frame_timeout: Duration,
) {
    spawn_reader(read_half, conn_id, shared, cancel.clone(), Some(first_frame_timeout));
    tokio::spawn(async move {
        cancel.cancelled().await;
        let mut write_half = write_half;
        let _ = write_half.shutdown().await;
    });
}

async fn read_one(
    read_half: &mut AsyncConnReadHalf,
    pool: &Arc<TcpBufferPool>,
    timeout: Option<Duration>,
) -> io::Result<TcpFrame> {
    match timeout {
        Some(duration) => {
            tokio::time::timeout(duration, read_framed_message_pooled(read_half, pool))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "first TCP frame timed out"))?
        }
        None => read_framed_message_pooled(read_half, pool).await,
    }
}

async fn reader_task(
    mut read_half: AsyncConnReadHalf,
    conn_id: ConnectionId,
    shared: Arc<ConnectionRegistry>,
    cancel: CancellationToken,
    first_frame_timeout: Option<Duration>,
) {
    let mut timeout = first_frame_timeout;
    loop {
        let frame = tokio::select! {
            result = read_one(&mut read_half, shared.buffer_pool(), timeout) => match result {
                Ok(frame) => frame,
                Err(error) => {
                    debug!("TCP conn {} read stopped: {}", conn_id, error);
                    break;
                }
            },
            _ = cancel.cancelled() => {
                debug!("TCP conn {} reader cancelled", conn_id);
                break;
            }
        };
        timeout = None;

        tokio::select! {
            _ = shared.route_frame(conn_id, frame) => {}
            _ = cancel.cancelled() => break,
        }
    }

    cancel.cancel();
    shared.remove_connection(conn_id);
}
