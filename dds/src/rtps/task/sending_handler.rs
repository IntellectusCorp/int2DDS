use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread;
use std::time::{Duration as StdDuration, Instant};

use log::{debug, error};
use mio::Waker;

use crate::rtps::builtin::data::participant_message_data::ParticipantMessageData;
use crate::rtps::builtin::data::spdp_discovered_participant_data::SPDPDiscoveredParticipantData;
use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::common::sequence::SequenceNumber;
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::history::cache_change::CacheChange;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::wlp_logic::WlpLogic;
use crate::rtps::task::sending_task::SendingTask;
use crate::rtps::transport::TransportSender;

#[derive(Debug, Clone)]
pub(crate) enum MessageType {
    // WLP
    P2pData(Option<Instant>, StdDuration, Arc<ParticipantMessageData>),
    P2pHeartbeat(Option<GuidPrefix>),

    // Discovery traffic
    PeriodicParticipantDataMulticast(Option<Instant>, StdDuration, DomainId, Option<Arc<Vec<u8>>>),
    PeriodicParticipantDataUnicast(
        Option<Instant>,
        StdDuration,
        Arc<SPDPDiscoveredParticipantData>,
        Option<Arc<Vec<u8>>>,
    ),
    PeriodicPublicationHeartbeat(Option<Instant>, StdDuration, Arc<GuidPrefix>),
    PeriodicSubscriptionHeartbeat(Option<Instant>, StdDuration, Arc<GuidPrefix>),
    PeriodicSedpTopicHeartbeat(Option<Instant>, StdDuration, Arc<GuidPrefix>),
    SedpTerminateEndpoint(Guid, Arc<CacheChange>),

    // User traffic
    UserHeartbeatToOne(EntityId, Guid, bool),
    UserHeartbeatToAll(EntityId),
    UserUnsentChanges(EntityId),
    UserRequestedChanges(EntityId, Guid),
    UserPreemptiveAcknack(EntityId, Guid),
    OnUserCacheChangeRemoval(bool, SequenceNumber, EntityId),
}

pub(crate) static INSTANCE: OnceLock<Mutex<HashMap<Guid, Arc<SendingHandler>>>> = OnceLock::new();

pub(crate) struct SendingHandler {
    // Immutable fields - no lock needed
    participant: Weak<Participant>,
    udp_sender: Mutex<Option<Arc<TransportSender>>>,
    tcp_sender: Mutex<Option<Arc<TransportSender>>>,

    // Mutable fields - use interior mutability
    sending_task: Mutex<Option<Arc<Mutex<SendingTask>>>>,
    sending_thread_join_handle: Mutex<Option<thread::JoinHandle<()>>>,

    // Already has own lock
    message_queue: Arc<Mutex<Vec<MessageType>>>,

    // Thread-safe via Arc - no lock needed
    waker: Mutex<Option<Arc<Waker>>>,
}

