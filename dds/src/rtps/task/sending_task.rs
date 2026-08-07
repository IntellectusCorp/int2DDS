use std::sync::{Arc, Mutex, MutexGuard, Weak};

use mio::{Events, Poll, Token, Waker};

use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::sedp_logic::SedpLogic;
use crate::rtps::logic::spdp_logic::SpdpLogic;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::task::sending_handler::MessageType;
use crate::rtps::transport::socket::MAX_EVENTS;

/// Locks the send queue, recovering from poisoning instead of failing on it.
///
/// Both the producer and the consumer went through `Mutex::lock`/`try_lock` directly and had
/// no way back from a poisoned queue. `try_lock` in particular cannot tell `WouldBlock` from
/// `Poisoned`, and poisoning is permanent, so the consumer's retry loop had no exit and spun at
/// full speed forever. (It also held the lock inside the else branch, since the
/// `Err(PoisonError<MutexGuard>)` scrutinee lives to the end of the `if let` -- but `continue`
/// is a scope exit, so it was re-taken and released each iteration rather than held.) The
/// producer fared worse: its recovery path fetched the handler for the same participant GUID,
/// which is the very handler it was called on, and re-locked a mutex the thread already held --
/// `PoisonError` owns the guard, so the error arm never released it. That one deadlocked
/// immediately, and the guard it leaked is what starved the consumer.
///
/// Recovering is safe here. The queue is a plain `Vec` of owned messages, so a producer that
/// unwound mid-push leaves it structurally sound and a partially-pushed message is safe to
/// observe. `into_inner` matches the recovery already used in `time_based_filter.rs` and
/// `tcp_sender.rs`.
///
/// The returned guard must be released before dispatching anything: `message_queue` is a leaf
/// lock, and holding it across `create_worker_task` would invert the order the receive thread
/// takes, which acquires `reader_proxies` and then wants this queue.
pub(crate) fn lock_message_queue(
    queue: &Mutex<Vec<MessageType>>,
) -> MutexGuard<'_, Vec<MessageType>> {
    queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) struct SendingTask {
    participant: Weak<Participant>,
    spdp_logic: Arc<Option<SpdpLogic>>,
    sedp_logic: Arc<Option<SedpLogic>>,
    user_logic: Arc<Option<UserLogic>>,
    poll: Poll,
    events: Events,
    waker: Arc<Waker>,
}

impl SendingTask {
    pub(crate) fn new(participant: Arc<Participant>, port: u16) -> Self {
        let (spdp_logic, sedp_logic, user_logic) = participant.get_logics();
        let poll = Poll::new().unwrap();
        let events = Events::with_capacity(MAX_EVENTS);
        let sending_token = Token(port as usize);

        let waker = Arc::new(Waker::new(poll.registry(), sending_token).unwrap());
        Self {
            participant: Arc::downgrade(&participant),
            sedp_logic,
            spdp_logic,
            user_logic,
            poll,
            events,
            waker,
        }
    }

    pub(crate) fn waker(&self) -> Arc<Waker> {
        self.waker.clone()
    }

