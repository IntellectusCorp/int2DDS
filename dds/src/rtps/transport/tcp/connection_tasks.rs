//! Long-lived per-connection read and write loops.

use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::debug;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::rtps::transport::tcp::connection_registry::{ConnectionId, ConnectionRegistry};
use crate::rtps::transport::tcp::framing::{read_framed_message_pooled, write_framed_message};
use crate::rtps::transport::tcp::stream::{AsyncConnReadHalf, AsyncConnWriteHalf};
use crate::rtps::transport::tcp::write_state::{
    mark_failed, ConnHealth, PendingFrame, SharedWriteState, WriterCommand,
};

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

async fn reader_task(
    mut read_half: AsyncConnReadHalf,
    conn_id: ConnectionId,
    shared: Arc<ConnectionRegistry>,
    cancel: CancellationToken,
    first_frame_timeout: Option<Duration>,
) {
    let mut first_frame_timeout = first_frame_timeout;
    loop {
        let frame = tokio::select! {
            result = async {
                match first_frame_timeout.take() {
                    Some(duration) => {
                        tokio::time::timeout(
                            duration,
                            read_framed_message_pooled(&mut read_half, shared.buffer_pool()),
                        )
                        .await
                        .map_err(|_| {
                            io::Error::new(io::ErrorKind::TimedOut, "first TCP frame timed out")
                        })?
                    }
                    None => {
                        read_framed_message_pooled(&mut read_half, shared.buffer_pool()).await
                    }
                }
            } => match result {
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

        tokio::select! {
            _ = shared.route_frame(conn_id, frame) => {}
            _ = cancel.cancelled() => break,
        }
    }

    cancel.cancel();
    shared.remove_connection(conn_id);
}

/// Run the persistent writer for one outbound connection.
///
/// Connection selection, admission, and statistics remain in `TcpSender`; this
/// loop owns only ordered socket I/O and the terminal state transition.
pub(crate) async fn writer_task(
    mut write_half: AsyncConnWriteHalf,
    mut backlog: VecDeque<PendingFrame>,
    mut receiver: mpsc::Receiver<WriterCommand>,
    write_state: SharedWriteState,
    health: Arc<ConnHealth>,
    send_deadline: Option<Duration>,
    cancel: CancellationToken,
) -> io::Result<()> {
    let result = async {
        while let Some(frame) = backlog.pop_front() {
            if cancel.is_cancelled() {
                return Ok(());
            }
            write_framed_message(&mut write_half, frame.kind, &frame.payload).await?;
        }

        loop {
            let command = tokio::select! {
                command = receiver.recv() => match command {
                    Some(command) => command,
                    None => return Ok(()),
                },
                _ = cancel.cancelled() => return Ok(()),
            };

            let started = Instant::now();
            write_framed_message(&mut write_half, command.frame.kind, &command.frame.payload)
                .await?;
            if send_deadline.is_some_and(|deadline| started.elapsed() <= deadline) {
                health.on_success();
            }
            // `command` and its admission permit drop here, after the write.
        }
    }
    .await;

    let _ = write_half.shutdown().await;
    mark_failed(&write_state).await;
    cancel.cancel();
    result
}
