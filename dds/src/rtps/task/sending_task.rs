use std::sync::{Arc, Mutex, Weak};

use mio::{Events, Poll, Token, Waker};

use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::sedp_logic::SedpLogic;
use crate::rtps::logic::spdp_logic::SpdpLogic;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::task::sending_handler::MessageType;
use crate::rtps::transport::plugin::TransportPlugin;
use crate::rtps::transport::socket::MAX_EVENTS;

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
    pub(crate) fn new(participant: Arc<Participant>, transport: Arc<dyn TransportPlugin>) -> Self {
        let port = transport.port();

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

            loop {
                let messages: Vec<MessageType> = if let Ok(mut queue_guard) = queue.try_lock() {
                    // Take all messages from queue at once to release lock quickly
                    std::mem::take(&mut *queue_guard)
                } else {
                    // thread::sleep(StdDuration::from_nanos(random_range(10..20)));
                    continue;
                };

                // Process messages after releasing lock (prevent deadlock)
                if !messages.is_empty() {
                    for message in messages {
                        let _ = self.create_worker_task(message);
                    }
                }
                break;
            }
        }
    }
}
