//! The loop that drains this participant's ring and hands each message to the
//! RTPS parser.

use std::sync::Arc;
use std::time::Duration;

use log::warn;

use crate::rtps::transport::shm::ring::RING_INLINE;
use crate::rtps::transport::shm::segment::OwnedSegment;

/// Bounds two things and nothing else: how long a wedged ring stays wedged
/// before a `pop` recovers it (spec §5.7), and how long this thread takes to
/// notice the runtime is gone. Normal delivery latency comes from the notifier.
pub(crate) const RECV_WAIT: Duration = Duration::from_secs(1);

/// Pops until the ring is empty, handing each message to `sink`. Returns how
/// many it delivered.
///
/// Every `pop` takes and releases the ring guard inside its own `let`
/// statement, so no guard is alive when `sink` runs or when the caller waits.
/// The shape to avoid is `if let Some(..) = ring_mut().pop(..) { .. } else {
/// wait_for_message(..) }`: the scrutinee's guard lives to the end of the
/// whole `if let`, and `wait_for_message` re-locks the same mutex -- which
/// deadlocks rather than panics.
pub(crate) fn drain<F: FnMut(&[u8])>(segment: &OwnedSegment, mut sink: F) -> usize {
    let mut buf = [0u8; RING_INLINE];
    let mut delivered = 0;
    loop {
        let popped = segment.ring_mut().pop(&mut buf);
        let Some((len, _spill)) = popped else { return delivered };
        // Spill is ignored: `send_to_peer` is the only producer and always
        // pushes `SPILL_NONE`. The payload rides in a pool slot the message
        // itself points at, not in a spill cell.
        sink(&buf[..len as usize]);
        delivered += 1;
    }
}

/// Runs until `session` returns `None`, the same shape as the sweep thread in
/// `runtime.rs`.
///
/// `session` yields the segment to drain plus a sink built fresh for that
/// round. The sink is dropped before the wait on purpose: a DDS-layer sink
/// transitively owns the transport plugin and so the `ShmRuntime` whose
/// absence is this loop's exit condition, and holding one across rounds would
/// make that condition unreachable. The segment `Arc` does have to live across
/// the wait, which defers its unlink by at most `RECV_WAIT`.
pub(crate) fn recv_loop<S, F>(mut session: S)
where
    S: FnMut() -> Option<(Arc<OwnedSegment>, F)>,
    F: FnMut(&[u8]),
{
    loop {
        let Some((segment, sink)) = session() else { return };
        // Drain first, wait second: `Ring::pop` is the only thing that
        // recovers a wedged cell (spec §5.7).
        drain(&segment, sink);
        segment.wait_for_message(RECV_WAIT);
    }
}

