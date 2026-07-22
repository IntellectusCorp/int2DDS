//! Per-connection actor: owns one TCP/TLS stream and drives it with a reader
//! task, plus a writer task on connections that send protocol frames.

use std::sync::Arc;

use log::{debug, warn};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::rtps::transport::tcp::{
    connection_registry::{ConnectionId, ConnectionRegistry},
    framing::{read_framed_message, write_framed_message},
    stream::{AsyncConnReadHalf, AsyncConnWriteHalf},
};

/// Control inbox capacity (PORT_RESERVE round-trips + handshake ACK/Error
/// replies).
pub(crate) const INBOX_CAPACITY: usize = 1024;

pub(crate) fn inbox_capacity() -> usize {
    INBOX_CAPACITY
}

/// Spawn the reader/writer task pair, for a connection that sends protocol
/// frames (control, or an inbound connection during its handshake).
pub(crate) fn spawn_tasks(
    read_half: AsyncConnReadHalf,
    conn_id: ConnectionId,
    shared: Arc<ConnectionRegistry>,
    conn_cancel: CancellationToken,
    writer_tx: mpsc::Sender<Vec<u8>>,
    control_rx: mpsc::Receiver<Vec<u8>>,
    write_half: AsyncConnWriteHalf,
) {
    {
        let cancel = conn_cancel.clone();
        let shared = shared.clone();
        tokio::spawn(reader_task(read_half, conn_id, shared, writer_tx, cancel));
    }
    {
        let cancel = conn_cancel.clone();
        tokio::spawn(writer_task(write_half, control_rx, cancel));
    }
}

/// Spawn only the reader task, for a connection that never sends protocol frames
/// (an outbound data connection: `Active` from its first frame, so `dispatch`
/// only forwards RTPS data and never touches `writer_tx`).
pub(crate) fn spawn_reader_only(
    read_half: AsyncConnReadHalf,
    conn_id: ConnectionId,
    shared: Arc<ConnectionRegistry>,
    conn_cancel: CancellationToken,
    writer_tx: mpsc::Sender<Vec<u8>>,
) {
    tokio::spawn(reader_task(read_half, conn_id, shared, writer_tx, conn_cancel));
}

/// Read frames from the stream and hand them to `ConnectionRegistry::dispatch`.
///
/// Exits on read error, EOF, or cancellation. On exit, cancels the shared
/// token so the writer task wakes and tears the connection entry down.
async fn reader_task(
    mut read_half: AsyncConnReadHalf,
    conn_id: ConnectionId,
    shared: Arc<ConnectionRegistry>,
    writer_tx: mpsc::Sender<Vec<u8>>,
    cancel: CancellationToken,
) {
    loop {
        let payload = tokio::select! {
            result = read_framed_message(&mut read_half) => match result {
                Ok(payload) => payload,
                Err(e) => {
                    // Expected at EOF / RST on normal disconnect — debug, not warn.
                    debug!("conn {} read error: {:?}", conn_id, e);
                    break;
                }
            },

            _ = cancel.cancelled() => {
                debug!("conn {} reader cancelled", conn_id);
                break;
            }
        };

        tokio::select! {
            _ = shared.dispatch(conn_id, payload, &writer_tx) => {}
            _ = cancel.cancelled() => {
                debug!("conn {} reader cancelled during dispatch", conn_id);
                break;
            }
        }
    }

    // Wake the writer task so the pair tears down together.
    cancel.cancel();
    shared.remove_connection(conn_id);
}

/// Write control frames from `control_rx` to the connection's write half, which
/// this task owns outright — it is the connection's only writer.
async fn writer_task(
    mut write_half: AsyncConnWriteHalf,
    mut control_rx: mpsc::Receiver<Vec<u8>>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            maybe = control_rx.recv() => {
                match maybe {
                    Some(frame) => {
                        if let Err(e) = write_framed_message(&mut write_half, &frame).await {
                            warn!("control write error: {:?}", e);
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = cancel.cancelled() => break,
        }
    }

    // Flush any control reply still queued, then close — a control connection may
    // still owe the peer a reply.
    while let Ok(frame) = control_rx.try_recv() {
        if write_framed_message(&mut write_half, &frame).await.is_err() {
            break;
        }
    }
    let _ = write_half.shutdown().await;
    cancel.cancel();
}
