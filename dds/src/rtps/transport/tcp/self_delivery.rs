//! Re-entrant queue for frames a participant addresses to itself.
//!
//! The thread that decodes RTPS messages is also the only consumer of the
//! transport's inbound channel. A self-addressed frame raised while that thread
//! is inside message processing therefore cannot go onto that channel: the only
//! party that could drain it is the thread that would be blocking on it.

use std::cell::RefCell;
use std::collections::VecDeque;

use log::warn;

use crate::rtps::transport::error::TransportErrorCode;
use crate::rtps::transport::plugin::IncomingMessage;

const PENDING_BYTE_CAP: usize = 64 * 1024 * 1024;

const PENDING_COUNT_BACKSTOP: usize = 65_536;

struct PendingQueue {
    queue: VecDeque<IncomingMessage>,
    bytes: usize,
}

thread_local! {
    static PENDING: RefCell<Option<PendingQueue>> = RefCell::new(None);
}

pub(crate) fn install() {
    PENDING.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(PendingQueue { queue: VecDeque::new(), bytes: 0 });
        }
    });
}

pub(crate) fn push(msg: IncomingMessage) -> Option<IncomingMessage> {
    PENDING.with(|cell| {
        let mut slot = cell.borrow_mut();
        let pending = match slot.as_mut() {
            Some(pending) => pending,
            None => return Some(msg),
        };

        let len = msg.data.len();
        if pending.bytes + len > PENDING_BYTE_CAP || pending.queue.len() >= PENDING_COUNT_BACKSTOP {
            warn!(
                "TcpSender [{}]: intra-participant queue over backstop ({} frames, {} bytes), \
                 dropping a {} byte frame",
                TransportErrorCode::TcpChannelFull,
                pending.queue.len(),
                pending.bytes,
                len
            );
            return None;
        }

        pending.bytes += len;
        pending.queue.push_back(msg);
        None
    })
}

pub(crate) fn drain(mut f: impl FnMut(IncomingMessage)) {
    loop {
        let next = PENDING.with(|cell| {
            let mut slot = cell.borrow_mut();
            let pending = slot.as_mut()?;
            let msg = pending.queue.pop_front()?;
            pending.bytes -= msg.data.len();
            Some(msg)
        });

        match next {
            Some(msg) => f(msg),
            None => break,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(byte: u8) -> IncomingMessage {
        IncomingMessage { data: vec![byte], source: "127.0.0.1:7400".parse().unwrap() }
    }

    /// Each test runs on a thread of its own: the queue is thread-local, so a
    /// queue installed elsewhere must not decide the outcome here.
    fn on_a_fresh_thread(body: impl FnOnce() + Send + 'static) {
        std::thread::spawn(body).join().expect("test thread panicked");
    }

    /// A drain settles the frames it queues itself and ends empty. Refills come
    /// only from the thread running the drain, so they cannot outrun it.
    #[test]
    fn drain_settles_frames_queued_while_draining() {
        on_a_fresh_thread(|| {
            install();
            assert!(push(frame(0)).is_none());

            let mut settled = Vec::new();
            drain(|msg| {
                let byte = msg.data[0];
                settled.push(byte);
                if byte < 3 {
                    assert!(push(frame(byte + 1)).is_none());
                }
            });
            assert_eq!(settled, vec![0, 1, 2, 3]);

            let mut leftover = 0;
            drain(|_| leftover += 1);
            assert_eq!(leftover, 0);
        });
    }

    /// Without a queue the frame comes back to the caller — that is what keeps
    /// every other thread on the inbound channel.
    #[test]
    fn push_hands_the_frame_back_when_no_queue_is_installed() {
        on_a_fresh_thread(|| {
            assert!(push(frame(7)).is_some());
        });
    }
}