/// Puts [`recv_loop`] on its own thread. Failing to spawn is not fatal: the
/// ring goes unread and the send side falls back to the copy path once it
/// fills.
pub(crate) fn spawn_receiver<S, F>(session: S)
where
    S: FnMut() -> Option<(Arc<OwnedSegment>, F)> + Send + 'static,
    F: FnMut(&[u8]),
{
    let spawned = std::thread::Builder::new().name("int2dds-shm-recv".into()).spawn(move || {
        recv_loop(session);
    });
    if let Err(e) = spawned {
        warn!("[shm] no receive thread; the zero-copy ring stays unread: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::notify::notify_supported;
    use crate::rtps::transport::shm::ring::SPILL_NONE;
    use crate::rtps::transport::shm::segment::{unlink_segment, OwnedSegment, PeerSegment};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    const DOMAIN: u32 = 232;

    #[test]
    fn the_loop_drains_every_queued_message_before_waiting_again() {
        // `OwnedSegment::create` needs a notifier, so there is nothing to
        // exercise where notification is unsupported.
        if !notify_supported() {
            return;
        }
        unlink_segment(DOMAIN, 0);
        let owned = OwnedSegment::create(DOMAIN, 0, 1, &[(64, 2)], 8).unwrap();
        let peer = PeerSegment::attach(DOMAIN, 0, 1).unwrap();

        for msg in [b"one".as_slice(), b"two".as_slice(), b"three".as_slice()] {
            peer.push_and_signal(msg, SPILL_NONE).unwrap();
        }

        let mut seen: Vec<Vec<u8>> = Vec::new();
        let drained = drain(&owned, |bytes| seen.push(bytes.to_vec()));

        assert_eq!(drained, 3);
        assert_eq!(seen, vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]);

        drop(peer);
        drop(owned);
        unlink_segment(DOMAIN, 0);
    }

    #[test]
    fn the_loop_delivers_then_stops_once_the_session_is_gone() {
        if !notify_supported() {
            return;
        }
        unlink_segment(DOMAIN, 2);
        let owned = Arc::new(OwnedSegment::create(DOMAIN, 2, 1, &[(64, 2)], 8).unwrap());
        let peer = PeerSegment::attach(DOMAIN, 2, 3).unwrap();
        peer.push_and_signal(b"last", SPILL_NONE).unwrap();

        let seen = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let segment = Arc::clone(&owned);
            let seen = Arc::clone(&seen);
            let mut rounds = 0u32;
            std::thread::spawn(move || {
                recv_loop(move || {
                    rounds += 1;
                    if rounds > 1 {
                        return None;
                    }
                    let seen = Arc::clone(&seen);
                    let sink = move |bytes: &[u8]| seen.lock().unwrap().push(bytes.to_vec());
                    Some((Arc::clone(&segment), sink))
                });
                let _ = tx.send(());
            });
        }

        // Round one drains, then waits out `RECV_WAIT` on an empty ring;
        // round two exits. Holding the ring guard across that wait would
        // deadlock, and this timeout is what turns that into a failure
        // instead of a hang.
        assert!(
            rx.recv_timeout(RECV_WAIT * 8).is_ok(),
            "the loop neither drained nor exited within the wait bound"
        );
        assert_eq!(seen.lock().unwrap().as_slice(), &[b"last".to_vec()]);

        drop(peer);
        drop(owned);
        unlink_segment(DOMAIN, 2);
    }

    #[test]
    fn queued_messages_come_out_without_waiting_a_round() {
        if !notify_supported() {
            return;
        }
        unlink_segment(DOMAIN, 4);
        let owned = Arc::new(OwnedSegment::create(DOMAIN, 4, 1, &[(64, 2)], 8).unwrap());
        let peer = PeerSegment::attach(DOMAIN, 4, 5).unwrap();
        for msg in [b"one".as_slice(), b"two".as_slice(), b"three".as_slice()] {
            peer.push_and_signal(msg, SPILL_NONE).unwrap();
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let worker = {
            let segment = Arc::clone(&owned);
            let mut rounds = 0u32;
            std::thread::spawn(move || {
                recv_loop(move || {
                    rounds += 1;
                    if rounds > 1 {
                        return None;
                    }
                    let tx = tx.clone();
                    let sink = move |bytes: &[u8]| {
                        let _ = tx.send(bytes.to_vec());
                    };
                    Some((Arc::clone(&segment), sink))
                });
            })
        };

        // The round drains before it waits, so what was already queued is out
        // in a small fraction of `RECV_WAIT`.
        let budget = Duration::from_millis(200);
        let mut seen = Vec::new();
        for _ in 0..3 {
            seen.push(rx.recv_timeout(budget).expect("a queued message missed the budget"));
        }
        assert_eq!(seen, vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]);

        worker.join().unwrap();
        drop(peer);
        drop(owned);
        unlink_segment(DOMAIN, 4);
    }

    #[test]
    fn an_empty_ring_does_not_spin_the_loop() {
        if !notify_supported() {
            return;
        }
        unlink_segment(DOMAIN, 6);
        let owned = Arc::new(OwnedSegment::create(DOMAIN, 6, 1, &[(64, 2)], 8).unwrap());

        let rounds = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let segment = Arc::clone(&owned);
            let rounds = Arc::clone(&rounds);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                recv_loop(move || {
                    if stop.load(Ordering::Relaxed) {
                        return None;
                    }
                    rounds.fetch_add(1, Ordering::Relaxed);
                    Some((Arc::clone(&segment), |_: &[u8]| {}))
                });
            })
        };

        std::thread::sleep(Duration::from_millis(300));
        let spun = rounds.load(Ordering::Relaxed);
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap();

        // 300 ms is well inside one `RECV_WAIT`, so the loop should still be
        // parked in its first wait. Without the wait it runs millions.
        assert!(spun < 10, "the loop spun {spun} times on an empty ring");

        drop(owned);
        unlink_segment(DOMAIN, 6);
    }
}
