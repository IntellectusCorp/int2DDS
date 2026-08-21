//! Outbound writer admission and connect-window backlog.
//!
//! The persistent writer exclusively owns the socket write half, so socket I/O
//! needs no mutex. A one-permit semaphore retains the old send-deadline
//! semantics: the permit is released only after the previous frame has reached
//! the socket (or failed).

use std::collections::VecDeque;
use std::io;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex, OwnedSemaphorePermit, Semaphore};

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::tcp::framing::TcpFrameKind;

pub(crate) const CONNECT_BUFFER_DEPTH: usize = 256;
pub(crate) const CONNECT_BUFFER_BYTE_CAP: usize = 64 * 1024 * 1024;
const WRITER_CHANNEL_CAPACITY: usize = 1;

pub(crate) struct PendingFrame {
    pub(crate) kind: TcpFrameKind,
    pub(crate) payload: Vec<u8>,
}

pub(crate) struct WriterCommand {
    pub(crate) frame: PendingFrame,
    /// Held through the complete write. Dropping it wakes the next sender.
    pub(crate) _permit: OwnedSemaphorePermit,
}

#[derive(Clone)]
pub(crate) struct ReadyWrite {
    pub(crate) tx: mpsc::Sender<WriterCommand>,
    pub(crate) admission: Arc<Semaphore>,
}

pub(crate) enum WriteState {
    Connecting { backlog: VecDeque<PendingFrame>, backlog_bytes: usize },
    Ready(ReadyWrite),
    Failed,
}

pub(crate) type SharedWriteState = Arc<Mutex<WriteState>>;

pub(crate) fn init_connection_state() -> SharedWriteState {
    Arc::new(Mutex::new(WriteState::Connecting { backlog: VecDeque::new(), backlog_bytes: 0 }))
}

pub(crate) fn push_connecting(
    backlog: &mut VecDeque<PendingFrame>,
    backlog_bytes: &mut usize,
    frame: PendingFrame,
) -> io::Result<()> {
    let next_bytes = backlog_bytes.checked_add(frame.payload.len()).ok_or_else(|| {
        transport_io_error(TransportErrorCode::TcpConnectBufferFull, "connect backlog overflow")
    })?;
    if backlog.len() >= CONNECT_BUFFER_DEPTH || next_bytes > CONNECT_BUFFER_BYTE_CAP {
        return Err(transport_io_error(
            TransportErrorCode::TcpConnectBufferFull,
            format!(
                "connect backlog full: {} frames/{} bytes (limits {}/{})",
                backlog.len(),
                *backlog_bytes,
                CONNECT_BUFFER_DEPTH,
                CONNECT_BUFFER_BYTE_CAP
            ),
        ));
    }
    *backlog_bytes = next_bytes;
    backlog.push_back(frame);
    Ok(())
}

/// Publish the live writer admission handle and move the connect backlog to the
/// new persistent writer. New sends can queue at most one live frame while the
/// writer drains the older backlog, preserving order and bounded memory.
pub(crate) async fn install_writer(
    state: &SharedWriteState,
) -> io::Result<(VecDeque<PendingFrame>, mpsc::Receiver<WriterCommand>)> {
    let mut guard = state.lock().await;
    let old = std::mem::replace(&mut *guard, WriteState::Failed);
    let WriteState::Connecting { backlog, .. } = old else {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "writer installed after connection left Connecting state",
        ));
    };

    let (tx, rx) = mpsc::channel(WRITER_CHANNEL_CAPACITY);
    *guard = WriteState::Ready(ReadyWrite { tx, admission: Arc::new(Semaphore::new(1)) });
    Ok((backlog, rx))
}

pub(crate) async fn mark_failed(state: &SharedWriteState) {
    *state.lock().await = WriteState::Failed;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_backlog_has_count_and_byte_bounds() {
        let mut backlog = VecDeque::new();
        let mut bytes = 0;
        for _ in 0..CONNECT_BUFFER_DEPTH {
            push_connecting(
                &mut backlog,
                &mut bytes,
                PendingFrame { kind: TcpFrameKind::Discovery, payload: vec![0] },
            )
            .unwrap();
        }
        assert!(push_connecting(
            &mut backlog,
            &mut bytes,
            PendingFrame { kind: TcpFrameKind::Discovery, payload: vec![0] },
        )
        .is_err());

        let mut backlog = VecDeque::new();
        let mut bytes = CONNECT_BUFFER_BYTE_CAP;
        assert!(push_connecting(
            &mut backlog,
            &mut bytes,
            PendingFrame { kind: TcpFrameKind::UserData, payload: vec![0] },
        )
        .is_err());
    }
}
