//! DCPS-RTPS bridge for connecting DDS and RTPS layers.
//!
//! This module provides the bridge between the DCPS (DDS) layer and RTPS layer,
//! managing participant lifecycles, entity creation, and message routing between
//! the two layers.

use std::sync::{Arc, RwLock, Weak};

use log::debug;

use crate::{
    common::{
        builtin::topic::{
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    infrastructure::{
        qos_policy::ReliabilityQosPolicyKind,
        status::{StatusInfo, StatusKind},
    },
    rtps::{
        builtin::data::{
            content_filtered_topic::ContentFilterProperty,
            discovered_data::{DiscoveredReaderData, DiscoveredWriterData},
        },
        common::{
            entity_id::EntityId,
            entity_kind::EntityKind,
            guid::{Guid, GuidPrefix},
            locator::Locator,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            time::{RtpsDuration, RtpsTime},
            types::{ChangeKind, DomainId, TopicKind},
        },
        entities::{
            entity::Entity,
            history::{cache_change::CacheChange, history_cache::HistoryCache},
            participant::Participant,
            reader::{Reader, StatefulReader, StatelessReader},
            writer::{StatefulWriter, StatelessWriter, Writer},
        },
        logic::{
            common::{JoinAllThread as _, UnicastThreadHandler as _},
            sedp_logic::{SedpLogic, BUILTIN_SEDP_HB_PERIOD},
            spdp_logic::SpdpLogic,
            user_logic::UserLogic,
        },
        messages::sedp_message::SEDPMessage,
        task::{
            sending_handler::{MessageType, SendingHandler},
            thread_monitor::ThreadMonitor,
        },
        transport::{
            plugin::{TransportPlugin, TransportPluginFactory},
            socket::Socket,
        },
    },
    utils::timer::timer_handler::TimerHandler,
};

pub(crate) struct DcpsBridge {
    guid_prefix: GuidPrefix,
    domain_id: DomainId,
    participant: Arc<Participant>,

    socket: Socket,

    spdp_logic: Arc<Option<SpdpLogic>>,
    sedp_logic: Arc<Option<SedpLogic>>,
    user_logic: Arc<Option<UserLogic>>,
    thread_monitor: Option<ThreadMonitor>,
}

pub(crate) static PARTICIPANTS: RwLock<Vec<Weak<Participant>>> = RwLock::new(Vec::new());

impl DcpsBridge {
    pub(crate) fn new(
        domain_id: DomainId,
        property: &crate::infrastructure::qos_policy::PropertyQosPolicy,
    ) -> RtpsResult<Self> {
        let mut socket = Socket::new(domain_id);

        let transport_type = property
            .find_property("int2dds.transport")
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(crate::rtps::transport::get_transport_type);

        // Build optional TLS config from the PropertyQosPolicy.
        let tls_config = match crate::rtps::transport::tcp::tls::TlsConfig::from_property(property)
        {
            Ok(cfg) => cfg.map(std::sync::Arc::new),
            Err(e) => panic!("Invalid TLS configuration in DomainParticipantQos: {e}"),
        };

        let participant_id = socket.participant_id();

        // For TCP/Hybrid, we need guid_prefix before creating the transport.
        // Only guid() is used from this temporary participant, so locator
        // lists are left empty.
        let guid_prefix = {
            let temp = Participant::new(
                domain_id,
                participant_id,
                socket.working_ips(),
                Vec::new(),
                Vec::new(),
            );
            temp.guid().prefix()
        };

        let bind_ip = socket.get_sender_bind_addr();
        let multicast_if_ip = socket.get_sender_multicast_if_addr();
        let working_ips: Vec<String> =
            socket.working_ips().iter().map(|ip| ip.to_string()).collect();

        let transport: Arc<dyn TransportPlugin> = Arc::from(
            TransportPluginFactory::create(
                transport_type,
                domain_id,
                participant_id,
                bind_ip,
                multicast_if_ip,
                working_ips,
                guid_prefix,
                tls_config,
                property,
            )
            .map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::Io,
                    format!("Failed to create transport plugin (domain={domain_id}): {e}"),
                )
            })?,
        );

        socket.set_transport(transport.clone());

        // Use the transport's final participant_id (may have been incremented
        // due to unicast port conflicts with other participants on the same host).
        let participant_id = transport.participant_id();

        // Ask the transport itself which locators this participant should
        // advertise over SPDP.
        let metatraffic_unicast_locators = transport.advertised_metatraffic_unicast_locators();
        let default_unicast_locators = transport.advertised_default_unicast_locators();

        let mut participant = Participant::new(
            domain_id,
            participant_id,
            socket.working_ips(),
            metatraffic_unicast_locators,
            default_unicast_locators,
        );
        participant.set_local_receive_buffer_size(transport.advertised_receive_buffer_size());
        let guid_prefix = participant.guid().prefix();
        let participant = Arc::new(participant);

        participant.init_logics(transport.clone(), property);

        let (spdp_logic, sedp_logic, user_logic) = participant.get_logics();

        let _ = SendingHandler::get_instance(participant.clone(), Some(transport.port()));

        if let Ok(mut participants) = PARTICIPANTS.write() {
            participants.push(Arc::downgrade(&participant));
        }

        Ok(Self {
            guid_prefix,
            domain_id,
            participant,
            socket,
            spdp_logic,
            sedp_logic,
            user_logic,
            thread_monitor: None,
        })
    }

    pub(crate) fn init(&mut self) -> RtpsResult<()> {
        let transport = self.socket.transport();

        // Start SEDP threads (discovery multicast + unicast listening)
        if let Some(sedp_logic) = self.sedp_logic.as_ref() {
            sedp_logic.start_sedp(
                transport.take_discovery_multicast_source(),
                transport.take_discovery_unicast_source(),
                transport.take_stream_source(),
            )?;
        } else {
            log::error!("sedp_logic is not set");
        }

        // Start SPDP threads
        if let Some(spdp_logic) = self.spdp_logic.as_ref() {
            spdp_logic.start_spdp()?;
        } else {
            log::error!("spdp_logic is not set");
        }

        // Start User traffic threads
        if let Some(user_logic) = self.user_logic.as_ref() {
            user_logic
                .start_user_traffic(self.domain_id, transport.take_user_data_unicast_source())?;
        } else {
            log::error!("user_logic is not set");
        }

        // Initialize thread monitoring
        self.thread_monitor = Some(ThreadMonitor::new(self.participant.clone()));
        match self.thread_monitor.as_ref() {
            Some(thread_monitor) => {
                thread_monitor.start_monitoring();
                debug!("Thread monitoring started");
            }
            None => {
                log::error!("thread_monitor is not set");
            }
        }

        Ok(())
    }

    pub(crate) fn next_entity_guid(&self, entity_kind: EntityKind) -> Guid {
        let next_entity_id = self.participant.next_entity_key(entity_kind);
        Guid::new(self.guid_prefix, next_entity_id)
    }

    fn default_endpoint_info(&self) -> RtpsResult<(Vec<Locator>, Vec<Locator>)> {
        let local_participant_data = self.participant.local_participant_proxy_data();
        Ok((
            local_participant_data.default_unicast_locator_list().clone(),
            local_participant_data.default_multicast_locator_list().clone(),
        ))
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn create_rtps_writer(
        &mut self,
        mut publication_builtin_topic_data: PublicationBuiltinTopicData,
        f: Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>,
    ) -> Result<Arc<dyn Writer + Send + Sync>, RtpsError> {
        let datawriter_guid = publication_builtin_topic_data.endpoint_guid();
        let entity_kind = datawriter_guid.entity_kind();
        let topic_kind = if entity_kind == EntityKind::USER_DEFINED_WRITER_NO_KEY {
            TopicKind::NoKey
        } else if entity_kind == EntityKind::USER_DEFINED_WRITER_WITH_KEY {
            TopicKind::WithKey
        } else {
            return Err(RtpsError::new(RtpsErrorCode::InvalidEntityKind, "Invalid entity kind"));
        };

        let (unicast_locator_list, multicast_locator_list) = self.default_endpoint_info()?;

        for locator in &unicast_locator_list {
            publication_builtin_topic_data.add_unicast_locator(locator.clone());
        }

        let fragment_size = publication_builtin_topic_data.data_frag().effective_max_size();

        let writer: Arc<dyn Writer + Send + Sync> = if publication_builtin_topic_data.is_reliable()
        {
            Arc::new(StatefulWriter::new(
                datawriter_guid,
                unicast_locator_list,
                multicast_locator_list,
                ReliabilityQosPolicyKind::Reliable,
                topic_kind,
                datawriter_guid.entity_id(),
                fragment_size,
                f,
                publication_builtin_topic_data.clone(),
                Arc::downgrade(&self.participant),
            ))
        } else {
            Arc::new(StatelessWriter::new(
                datawriter_guid,
                unicast_locator_list,
                multicast_locator_list,
                ReliabilityQosPolicyKind::BestEffort,
                topic_kind,
                datawriter_guid.entity_id(),
                true,
                RtpsDuration::new(2, 0),
                fragment_size,
                f,
                publication_builtin_topic_data.clone(),
                Arc::downgrade(&self.participant),
            ))
        };

        let writer_data = DiscoveredWriterData {
            publication_builtin_topic_data: publication_builtin_topic_data.clone(),
        };
        let payload = SEDPMessage::create_publication_serialized_data(&writer_data);

        let change = self.participant.sedp_builtin_publications_writer().new_change(
            ChangeKind::Alive,
            payload.to_vec(),
            InstanceHandle::from_guid(&publication_builtin_topic_data.endpoint_guid()),
            Some(RtpsTime::now()),
        );
        let cache_change = Arc::new(change);
        let sedp_writer = self.participant.sedp_builtin_publications_writer();
        let _ = sedp_writer
            .writer_cache()
            .lock()
            .unwrap()
            .add_change_builtin(cache_change.clone(), sedp_writer.as_ref());

        self.participant
            .remote_publications()
            .entry(publication_builtin_topic_data.topic_name().to_string())
            .or_default()
            .insert(
                publication_builtin_topic_data.endpoint_guid(),
                publication_builtin_topic_data.clone(),
            );

        let _ = self
            .participant
            .add_writer(&publication_builtin_topic_data.topic_name(), writer.clone());

        self.send_sedp_message_and_match(
            cache_change,
            publication_builtin_topic_data.topic_name().as_str(),
            EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
            EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
            |topic_name| {
                self.participant
                    .remote_subscriptions()
                    .get(topic_name)
                    .map(|r| r.value().values().cloned().collect())
            },
            |sedp_logic, writer, subscription_data| {
                sedp_logic
                    .match_writer_with_subscription(writer.clone(), subscription_data)
                    .map(|_| false)
            },
            Some(&writer),
        );

        Ok(writer)
    }

    pub(crate) fn get_participant(&self) -> Result<Participant, RtpsError> {
        Ok(Arc::try_unwrap(self.participant.clone()).unwrap_or_else(|a| (*a).clone()))
    }

    pub(crate) fn delete_rtps_writer(
        &mut self,
        topic_name: String,
        entity_id: EntityId,
    ) -> Result<(), RtpsError> {
        let _ = self.participant.remove_writer(topic_name, entity_id);
        Ok(())
    }

    pub(crate) fn delete_rtps_reader(
        &mut self,
        topic_name: String,
        entity_id: EntityId,
    ) -> Result<(), RtpsError> {
        let _ = self.participant.remove_reader(topic_name, entity_id);
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn create_rtps_reader(
        &mut self,
        mut subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
        content_filter_property: Option<ContentFilterProperty>,
        change_callback: Option<Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>>,
        status_callback: Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>,
    ) -> Result<Arc<dyn Reader + Send + Sync>, RtpsError> {
        let datareader_guid = subscription_builtin_topic_data.endpoint_guid();
        let entity_kind = datareader_guid.entity_kind();
        let topic_kind = if entity_kind == EntityKind::USER_DEFINED_READER_NO_KEY {
            TopicKind::NoKey
        } else if entity_kind == EntityKind::USER_DEFINED_READER_WITH_KEY {
            TopicKind::WithKey
        } else {
            return Err(RtpsError::new(RtpsErrorCode::InvalidEntityKind, "Invalid entity kind"));
        };

        let (unicast_locator_list, multicast_locator_list) = self.default_endpoint_info()?;

        // Clone locator lists before using them in Reader constructors
        let unicast_locator_list_clone = unicast_locator_list.clone();
        let multicast_locator_list_clone = multicast_locator_list.clone();

        for locator in &unicast_locator_list_clone {
            subscription_builtin_topic_data.add_unicast_locator(locator.clone());
        }

        for locator in &multicast_locator_list_clone {
            subscription_builtin_topic_data.add_multicast_locator(locator.clone());
        }

        let reader: Arc<dyn Reader + Send + Sync> = if subscription_builtin_topic_data.is_reliable()
        {
            Arc::new(StatefulReader::new(
                datareader_guid,
                topic_kind,
                ReliabilityQosPolicyKind::Reliable,
                unicast_locator_list_clone,
                multicast_locator_list_clone,
                datareader_guid.entity_id(),
                false,
                change_callback,
                status_callback,
                subscription_builtin_topic_data.clone(),
                self.participant.guid(),
            ))
        } else {
            Arc::new(StatelessReader::new(
                datareader_guid,
                topic_kind,
                ReliabilityQosPolicyKind::BestEffort,
                unicast_locator_list_clone,
                multicast_locator_list_clone,
                datareader_guid.entity_id(),
                false,
                change_callback,
                status_callback,
                subscription_builtin_topic_data.clone(),
                self.participant.guid(),
            ))
        };

        // Create DiscoveredReaderData and serialize using SEDPMessage.
        let reader_data = DiscoveredReaderData {
            subscription_builtin_topic_data: subscription_builtin_topic_data.clone(),
            content_filter: content_filter_property,
        };

        let payload = SEDPMessage::create_subscription_serialized_data(&reader_data);

        let change = self.participant.sedp_builtin_subscriptions_writer().new_change(
            ChangeKind::Alive,
            payload.to_vec(),
            InstanceHandle::from_guid(&subscription_builtin_topic_data.endpoint_guid()),
            Some(RtpsTime::now()),
        );
        let cache_change = Arc::new(change);
        let sedp_writer = self.participant.sedp_builtin_subscriptions_writer();
        let _ = sedp_writer
            .writer_cache()
            .lock()
            .unwrap()
            .add_change_builtin(cache_change.clone(), sedp_writer.as_ref());

        self.participant
            .remote_subscriptions()
            .entry(subscription_builtin_topic_data.topic_name().to_string())
            .or_default()
            .insert(
                subscription_builtin_topic_data.endpoint_guid(),
                subscription_builtin_topic_data.clone(),
            );

        // Register the reader in the participant store BEFORE matching so that
        // any liveliness/match notifications fired during cross-match can find it.
        self.participant.add_reader(&subscription_builtin_topic_data.topic_name(), reader.clone());

        self.send_sedp_message_and_match(
            cache_change,
            subscription_builtin_topic_data.topic_name().as_str(),
            EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER,
            EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
            |topic_name| {
                self.participant
                    .remote_publications()
                    .get(topic_name)
                    .map(|r| r.value().values().cloned().collect())
            },
            |sedp_logic, reader, publication_data| {
                sedp_logic.match_reader_with_publication(reader.clone(), publication_data);
                Ok(false)
            },
            Some(&reader),
        );

        Ok(reader)
    }

    /// send sedp message to remote participants and match pending endpoints
    #[allow(clippy::too_many_arguments)]
    fn send_sedp_message_and_match<F, G, T, E>(
        &self,
        cache_change: Arc<CacheChange>,
        topic_name: &str,
        reader_id: EntityId,
        writer_id: EntityId,
        get_endpoint_data: F,
        match_endpoint: G,
        endpoint: Option<&Arc<T>>,
    ) where
        F: FnOnce(&str) -> Option<Vec<E>>,
        G: Fn(&SedpLogic, &Arc<T>, E) -> RtpsResult<bool>,
        T: ?Sized,
    {
        // Send SEDP message to all remote participants
        if let Some(sedp_logic) = self.sedp_logic.as_ref() {
            let remote_guids: Vec<_> = self
                .participant
                .remote_participant_proxy_datas()
                .lock()
                .map(|participants| {
                    participants.iter().map(|data| data.participant_guid()).collect()
                })
                .unwrap_or_default();

            // Now send messages without holding the lock
            for remote_guid in remote_guids {
                let _ = sedp_logic.send_sedp_data_message(
                    cache_change.clone(),
                    remote_guid,
                    reader_id,
                    writer_id,
                );

                // Re-arm this remote's periodic heartbeat so the new change is
                // retransmitted until acked. add_timer skips it if still running.
                let remote_prefix = remote_guid.prefix();
                let heartbeat_message = if writer_id == EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER {
                    MessageType::PeriodicPublicationHeartbeat(
                        None,
                        BUILTIN_SEDP_HB_PERIOD,
                        Arc::new(remote_prefix),
                    )
                } else {
                    MessageType::PeriodicSubscriptionHeartbeat(
                        None,
                        BUILTIN_SEDP_HB_PERIOD,
                        Arc::new(remote_prefix),
                    )
                };
                let _ = sedp_logic.register_periodic_send_timer(
                    remote_prefix,
                    writer_id,
                    BUILTIN_SEDP_HB_PERIOD,
                    heartbeat_message,
                );
            }
        }

        // Match endpoints
        if let Some(datas) = get_endpoint_data(topic_name) {
            if let Some(sedp_logic) = self.sedp_logic.as_ref() {
                if let Some(endpoint) = endpoint {
                    for data in datas {
                        if let Err(e) = match_endpoint(sedp_logic, endpoint, data) {
                            log::error!("Failed to handle pending endpoint: {:?}", e);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn disable(&mut self) -> Result<(), RtpsError> {
        // Remove current participant from PARTICIPANTS global variable
        if let Ok(mut participants) = PARTICIPANTS.write() {
            participants.retain(|weak_ref| {
                if let Some(participant) = weak_ref.upgrade() {
                    // Check if same participant by comparing GUID
                    participant.guid() != self.participant.guid()
                } else {
                    false // Remove dead reference
                }
            });
        }

        // Set terminate flag
        self.participant.terminate();

        // Stop thread monitoring
        match &self.thread_monitor {
            Some(thread_monitor) => {
                thread_monitor.stop_monitoring();
                debug!("Thread monitoring stopped");
            }
            None => {
                ThreadMonitor::remove_threads_by_guid_prefix(&self.participant.guid().prefix());
            }
        }

        let timer_handler = TimerHandler::get_instance(self.participant.guid().prefix());
        if let Ok(mut handler) = timer_handler.lock() {
            handler.terminate();
            let _ = handler.join_timer_thread();
        }
        drop(timer_handler);

        // Wake and join the sending thread first so it releases the SendingTask mutex.
        // Otherwise the send_termination_message_on_shutdown() below blocks waiting for it.
        let sending_handler = SendingHandler::get_instance(self.participant.clone(), None);
        sending_handler.wake_event_loop();

        // Send termination message before stopping sending thread
        let _ = self.participant.send_termination_message_on_shutdown();

        // Terminate sending task thread
        let _ = sending_handler.join_sending_thread();
        drop(sending_handler);

        // Terminate discovery listening task
        if let Some(sedp_logic) = self.sedp_logic.as_ref() {
            sedp_logic.wake_listening_threads();
            sedp_logic.join_all_listening_threads()?;
        }

        // Terminate user traffic listening task
        if let Some(user_logic) = self.user_logic.as_ref() {
            user_logic.wake_unicast_listening_thread();
            user_logic.join_unicast_listening_thread()?;
        }

        // Shutdown participant liveliness monitor
        self.participant.shutdown_liveliness_monitor();

        // Shutdown WLP liveliness monitor
        if let Some(wlp_logic) = self.participant.wlp_logic() {
            wlp_logic.shutdown();
        }

        // Drop logic instances to release sender references
        self.spdp_logic = Arc::new(None);
        self.sedp_logic = Arc::new(None);
        self.user_logic = Arc::new(None);

        // Clear wlp_logic sender reference to allow Arc cleanup
        self.participant.clear_wlp_logic_sender();

        // Remove all threads spawned
        SendingHandler::remove_map_guard(&self.participant.guid());
        TimerHandler::remove_map_guard(&self.participant.guid().prefix());
        self.thread_monitor = None;
        self.socket.close();
        Ok(())
    }

    pub(crate) fn update_reader(
        &self,
        reader: &Arc<dyn Reader + Send + Sync>,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
        content_filter_property: Option<ContentFilterProperty>,
    ) -> RtpsResult<()> {
        let datareader_guid = subscription_builtin_topic_data.endpoint_guid();

        // remove prev endpoint discovery data
        let cache = self.participant.sedp_builtin_subscriptions_writer().writer_cache();
        let mut cache = cache.lock().map_err(|e| {
            RtpsError::new(RtpsErrorCode::LockError, format!("Failed to lock history cache: {}", e))
        })?;
        let prev_change =
            cache.get_change_from_instance_handle(InstanceHandle::from_guid(&datareader_guid));

        if !prev_change.is_empty() {
            for prev in prev_change {
                cache.remove_change(prev)?;
            }
        }

        drop(cache);

        // update endpoint discovery data with the reader's new info

        let reader_data = DiscoveredReaderData {
            subscription_builtin_topic_data: subscription_builtin_topic_data.clone(),
            content_filter: content_filter_property,
        };

        let payload = SEDPMessage::create_subscription_serialized_data(&reader_data);

        let change = self.participant.sedp_builtin_subscriptions_writer().new_change(
            ChangeKind::Alive,
            payload.to_vec(),
            InstanceHandle::from_guid(&subscription_builtin_topic_data.endpoint_guid()),
            Some(RtpsTime::now()),
        );
        let cache_change = Arc::new(change);
        let sedp_writer = self.participant.sedp_builtin_subscriptions_writer();
        let _ = sedp_writer
            .writer_cache()
            .lock()
            .map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to lock history cache: {}", e),
                )
            })?
            .add_change_builtin(cache_change.clone(), sedp_writer.as_ref());

        self.send_sedp_message_and_match(
            cache_change,
            subscription_builtin_topic_data.topic_name().as_str(),
            EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER,
            EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
            |topic_name| {
                self.participant
                    .remote_publications()
                    .get(topic_name)
                    .map(|r| r.value().values().cloned().collect())
            },
            |sedp_logic, reader, publication_data| {
                sedp_logic.match_reader_with_publication(reader.clone(), publication_data);
                Ok(false)
            },
            Some(reader),
        );

        Ok(())
    }

    pub(crate) fn update_writer(
        &self,
        writer: &Arc<dyn Writer + Send + Sync>,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> RtpsResult<()> {
        let datawriter_guid = publication_builtin_topic_data.endpoint_guid();

        // remove prev endpoint discovery data
        let cache = self.participant.sedp_builtin_publications_writer().writer_cache();
        let mut cache = cache.lock().map_err(|e| {
            RtpsError::new(RtpsErrorCode::LockError, format!("Failed to lock history cache: {}", e))
        })?;
        let prev_change =
            cache.get_change_from_instance_handle(InstanceHandle::from_guid(&datawriter_guid));

        if !prev_change.is_empty() {
            for prev in prev_change {
                cache.remove_change(prev)?;
            }
        }

        drop(cache);

        // update endpoint discovery data with the reader's new info

        let writer_data = DiscoveredWriterData {
            publication_builtin_topic_data: publication_builtin_topic_data.clone(),
        };

        let payload = SEDPMessage::create_publication_serialized_data(&writer_data);

        let change = self.participant.sedp_builtin_publications_writer().new_change(
            ChangeKind::Alive,
            payload.to_vec(),
            InstanceHandle::from_guid(&publication_builtin_topic_data.endpoint_guid()),
            Some(RtpsTime::now()),
        );
        let cache_change = Arc::new(change);
        let sedp_writer = self.participant.sedp_builtin_publications_writer();
        let _ = sedp_writer
            .writer_cache()
            .lock()
            .map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to lock history cache: {}", e),
                )
            })?
            .add_change_builtin(cache_change.clone(), sedp_writer.as_ref());

        self.send_sedp_message_and_match(
            cache_change,
            publication_builtin_topic_data.topic_name().as_str(),
            EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
            EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
            |topic_name| {
                self.participant
                    .remote_subscriptions()
                    .get(topic_name)
                    .map(|r| r.value().values().cloned().collect())
            },
            |sedp_logic, writer, subscription_data| {
                sedp_logic
                    .match_writer_with_subscription(writer.clone(), subscription_data)
                    .map(|_| false)
            },
            Some(writer),
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use log::debug;
    use std::{sync::Mutex, thread, time::Duration as StdDuration};

    use super::*;
    use crate::{
        core::time::Duration,
        infrastructure::{
            qos_policy::ReliabilityQosPolicy,
            status::{PublicationMatchedStatus, StatusInfo, StatusKind, SubscriptionMatchedStatus},
        },
        publication::qos::{DataWriterQos, PublisherQos},
        rtps::{
            common::{entity_kind::EntityKind, sequence::SequenceNumber},
            entities::{
                history::history_cache::HistoryCache,
                reader::WriterProxy,
                writer::{reader_locator::ReaderLocator, reader_proxy::ReaderProxy},
            },
            logic::common::MulticastThreadHandler as _,
        },
        subscription::qos::{DataReaderQos, SubscriberQos},
        test_utils::unique_domain_id,
        topic::{qos::TopicQos, type_support::DdsType},
    };
    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    pub(crate) struct HelloWorld {
        pub index: u32,
        pub message: String,
    }

    #[test]
    #[ignore]
    fn test_dcps_bridge_init() {
        env_logger::builder().filter_level(log::LevelFilter::Debug).init();

        //initialize dcps_bridge
        let domain_id = unique_domain_id();
        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));
        match dcps_bridge.lock() {
            Ok(mut dcps_bridge) => dcps_bridge.init().unwrap(),
            Err(e) => {
                log::error!("dcps_bridge lock error: {:?}", e);
            }
        }

        thread::sleep(std::time::Duration::from_secs(100));
    }

    /// Test that all Logic objects (SpdpLogic, SedpLogic, UserLogic, WlpLogic) are properly cleaned up
    #[test]
    fn test_all_logic_cleanup() {
        let domain_id = unique_domain_id();
        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        {
            let mut bridge = dcps_bridge.lock().unwrap();
            bridge.init().unwrap();
            thread::sleep(StdDuration::from_millis(50));

            // All logics should exist after init
            assert!(bridge.spdp_logic.as_ref().is_some(), "SpdpLogic should exist after init");
            assert!(bridge.sedp_logic.as_ref().is_some(), "SedpLogic should exist after init");
            assert!(bridge.user_logic.as_ref().is_some(), "UserLogic should exist after init");
            assert!(bridge.participant.wlp_logic().is_some(), "WlpLogic should exist after init");

            let _ = bridge.disable();

            // DcpsBridge-owned logics should be None after disable
            assert!(bridge.spdp_logic.as_ref().is_none(), "SpdpLogic should be None after disable");
            assert!(bridge.sedp_logic.as_ref().is_none(), "SedpLogic should be None after disable");
            assert!(bridge.user_logic.as_ref().is_none(), "UserLogic should be None after disable");
            // WlpLogic is in OnceLock, cleaned up when Participant drops
        }
    }

    /// Test that all Handler objects (SendingHandler, TimerHandler) are properly cleaned up
    #[test]
    fn test_all_handler_cleanup() {
        use crate::rtps::task::sending_handler::SendingHandler;
        use crate::utils::timer::timer_handler::TimerHandler;

        let domain_id = unique_domain_id();
        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));
        let participant_guid: crate::rtps::common::guid::Guid;

        {
            let mut bridge = dcps_bridge.lock().unwrap();
            participant_guid = bridge.participant.guid();
            bridge.init().unwrap();
            thread::sleep(StdDuration::from_millis(50));

            // All handlers should exist in global maps after init
            assert!(
                SendingHandler::get_instance_by_participant_guid(participant_guid).is_some(),
                "SendingHandler should exist after init"
            );
            assert!(
                TimerHandler::get_instance_by_participant_guid(participant_guid).is_some(),
                "TimerHandler should exist after init"
            );

            let _ = bridge.disable();

            // All handlers should be removed from global maps after disable
            assert!(
                SendingHandler::get_instance_by_participant_guid(participant_guid).is_none(),
                "SendingHandler should be removed after disable"
            );
            assert!(
                TimerHandler::get_instance_by_participant_guid(participant_guid).is_none(),
                "TimerHandler should be removed after disable"
            );
        }
    }

    // Env is process-global: serialize the frag-size/message-size overrides
    // against any other test mutating them concurrently.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Two participants, one host: each must learn the other's advertised
    /// receive-buffer size over SPDP, and that value must match what a fresh
    /// socket on this host is actually granted - not what int2dds requested.
    #[test]
    fn spdp_advertises_receive_buffer_size_between_peers() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::set_var("INT2DDS_DATA_FRAG_SIZE", "1344");
            std::env::set_var("INT2DDS_MAX_MESSAGE_SIZE", "13440");
            // Both peers must take the default-policy branch below,
            // not an explicit override left set by another test/session.
            std::env::remove_var("INT2DDS_UDP_SOCKET_BUFFER");
        }

        // Independent oracle, computed with socket2 directly (not via
        // UdpListener::new): what any fresh socket on this host is actually
        // granted under the default policy (raise SO_RCVBUF to 1 MiB if smaller).
        let probe = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::DGRAM,
            Some(socket2::Protocol::UDP),
        )
        .expect("probe socket");
        let current = probe.recv_buffer_size().expect("read default SO_RCVBUF");
        if current < 1024 * 1024 {
            probe.set_recv_buffer_size(1024 * 1024).expect("set SO_RCVBUF");
        }
        let expected = probe.recv_buffer_size().expect("read granted SO_RCVBUF");

        let domain_id = unique_domain_id() as u32;
        let mut bridge_a = DcpsBridge::new(domain_id, &Default::default()).unwrap();
        bridge_a.init().unwrap();
        let mut bridge_b = DcpsBridge::new(domain_id, &Default::default()).unwrap();
        bridge_b.init().unwrap();

        let guid_a = bridge_a.participant.guid();
        let guid_b = bridge_b.participant.guid();

        // Each side's own advertised value must already be the granted one,
        // before either has heard from the other.
        assert_eq!(
            bridge_a.participant.local_participant_proxy_data().receive_buffer_size(),
            Some(expected)
        );
        assert_eq!(
            bridge_b.participant.local_participant_proxy_data().receive_buffer_size(),
            Some(expected)
        );

        let deadline = std::time::Instant::now() + StdDuration::from_secs(15);
        loop {
            let a_knows_b =
                bridge_a.participant.find_remote_participant_proxy_data(guid_b.prefix()).is_some();
            let b_knows_a =
                bridge_b.participant.find_remote_participant_proxy_data(guid_a.prefix()).is_some();
            if a_knows_b && b_knows_a {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "participants did not discover each other in time"
            );
            thread::sleep(StdDuration::from_millis(50));
        }

        let a_view_of_b =
            bridge_a.participant.find_remote_participant_proxy_data(guid_b.prefix()).unwrap();
        let b_view_of_a =
            bridge_b.participant.find_remote_participant_proxy_data(guid_a.prefix()).unwrap();

        assert_eq!(a_view_of_b.receive_buffer_size(), Some(expected));
        assert_eq!(b_view_of_a.receive_buffer_size(), Some(expected));

        unsafe {
            std::env::remove_var("INT2DDS_DATA_FRAG_SIZE");
            std::env::remove_var("INT2DDS_MAX_MESSAGE_SIZE");
        }
    }

    fn change_callback(change: Arc<CacheChange>) {
        log::info!("change: {}", change);
    }
    fn status_callback(status: StatusKind, info: Option<Arc<dyn StatusInfo>>) {
        log::info!("status: {:?}", status);
        if let Some(info) = info {
            if let Some(subscription_matched_status) =
                info.as_any().downcast_ref::<SubscriptionMatchedStatus>()
            {
                log::info!("subscription_matched_status: {:?}", subscription_matched_status);
            }
            if let Some(publication_matched_status) =
                info.as_any().downcast_ref::<PublicationMatchedStatus>()
            {
                log::info!("publication_matched_status: {:?}", publication_matched_status);
            }
        }
    }

    #[test]
    #[ignore]
    fn test_dcps_bridge_create_stateless_datareader() {
        env_logger::builder().filter_level(log::LevelFilter::Info).init();

        let domain_id = unique_domain_id();
        let test_topic_name = "hello_world_topic_sub";
        let test_type_name = "HelloWorld";

        //initialize dcps_bridge
        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let _participant = dcps_bridge.lock().unwrap().get_participant().unwrap();

        let mut subscription_builtin_topic_data = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::BestEffort,
                    max_blocking_time: Duration::new(0, 100_000_000),
                },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        subscription_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        subscription_builtin_topic_data.set_type_name(test_type_name.to_string());

        match dcps_bridge.lock() {
            Ok(mut dcps_bridge) => {
                dcps_bridge.init().unwrap();

                let guid = dcps_bridge.next_entity_guid(EntityKind::USER_DEFINED_READER_NO_KEY);
                subscription_builtin_topic_data.set_endpoint_guid(guid);

                let reader = dcps_bridge
                    .create_rtps_reader(
                        subscription_builtin_topic_data,
                        None,
                        Some(Arc::new(tests::change_callback)),
                        Some(Arc::new(tests::status_callback)),
                    )
                    .unwrap();

                let mut i = 0;
                loop {
                    let changes = reader.available_changes();
                    if !changes.is_empty() {
                        for change in changes {
                            log::info!("change: {}", change);
                        }
                    } else {
                        continue;
                    }
                    thread::sleep(StdDuration::from_secs(1));
                    i += 1;
                    if i > 10 {
                        break;
                    }
                }
            }
            Err(e) => {
                log::error!("dcps_bridge lock error: {:?}", e);
            }
        };
    }

    #[test]
    #[ignore]
    fn test_dcps_bridge_create_stateless_datawriter() {
        env_logger::builder().filter_level(log::LevelFilter::Debug).init();

        let domain_id = unique_domain_id();
        let test_topic_name = "hello_world_topic_sub";
        let test_type_name = "HelloWorld";

        let dcps_bridge_test: Arc<Mutex<DcpsBridge>> =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let _participant = dcps_bridge_test.lock().unwrap().get_participant().unwrap();

        let mut publication_builtin_topic_data = PublicationBuiltinTopicData::new(
            // &DataWriterQos::default(),
            &DataWriterQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::BestEffort,
                    max_blocking_time: Duration::new(0, 100_000_000),
                },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );
        publication_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        publication_builtin_topic_data.set_type_name(test_type_name.to_string());

        {
            let mut guard = dcps_bridge_test.lock().unwrap();
            guard.init().unwrap();

            let guid = guard.next_entity_guid(EntityKind::USER_DEFINED_WRITER_NO_KEY);
            publication_builtin_topic_data.set_endpoint_guid(guid);

            let writer = guard
                .create_rtps_writer(
                    publication_builtin_topic_data,
                    Some(Arc::new(tests::status_callback)),
                )
                .unwrap();

            writer.set_update_status(Arc::new(status_callback));

            debug!("writer: {:?}", writer);
            for i in 0..1000 {
                let hello_world = HelloWorld { index: i, message: "p Hello, world!".to_string() };
                let payload = hello_world.serialize().unwrap().to_vec();
                let change = writer.new_change(
                    ChangeKind::Alive,
                    payload,
                    InstanceHandle::NIL,
                    Some(RtpsTime::now()),
                );
                match writer.writer_cache().lock() {
                    Ok(mut writer_cache) => {
                        let _ = writer_cache.add_change_builtin(Arc::new(change), writer.as_ref());
                    }
                    Err(e) => {
                        log::error!("writer_cache lock error: {:?}", e);
                    }
                }
                thread::sleep(StdDuration::from_secs(1));
            }
        }
    }

    #[test]
    fn test_dcps_bridge_remove_datareader() {
        let domain_id = unique_domain_id();
        let test_topic_name = "remove_writer_topic";
        let test_type_name = "HelloWorld";

        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let mut subscription_builtin_topic_data = SubscriptionBuiltinTopicData::new(
            &DataReaderQos::default(),
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        subscription_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        subscription_builtin_topic_data.set_type_name(test_type_name.to_string());

        let (reader_guid, weak_reader, weak_history_cache) = {
            let mut guard = dcps_bridge.lock().unwrap();
            guard.init().unwrap();

            let guid1 = guard.next_entity_guid(EntityKind::USER_DEFINED_READER_NO_KEY);
            subscription_builtin_topic_data.set_endpoint_guid(guid1);

            // Create DataReader
            let reader = guard
                .create_rtps_reader(
                    subscription_builtin_topic_data.clone(),
                    None,
                    Some(Arc::new(tests::change_callback)),
                    Some(Arc::new(tests::status_callback)),
                )
                .unwrap();

            let guid2 = guard.next_entity_guid(EntityKind::USER_DEFINED_READER_NO_KEY);
            subscription_builtin_topic_data.set_endpoint_guid(guid2);

            let _reader_2 = guard
                .create_rtps_reader(
                    subscription_builtin_topic_data.clone(),
                    None,
                    Some(Arc::new(tests::change_callback)),
                    Some(Arc::new(tests::status_callback)),
                )
                .unwrap();

            assert_eq!(
                guard
                    .participant
                    .builtin_endpoints()
                    .sedp_builtin_subscriptions_writer
                    .writer_cache()
                    .lock()
                    .unwrap()
                    .get_changes()
                    .len(),
                2
            );

            // Extract reader's guid
            let weak_history_cache = Arc::downgrade(&reader.reader_cache());
            let weak_reader = Arc::downgrade(&reader);
            let reader_guid = reader.guid();
            (reader_guid, weak_reader, weak_history_cache)
        };

        // Delete DataReader
        {
            let mut guard = dcps_bridge.lock().unwrap();
            let res =
                guard.delete_rtps_reader(test_topic_name.to_string(), reader_guid.entity_id());
            assert!(res.is_ok(), "delete_rtps_reader should succeed");
        }

        assert!(weak_reader.upgrade().is_none(), "Reader Arc should be dropped after removal");
        assert!(
            weak_history_cache.upgrade().is_none(),
            "HistoryCache Arc should be dropped after removal"
        );

        // Two instances, one change each: the surviving reader's announcement and the deleted
        // reader's dispose. The dispose stays because this builtin writer is RELIABLE -- a peer
        // that missed the datagram can only ask for a sequence number the history still holds.
        assert_eq!(
            dcps_bridge
                .lock()
                .unwrap()
                .participant
                .builtin_endpoints()
                .sedp_builtin_subscriptions_writer
                .writer_cache()
                .lock()
                .unwrap()
                .get_changes()
                .len(),
            2
        );
    }

    #[test]
    fn test_dcps_bridge_remove_datawriter() {
        let domain_id = unique_domain_id();
        let test_topic_name = "remove_writer_topic";
        let test_type_name = "HelloWorld";

        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let mut publication_builtin_topic_data = PublicationBuiltinTopicData::new(
            &DataWriterQos::default(),
            &PublisherQos::default(),
            &TopicQos::default(),
        );
        publication_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        publication_builtin_topic_data.set_type_name(test_type_name.to_string());

        let (writer_guid, weak_writer, weak_history_cache) = {
            let mut guard = dcps_bridge.lock().unwrap();
            guard.init().unwrap();

            let guid1 = guard.next_entity_guid(EntityKind::USER_DEFINED_WRITER_NO_KEY);
            publication_builtin_topic_data.set_endpoint_guid(guid1);

            // Create DataWriter
            let writer = guard
                .create_rtps_writer(
                    publication_builtin_topic_data.clone(),
                    Some(Arc::new(tests::status_callback)),
                )
                .unwrap();

            let guid2 = guard.next_entity_guid(EntityKind::USER_DEFINED_WRITER_NO_KEY);
            publication_builtin_topic_data.set_endpoint_guid(guid2);

            let _writer_2 = guard
                .create_rtps_writer(
                    publication_builtin_topic_data.clone(),
                    Some(Arc::new(tests::status_callback)),
                )
                .unwrap();

            assert_eq!(
                guard
                    .participant
                    .builtin_endpoints()
                    .sedp_builtin_publications_writer
                    .writer_cache()
                    .lock()
                    .unwrap()
                    .get_changes()
                    .len(),
                2
            );

            // Extract writer's guid
            let weak_history_cache = Arc::downgrade(&writer.writer_cache());
            let weak_writer = Arc::downgrade(&writer);
            let writer_guid = writer.guid();
            (writer_guid, weak_writer, weak_history_cache)
        };

        // Delete DataWriter
        {
            let mut guard = dcps_bridge.lock().unwrap();
            let res =
                guard.delete_rtps_writer(test_topic_name.to_string(), writer_guid.entity_id());
            assert!(res.is_ok(), "delete_rtps_writer should succeed");
        }

        assert!(weak_writer.upgrade().is_none(), "Writer Arc should be dropped after removal");
        assert!(
            weak_history_cache.upgrade().is_none(),
            "HistoryCache Arc should be dropped after removal"
        );

        // Two instances, one change each: the surviving writer's announcement and the deleted
        // writer's dispose. The dispose stays because this builtin writer is RELIABLE -- a peer
        // that missed the datagram can only ask for a sequence number the history still holds.
        assert_eq!(
            dcps_bridge
                .lock()
                .unwrap()
                .participant
                .builtin_endpoints()
                .sedp_builtin_publications_writer
                .writer_cache()
                .lock()
                .unwrap()
                .get_changes()
                .len(),
            2
        );
    }

    #[test]
    #[ignore]
    fn test_best_effort_writer_matched_reader_locators_is_empty() {
        env_logger::builder().filter_level(log::LevelFilter::Error).init();

        let domain_id = unique_domain_id();
        let test_topic_name = "hello_world_topic";
        let test_type_name = "HelloWorld";

        let dcps_bridge_test: Arc<Mutex<DcpsBridge>> =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let mut publication_builtin_topic_data = PublicationBuiltinTopicData::new(
            // &DataWriterQos::default(),
            &DataWriterQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::BestEffort,
                    max_blocking_time: Duration::new(0, 100_000_000),
                },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );
        publication_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        publication_builtin_topic_data.set_type_name(test_type_name.to_string());

        {
            let mut guard = dcps_bridge_test.lock().unwrap();
            guard.init().unwrap();

            let guid = guard.next_entity_guid(EntityKind::USER_DEFINED_WRITER_NO_KEY);
            publication_builtin_topic_data.set_endpoint_guid(guid);

            let writer = guard
                .create_rtps_writer(
                    publication_builtin_topic_data,
                    Some(Arc::new(tests::status_callback)),
                )
                .unwrap();

            // Wait to yield Writer lock to SEDP Logic first
            thread::sleep(StdDuration::from_secs(10));

            if let Some(stateless_writer) = writer.as_any().downcast_ref::<StatelessWriter>() {
                for _i in 0..1000 {
                    println!(
                        "is reader proxy list empty?: {}",
                        stateless_writer.reader_locator().lock().unwrap().is_empty()
                    );
                    thread::sleep(StdDuration::from_secs(1));
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn test_reliable_writer_matched_reader_proxies_is_empty() {
        env_logger::builder().filter_level(log::LevelFilter::Error).init();

        let domain_id = unique_domain_id();
        let test_topic_name = "hello_world_topic";
        let test_type_name = "HelloWorld";

        let dcps_bridge_test: Arc<Mutex<DcpsBridge>> =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let mut publication_builtin_topic_data = PublicationBuiltinTopicData::new(
            &DataWriterQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::new(0, 100_000_000),
                },
                ..Default::default()
            },
            &PublisherQos::default(),
            &TopicQos::default(),
        );
        publication_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        publication_builtin_topic_data.set_type_name(test_type_name.to_string());

        {
            let mut guard = dcps_bridge_test.lock().unwrap();
            guard.init().unwrap();

            let guid = guard.next_entity_guid(EntityKind::USER_DEFINED_WRITER_NO_KEY);
            publication_builtin_topic_data.set_endpoint_guid(guid);

            let writer = guard
                .create_rtps_writer(
                    publication_builtin_topic_data,
                    Some(Arc::new(tests::status_callback)),
                )
                .unwrap();

            // Wait to yield Writer lock to SEDP Logic first
            thread::sleep(StdDuration::from_secs(10));

            if let Some(stateful_writer) = writer.as_any().downcast_ref::<StatefulWriter>() {
                for _i in 0..1000 {
                    println!(
                        "is reader proxy list empty?: {}",
                        stateful_writer.reader_proxies().lock().unwrap().is_empty()
                    );
                    thread::sleep(StdDuration::from_secs(1));
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn test_reliable_reader_matched_writer_proxies_is_empty() {
        env_logger::builder().filter_level(log::LevelFilter::Error).init();

        let domain_id = unique_domain_id();
        let test_topic_name = "hello_world_topic";
        let test_type_name = "HelloWorld";

        //initialize dcps_bridge
        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let mut subscription_builtin_topic_data = SubscriptionBuiltinTopicData::new(
            &DataReaderQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::new(0, 100_000_000),
                },
                ..Default::default()
            },
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        subscription_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        subscription_builtin_topic_data.set_type_name(test_type_name.to_string());

        match dcps_bridge.lock() {
            Ok(mut dcps_bridge) => {
                dcps_bridge.init().unwrap();

                let guid = dcps_bridge.next_entity_guid(EntityKind::USER_DEFINED_READER_NO_KEY);
                subscription_builtin_topic_data.set_endpoint_guid(guid);

                let reader = dcps_bridge
                    .create_rtps_reader(
                        subscription_builtin_topic_data,
                        None,
                        Some(Arc::new(tests::change_callback)),
                        Some(Arc::new(tests::status_callback)),
                    )
                    .unwrap();

                // Wait to yield Reader lock to SEDP Logic first
                thread::sleep(StdDuration::from_secs(10));

                if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                    for _i in 0..1000 {
                        println!(
                            "is reader proxy list empty?: {}",
                            stateful_reader.writer_proxies().lock().unwrap().is_empty()
                        );
                        thread::sleep(StdDuration::from_secs(1));
                    }
                }
            }
            Err(e) => {
                log::error!("dcps_bridge lock error: {:?}", e);
            }
        };
    }

    #[test]
    fn test_dcps_bridge_remove_remote_writer_from_reliable_reader() {
        let domain_id = unique_domain_id();
        let test_topic_name = "remove_writer_topic";
        let test_type_name = "HelloWorld";

        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let reader_qos = DataReaderQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            ..Default::default()
        };

        let mut subscription_builtin_topic_data = SubscriptionBuiltinTopicData::new(
            &reader_qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        subscription_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        subscription_builtin_topic_data.set_type_name(test_type_name.to_string());

        let mut guard: std::sync::MutexGuard<'_, DcpsBridge> = dcps_bridge.lock().unwrap();
        guard.init().unwrap();

        let guid = guard.next_entity_guid(EntityKind::USER_DEFINED_READER_NO_KEY);
        subscription_builtin_topic_data.set_endpoint_guid(guid);

        // Create DataReader
        let reader = guard
            .create_rtps_reader(
                subscription_builtin_topic_data,
                None,
                Some(Arc::new(tests::change_callback)),
                Some(Arc::new(tests::status_callback)),
            )
            .unwrap();

        let remote_writer_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_WRITER_NO_KEY));

        let stateful_reader = reader.as_any().downcast_ref::<StatefulReader>().unwrap();

        // Add mocked writer proxy
        stateful_reader.matched_writer_add(WriterProxy::new(
            remote_writer_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            0,
            PublicationBuiltinTopicData::default(),
            Arc::new(Mutex::new(None)),
        ));
        assert!(
            stateful_reader.writer_proxies().lock().unwrap().len() == 1,
            "WriterProxy list should contain 1 element after addition"
        );

        // Remove mocked writer proxy
        guard
            .participant
            .cleanup_resources_for_remote_writer(remote_writer_guid, test_topic_name)
            .unwrap();
        assert!(
            stateful_reader.writer_proxies().lock().unwrap().is_empty(),
            "WriterProxy list should be empty after removal"
        );
    }

    #[test]
    fn test_dcps_bridge_remove_remote_reader_from_besteffort_writer() {
        let domain_id = unique_domain_id();
        let test_topic_name = "remove_writer_topic";
        let test_type_name = "HelloWorld";

        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let writer_qos = DataWriterQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::BestEffort,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            ..Default::default()
        };

        let mut publication_builtin_topic_data = PublicationBuiltinTopicData::new(
            &writer_qos,
            &PublisherQos::default(),
            &TopicQos::default(),
        );
        publication_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        publication_builtin_topic_data.set_type_name(test_type_name.to_string());

        let mut guard = dcps_bridge.lock().unwrap();
        guard.init().unwrap();

        let guid = guard.next_entity_guid(EntityKind::USER_DEFINED_WRITER_NO_KEY);
        publication_builtin_topic_data.set_endpoint_guid(guid);

        // Create DataWriter
        let writer = guard
            .create_rtps_writer(
                publication_builtin_topic_data,
                Some(Arc::new(tests::status_callback)),
            )
            .unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_NO_KEY));

        let stateless_writer = writer.as_any().downcast_ref::<StatelessWriter>().unwrap();

        // Add mocked reader locator
        stateless_writer.reader_locator_add(ReaderLocator::new(
            Locator::new(1, 1000, [0; 16]),
            None,
            false,
            remote_reader_guid.prefix(),
            remote_reader_guid.entity_id(),
            SubscriptionBuiltinTopicData::default(),
        ));
        assert!(
            stateless_writer.reader_locator().lock().unwrap().len() == 1,
            "ReaderLocator list should contain 1 element after addition"
        );

        // Remove mocked reader locator
        guard
            .participant
            .cleanup_resources_for_remote_reader(remote_reader_guid, test_topic_name)
            .unwrap();
        assert!(
            stateless_writer.reader_locator().lock().unwrap().is_empty(),
            "ReaderLocator list should be empty after removal"
        );
    }

    #[test]
    fn test_dcps_bridge_remove_remote_reader_from_reliable_writer() {
        let domain_id = unique_domain_id();
        let test_topic_name = "remove_writer_topic";
        let test_type_name = "HelloWorld";

        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let writer_qos = DataWriterQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            ..Default::default()
        };

        let mut publication_builtin_topic_data = PublicationBuiltinTopicData::new(
            &writer_qos,
            &PublisherQos::default(),
            &TopicQos::default(),
        );
        publication_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        publication_builtin_topic_data.set_type_name(test_type_name.to_string());

        let mut guard = dcps_bridge.lock().unwrap();
        guard.init().unwrap();

        let guid = guard.next_entity_guid(EntityKind::USER_DEFINED_WRITER_NO_KEY);
        publication_builtin_topic_data.set_endpoint_guid(guid);

        // Create DataWriter
        let writer = guard
            .create_rtps_writer(
                publication_builtin_topic_data,
                Some(Arc::new(tests::status_callback)),
            )
            .unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_NO_KEY));

        let stateful_writer = writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        // Add mocked reader proxy
        stateful_writer.matched_reader_add(ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 0 },
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        ));
        assert!(
            stateful_writer.reader_proxies().lock().unwrap().len() == 1,
            "MatchedReaders list should contain 1 element after addition"
        );

        // Remove mocked reader proxy
        guard
            .participant
            .cleanup_resources_for_remote_reader(remote_reader_guid, test_topic_name)
            .unwrap();
        assert!(
            stateful_writer.reader_proxies().lock().unwrap().is_empty(),
            "MatchedReaders list should be empty after removal"
        );
    }

    #[test]
    fn test_dcps_bridge_remove_remote_reader_multiple_writer_with_multiple_matches() {
        let domain_id = unique_domain_id();
        let test_topic_name = "remove_writer_topic";
        let test_type_name = "HelloWorld";

        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        let writer_qos = DataWriterQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            ..Default::default()
        };

        let mut publication_builtin_topic_data = PublicationBuiltinTopicData::new(
            &writer_qos,
            &PublisherQos::default(),
            &TopicQos::default(),
        );
        publication_builtin_topic_data.set_topic_name(test_topic_name.to_string());
        publication_builtin_topic_data.set_type_name(test_type_name.to_string());

        let mut guard = dcps_bridge.lock().unwrap();
        guard.init().unwrap();

        let guid1 = guard.next_entity_guid(EntityKind::USER_DEFINED_WRITER_NO_KEY);
        publication_builtin_topic_data.set_endpoint_guid(guid1);

        // Create DataWriter
        let writer_1 = guard
            .create_rtps_writer(
                publication_builtin_topic_data.clone(),
                Some(Arc::new(tests::status_callback)),
            )
            .unwrap();

        let guid2 = guard.next_entity_guid(EntityKind::USER_DEFINED_WRITER_NO_KEY);
        publication_builtin_topic_data.set_endpoint_guid(guid2);

        let writer_2 = guard
            .create_rtps_writer(
                publication_builtin_topic_data.clone(),
                Some(Arc::new(tests::status_callback)),
            )
            .unwrap();

        let remote_reader_guid_1 =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_NO_KEY));

        let remote_reader_guid_2 =
            Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_READER_NO_KEY));

        let _remote_reader_guid_3 =
            Guid::new([2; 12], EntityId::new([0, 0, 2], EntityKind::USER_DEFINED_READER_NO_KEY));

        let stateful_writer_1 = writer_1.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let stateful_writer_2 = writer_2.as_any().downcast_ref::<StatefulWriter>().unwrap();

        // Add 3 mocked reader proxies
        stateful_writer_1.matched_reader_add(ReaderProxy::new(
            remote_reader_guid_1,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 0 },
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        ));

        stateful_writer_1.matched_reader_add(ReaderProxy::new(
            remote_reader_guid_2,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 0 },
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        ));

        stateful_writer_2.matched_reader_add(ReaderProxy::new(
            remote_reader_guid_1,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 0 },
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        ));
        assert!(
            stateful_writer_1.reader_proxies().lock().unwrap().len() == 2,
            "Stateful writer 1's reader proxy list should contain 2 elements after addition"
        );
        assert!(
            stateful_writer_2.reader_proxies().lock().unwrap().len() == 1,
            "Stateful writer 2's reader proxy list should contain 1 elements after addition"
        );

        // Remove mocked reader proxy
        guard
            .participant
            .cleanup_resources_for_remote_reader(remote_reader_guid_1, test_topic_name)
            .unwrap();
        assert!(
            stateful_writer_1.reader_proxies().lock().unwrap().len() == 1,
            "Stateful writer 1's reader proxy list should contain 1 elements after removal"
        );
        assert!(
            stateful_writer_2.reader_proxies().lock().unwrap().is_empty(),
            "Stateful writer 2's reader proxy list should contain 0 after removal"
        );
    }

    #[test]
    fn test_remove_unmatched_endpoint_from_terminated_participant() {
        let domain_id = unique_domain_id();
        let dcps_bridge =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        // Create Writer
        let mut publication_builtin_topic_data = PublicationBuiltinTopicData::default();

        let writer_guid = Guid::new(
            [0; 12],
            EntityId { entity_key: [0, 0, 1], entity_kind: EntityKind::USER_DEFINED_WRITER_NO_KEY },
        );
        publication_builtin_topic_data.set_endpoint_guid(writer_guid);

        let mut guard = dcps_bridge.lock().unwrap();
        guard.init().unwrap();

        let writer = guard
            .create_rtps_writer(
                publication_builtin_topic_data.clone(),
                Some(Arc::new(tests::status_callback)),
            )
            .unwrap();

        let stateful_writer = writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        // Mirror SEDP discovery: register each remote reader in the participant's
        // subscription catalog AND attach a proxy to the writer. The catalog is
        // what prefix-based cleanup keys off in production.
        let topic_name = publication_builtin_topic_data.topic_name().to_string();
        let remote_readers = [
            Guid::new(
                [0; 12], // REMOTE PREFIX 1
                EntityId {
                    entity_key: [0, 0, 1],
                    entity_kind: EntityKind::USER_DEFINED_READER_NO_KEY,
                },
            ),
            Guid::new(
                [5; 12], // REMOTE PREFIX 2
                EntityId {
                    entity_key: [0, 0, 1],
                    entity_kind: EntityKind::USER_DEFINED_READER_NO_KEY,
                },
            ),
            Guid::new(
                [5; 12], // REMOTE PREFIX 2
                EntityId {
                    entity_key: [0, 0, 2],
                    entity_kind: EntityKind::USER_DEFINED_READER_NO_KEY,
                },
            ),
        ];
        for reader_guid in remote_readers {
            let mut sub_data = SubscriptionBuiltinTopicData::default();
            sub_data.set_endpoint_guid(reader_guid);
            guard
                .participant
                .remote_subscriptions()
                .entry(topic_name.clone())
                .or_default()
                .insert(reader_guid, sub_data.clone());

            stateful_writer.matched_reader_add(ReaderProxy::new(
                reader_guid,
                EntityId::UNKNOWN,
                Vec::new(),
                Vec::new(),
                SequenceNumber { high: 0, low: 0 },
                SequenceNumber { high: 0, low: 0 },
                false,
                true,
                sub_data,
                SequenceNumber::new(0, 0),
            ));
        }

        assert!(
            stateful_writer.reader_proxies().lock().unwrap().len() == 3,
            "Stateful writer's reader proxy list should contain 3 elements after addition"
        );

        // This would remove 2 mocked reader proxies
        guard
            .participant
            .remove_all_unmatched_endpoint_from_terminated_participant([5; 12])
            .unwrap();

        assert!(
            stateful_writer.reader_proxies().lock().unwrap().len() == 1,
            "Stateful writer's reader proxy list should contain 1 elements after removal"
        );
    }

    #[test]
    #[ignore]
    fn test_delete_participant() {
        // Test first participant
        let domain_id = unique_domain_id();
        let dcps_bridge_1 =
            Arc::new(Mutex::new(DcpsBridge::new(domain_id as u32, &Default::default()).unwrap()));

        {
            let mut bridge_guard = dcps_bridge_1.lock().unwrap();
            bridge_guard.init().unwrap();

            // Verify initialization
            if let Some(ref sedp_logic) = bridge_guard.sedp_logic.as_ref() {
                assert!(sedp_logic
                    .get_multicast_listening_handle()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .is_some());
                assert!(sedp_logic
                    .get_unicast_listening_handle()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .is_some());
            }

            if let Some(ref user_logic) = bridge_guard.user_logic.as_ref() {
                assert!(user_logic
                    .get_unicast_listening_handle()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .is_some());
            }

            // Explicitly call disable
            let disable_result = bridge_guard.disable();
            assert!(disable_result.is_ok(), "disable should succeed");

            // Verify termination
            if let Some(ref sedp_logic) = bridge_guard.sedp_logic.as_ref() {
                assert!(sedp_logic
                    .get_multicast_listening_handle()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .is_none());
                assert!(sedp_logic
                    .get_unicast_listening_handle()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .is_none());
            }

            if let Some(ref user_logic) = bridge_guard.user_logic.as_ref() {
                assert!(user_logic
                    .get_unicast_listening_handle()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .is_none());
            }
        }
    }
}