    pub(crate) fn create_worker_task(&self, message: MessageType) -> RtpsResult<()> {
        let participant = self.participant.upgrade().ok_or_else(|| {
            RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped")
        })?;
        let mut spdp_logic = self
            .spdp_logic
            .as_ref()
            .as_ref()
            .ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::DataNotSet, "SpdpLogic is not initialized")
            })?
            .clone();
        let sedp_logic = self.sedp_logic.as_ref().as_ref().ok_or_else(|| {
            RtpsError::new(RtpsErrorCode::DataNotSet, "SedpLogic is not initialized")
        })?;
        let user_logic = self.user_logic.as_ref().as_ref().ok_or_else(|| {
            RtpsError::new(RtpsErrorCode::DataNotSet, "UserLogic is not initialized")
        })?;

        match message {
            MessageType::P2pData(start_time, duration, participant_message_data) => {
                if let Some(wlp_logic) = participant.wlp_logic() {
                    wlp_logic.send_participant_message_data(
                        start_time,
                        duration,
                        participant_message_data,
                    )?;
                }

                Ok(())
            }

            MessageType::P2pHeartbeat(target_guid_prefix) => {
                if let Some(wlp_logic) = participant.wlp_logic() {
                    wlp_logic.send_liveliness_heartbeat(false, false, None, target_guid_prefix)?;
                }

                Ok(())
            }

            MessageType::PeriodicParticipantDataMulticast(
                start_time,
                duration,
                domain_id,
                data,
            ) => {
                spdp_logic.send_periodic_participant_data_multicast(
                    start_time, duration, domain_id, data,
                )?;
                Ok(())
            }

            MessageType::PeriodicParticipantDataUnicast(
                start_time,
                duration,
                spdp_discovered_participant_data,
                data,
            ) => sedp_logic.send_periodic_participant_data_unicast(
                start_time,
                duration,
                spdp_discovered_participant_data,
                data,
            ),

            MessageType::PeriodicPublicationHeartbeat(start_time, duration, guid_prefix) => {
                sedp_logic.send_sedp_periodic_heartbeat_message(
                    start_time,
                    duration,
                    guid_prefix,
                    EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
                )?;
                Ok(())
            }

            MessageType::PeriodicSubscriptionHeartbeat(start_time, duration, guid_prefix) => {
                sedp_logic.send_sedp_periodic_heartbeat_message(
                    start_time,
                    duration,
                    guid_prefix,
                    EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
                )?;
                Ok(())
            }

            // MessageType::PeriodicSedpTopicHeartbeat(start_time, duration, guid_prefix) => {
            //     sedp_logic.send_sedp_periodic_heartbeat_message(
            //         start_time,
            //         duration,
            //         guid_prefix,
            //         EntityId::SEDP_BUILTIN_TOPICS_WRITER,
            //     )?;
            //     Ok(())
            // }

            // MessageType::SedpTerminateEndpoint(builtin_writer_guid, cache_change) => {
            //     sedp_logic.send_endpoint_termination_message(builtin_writer_guid, cache_change)?;
            //     Ok(())
            // }
            MessageType::UserHeartbeatToAll(entity_id) => {
                user_logic.send_heartbeat_to_anonymous_matched_readers(entity_id)?;
                Ok(())
            }

            MessageType::UserHeartbeatToOne(
                writer_entity_id,
                remote_reader_guid,
                is_preemptive,
            ) => {
                user_logic.send_heartbeat_to_a_reader_proxy(
                    writer_entity_id,
                    remote_reader_guid,
                    is_preemptive,
                )?;
                Ok(())
            }

            // MessageType::UserUnsentChanges(writer_entity_id) => {
            //     user_logic.send_unsent_changes(writer_entity_id)?;
            //     Ok(())
            // }
            MessageType::UserRequestedChanges(writer_entity_id, remote_reader_guid) => {
                user_logic.send_requested_changes(writer_entity_id, remote_reader_guid)?;
                Ok(())
            }

            MessageType::UserAcknack(reader_id, remote_writer_guid, final_flag, is_preemptive) => {
                if is_preemptive {
                    user_logic.send_preemptive_acknack(reader_id, remote_writer_guid)?;
                } else {
                    user_logic.send_acknack(reader_id, remote_writer_guid, final_flag)?;
                }
                Ok(())
            }

            MessageType::OnUserCacheChangeRemoval(is_writer, sequence_number, entity_id) => {
                if is_writer {
                    user_logic.on_writer_cache_change_removal(entity_id, sequence_number)?;
                } else {
                    // user_logic
                    //     .on_reader_cache_change_removal(entity_id, sequence_number);
                }
                Ok(())
            }
        }
    }

    pub(crate) fn event_loop(&mut self, queue: Arc<Mutex<Vec<MessageType>>>) -> RtpsResult<()> {
        log::info!("start sending task thread");

        let participant = self.participant.upgrade().ok_or_else(|| {
            RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped")
        })?;

        loop {
            match self.poll.poll(&mut self.events, Some(core::time::Duration::from_secs(1))) {
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    log::warn!("Poll interrupted in sending task, continuing: {}", e);
                }
                Err(e) => {
                    return Err(RtpsError::new(RtpsErrorCode::Io, format!("poll error: {}", e)));
                }
                Ok(()) => {}
            }

            if participant.is_terminated() {
                log::debug!("Detected global termination flag, exiting sending handler loop");
                return Ok(());
            }

            // Take everything at once, release, then dispatch. The release is load-bearing:
            // holding the queue across `create_worker_task` would invert the lock order the
            // receive thread uses. There is no retry loop any more, so the outer
            // `is_terminated()` check above stays reachable on every pass.
            let messages: Vec<MessageType> = std::mem::take(&mut *lock_message_queue(&queue));

            for message in messages {
                let _ = self.create_worker_task(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration as StdDuration;

    /// The old consumer used `try_lock` and `continue`d on any error. This is the property that
    /// made that an infinite loop: poisoning is permanent and indistinguishable from
    /// contention, so the else branch was taken on every iteration forever.
    #[test]
    fn try_lock_cannot_recover_from_poisoning() {
        let queue: Arc<Mutex<Vec<MessageType>>> = Arc::new(Mutex::new(Vec::new()));
        poison(&queue);

        for _ in 0..3 {
            assert!(
                matches!(queue.try_lock(), Err(std::sync::TryLockError::Poisoned(_))),
                "try_lock recovered from poisoning, so the old spin loop had an exit after all"
            );
        }
    }

    /// Draining a poisoned queue must return, and must return the queued messages rather than
    /// dropping them. Bounded on a worker thread because the pre-fix code never returned here.
    #[test]
    fn draining_a_poisoned_queue_returns_its_messages() {
        let queue: Arc<Mutex<Vec<MessageType>>> = Arc::new(Mutex::new(Vec::new()));
        lock_message_queue(&queue).push(MessageType::P2pHeartbeat(None));
        poison(&queue);
        assert!(queue.is_poisoned(), "the queue under test was not actually poisoned");

        let (tx, rx) = mpsc::sync_channel::<usize>(1);
        let drained = Arc::clone(&queue);
        std::thread::Builder::new()
            .name("drain-poisoned-queue".into())
            .spawn(move || {
                let messages: Vec<MessageType> = std::mem::take(&mut *lock_message_queue(&drained));
                let _ = tx.send(messages.len());
            })
            .expect("failed to spawn the drain thread");

        let drained_count = rx
            .recv_timeout(StdDuration::from_secs(5))
            .expect("draining a poisoned queue never returned");
        assert_eq!(drained_count, 1, "the queued message was lost on the recovery path");
        assert!(lock_message_queue(&queue).is_empty(), "the queue was not actually drained");
    }

    /// The producer must still enqueue after poisoning. Its old recovery path looked the
    /// handler up by participant GUID, got itself back, and re-locked a mutex it already held.
    #[test]
    fn pushing_to_a_poisoned_queue_still_enqueues() {
        let queue: Arc<Mutex<Vec<MessageType>>> = Arc::new(Mutex::new(Vec::new()));
        poison(&queue);

        let (tx, rx) = mpsc::sync_channel::<()>(1);
        let pushed = Arc::clone(&queue);
        std::thread::Builder::new()
            .name("push-poisoned-queue".into())
            .spawn(move || {
                lock_message_queue(&pushed).push(MessageType::P2pHeartbeat(None));
                let _ = tx.send(());
            })
            .expect("failed to spawn the push thread");

        rx.recv_timeout(StdDuration::from_secs(5))
            .expect("pushing to a poisoned queue never returned");
        assert_eq!(lock_message_queue(&queue).len(), 1, "the message never reached the queue");
    }

    /// Poisons `queue` by unwinding while its guard is held.
    fn poison(queue: &Arc<Mutex<Vec<MessageType>>>) {
        let victim = Arc::clone(queue);
        let _ = std::thread::Builder::new()
            .name("poisoner".into())
            .spawn(move || {
                let _guard = victim.lock().unwrap();
                panic!("deliberate panic to poison the queue under test");
            })
            .expect("failed to spawn the poisoning thread")
            .join();
    }
}
