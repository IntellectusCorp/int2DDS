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
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::rtps::transport::tcp_async::{
    framing::{read_framed_message, write_framed_message},
    mux_state::{ConnectionId, MuxState},
    stream::{AsyncConnReadHalf, AsyncConnStream, AsyncConnWriteHalf},
};

/// Per-connection writer inbox capacity for **sender-initiated (outbound)**
/// connections. Tunes producer-side backpressure when paired with
/// `INT2DDS_TCP_SEND_MODE=blocking`: a full inbox makes `blocking_send`
/// park the caller until the writer task drains a slot, or makes `try_send`
/// return `Full` so the caller drops the frame.
///
/// Overridable via `INT2DDS_TCP_OUTBOUND_INBOX_CAPACITY`. Default 4
/// Read once via `OnceLock` so the value is stable across all connections
/// created in a single run.
static OUTBOUND_INBOX_CAPACITY_CACHE: OnceLock<usize> = OnceLock::new();

pub(crate) fn outbound_inbox_capacity() -> usize {
    *OUTBOUND_INBOX_CAPACITY_CACHE.get_or_init(|| {
        let cap = crate::common::env::get_tcp_outbound_inbox_capacity();
        log::info!("[tcp_async] OUTBOUND_INBOX_CAPACITY = {}", cap);
        cap
    })
}

/// Per-connection writer inbox capacity for **listener-accepted (inbound)**
/// connections. Carries control responses (acks, keepalive replies) sent
/// back over the inbound socket. Rarely the bottleneck for RTPS traffic —
/// keep generous unless deliberately experimenting.
///
/// Overridable via `INT2DDS_TCP_INBOUND_INBOX_CAPACITY`. Default 512.
static INBOUND_INBOX_CAPACITY_CACHE: OnceLock<usize> = OnceLock::new();

pub(crate) fn inbound_inbox_capacity() -> usize {
    *INBOUND_INBOX_CAPACITY_CACHE.get_or_init(|| {
        let cap = crate::common::env::get_tcp_inbound_inbox_capacity();
        log::info!("[tcp_async] INBOUND_INBOX_CAPACITY = {}", cap);
        cap
    })
}

/// Spawn the reader/writer task pair for one connection.
///
/// `tx` is the outbound inbox passed in by the caller; it has already been
/// stored in `MuxState::ConnectionEntry::writer_tx` (so dispatched-into-here
/// frames and externally-sent frames all converge on the same writer task).
///
/// — protocol acks, keepalives, RTPS data — push frames into it; the writer
/// task serialises them onto the wire in FIFO order.
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
) {
    // Child token: cancelling this pair does not affect siblings, but
    // parent shutdown still propagates down to both tasks.
    let conn_cancel = parent_cancel.child_token();

    let (read_half, write_half) = stream.into_split();

    // Reader task — owns the read half. The `writer_tx` clone lets the
    // reader push protocol responses (acks, replies) through the writer
    // pipeline so all outbound traffic stays serialised on one task.
    {
        let cancel = conn_cancel.clone();
        let shared = shared.clone();
        tokio::spawn(reader_task(read_half, conn_id, shared, tx, cancel));
    }

    // Writer task — owns the write half and the inbox receiver.
    {
        let cancel = conn_cancel.clone();
        tokio::spawn(writer_task(write_half, rx, cancel));
    }
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

/// Drain the inbox channel and write each frame to the stream.
///
/// Exits when the channel closes (all senders dropped), on a write error,
/// or on cancellation. Sends a TCP FIN via `shutdown()` before returning
/// for a graceful close.
async fn writer_task(
    mut write_half: AsyncConnWriteHalf,
    mut rx: mpsc::Receiver<Vec<u8>>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            maybe_frame = rx.recv() => {
                match maybe_frame {
                    Some(frame) => {
                        if let Err(e) = write_framed_message(&mut write_half, &frame).await {
                            // Real I/O failure — peer likely unreachable.
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

    // Drain any frames the inbox already holds before sending FIN — e.g. the
    // idle-timeout Error pushed by `prune_idle_connections` immediately
    // before cancel, which would otherwise be lost to the `select!` race.
    // `try_recv` only: we don't await fresh senders here.
    while let Ok(frame) = rx.try_recv() {
        if write_framed_message(&mut write_half, &frame).await.is_err() {
            break;
        }
    }

    // Wake the reader and send a graceful TCP FIN.
    cancel.cancel();
    let _ = write_half.shutdown().await;
}
