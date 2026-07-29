//! The write side of a **data** connection — the shared handle over its write
//! half that the send path and the reaper both reach through.
//!
//! Only data connections use this. A control (or inbound) connection has a
//! single writer, its `writer_task`, which owns its write half outright and
//! needs no shared state. A data connection is the one with two parties on the
//! half — the sync send path (`TcpSender::write_frame`, user/discovery data) and
//! the reaper's [`graceful_close`] — so its half lives inside an
//! `Arc<Mutex<WriteState>>` and both reach it only by taking the lock.
//!
//! That one lock does three jobs at once:
//!
//! 1. Serialize the two writers — two overlapping `write_framed_message` calls
//!    would interleave bytes and corrupt framing (a send racing the close).
//! 2. Admission control — a frame is written by a task that owns the guard for
//!    the whole write, so at most one write per connection is ever in flight. A
//!    sender that cannot take the lock is exactly a sender whose previous frame
//!    has not reached the wire yet: real backpressure with no intermediate queue.
//! 3. Measurement — for the same reason, how long the lock takes to acquire is
//!    how long the previous frame has been stuck.
//!
//! Before the handshake completes the state is `Connecting` and frames are
//! buffered instead, then flushed in order by [`flush_and_ready`].

use std::collections::VecDeque;
use std::io;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as TokioMutex;

use crate::rtps::transport::tcp::framing::write_framed_message;
use crate::rtps::transport::tcp::stream::{AsyncConnReadHalf, AsyncConnStream, AsyncConnWriteHalf};

/// Write side of a data connection, shared between the sync-send path and the
/// reaper's graceful close.
pub(crate) enum WriteState {
    /// Handshake not finished yet: discovery/user-data frames are buffered here
    /// for a moment and flushed in order once the connection becomes
    /// [`WriteState::Ready`].
    Connecting(VecDeque<Vec<u8>>),
    /// Live write half — writes go straight to the wire.
    Ready(WriteHandoff),
}

/// The live write half, plus the reusable staging buffer the send path hands off
/// to write user/discovery frames.
///
/// `data` copies the frame in while the state lock is held, then the whole owned
/// guard moves into the task that runs the write. That hands the write owned
/// bytes — required to drive it to completion off the caller's thread — and
/// reuses one buffer instead of allocating per frame.
pub(crate) struct WriteHandoff {
    pub(crate) half: AsyncConnWriteHalf,
    /// Staging buffer for the send handoff, reused across frames. Only
    /// meaningful while a write is in flight; the lock makes that at most one
    /// at a time.
    pub(crate) data: Vec<u8>,
}

pub(crate) type SharedWriteState = Arc<TokioMutex<WriteState>>;

/// A fresh write state that buffers sends until the connection is ready.
pub(crate) fn init_connection_state() -> SharedWriteState {
    Arc::new(TokioMutex::new(WriteState::Connecting(VecDeque::new())))
}

/// Split the stream, flush the frames buffered during the connect (in order),
/// and transition `write_state` to `Ready`. Returns the read half for the
/// caller to hand to the reader task.
///
/// All of it happens under the state lock, so a direct write cannot overtake a
/// buffered frame. A flush error aborts here — before any registration — so the
/// connect is treated as failed.
pub(crate) async fn flush_and_ready(
    stream: AsyncConnStream,
    write_state: &SharedWriteState,
) -> io::Result<AsyncConnReadHalf> {
    let (read_half, mut write_half) = stream.into_split();
    let mut st = write_state.lock().await;
    if let WriteState::Connecting(buf) = &mut *st {
        while let Some(frame) = buf.pop_front() {
            write_framed_message(&mut write_half, &frame).await?;
        }
    }
    *st = WriteState::Ready(WriteHandoff { half: write_half, data: Vec::new() });
    Ok(read_half)
}

/// Send the connection's write half a graceful close — a TCP FIN, and for TLS a
/// `close_notify` alert.
///
/// This is the close path for a connection with no writer task: an outbound data
/// connection, whose `reader_task` never writes, so the sender's reaper calls
/// this when the connection's token fires. (A connection that has a writer task
/// closes from there instead.) It matters most for TLS: tokio-rustls emits
/// `close_notify` only from `poll_shutdown` and has no `Drop`, so merely dropping
/// the write half would close the TCP socket without it and the peer would see an
/// unclean shutdown.
///
/// Taking the state lock first serialises against any in-flight send, so the
/// frame currently on the wire is written to completion — never truncated —
/// before the half is shut. A `Connecting` state means the stream never reached
/// `Ready` (the connect died mid-handshake); there is nothing to close.
pub(crate) async fn graceful_close(write_state: &SharedWriteState) {
    let mut st = write_state.lock().await;
    if let WriteState::Ready(r) = &mut *st {
        let _ = r.half.shutdown().await;
    }
}
