//! Per-connection actor: owns one TCP/TLS stream and runs a reader/writer
//! task pair against it.
//!
//! `spawn_conn_actor` splits the stream into independent halves, spawns a
//! reader task that hands incoming frames to `MuxState::dispatch`, and a
//! writer task that drains frames from an mpsc inbox onto the wire. The
//! returned `mpsc::Sender` is the only handle external code needs to send
//! on this connection — the actor lifecycle is otherwise self-managed via
//! a child `CancellationToken` shared by both tasks.

use std::sync::{Arc, OnceLock};

use log::{debug, warn};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Mutex as TokioMutex};
use tokio_util::sync::CancellationToken;

use crate::rtps::transport::tcp::{
    framing::{read_framed_message, write_framed_batch},
    mux_state::{ConnectionId, MuxState},
    stream::{AsyncConnReadHalf, AsyncConnStream, AsyncConnWriteHalf},
};

/// Per-connection writer inbox capacity for both outbound (sender-initiated)
/// and inbound (listener-accepted) conn_actor pairs. Sized generously so the
/// control protocol's KEEPALIVE seed + PORT_RESERVE round-trips never race
/// the writer_task's first drain, while still applying backpressure on
/// sustained bursts.
pub(crate) const INBOX_CAPACITY: usize = 4;

pub(crate) fn inbox_capacity() -> usize {
    INBOX_CAPACITY
}

/// Per-connection writer batch cap: how many inbox frames the writer task
/// coalesces into one `write_vectored` call. See
/// `env::get_tcp_batch_max_frames` for the meaning and tuning notes. Cached
/// once so the value is stable across all connections in a run.
static BATCH_MAX_FRAMES_CACHE: OnceLock<usize> = OnceLock::new();

fn batch_max_frames() -> usize {
    *BATCH_MAX_FRAMES_CACHE.get_or_init(|| {
        let cap = crate::common::env::get_tcp_batch_max_frames();
        log::info!("[tcp] BATCH_MAX_FRAMES = {}", cap);
        cap
    })
}

/// Shared handle to a connection's write half. The async writer task and
/// any synchronous user-thread sender both acquire this `Mutex` before
/// touching the wire. Hold time is intentionally short — one batched
/// `writev` syscall — so contention between the two paths stays bounded.
pub(crate) type SharedWriteHalf = Arc<TokioMutex<AsyncConnWriteHalf>>;

/// Spawn the reader/writer task pair for one connection.
///
/// Returns a [`SharedWriteHalf`] that the caller stores alongside the
/// `mpsc::Sender` inbox. Async writes still flow through the inbox →
/// `writer_task` → batched `writev`; synchronous writes (driven by a
/// `PublishModeQosPolicy::Synchronous` DataWriter) acquire the same
/// `SharedWriteHalf` directly and write inline on the user thread.
///
/// `tx` is the outbound inbox passed in by the caller; it has already been
/// stored in `MuxState::ConnectionEntry::writer_tx` (so dispatched-into-here
/// frames and externally-sent frames all converge on the same writer task).
///
/// Cancellation: a child of `parent_cancel` is created and shared by both
/// tasks. Either task's exit (read EOF, write error, …) cancels the child
/// so the pair always tears down together. Cancelling the parent (e.g. on
/// plugin shutdown) propagates here automatically.
pub(crate) fn spawn_conn_actor(
    stream: AsyncConnStream,
    conn_id: ConnectionId,
    shared: Arc<MuxState>,
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
    // reader push protocol responses (acks, replies) through the writer
    // pipeline so all outbound traffic stays serialised on one task.
    {
        let cancel = conn_cancel.clone();
        let shared = shared.clone();
        tokio::spawn(reader_task(read_half, conn_id, shared, tx, cancel));
    }

    // Writer task — shares the write half with sync senders.
    {
        let cancel = conn_cancel.clone();
        let write_half = Arc::clone(&write_half);
        tokio::spawn(writer_task(write_half, rx, cancel));
    }

    write_half
}

/// Read frames from the stream and hand them to `MuxState::dispatch`.
///
/// Exits on read error, EOF, or cancellation. On exit, cancels the shared
/// token so the writer task wakes and tears the connection entry down.
async fn reader_task(
    mut read_half: AsyncConnReadHalf,
    conn_id: ConnectionId,
    shared: Arc<MuxState>,
    writer_tx: mpsc::Sender<Vec<u8>>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            result = read_framed_message(&mut read_half) => {
                match result {
                    Ok(payload) => {
                        // dispatch routes the frame: control → writer_tx,
                        // RTPS data → MuxState's crossbeam channels.
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

/// Drain the inbox channel and write batched frames to the stream.
///
/// After awaiting a single frame, the task greedily collects any frames
/// already queued on the inbox (`try_recv`) up to `batch_max_frames()`,
/// acquires the [`SharedWriteHalf`] mutex, and hands the batch to
/// `write_framed_batch` in one `write_vectored` call. This turns "one
/// syscall per RTPS submessage" into "one syscall per burst", letting TCP
/// TSO/GSO segment large coalesced writes on the NIC. The mutex is held
/// only for the writev itself, so a synchronous user-thread sender racing
/// on the same connection is blocked at most one batch.
///
/// Exits when the channel closes (all senders dropped), on a write error,
/// or on cancellation. Sends a TCP FIN via `shutdown()` before returning
/// for a graceful close.
async fn writer_task(
    write_half: SharedWriteHalf,
    mut rx: mpsc::Receiver<Vec<u8>>,
    cancel: CancellationToken,
) {
    let max_batch = batch_max_frames();
    let mut batch: Vec<Vec<u8>> = Vec::with_capacity(max_batch);

    loop {
        tokio::select! {
            maybe_frame = rx.recv() => {
                match maybe_frame {
                    Some(frame) => {
                        batch.push(frame);
                        // Greedy drain: pull anything already enqueued without
                        // yielding. Stops at `max_batch` to bound writev size
                        // (Linux IOV_MAX = 1024, 3 slices per frame).
                        while batch.len() < max_batch {
                            match rx.try_recv() {
                                Ok(f) => batch.push(f),
                                Err(_) => break,
                            }
                        }
                        let mut wh = write_half.lock().await;
                        if let Err(e) = write_framed_batch(&mut *wh, &batch).await {
                            // Real I/O failure — peer likely unreachable.
                            warn!("write error: {:?}", e);
                            break;
                        }
                        drop(wh);
                        batch.clear();
                    }
                    // All senders dropped → no more outbound traffic possible.
                    None => break,
                }
            }
            _ = cancel.cancelled() => break,
        }
    }

    // Drain any frames the inbox already holds before sending FIN — e.g. the
    // idle-timeout Error pushed by `prune_idle_connections` immediately
    // before cancel, which would otherwise be lost to the `select!` race.
    // `try_recv` only: we don't await fresh senders here.
    batch.clear();
    let mut wh = write_half.lock().await;
    while let Ok(frame) = rx.try_recv() {
        batch.push(frame);
        if batch.len() >= max_batch {
            if write_framed_batch(&mut *wh, &batch).await.is_err() {
                cancel.cancel();
                let _ = wh.shutdown().await;
                return;
            }
            batch.clear();
        }
    }
    if !batch.is_empty() {
        let _ = write_framed_batch(&mut *wh, &batch).await;
    }

    // Wake the reader and send a graceful TCP FIN.
    cancel.cancel();
    let _ = wh.shutdown().await;
}
