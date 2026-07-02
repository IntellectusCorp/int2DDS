//! Per-connection actor: owns one TCP/TLS stream and runs a reader/writer
//! task pair against it.
//!
//! The write half is written two ways, chosen by whether the caller needs the
//! write result back:
//!
//! - **Inline** — the user-data send path locks [`SharedWriteHalf`] and calls
//!   `write_framed_message` on the caller's (sync) thread. A DDS `write()` must
//!   see the wire error (RST / unacked timeout) synchronously, and getting the
//!   result requires blocking the caller anyway; inline does that with the
//!   least overhead, so the high-rate data path uses it.
//! - **Indirect** — control-plane frames produced *inside* the runtime go
//!   through `writer_tx` to the writer task. The reader produces these
//!   (handshake ACK/Error replies, PORT_RESERVE requests) and must not block
//!   its read loop on a write (backpressure deadlock), so it enqueues and keeps
//!   reading; the writer task drains the inbox. A reply value that must return
//!   travels via the pending_ack oneshot.
//!
//! The reader hands incoming frames to `ConnectionRegistry::dispatch`; both
//! tasks share a child `CancellationToken` so the pair tears down together.

use std::sync::Arc;

use log::{debug, warn};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Mutex as TokioMutex};
use tokio_util::sync::CancellationToken;

use crate::rtps::transport::tcp::{
    connection_registry::{ConnectionId, ConnectionRegistry},
    framing::{read_framed_message, write_framed_message},
    stream::{AsyncConnReadHalf, AsyncConnStream, AsyncConnWriteHalf},
};

/// Per-connection writer inbox capacity. The inbox carries control-plane
/// frames only (PORT_RESERVE round-trips + handshake ACK/Error replies);
/// user-data writes bypass it. 256 gives ample headroom for the control
/// protocol burst while keeping memory bounded.
pub(crate) const INBOX_CAPACITY: usize = 256;

pub(crate) fn inbox_capacity() -> usize {
    INBOX_CAPACITY
}

/// Shared handle to a connection's write half. The synchronous user-data
/// send path (`TcpSender::send_to_*`) and the writer task both acquire
/// this mutex before touching the wire. Hold time is the duration of one
/// `write_vectored` syscall.
pub(crate) type SharedWriteHalf = Arc<TokioMutex<AsyncConnWriteHalf>>;

/// Spawn the reader/writer task pair for one connection.
///
/// Returns a [`SharedWriteHalf`] used by the synchronous user-data send
/// path to lock the write half and perform the wire `writev` inline on
/// the calling thread.
///
/// `tx` is the writer inbox passed in by the caller; it has already been
/// stored in `ConnectionRegistry::ConnectionEntry::writer_tx` so the reader task
/// (for protocol replies) and lifecycle tasks (keepalive, PORT_RESERVE)
/// can push control frames through the same writer task.
///
/// Cancellation: a child of `parent_cancel` is created and shared by both
/// tasks. Either task's exit (read EOF, write error, …) cancels the child
/// so the pair always tears down together. Cancelling the parent (e.g. on
/// plugin shutdown) propagates here automatically.
pub(crate) fn spawn_connection_tasks(
    stream: AsyncConnStream,
    conn_id: ConnectionId,
    shared: Arc<ConnectionRegistry>,
    parent_cancel: CancellationToken,
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
) -> SharedWriteHalf {
    // Child token: cancelling this pair does not affect siblings, but
    // parent shutdown still propagates down to both tasks.
    let conn_cancel = parent_cancel.child_token();

    let (read_half, write_half) = stream.into_split();
    let write_half: SharedWriteHalf = Arc::new(TokioMutex::new(write_half));

    // Reader task — owns the read half. The `writer_tx` clone lets the
    // reader push protocol responses (acks, replies) back through the
    // writer task.
    {
        let cancel = conn_cancel.clone();
        let shared = shared.clone();
        tokio::spawn(reader_task(read_half, conn_id, shared, tx, cancel));
    }

    // Writer task — shares the write half with the user-data send path.
    {
        let cancel = conn_cancel.clone();
        let write_half = Arc::clone(&write_half);
        tokio::spawn(writer_task(write_half, rx, cancel));
    }

    write_half
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
        tokio::select! {
            result = read_framed_message(&mut read_half) => {
                match result {
                    Ok(payload) => {
                        // dispatch routes the frame: control → writer_tx,
                        // RTPS data → ConnectionRegistry's crossbeam channels.
                        shared.dispatch(conn_id, payload, &writer_tx).await;
                    }
                    Err(e) => {
                        // Expected at EOF / RST on normal disconnect — debug, not warn.
                        debug!("conn {} read error: {:?}", conn_id, e);
                        break;
                    }
                }
            }

            _ = cancel.cancelled() => {
                debug!("conn {} reader cancelled", conn_id);
                break;
            }
        }
    }

    // Wake the writer task so the pair tears down together.
    cancel.cancel();
    shared.remove_connection(conn_id);
}

/// Drain the control-plane inbox and write each frame to the stream.
///
/// One frame per iteration: locks the [`SharedWriteHalf`], does a single
/// `write_framed_message`, releases. Control traffic is low rate, so no
/// batching is needed; keeping the mutex hold time to one frame minimises
/// contention with the user-data sender.
///
/// Exits when the channel closes (all senders dropped), on a write error,
/// or on cancellation. Sends a TCP FIN via `shutdown()` before returning
/// for a graceful close.
async fn writer_task(
    write_half: SharedWriteHalf,
    mut rx: mpsc::Receiver<Vec<u8>>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            maybe_frame = rx.recv() => {
                match maybe_frame {
                    Some(frame) => {
                        let mut wh = write_half.lock().await;
                        if let Err(e) = write_framed_message(&mut *wh, &frame).await {
                            warn!("write error: {:?}", e);
                            break;
                        }
                    }
                    // All senders dropped → no more outbound traffic possible.
                    None => break,
                }
            }
            _ = cancel.cancelled() => break,
        }
    }

    // Drain any frames the inbox already holds before sending FIN, so a frame
    // enqueued just before cancel is not lost to the `select!` race.
    let mut wh = write_half.lock().await;
    while let Ok(frame) = rx.try_recv() {
        if write_framed_message(&mut *wh, &frame).await.is_err() {
            cancel.cancel();
            let _ = wh.shutdown().await;
            return;
        }
    }

    // Wake the reader and send a graceful TCP FIN.
    cancel.cancel();
    let _ = wh.shutdown().await;
}