impl SendingHandler {
    fn new(
        participant: Arc<Participant>,
        udp_sender: Option<Arc<TransportSender>>,
        tcp_sender: Option<Arc<TransportSender>>,
    ) -> Self {
        Self {
            participant: Arc::downgrade(&participant),
            udp_sender: Mutex::new(udp_sender),
            tcp_sender: Mutex::new(tcp_sender),
            sending_task: Mutex::new(None),
            sending_thread_join_handle: Mutex::new(None),
            waker: Mutex::new(None),
            message_queue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn get_instance(
        participant: Arc<Participant>,
        udp_sender: Option<Arc<TransportSender>>,
        tcp_sender: Option<Arc<TransportSender>>,
    ) -> Arc<SendingHandler> {
        let map_mutex = INSTANCE.get_or_init(|| Mutex::new(HashMap::new()));

        if let Ok(map_guard) = map_mutex.lock() {
            if let Some(handler) = map_guard.get(&participant.guid()) {
                return handler.clone();
            }
        } else {
            panic!("Failed to acquire sending handler map lock");
        }

        let new_handler = SendingHandler::new(participant.clone(), udp_sender, tcp_sender);
        new_handler.spawn_event_loop();
        let handler_arc = Arc::new(new_handler);

        let mut map_guard = INSTANCE
            .get()
            .expect("Sending handler map should be initialized")
            .lock()
            .expect("Failed to acquire sending handler map lock");
        let entry = map_guard.entry(participant.guid()).or_insert_with(|| handler_arc.clone());
        entry.clone()
    }

    pub(crate) fn get_instance_by_participant_guid(
        participant_guid: Guid,
    ) -> Option<Arc<SendingHandler>> {
        let map_mutex = INSTANCE.get()?;
        match map_mutex.lock() {
            Ok(map_guard) => map_guard.get(&participant_guid).cloned(),
            Err(_) => None,
        }
    }

    // Create event loop thread
    fn spawn_event_loop(&self) {
        let mut sending_task_guard = self.sending_task.lock().expect("Failed to lock sending_task");
        if sending_task_guard.is_none() {
            let udp_sender = self.udp_sender.lock().ok().and_then(|g| g.clone());
            let tcp_sender = self.tcp_sender.lock().ok().and_then(|g| g.clone());

            let sending_task = SendingTask::new(
                self.participant.upgrade().expect("Participant already dropped"),
                udp_sender,
                tcp_sender,
            );
            let waker = sending_task.waker();
            *sending_task_guard = Some(Arc::new(Mutex::new(sending_task)));

            // Update waker
            let mut waker_guard = self.waker.lock().expect("Failed to lock waker");
            *waker_guard = Some(waker);
            drop(waker_guard);
        }

        let mut handle_guard = self
            .sending_thread_join_handle
            .lock()
            .expect("Failed to lock sending_thread_join_handle");
        if handle_guard.is_none() {
            match sending_task_guard.clone() {
                Some(sending_task) => {
                    let sending_task_clone = sending_task.clone();
                    let message_queue_clone = self.message_queue.clone();
                    let participant_guid =
                        self.participant.upgrade().expect("Participant already dropped").guid();
                    let handle = thread::Builder::new()
                        .name("sending task thread".to_string())
                        .spawn(move || {
                            // Register thread name for monitoring
                            {
                                use crate::rtps::task::thread_monitor::ThreadMonitor;
                                ThreadMonitor::register_current_thread_name_with_guid(
                                    "sending task thread",
                                    &participant_guid,
                                );
                            }

                            if let Err(e) =
                                sending_task_clone.lock().unwrap().event_loop(message_queue_clone)
                            {
                                error!("Sending task event loop terminated with error: {:?}", e);
                            }
                            // Cleanup thread from registry before exit
                            {
                                use crate::rtps::task::thread_monitor::ThreadMonitor;
                                ThreadMonitor::remove_map_guard();
                            }
                            debug!("sending task thread finished");
                        })
                        .expect("Failed to create sending task thread");
                    *handle_guard = Some(handle);
                }
                None => {
                    panic!("sending task is not set");
                }
            }
        }
    }

    // pub(crate) fn waker(&self) -> Option<Arc<Waker>> {
    //     self.waker.clone()
    // }

    pub(crate) fn wake_event_loop(&self) {
        if let Ok(waker_guard) = self.waker.lock() {
            if let Some(waker) = waker_guard.as_ref() {
                if let Err(e) = waker.wake() {
                    error!("Failed to wake event loop: {}", e);
                }
            }
        }
    }

    // Add message to message queue
    pub(crate) fn push_message_and_wake(&self, message: MessageType) {
        match self.message_queue.lock() {
            Ok(mut queue_guard) => {
                // Deduplication: skip if SendUnsentChanges for same EntityId already exists
                if let MessageType::UserUnsentChanges(entity_id) = &message {
                    if queue_guard.iter().any(
                        |m| matches!(m, MessageType::UserUnsentChanges(eid) if eid == entity_id),
                    ) {
                        return; // Already queued, no need to add duplicate
                    }
                }
                queue_guard.push(message);
            }
            Err(e) => {
                error!("Failed to acquire message queue lock (poisoned): {}", e);
                // If the lock is poisoned, try to get a new instance and retry once
                if let Some(handler) = SendingHandler::get_instance_by_participant_guid(
                    self.participant.upgrade().expect("Participant already dropped").guid(),
                ) {
                    handler.push_message_and_wake(message);
                    return; // Early return to avoid double wake
                }
            }
        }
        self.wake_event_loop();
    }

    // Add message to message queue
    pub(crate) fn push_message(&self, message: MessageType) {
        match self.message_queue.lock() {
            Ok(mut queue_guard) => {
                queue_guard.push(message);
            }
            Err(e) => {
                error!("Failed to acquire message queue lock: {}", e);
                self.push_message_and_wake(message);
            }
        }
    }

    /// Allows direct access to SendingTask when synchronous transmission is needed instead of event loop
    pub(crate) fn get_sending_task(&self) -> Option<Arc<Mutex<SendingTask>>> {
        self.sending_task.lock().ok()?.clone()
    }

    pub(crate) fn join_sending_thread(&self) -> RtpsResult<()> {
        let mut handle_guard = self.sending_thread_join_handle.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to lock sending_thread_join_handle: {}", e),
            )
        })?;

        if let Some(handle) = handle_guard.take() {
            handle.join().map_err(|_| RtpsError::new(RtpsErrorCode::ThreadJoinError, None))?;
        }

        // Clear sending_task to release Poll and Waker file descriptors
        if let Ok(mut task_guard) = self.sending_task.lock() {
            *task_guard = None;
        }

        // Clear waker reference
        if let Ok(mut waker_guard) = self.waker.lock() {
            *waker_guard = None;
        }

        if let Ok(mut queue) = self.message_queue.lock() {
            queue.clear();
        }

        if let Ok(mut sender_guard) = self.udp_sender.lock() {
            *sender_guard = None;
        }
        if let Ok(mut sender_guard) = self.tcp_sender.lock() {
            *sender_guard = None;
        }

        Ok(())
    }

    pub(crate) fn remove_map_guard(guid: &Guid) {
        if let Some(map_mutex) = INSTANCE.get() {
            if let Ok(mut map_guard) = map_mutex.lock() {
                map_guard.remove(guid);
            }
        }
    }

    pub(crate) fn wlp_logic(&self) -> Option<WlpLogic> {
        self.participant.upgrade().expect("Participant already dropped").wlp_logic()
    }

    pub(crate) fn cancel_p2p_messages(&self) {
        if let Ok(mut queue) = self.message_queue.lock() {
            queue.retain(|msg| !matches!(msg, MessageType::P2pData(_, _, _)));
        }
    }
}
