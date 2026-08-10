//! SEDP (Simple Endpoint Discovery Protocol) logic implementation.
//!
//! This module implements the SEDP protocol for discovering DataReaders and DataWriters.
//! SEDP announces local endpoints and processes endpoint announcements from remote
//! participants to enable reader-writer matching.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    thread::{self, JoinHandle},
    time::{Duration as StdDuration, Instant},
};

use log::{debug, error, warn};
use speedy::{Endianness, Writable};

use crate::{
    common::{
        builtin::topic::{
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    dcps::infrastructure::status::StatusKind,
    infrastructure::qos_policy::{DurabilityQosPolicyKind, QosPolicyId, ReliabilityQosPolicyKind},
    rtps::{
        builtin::{
            builtin_endpoints::BuiltinEndpoints,
            data::{
                content_filtered_topic::ContentFilterProperty,
                discovered_data::{DiscoveredReaderData, DiscoveredWriterData},
                spdp_discovered_participant_data::SPDPDiscoveredParticipantData,
            },
        },
        common::{
            count_filter::should_accept_count,
            entity_id::EntityId,
            guid::{Guid, GuidPrefix},
            locator::{
                Locator, LOCATOR_KIND_SHM, LOCATOR_KIND_TCP_V4, LOCATOR_KIND_TCP_V6,
                LOCATOR_KIND_UDP_V4, LOCATOR_KIND_UDP_V6,
            },
            parameters::ParameterList,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            types::ChangeKind,
        },
        entities::{
            entity::Entity,
            history::{cache_change::CacheChange, history_cache::HistoryCache},
            participant::Participant,
            qos::{check_qos_compatibility, check_qos_compatibility_with_policy_id},
            reader::{Reader, RemoteWriterInfo, StatefulReader, StatelessReader, WriterProxy},
            writer::{
                reader_locator::ReaderLocator, reader_proxy::ReaderProxy, StatefulWriter,
                StatelessWriter, Writer,
            },
        },
        logic::{
            common::{
                impl_multicast_thread_handler, impl_participant_accessor,
                impl_unicast_thread_handler, JoinAllThread, MulticastThreadHandler,
                ParticipantAccessor, UnicastThreadHandler,
            },
            data::builtin_endpoint_pair::BuiltinEndpointPair,
            message_processor::{
                participant_message_processor::ParticipantMessageProcessor,
                unicast_message_processor::UnicastMessageProcessor,
            },
        },
        messages::{
            header::Header,
            message_creator::MessageCreator,
            message_receiver::MessageReceiver,
            sedp_message::SEDPMessage,
            submessage_header::SubmessageHeader,
            submessages::{ack_nack::AckNack, data::Data, gap::Gap, heartbeat::Heartbeat},
        },
        task::{
            discovery_traffic::{
                discovery_multicast_listening_task::DiscoveryMulticastListeningTask,
                discovery_unicast_listening_task::DiscoveryUnicastListeningTask,
            },
            sending_handler::{MessageType, SendingHandler},
        },
        transport::plugin::{MessageSource, SendTarget, TransportPlugin},
    },
    serialize::pl_cdr::InlineQosParameters,
    utils::timer::{timer_handler::TimerHandler, timer_id::TimerId},
    xtypes::{
        evaluate_structural_compatibility, SampleIdentity, TypeCompatibility, TypeIdentifier,
        TypeObject,
    },
};

enum MatchType {
    ReaderPublication,
    WriterSubscription,
}

enum BuiltinTopicData {
    Publication(PublicationBuiltinTopicData),
    Subscription(SubscriptionBuiltinTopicData),
}

/// Outcome of endpoint compatibility validation.
enum MatchDecision {
    Match,
    Reject(RtpsError),
    Defer,
}

const TYPE_LOOKUP_MATCH_TIMEOUT: StdDuration = StdDuration::from_secs(5);

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct SedpLogic {
    participant: Weak<Participant>,
    builtin_endpoints: Arc<BuiltinEndpoints>,
    spdp_message: Option<Arc<Vec<u8>>>,
    transport: Arc<dyn TransportPlugin>,
    multicast_listening_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    unicast_listening_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    multicast_listening_waker: Arc<std::sync::OnceLock<Arc<mio::Waker>>>,
    unicast_listening_waker: Arc<std::sync::OnceLock<Arc<mio::Waker>>>,
    timer_handler: Arc<Mutex<TimerHandler>>,
    pub(crate) type_lookup_pending:
        Arc<Mutex<HashMap<SampleIdentity, (GuidPrefix, TypeIdentifier)>>>,
    deferred_type_matches: Arc<Mutex<HashMap<Guid, Instant>>>,
}

impl SedpLogic {
    #[allow(clippy::too_many_arguments)]
    fn validate_endpoint_compatibility<L>(
        &self,
        local: &L,
        remote_guid: Guid,
        requested: &SubscriptionBuiltinTopicData,
        offered: &PublicationBuiltinTopicData,
        update_incompatible_qos: impl Fn(&L, QosPolicyId),
        update_incompatible_type: impl Fn(&L),
        update_inconsistent_topic: impl Fn(&L),
        who: &'static str, // for log
    ) -> MatchDecision {
        // TopicKind - reject if writer and reader disagree on keyed vs keyless
        let writer_keyed = offered.endpoint_guid().entity_kind().is_with_key();
        let reader_keyed = requested.endpoint_guid().entity_kind().is_with_key();
        if writer_keyed != reader_keyed {
            update_inconsistent_topic(local);
            debug!(
                "[{}] TopicKind mismatch: writer keyed={}, reader keyed={}",
                who, writer_keyed, reader_keyed
            );
            return MatchDecision::Reject(RtpsError::new(
                RtpsErrorCode::TopicKindIncompatible,
                format!(
                    "[TopicKind mismatch: writer keyed={}, reader keyed={} :{}]",
                    writer_keyed, reader_keyed, who
                ),
            ));
        }

        // QoS
        if !check_qos_compatibility(requested, offered) {
            if let Some(pid) = check_qos_compatibility_with_policy_id(requested, offered) {
                update_incompatible_qos(local, pid);
            }
            return MatchDecision::Reject(RtpsError::new(
                RtpsErrorCode::QosIncompatible,
                format!("[QoS failed :{}]", who),
            ));
        }

        // Partition
        if !is_partition_compatible(&requested.partition().name, &offered.partition().name) {
            return MatchDecision::Reject(RtpsError::new(
                RtpsErrorCode::PartitionIncompatible,
                format!("[Partition failed :{}]", who),
            ));
        }

        // Type Compatibility (DDS-XTypes): resolve via registry, defer if unknown.
        match self.evaluate_type_match(offered, requested, remote_guid) {
            MatchDecision::Reject(e) => {
                debug!(
                    "[{}] Type compatibility check failed: writer={:?}, reader={:?}",
                    who,
                    offered.type_identifier(),
                    requested.type_identifier()
                );
                update_incompatible_type(local);
                MatchDecision::Reject(e)
            }
            decision => decision,
        }
    }

    fn evaluate_type_match(
        &self,
        offered: &PublicationBuiltinTopicData,
        requested: &SubscriptionBuiltinTopicData,
        remote_guid: Guid,
    ) -> MatchDecision {
        let w_id = offered.type_identifier();
        let r_id = requested.type_identifier();
        let tce = requested.type_consistency_enforcement();

        let participant = match self.get_upgraded_participant() {
            Ok(p) => p,
            Err(_) => return MatchDecision::Defer,
        };
        let type_registry = participant.type_registry();
        let compat = match type_registry.read() {
            Ok(registry) => evaluate_structural_compatibility(
                w_id,
                r_id,
                offered.type_object(),
                requested.type_object(),
                tce,
                &*registry,
            ),
            Err(_) => return MatchDecision::Defer,
        };

        match compat {
            TypeCompatibility::Compatible => {
                self.clear_deferred(remote_guid);
                MatchDecision::Match
            }
            TypeCompatibility::Incompatible(_) => {
                self.clear_deferred(remote_guid);
                MatchDecision::Reject(RtpsError::new(
                    RtpsErrorCode::QosIncompatible,
                    "[Type compatibility failed]",
                ))
            }
            TypeCompatibility::Indeterminate => {
                self.decide_deferred(remote_guid, tce.force_type_validation)
            }
        }
    }

    fn decide_deferred(&self, remote_guid: Guid, force_validation: bool) -> MatchDecision {
        let now = Instant::now();
        let mut map = match self.deferred_type_matches.lock() {
            Ok(m) => m,
            Err(_) => return MatchDecision::Defer,
        };
        match map.get(&remote_guid).copied() {
            Some(started) => {
                if now.duration_since(started) < TYPE_LOOKUP_MATCH_TIMEOUT {
                    return MatchDecision::Defer;
                }
                map.remove(&remote_guid);
                drop(map);
                if force_validation {
                    MatchDecision::Reject(RtpsError::new(
                        RtpsErrorCode::QosIncompatible,
                        "[Type resolution timed out with force_type_validation]",
                    ))
                } else {
                    MatchDecision::Match
                }
            }
            None => {
                map.insert(remote_guid, now);
                drop(map);
                self.register_deferred_match_timer(remote_guid);
                MatchDecision::Defer
            }
        }
    }

    fn clear_deferred(&self, remote_guid: Guid) {
        if let Ok(mut map) = self.deferred_type_matches.lock() {
            map.remove(&remote_guid);
        }
    }

    fn register_deferred_match_timer(&self, remote_guid: Guid) {
        let timer_id = TimerId::DeferredTypeMatch { remote_guid };
        let this = self.clone();
        let callback = move || this.re_match_resolved_types(remote_guid.prefix());
        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(timer_id, TYPE_LOOKUP_MATCH_TIMEOUT, false, callback);
        }
    }

    pub(crate) fn re_match_resolved_types(&self, remote_prefix: GuidPrefix) {
        let participant = match self.get_upgraded_participant() {
            Ok(p) => p,
            Err(_) => return,
        };

        // Snapshot first to avoid holding DashMap shard locks during matching.
        let pubs: Vec<(String, PublicationBuiltinTopicData)> = participant
            .remote_publications()
            .iter()
            .flat_map(|e| {
                e.value()
                    .iter()
                    .filter(|(g, _)| g.prefix() == remote_prefix)
                    .map(|(_, d)| (e.key().clone(), d.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        for (topic, data) in pubs {
            for reader in participant.find_readers_from_topic_name(&topic) {
                self.match_reader_with_publication(reader, data.clone());
            }
        }

        let subs: Vec<(String, SubscriptionBuiltinTopicData)> = participant
            .remote_subscriptions()
            .iter()
            .flat_map(|e| {
                e.value()
                    .iter()
                    .filter(|(g, _)| g.prefix() == remote_prefix)
                    .map(|(_, d)| (e.key().clone(), d.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        for (topic, data) in subs {
            for writer in participant.find_writers_from_topic_name(&topic) {
                let _ = self.match_writer_with_subscription(writer, data.clone());
            }
        }
    }
}

fn is_partition_compatible(requested: &[String], offered: &[String]) -> bool {
    if requested.is_empty() && offered.is_empty() {
        return true;
    }
    let requested = if requested.is_empty() { vec!["".into()] } else { requested.to_vec() };
    let offered = if offered.is_empty() { vec!["".into()] } else { offered.to_vec() };

    for r in &requested {
        for o in &offered {
            if r == o || r == "*" || o == "*" {
                return true;
            }
            if let Some(prefix) = r.strip_suffix('*') {
                if o.starts_with(prefix) {
                    return true;
                }
            }
            if let Some(suffix) = r.strip_prefix('*') {
                if o.ends_with(suffix) {
                    return true;
                }
            }
        }
    }
    false
}

/// Initialization
impl SedpLogic {
    pub(crate) fn new(participant: Arc<Participant>, transport: Arc<dyn TransportPlugin>) -> Self {
        let builtin_endpoints = participant.builtin_endpoints();
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());
        Self {
            participant: Arc::downgrade(&participant),
            builtin_endpoints,
            spdp_message: None,
            transport,
            multicast_listening_handle: Arc::new(Mutex::new(None)),
            unicast_listening_handle: Arc::new(Mutex::new(None)),
            multicast_listening_waker: Arc::new(std::sync::OnceLock::new()),
            unicast_listening_waker: Arc::new(std::sync::OnceLock::new()),
            timer_handler,
            type_lookup_pending: Arc::new(Mutex::new(HashMap::new())),
            deferred_type_matches: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn wake_listening_threads(&self) {
        if let Some(waker) = self.multicast_listening_waker.get() {
            let _ = waker.wake();
        }
        if let Some(waker) = self.unicast_listening_waker.get() {
            let _ = waker.wake();
        }
    }

    #[allow(clippy::clone_on_copy)]
    pub(crate) fn start_sedp(
        &self,
        discovery_multicast_source: Option<MessageSource>,
        discovery_unicast_source: Option<MessageSource>,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let participant_guid = participant.guid().clone();

        // multicast listening (only if transport provides a multicast source)
        if let Some(multicast_source) = discovery_multicast_source {
            let mut discovery_multicast_listening_task =
                DiscoveryMulticastListeningTask::new(participant.clone());
            discovery_multicast_listening_task
                .set_shutdown_waker(self.multicast_listening_waker.clone());

            let mc_guid = participant_guid;
            let multicast_handle = thread::Builder::new()
                .name("discovery_traffic_multicast_listening".to_string())
                .spawn(move || {
                    {
                        use crate::rtps::task::thread_monitor::ThreadMonitor;
                        ThreadMonitor::register_current_thread_name_with_guid_prefix(
                            "discovery_traffic_multicast_listening",
                            mc_guid.prefix(),
                        );
                    }

                    let _ =
                        discovery_multicast_listening_task.multicast_listening(multicast_source);
                    {
                        use crate::rtps::task::thread_monitor::ThreadMonitor;
                        ThreadMonitor::remove_map_guard();
                    }
                    debug!("discovery multicast listening thread finished");
                })
                .expect("Failed to create discovery multicast listening thread");

            if let Ok(mut handle_guard) = self.multicast_listening_handle.lock() {
                *handle_guard = Some(multicast_handle);
            }
        }

        // unicast listening (only if transport provides a unicast source)
        if let Some(unicast_source) = discovery_unicast_source {
            let mut discovery_unicast_listening_task =
                DiscoveryUnicastListeningTask::new(participant.clone());
            discovery_unicast_listening_task
                .set_shutdown_waker(self.unicast_listening_waker.clone());

            let unicast_guid = participant_guid;
            let unicast_handle = thread::Builder::new()
                .name("discovery_traffic_unicast_listening".to_string())
                .spawn(move || {
                    {
                        use crate::rtps::task::thread_monitor::ThreadMonitor;
                        ThreadMonitor::register_current_thread_name_with_guid_prefix(
                            "discovery_traffic_unicast_listening",
                            unicast_guid.prefix(),
                        );
                    }

                    let _ = discovery_unicast_listening_task.unicast_listening(unicast_source);
                    {
                        use crate::rtps::task::thread_monitor::ThreadMonitor;
                        ThreadMonitor::remove_map_guard();
                    }
                    debug!("discovery unicast listening thread finished");
                })
                .expect("Failed to create discovery unicast listening thread");

            if let Ok(mut handle_guard) = self.unicast_listening_handle.lock() {
                *handle_guard = Some(unicast_handle);
            }
        }

        Ok(())
    }
}

/// Endpoint Matching
impl SedpLogic {
    pub(crate) fn match_writer_with_subscription(
        &self,
        writer: Arc<dyn Writer + Send + Sync>,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> RtpsResult<()> {
        self.match_endpoint(
            MatchType::WriterSubscription,
            writer.as_any(),
            BuiltinTopicData::Subscription(subscription_builtin_topic_data),
            false,
        )
    }

    pub(crate) fn match_reader_with_publication(
        &self,
        reader: Arc<dyn Reader + Send + Sync>,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) {
        let _ = self.match_endpoint(
            MatchType::ReaderPublication,
            reader.as_any(),
            BuiltinTopicData::Publication(publication_builtin_topic_data),
            false,
        );
    }

    fn match_endpoint(
        &self,
        match_type: MatchType,
        endpoint: &dyn std::any::Any,
        builtin_topic_data: BuiltinTopicData,
        skip_cross_match: bool, // To prevent infinite recursion during cross-matching
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        match (match_type, builtin_topic_data) {
            (MatchType::ReaderPublication, BuiltinTopicData::Publication(mut publication_data)) => {
                if let Some(stateful_reader) = endpoint.downcast_ref::<StatefulReader>() {
                    self.handle_empty_locator_lists_for_publication(
                        &mut publication_data,
                        &participant,
                    );

                    if let Err(e) = self.handle_stateful_reader_publication(
                        stateful_reader,
                        publication_data.clone(),
                    ) {
                        debug!("match_endpoint failed: {:?}", e);
                    }

                    // Match for local endpoints only if not skipped
                    if !skip_cross_match {
                        self.check_if_local_and_cross_match(
                            publication_data.endpoint_guid(),
                            BuiltinTopicData::Subscription(
                                stateful_reader.subscription_builtin_topic_data()?,
                            ),
                        )?;
                    }
                } else if let Some(stateless_reader) = endpoint.downcast_ref::<StatelessReader>() {
                    self.handle_empty_locator_lists_for_publication(
                        &mut publication_data,
                        &participant,
                    );

                    if let Err(e) = self.handle_stateless_reader_publication(
                        stateless_reader,
                        publication_data.clone(),
                    ) {
                        debug!("match_endpoint failed: {:?}", e);
                    }

                    // Match for local endpoints only if not skipped
                    if !skip_cross_match {
                        self.check_if_local_and_cross_match(
                            publication_data.endpoint_guid(),
                            BuiltinTopicData::Subscription(
                                stateless_reader.subscription_builtin_topic_data()?,
                            ),
                        )?;
                    }
                } else {
                    warn!("SEDP Logic: reader is not StatefulReader or StatelessReader");
                    return Err(RtpsError::new(
                        RtpsErrorCode::DowncastError,
                        "Unknown reader type",
                    ));
                }

                Ok(())
            }
            (
                MatchType::WriterSubscription,
                BuiltinTopicData::Subscription(mut subscription_data),
            ) => {
                if let Some(stateful_writer) = endpoint.downcast_ref::<StatefulWriter>() {
                    self.handle_empty_locator_lists(&mut subscription_data, &participant);

                    if let Err(e) = self.handle_stateful_writer_subscription(
                        stateful_writer,
                        subscription_data.clone(),
                    ) {
                        debug!("match_endpoint failed: {:?}", e);
                    }

                    // Match for local endpoints only if not skipped
                    if !skip_cross_match {
                        self.check_if_local_and_cross_match(
                            subscription_data.endpoint_guid(),
                            BuiltinTopicData::Publication(
                                stateful_writer.publication_builtin_topic_data()?,
                            ),
                        )?;
                    }
                } else if let Some(stateless_writer) = endpoint.downcast_ref::<StatelessWriter>() {
                    self.handle_empty_locator_lists(&mut subscription_data, &participant);

                    if let Err(e) = self.handle_stateless_writer_subscription(
                        stateless_writer,
                        subscription_data.clone(),
                    ) {
                        debug!("match_endpoint failed: {:?}", e);
                    }

                    // Match for local endpoints only if not skipped
                    if !skip_cross_match {
                        self.check_if_local_and_cross_match(
                            subscription_data.endpoint_guid(),
                            BuiltinTopicData::Publication(
                                stateless_writer.publication_builtin_topic_data()?,
                            ),
                        )?;
                    }
                } else {
                    warn!(
                        "SEDP Logic: Unknown writer type, cannot determine subscription handling"
                    );
                    return Err(RtpsError::new(
                        RtpsErrorCode::DowncastError,
                        "Unknown writer type",
                    ));
                }
                Ok(())
            }
            _ => Err(RtpsError::new(
                RtpsErrorCode::DowncastError,
                "Mismatched direction and data type",
            )),
        }
    }

    fn check_if_local_and_cross_match(
        &self,
        endpoint_guid: Guid,
        builtin_topic_data: BuiltinTopicData,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        if participant.guid().prefix() != endpoint_guid.prefix() {
            debug!("Not local endpoint, skipping matching for GUID: {}", endpoint_guid);
            return Ok(());
        }

        debug!("Local endpoint detected, proceeding to match for GUID: {}", endpoint_guid);
        if let BuiltinTopicData::Publication(publication_builtin_topic_data) = builtin_topic_data {
            let local_reader = participant
                .find_reader_from_entity_id(endpoint_guid.entity_id())
                .ok_or_else(|| {
                    RtpsError::new(
                        RtpsErrorCode::RtpsEntityNotFound,
                        "Local reader should exist since it was found on remote_subscriptions()",
                    )
                })?;

            self.match_endpoint(
                MatchType::ReaderPublication,
                local_reader.as_any(),
                BuiltinTopicData::Publication(publication_builtin_topic_data),
                true,
            )?;
        } else if let BuiltinTopicData::Subscription(subscription_builtin_topic_data) =
            builtin_topic_data
        {
            let local_writer = participant
                .find_writer_from_entity_id(endpoint_guid.entity_id())
                .ok_or_else(|| {
                    RtpsError::new(
                        RtpsErrorCode::RtpsEntityNotFound,
                        "Local writer should exist since it was found on remote_subscriptions()",
                    )
                })?;

            self.match_endpoint(
                MatchType::WriterSubscription,
                local_writer.as_any(),
                BuiltinTopicData::Subscription(subscription_builtin_topic_data),
                true,
            )?;
        }

        Ok(())
    }
}

/// Subscription Handling (Local Writer <-> Remote Reader)
impl SedpLogic {
    fn handle_subscription_builtin_topic_data(
        &self,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
        inline_qos_params: Option<ParameterList>,
        content_filter: Option<ContentFilterProperty>,
    ) -> RtpsResult<()> {
        let topic_name = subscription_builtin_topic_data.topic_name();
        let endpoint_guid = subscription_builtin_topic_data.endpoint_guid();

        let participant = self.get_upgraded_participant()?;

        // If STATUS_INFO indicates disposed or unregistered, remove remote reader and return early
        if let Some(inline_qos_params) = inline_qos_params {
            if let Some(status_info) = inline_qos_params.get_status_info() {
                if status_info.disposed() || status_info.unregistered() {
                    debug!("Received Data(r[UD])");

                    // Terminating endpoint provides GUID via KeyHash, but sometimes sends DATA message without SerializedData payload
                    let terminated_reader_guid =
                        if let Some(key_hash) = inline_qos_params.get_key_hash() {
                            key_hash.to_guid()
                        } else {
                            endpoint_guid
                        };

                    participant
                        .cleanup_resources_for_remote_reader(terminated_reader_guid, &topic_name)?;
                    return Ok(());
                }
            } else {
                debug!("ALIVE SubscriptionBuiltinTopicData, no STATUS_INFO parameter found");
            }
        }

        // First try to find local writer using exact match (find_writer_from_entry)
        // If no exact match, try finding writer using domain ID and topic name only
        let writers = participant.find_writers_from_topic_name(&topic_name);

        if !writers.is_empty() {
            for writer in writers {
                let subscription_result = self.match_writer_with_subscription(
                    writer.clone(),
                    subscription_builtin_topic_data.clone(),
                );

                // Apply content filter if present (only for stateful writer and if subscription handling succeeded)
                if let Ok(()) = subscription_result {
                    if let Some(ref filter) = content_filter {
                        if let Some(_stateful_writer) =
                            writer.as_any().downcast_ref::<StatefulWriter>()
                        {
                            self.apply_content_filter_to_reader_proxy(
                                _stateful_writer,
                                endpoint_guid,
                                filter,
                            );
                        }
                    }
                }
            }
        } else {
            debug!(
                "SEDP Logic: No local writer found for topic '{}', buffering remote reader",
                topic_name
            );
        }

        Self::register_discovered_type(
            &participant,
            subscription_builtin_topic_data.type_identifier(),
            subscription_builtin_topic_data.type_object(),
        );
        self.maybe_request_discovered_type(
            endpoint_guid.prefix(),
            subscription_builtin_topic_data.type_identifier(),
            subscription_builtin_topic_data.type_object().is_some(),
        );

        participant
            .remote_subscriptions()
            .entry(topic_name)
            .or_default()
            .insert(endpoint_guid, subscription_builtin_topic_data);

        Ok(())
    }

    #[allow(clippy::option_map_unit_fn)]
    pub(crate) fn handle_stateful_writer_subscription(
        &self,
        writer: &StatefulWriter,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> RtpsResult<()> {
        let endpoint_guid = subscription_builtin_topic_data.endpoint_guid();

        if writer.matched_reader_is_matched(endpoint_guid) {
            // Check QoS compatibility in case of QoS change of writer itself or remote reader
            if let MatchDecision::Reject(e) = self.validate_endpoint_compatibility(
                writer,
                endpoint_guid,
                &subscription_builtin_topic_data,
                &writer.publication_builtin_topic_data()?,
                |w, pid| w.update_offered_incompatible_qos_status(pid),
                |w| w.update_offered_incompatible_type_status(),
                |w| w.update_status(StatusKind::INCONSISTENT_TOPIC, None),
                "writer->reader",
            ) {
                // Incompatible - remove matching
                debug!(
                    "QoS changed for remote reader {}, now incompatible - removing matching",
                    endpoint_guid
                );
                writer
                    .reader_proxies()
                    .lock()
                    .map_err(|lock_err| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock ReaderProxies: {}", lock_err),
                        )
                    })?
                    .retain(|reader_proxy| reader_proxy.remote_reader_guid() != endpoint_guid);

                writer.update_publication_matched_status(
                    -1,
                    InstanceHandle::from_guid(&endpoint_guid),
                );
                return Err(e);
            }

            // Still compatible - update builtin_topic_data just in case QoS has changed
            // it is idempotent behavior
            if writer
                .matched_reader_lookup(endpoint_guid)
                .ok_or(RtpsError::new(
                    RtpsErrorCode::MatchedEntityNotFound,
                    "There is no reader proxy for writer",
                ))?
                .subscription_builtin_topic_data()
                .changeable_qos_equals(&subscription_builtin_topic_data)
            {
                debug!(
                    "Syncing subscription_builtin_topic_data for compatible remote reader {}",
                    endpoint_guid
                );
                writer
                    .reader_proxies()
                    .lock()
                    .map_err(|e| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock ReaderProxies: {}", e),
                        )
                    })?
                    .iter_mut()
                    .find(|proxy| proxy.remote_reader_guid() == endpoint_guid)
                    .map(|proxy| {
                        proxy.set_subscription_builtin_topic_data(subscription_builtin_topic_data)
                    });
            }

            return Ok(());
        }

        match self.validate_endpoint_compatibility(
            writer,
            endpoint_guid,
            &subscription_builtin_topic_data,
            &writer.publication_builtin_topic_data()?,
            |w, pid| w.update_offered_incompatible_qos_status(pid),
            |w| w.update_offered_incompatible_type_status(),
            |w| w.update_status(StatusKind::INCONSISTENT_TOPIC, None),
            "writer->reader",
        ) {
            MatchDecision::Match => {}
            MatchDecision::Defer => return Ok(()),
            MatchDecision::Reject(e) => return Err(e),
        }

        let is_volatile =
            subscription_builtin_topic_data.durability().kind == DurabilityQosPolicyKind::Volatile;
        let is_best_effort = subscription_builtin_topic_data.reliability().kind
            == ReliabilityQosPolicyKind::BestEffort;

        // For reliable volatile readers, last irrelevant SN is last change SN to avoid resending old changes
        let last_irrelevant_sn = if is_volatile {
            writer.last_change_sequence_number()
        } else {
            SequenceNumber::new(0, 0)
        };

        // For best-effort volatile readers, simply use hightest sent change SN to avoid resending old changes
        let highest_sent_change_sn = if is_volatile && is_best_effort {
            writer.last_change_sequence_number()
        } else {
            SequenceNumber::UNKNOWN
        };

        let reader_proxy = ReaderProxy::new(
            subscription_builtin_topic_data.endpoint_guid(),
            subscription_builtin_topic_data.endpoint_guid().entity_id(),
            subscription_builtin_topic_data.unicast_locator_list(),
            subscription_builtin_topic_data.multicast_locator_list(),
            highest_sent_change_sn,
            SequenceNumber::UNKNOWN,
            false,
            true,
            subscription_builtin_topic_data.clone(),
            last_irrelevant_sn,
        );

        writer.matched_reader_add(reader_proxy);

        let writer_guid = writer.guid();
        if writer_guid.entity_id().entity_kind().is_user_defined() {
            let participant = self.get_upgraded_participant()?;
            if let Some(wlp_logic) = participant.wlp_logic() {
                let _ = wlp_logic.register_asserting_writer(writer_guid, writer.liveliness()?);
            }
        }

        writer.update_publication_matched_status(
            1,
            InstanceHandle::from_guid(&subscription_builtin_topic_data.endpoint_guid()),
        );

        if subscription_builtin_topic_data.reliability().kind == ReliabilityQosPolicyKind::Reliable
        {
            self.register_preemptive_heartbeat_timer(
                writer,
                subscription_builtin_topic_data.endpoint_guid(),
            )?;

            // Restart the stopped periodic heartbeat so the new reader can recover history
            if !writer.heartbeat_timer_running()
                && writer.last_change_sequence_number() > SequenceNumber::new(0, 0)
            {
                debug!(
                    "Restarting periodic heartbeat of writer {} for late-joining reader {}",
                    writer.guid(),
                    subscription_builtin_topic_data.endpoint_guid()
                );
                writer.register_periodic_heartbeat_timer_after_delay(writer.heartbeat_period())?;
            }
        }

        Ok(())
    }

    #[allow(clippy::option_map_unit_fn)]
    pub(crate) fn handle_stateless_writer_subscription(
        &self,
        writer: &StatelessWriter,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> RtpsResult<()> {
        debug!("SEDP Logic: SubscriptionBuiltinTopicData is not reliable");
        let endpoint_guid = subscription_builtin_topic_data.endpoint_guid();

        if writer.matched_reader_is_matched(endpoint_guid) {
            // Check QoS compatibility in case of QoS change of writer itself or remote reader
            if let MatchDecision::Reject(e) = self.validate_endpoint_compatibility(
                writer,
                endpoint_guid,
                &subscription_builtin_topic_data,
                &writer.publication_builtin_topic_data()?,
                |w, pid| w.update_offered_incompatible_qos_status(pid),
                |w| w.update_offered_incompatible_type_status(),
                |w| w.update_status(StatusKind::INCONSISTENT_TOPIC, None),
                "writer->reader",
            ) {
                // Incompatible - remove matching
                debug!(
                    "QoS changed for remote reader {}, now incompatible - removing matching",
                    endpoint_guid
                );
                writer
                    .reader_locator()
                    .lock()
                    .map_err(|lock_err| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock ReaderLocator: {}", lock_err),
                        )
                    })?
                    .retain(|reader_locator| reader_locator.remote_reader_guid() != endpoint_guid);

                writer.update_publication_matched_status(
                    -1,
                    InstanceHandle::from_guid(&endpoint_guid),
                );
                error!("StatelessWriter compatibility error -> {}", e);
                return Err(e);
            }

            // Still compatible - update builtin_topic_data just in case QoS has changed
            // it is idempotent behavior
            if writer
                .matched_reader_lookup(endpoint_guid)
                .ok_or(RtpsError::new(
                    RtpsErrorCode::MatchedEntityNotFound,
                    "There is no reader proxy for writer",
                ))?
                .subscription_builtin_topic_data()
                .changeable_qos_equals(&subscription_builtin_topic_data)
            {
                debug!(
                    "Syncing subscription_builtin_topic_data for compatible remote reader {}",
                    endpoint_guid
                );
                writer
                    .reader_locator()
                    .lock()
                    .map_err(|e| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock ReaderLocator: {}", e),
                        )
                    })?
                    .iter_mut()
                    .find(|locator| locator.remote_reader_guid() == endpoint_guid)
                    .map(|locator| {
                        locator.set_subscription_builtin_topic_data(subscription_builtin_topic_data)
                    });
            }

            return Ok(());
        }

        match self.validate_endpoint_compatibility(
            writer,
            endpoint_guid,
            &subscription_builtin_topic_data,
            &writer.publication_builtin_topic_data()?,
            |w, pid| w.update_offered_incompatible_qos_status(pid),
            |w| w.update_offered_incompatible_type_status(),
            |w| w.update_status(StatusKind::INCONSISTENT_TOPIC, None),
            "writer->reader",
        ) {
            MatchDecision::Match => {}
            MatchDecision::Defer => return Ok(()),
            MatchDecision::Reject(e) => {
                error!("StatelessWriter compatibility error -> {}", e);
                return Err(e);
            }
        }

        let mut highest_sent_change_sn = None;

        // For Volatile, assume CacheChanges before matching were already sent, so don't resend
        if subscription_builtin_topic_data.durability().kind == DurabilityQosPolicyKind::Volatile {
            highest_sent_change_sn = Some(writer.last_change_sequence_number());
        }

        for locator in subscription_builtin_topic_data.unicast_locator_list() {
            if !(locator.kind() == LOCATOR_KIND_UDP_V4
                || locator.kind() == LOCATOR_KIND_UDP_V6
                || locator.kind() == LOCATOR_KIND_TCP_V4
                || locator.kind() == LOCATOR_KIND_TCP_V6
                || locator.kind() == LOCATOR_KIND_SHM)
            {
                continue;
            }
            writer.reader_locator_add(ReaderLocator::new(
                locator.clone(),
                highest_sent_change_sn,
                false,
                subscription_builtin_topic_data.endpoint_guid().prefix(),
                subscription_builtin_topic_data.endpoint_guid().entity_id(),
                subscription_builtin_topic_data.clone(),
            ));
        }
        for locator in subscription_builtin_topic_data.multicast_locator_list() {
            // Note: Currently, builtin_topic_data is sent via unicast only.
            // Multicast support for builtin topics may be added in the future if needed.
            // The following code is kept for reference:
            // if !writer.multicast_locator_list().contains(&locator) {
            //     writer.add_multicast_locator(locator.clone());
            // }
            if !(locator.kind() == LOCATOR_KIND_UDP_V4
                || locator.kind() == LOCATOR_KIND_UDP_V6
                || locator.kind() == LOCATOR_KIND_TCP_V4
                || locator.kind() == LOCATOR_KIND_TCP_V6
                || locator.kind() == LOCATOR_KIND_SHM)
            {
                continue;
            }

            let reader_locator = ReaderLocator::new(
                locator.clone(),
                highest_sent_change_sn,
                false,
                subscription_builtin_topic_data.endpoint_guid().prefix(),
                subscription_builtin_topic_data.endpoint_guid().entity_id(),
                subscription_builtin_topic_data.clone(),
            );
            writer.reader_locator_add(reader_locator);
        }

        writer.update_publication_matched_status(
            1,
            InstanceHandle::from_guid(&subscription_builtin_topic_data.endpoint_guid()),
        );

        Ok(())
    }

    fn handle_empty_locator_lists(
        &self,
        subscription_builtin_topic_data: &mut SubscriptionBuiltinTopicData,
        participant_guard: &Participant,
    ) {
        if subscription_builtin_topic_data.unicast_locator_list().is_empty()
        // || subscription_builtin_topic_data.multicast_locator_list().is_empty()
        {
            let remote_participant_data = participant_guard.find_remote_participant_proxy_data(
                subscription_builtin_topic_data.endpoint_guid().prefix(),
            );

            if let Some(remote_participant_data) = remote_participant_data {
                for locator in remote_participant_data.default_unicast_locator_list() {
                    if !subscription_builtin_topic_data.unicast_locator_list().contains(locator) {
                        subscription_builtin_topic_data.add_unicast_locator(locator.clone());
                    }
                }
            }
        }
    }
}

/// Publication Handling (Local Reader <-> Remote Writer)
impl SedpLogic {
    fn register_discovered_type(
        participant: &Participant,
        type_identifier: Option<&TypeIdentifier>,
        type_object: Option<&TypeObject>,
    ) {
        if let Some(type_object) = type_object {
            if let Ok(mut registry) = participant.type_registry().write() {
                match type_identifier {
                    Some(id) => registry.register_type_object_with_id(id, type_object.clone()),
                    None => registry.register_type_object(type_object.clone()),
                }
            }
        }
    }

    fn handle_publication_builtin_topic_data(
        &self,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
        inline_qos_params: Option<ParameterList>,
    ) -> RtpsResult<()> {
        let topic_name = publication_builtin_topic_data.topic_name().to_string();
        let endpoint_guid = publication_builtin_topic_data.endpoint_guid();

        let participant = self.get_upgraded_participant()?;

        if let Some(inline_qos_params) = inline_qos_params {
            if let Some(status_info) = inline_qos_params.get_status_info() {
                // According to RTPS spec, entity termination requires both
                // DISPOSED and UNREGISTERED status flags to be set
                if status_info.disposed() || status_info.unregistered() {
                    debug!("Received Data(w[UD])");

                    // Terminating endpoint provides GUID via KeyHash, but sometimes sends DATA message without SerializedData payload
                    let terminated_writer_guid =
                        if let Some(key_hash) = inline_qos_params.get_key_hash() {
                            InstanceHandle::to_guid(&key_hash)
                        } else {
                            endpoint_guid
                        };

                    participant
                        .cleanup_resources_for_remote_writer(terminated_writer_guid, &topic_name)?;
                    return Ok(());
                }
            } else {
                debug!("ALIVE SubscriptionBuiltinTopicData, no STATUS_INFO parameter found");
            }
        }

        // First try to find local reader using exact match (find_reader_from_entry)
        let readers = participant.find_readers_from_topic_name(&topic_name);

        if !readers.is_empty() {
            for reader in readers {
                self.match_reader_with_publication(reader, publication_builtin_topic_data.clone());
            }
        } else {
            // add to pending remote publications
            debug!(
                "SEDP Logic: No local reader found for topic '{}', buffering remote writer",
                topic_name
            );
        }

        Self::register_discovered_type(
            &participant,
            publication_builtin_topic_data.type_identifier(),
            publication_builtin_topic_data.type_object(),
        );
        self.maybe_request_discovered_type(
            endpoint_guid.prefix(),
            publication_builtin_topic_data.type_identifier(),
            publication_builtin_topic_data.type_object().is_some(),
        );

        participant
            .remote_publications()
            .entry(topic_name)
            .or_default()
            .insert(endpoint_guid, publication_builtin_topic_data);

        Ok(())
    }

    #[allow(clippy::option_map_unit_fn)]
    pub(crate) fn handle_stateful_reader_publication(
        &self,
        reader: &StatefulReader,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> RtpsResult<()> {
        let endpoint_guid = publication_builtin_topic_data.endpoint_guid();

        // Check if writer is already matched to avoid duplicates
        if reader.matched_writer_is_matched(endpoint_guid) {
            // Check QoS compatibility in case of QoS change of writer itself or remote reader
            if let MatchDecision::Reject(e) = self.validate_endpoint_compatibility(
                reader,
                endpoint_guid,
                &reader.subscription_builtin_topic_data()?,
                &publication_builtin_topic_data,
                |r, pid| r.update_requested_incompatible_qos_status(pid),
                |r| r.update_requested_incompatible_type_status(),
                |r| r.update_status(StatusKind::INCONSISTENT_TOPIC, None),
                "reader->writer",
            ) {
                // Incompatible - remove matching
                debug!(
                    "QoS changed for remote writer {}, now incompatible - removing matching",
                    endpoint_guid
                );

                // Fire LIVELINESS_CHANGED first; needs reader's matched list intact.
                if endpoint_guid.entity_id().entity_kind().is_user_defined() {
                    if let Some(wlp_logic) = self.get_upgraded_participant()?.wlp_logic() {
                        let _ = wlp_logic.deregister_monitored_writer(endpoint_guid);
                    }
                }

                reader
                    .writer_proxies()
                    .lock()
                    .map_err(|lock_err| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock WriterProxies: {}", lock_err),
                        )
                    })?
                    .retain(|writer_proxy| writer_proxy.remote_writer_guid() != endpoint_guid);

                reader.update_subscription_matched_status(
                    -1,
                    InstanceHandle::from_guid(&endpoint_guid),
                );
                return Err(e);
            }

            if reader
                .matched_writer_lookup(endpoint_guid)
                .ok_or(RtpsError::new(
                    RtpsErrorCode::MatchedEntityNotFound,
                    "There is no writer locator for reader",
                ))?
                .publication_builtin_topic_data()
                .changeable_qos_equals(&publication_builtin_topic_data)
            {
                // Still compatible - just update builtin_topic_data
                debug!(
                    "Syncing publication_builtin_topic_data for compatible remote writer {}",
                    endpoint_guid
                );
                reader
                    .writer_proxies()
                    .lock()
                    .map_err(|e| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock WriterProxies: {}", e),
                        )
                    })?
                    .iter_mut()
                    .find(|proxy| proxy.remote_writer_guid() == endpoint_guid)
                    .map(|proxy| {
                        proxy.set_publication_builtin_topic_data(publication_builtin_topic_data)
                    });
            }

            return Ok(());
        }

        // Check QoS and Partition compatibility
        match self.validate_endpoint_compatibility(
            reader,
            endpoint_guid,
            &reader.subscription_builtin_topic_data()?,
            &publication_builtin_topic_data,
            |r, pid| r.update_requested_incompatible_qos_status(pid),
            |r| r.update_requested_incompatible_type_status(),
            |r| r.update_status(StatusKind::INCONSISTENT_TOPIC, None),
            "reader->writer",
        ) {
            MatchDecision::Match => {}
            MatchDecision::Defer => return Ok(()),
            MatchDecision::Reject(e) => return Err(e),
        }

        let writer_proxy = WriterProxy::new(
            publication_builtin_topic_data.endpoint_guid(),
            publication_builtin_topic_data.endpoint_guid().entity_id(),
            publication_builtin_topic_data.unicast_locator_list(),
            publication_builtin_topic_data.multicast_locator_list(),
            0, // data_max_size_serialized
            publication_builtin_topic_data.clone(),
            reader.get_update_status_callback(),
        );

        reader.matched_writer_add(writer_proxy);

        let writer_guid = endpoint_guid;
        if writer_guid.entity_id().entity_kind().is_user_defined() {
            if let Some(wlp_logic) = self.get_upgraded_participant()?.wlp_logic() {
                let _ = wlp_logic.register_monitored_writer(
                    writer_guid,
                    *publication_builtin_topic_data.liveliness(),
                );
            }
        }

        self.register_preemptive_acknack_timer(
            reader,
            publication_builtin_topic_data.endpoint_guid(),
        )?;

        reader.update_subscription_matched_status(1, InstanceHandle::from_guid(&endpoint_guid));

        Ok(())
    }

    #[allow(clippy::option_map_unit_fn)]
    pub(crate) fn handle_stateless_reader_publication(
        &self,
        reader: &StatelessReader,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> RtpsResult<()> {
        let endpoint_guid = publication_builtin_topic_data.endpoint_guid();

        // Check if writer is already matched to avoid duplicates
        if reader.matched_writer_is_matched(endpoint_guid) {
            // Check QoS compatibility in case of QoS change of writer itself or remote reader
            if let MatchDecision::Reject(e) = self.validate_endpoint_compatibility(
                reader,
                endpoint_guid,
                &reader.subscription_builtin_topic_data()?,
                &publication_builtin_topic_data,
                |r, pid| r.update_requested_incompatible_qos_status(pid),
                |r| r.update_requested_incompatible_type_status(),
                |r| r.update_status(StatusKind::INCONSISTENT_TOPIC, None),
                "reader->writer",
            ) {
                // Incompatible - remove matching
                debug!(
                    "QoS changed for remote writer {}, now incompatible - removing matching",
                    endpoint_guid
                );

                // Fire LIVELINESS_CHANGED first; needs reader's matched list intact.
                if endpoint_guid.entity_id().entity_kind().is_user_defined() {
                    if let Some(wlp_logic) = self.get_upgraded_participant()?.wlp_logic() {
                        let _ = wlp_logic.deregister_monitored_writer(endpoint_guid);
                    }
                }

                reader
                    .remote_writer_infos()
                    .lock()
                    .map_err(|lock_err| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock WriterLocators: {}", lock_err),
                        )
                    })?
                    .retain(|remote_writer_info| {
                        remote_writer_info.remote_writer_guid() != endpoint_guid
                    });

                reader.update_subscription_matched_status(
                    -1,
                    InstanceHandle::from_guid(&endpoint_guid),
                );
                return Err(e);
            }

            // Still compatible - just update builtin_topic_data
            if reader
                .matched_writer_lookup(endpoint_guid)
                .ok_or(RtpsError::new(
                    RtpsErrorCode::MatchedEntityNotFound,
                    "There is no writer locator for reader",
                ))?
                .publication_builtin_topic_data()
                .changeable_qos_equals(&publication_builtin_topic_data)
            {
                debug!(
                    "Syncing publication_builtin_topic_data for compatible remote writer {}",
                    endpoint_guid
                );

                reader
                    .remote_writer_infos()
                    .lock()
                    .map_err(|e| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock WriterLocators: {}", e),
                        )
                    })?
                    .iter_mut()
                    .find(|locator| locator.remote_writer_guid() == endpoint_guid)
                    .map(|locator| {
                        locator.set_publication_builtin_topic_data(publication_builtin_topic_data)
                    });
            }

            return Ok(());
        }

        match self.validate_endpoint_compatibility(
            reader,
            endpoint_guid,
            &reader.subscription_builtin_topic_data()?,
            &publication_builtin_topic_data,
            |r, pid| r.update_requested_incompatible_qos_status(pid),
            |r| r.update_requested_incompatible_type_status(),
            |r| r.update_status(StatusKind::INCONSISTENT_TOPIC, None),
            "reader->writer",
        ) {
            MatchDecision::Match => {}
            MatchDecision::Defer => return Ok(()),
            MatchDecision::Reject(e) => return Err(e),
        }

        let remote_writer_info =
            RemoteWriterInfo::new(endpoint_guid, publication_builtin_topic_data.clone());
        reader.matched_writer_add(remote_writer_info);

        let writer_guid = endpoint_guid;
        if writer_guid.entity_id().entity_kind().is_user_defined() {
            let participant = self.get_upgraded_participant()?;
            if let Some(wlp_logic) = participant.wlp_logic() {
                let _ = wlp_logic.register_monitored_writer(
                    writer_guid,
                    *publication_builtin_topic_data.liveliness(),
                );
            }
        }

        reader.update_subscription_matched_status(1, InstanceHandle::from_guid(&endpoint_guid));

        Ok(())
    }

    fn handle_empty_locator_lists_for_publication(
        &self,
        publication_builtin_topic_data: &mut PublicationBuiltinTopicData,
        participant_guard: &Participant,
    ) {
        if publication_builtin_topic_data.unicast_locator_list().is_empty()
        // || publication_builtin_topic_data.multicast_locator_list().is_empty()
        {
            let remote_participant_data = participant_guard.find_remote_participant_proxy_data(
                publication_builtin_topic_data.endpoint_guid().prefix(),
            );

            if let Some(remote_participant_data) = remote_participant_data {
                for locator in remote_participant_data.default_unicast_locator_list() {
                    if !publication_builtin_topic_data.unicast_locator_list().contains(locator) {
                        publication_builtin_topic_data.add_unicast_locator(locator.clone());
                    }
                }
            }
        }
    }
}

/// Periodic Message Sending
impl SedpLogic {
    // Sends periodic SPDP Data & SEDP Heartbeat
    #[allow(unused_variables)]
    pub(crate) fn send_periodic_participant_data_unicast(
        &self,
        start_time: Option<Instant>,
        duration: StdDuration,
        spdp_discovered_participant_data: Arc<SPDPDiscoveredParticipantData>,
        data: Option<Arc<Vec<u8>>>,
    ) -> RtpsResult<()> {
        let data =
            data.ok_or_else(|| RtpsError::new(RtpsErrorCode::DataNotSet, "Data is not set"))?;
        let participant = self.get_upgraded_participant()?;
        let remote_prefix = spdp_discovered_participant_data.guid_prefix();

        let mut filtered_list: Vec<SPDPDiscoveredParticipantData> = Vec::new();
        match participant.remote_participant_proxy_datas().lock() {
            Ok(remote_participant_datas) => {
                // Filter to the target remote participant
                for remote_participant_data in remote_participant_datas.iter() {
                    if remote_prefix != remote_participant_data.guid_prefix() {
                        continue;
                    }
                    filtered_list.push(remote_participant_data.clone());
                }
            }
            Err(e) => {
                return Err(RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to lock remote_participant_datas: {:?}", e),
                ));
            }
        }

        // In case timer had not been removed after remote participant was unmatched
        if filtered_list.is_empty() {
            if let Ok(handler) = self.timer_handler.lock() {
                handler.remove_timer(TimerId::SedpScheduledMessage {
                    remote_prefix,
                    writer_entity_id: EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER,
                });
            }
            return Ok(());
        }

        for remote_participant_data in filtered_list.iter() {
            if let Err(e) = self.send_to_participant_metatraffic_locators(
                &data,
                remote_participant_data.participant_guid(),
                "spdp",
            ) {
                warn!("Failed to send SPDP discovery message: {:?}", e);
            }
        }
        Ok(())
    }

    /// Sends periodic SEDP HEARTBEAT messages to reader proxies
    /// and checks for unsent changes in the writer cache.
    #[allow(unused_variables)]
    pub(crate) fn send_sedp_periodic_heartbeat_message(
        &self,
        start_time: Option<Instant>,
        duration: StdDuration,
        // spdp_discovered_participant_data: Arc<SPDPDiscoveredParticipantData>,
        guid_prefix: Arc<GuidPrefix>,
        entity_id: EntityId,
    ) -> RtpsResult<bool> {
        // Get reader and writer corresponding to entity
        let participant = self.get_upgraded_participant()?;

        let builtin_endpoint_pair =
            BuiltinEndpointPair::reader_writer_from_entity_id(entity_id, participant.clone())?;
        let (reader, writer) = match builtin_endpoint_pair {
            Some(builtin_endpoint_pair) => {
                (builtin_endpoint_pair.reader(), builtin_endpoint_pair.writer())
            }
            None => {
                return Err(RtpsError::new(
                    RtpsErrorCode::BuiltinEndpointNotFound,
                    "Failed to get reader and writer proxy",
                ))
            }
        };

        let reader_entity_id = reader.guid().entity_id();
        let writer_entity_id = writer.guid().entity_id();

        let (first_sn, last_sn, heartbeat_count) = {
            let last_change_sn = writer.last_change_sequence_number();
            match writer.writer_cache().lock() {
                Ok(writer_cache) => (
                    writer_cache.get_seq_num_min().unwrap_or(last_change_sn + 1),
                    writer_cache.get_seq_num_max().unwrap_or(last_change_sn),
                    writer.heartbeat_count(),
                ),
                Err(e) => {
                    return Err(RtpsError::new(
                        RtpsErrorCode::WriterCacheNotSet,
                        format!("Failed to get writer cache: {}", e),
                    ));
                }
            }
        };

        let mut is_sent = false;
        let mut matched_any = false;

        let participant_guid = {
            let local_participant_data = participant.local_participant_proxy_data();
            local_participant_data.participant_guid()
        };

        match writer.reader_proxies().lock() {
            Ok(reader_proxies) => {
                for reader_proxy in reader_proxies.iter() {
                    if reader_proxy.remote_reader_guid().prefix() != *guid_prefix {
                        continue;
                    }
                    if !reader_proxy.is_active() {
                        continue;
                    }
                    matched_any = true;
                    let buffer = MessageCreator::create_heartbeat_message(
                        participant_guid.prefix(),
                        reader_proxy.remote_reader_guid().prefix(),
                        heartbeat_count,
                        reader_entity_id,
                        writer_entity_id,
                        first_sn,
                        last_sn,
                        false,
                        false,
                    );
                    match buffer {
                        Ok(buffer) => {
                            for locator in reader_proxy.unicast_locator_list() {
                                // SEDP heartbeat is a UDP/TCP-only path
                                if !locator.is_udp() && !locator.is_tcp() {
                                    continue;
                                }
                                match self
                                    .transport
                                    .send(&buffer, &SendTarget::SEDPDiscovery(&locator))
                                {
                                    Ok(_) => {
                                        is_sent = true;
                                    }
                                    Err(e) => {
                                        warn!("Failed to send SEDP heartbeat: {:?}", e);
                                    }
                                }
                            }
                            writer.increase_heartbeat_count();
                        }
                        Err(e) => {
                            warn!("Failed to create SEDP heartbeat message: {:?}", e);
                        }
                    }
                }
            }
            Err(e) => {
                return Err(RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to get reader proxies: {}", e),
                ));
            }
        }

        // No matched readers remain: remove the scheduled SEDP timer.
        if !matched_any {
            if let Ok(handler) = self.timer_handler.lock() {
                handler.remove_timer(TimerId::SedpScheduledMessage {
                    remote_prefix: *guid_prefix,
                    writer_entity_id: entity_id,
                });
            }
        }

        Ok(is_sent)
    }

    // Register a repeating timer that pushes `message` to the sending queue every `duration`.
    // Keyed by (remote_prefix, writer_entity_id) so each (remote, builtin writer) pair has at
    // most one active periodic schedule. Cleanup happens in unmatch_with_remote_participant.
    pub(crate) fn register_periodic_send_timer(
        &self,
        remote_prefix: GuidPrefix,
        writer_entity_id: EntityId,
        duration: StdDuration,
        message: MessageType,
    ) -> RtpsResult<()> {
        let participant_weak = self.participant.clone();
        let timer_id = TimerId::SedpScheduledMessage { remote_prefix, writer_entity_id };

        let message = Arc::new(message);
        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(timer_id, duration, true, {
                let message = message.clone();
                move || {
                    if let Some(participant) = participant_weak.upgrade() {
                        if !participant.is_terminated() {
                            let sending_handler = SendingHandler::get_instance(participant, None);
                            sending_handler.push_message_and_wake((*message).clone());
                        }
                    }
                }
            });
        } else {
            error!("Failed to acquire timer handler lock for SEDP message sending");
        }

        Ok(())
    }
}

/// General Message Sending
impl SedpLogic {
    pub(crate) fn send_sedp_data_message(
        &self,
        cache_change: Arc<CacheChange>,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;
        let mut send_buffer = participant
            .wire_buffer_pool()
            .lock()
            .map_err(|_| {
                RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
            })?
            .acquire();
        let result = MessageCreator::create_data_msg(
            &cache_change,
            remote_guid,
            reader_entity_id,
            writer_entity_id,
            None, // No heartbeat
            true, // Use inline QoS (default)
            None, // No content filter for SEDP messages
            &mut send_buffer,
        );

        match result {
            Ok(()) => {
                self.send_to_participant_metatraffic_locators(&send_buffer, remote_guid, "data")?
            }
            Err(e) => {
                participant
                    .wire_buffer_pool()
                    .lock()
                    .map_err(|_| {
                        RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
                    })?
                    .release(send_buffer);
                return Err(RtpsError::new(
                    RtpsErrorCode::SerializationError,
                    format!("Failed to create SEDP DATA message: {}", e),
                ));
            }
        };
        participant
            .wire_buffer_pool()
            .lock()
            .map_err(|_| {
                RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
            })?
            .release(send_buffer);

        Ok(())
    }

    fn send_sedp_acknack_message(
        &self,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        missing_changes: Vec<SequenceNumber>,
        acknack_count: u32,
        bitmap_base: SequenceNumber,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let buffer = MessageCreator::create_acknack_message(
            participant.guid(),
            remote_guid,
            reader_entity_id,
            writer_entity_id,
            missing_changes,
            acknack_count,
            bitmap_base,
            false,
        );

        match buffer {
            Ok(buffer) => {
                if let Err(e) =
                    self.send_to_participant_metatraffic_locators(&buffer, remote_guid, "AckNack")
                {
                    warn!("Failed to send SEDP AckNack message: {:?}", e);
                }
            }
            Err(e) => {
                warn!("Failed to create SEDP AckNack message: {:?}", e);
            }
        }

        Ok(())
    }

    fn send_sedp_gap_for_vec(
        &self,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        mut gap_list: Vec<SequenceNumber>,
    ) -> RtpsResult<()> {
        if gap_list.is_empty() {
            return Ok(());
        }

        let participant = self.get_upgraded_participant()?;

        let buffer_list = MessageCreator::create_multiple_gap_msgs(
            participant.guid(),
            remote_guid,
            reader_entity_id,
            writer_entity_id,
            &mut gap_list,
        )
        .map_err(|e| RtpsError::new(RtpsErrorCode::Io, e.to_string()))?;

        for buf in buffer_list {
            if let Err(e) = self.send_to_participant_metatraffic_locators(&buf, remote_guid, "GAP")
            {
                warn!("Failed to send SEDP GAP message: {:?}", e);
            }
        }

        Ok(())
    }

    pub(crate) fn send_endpoint_termination_message(
        &self,
        builtin_writer_guid: Guid,
        cache_change: Arc<CacheChange>,
    ) -> RtpsResult<()> {
        // Get remote builtin reader for corresponding builtin writer
        let mut remote_guid_list = Vec::new();

        let participant = self.get_upgraded_participant()?;

        if builtin_writer_guid.entity_id() == EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER {
            if let Ok(reader_proxies) = participant
                .builtin_endpoints()
                .sedp_builtin_publications_writer
                .reader_proxies()
                .lock()
            {
                remote_guid_list.extend(
                    reader_proxies
                        .iter()
                        .filter(|proxy| {
                            proxy.remote_reader_guid().entity_id()
                                == EntityId::SEDP_BUILTIN_PUBLICATIONS_READER
                        })
                        .map(|proxy| proxy.remote_reader_guid()),
                );
            }
        } else if builtin_writer_guid.entity_id() == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER {
            if let Ok(reader_proxies) = participant
                .builtin_endpoints()
                .sedp_builtin_subscriptions_writer
                .reader_proxies()
                .lock()
            {
                remote_guid_list.extend(
                    reader_proxies
                        .iter()
                        .filter(|proxy| {
                            proxy.remote_reader_guid().entity_id()
                                == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER
                        })
                        .map(|proxy| proxy.remote_reader_guid()),
                );
            }
        } else {
            error!("Invalid sender for SEDP termination: {}", builtin_writer_guid.entity_id());
        }

        for remote_guid in remote_guid_list {
            let mut send_buffer = participant
                .wire_buffer_pool()
                .lock()
                .map_err(|_| {
                    RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
                })?
                .acquire();
            let result = MessageCreator::create_data_msg(
                &cache_change,
                remote_guid,
                remote_guid.entity_id(),
                builtin_writer_guid.entity_id(),
                None, // No heartbeat
                true, // Use inline QoS (default)
                None, // No content filter for termination messages
                &mut send_buffer,
            );

            match result {
                Ok(()) => {
                    if let Err(e) = self.send_to_participant_metatraffic_locators(
                        &send_buffer,
                        remote_guid,
                        "termination",
                    ) {
                        warn!("Failed to send SEDP termination message: {:?}", e);
                    }
                }
                Err(e) => {
                    warn!("Failed to create SEDP DATA (termination) message: {:?}", e);
                }
            };
            participant
                .wire_buffer_pool()
                .lock()
                .map_err(|_| {
                    RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
                })?
                .release(send_buffer);
        }
        Ok(())
    }

    pub(crate) fn send_participant_termination_message_unicast(&self) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;
        let rtps_message = MessageCreator::create_spdp_msg_with_inline_qos(participant.clone())?;
        let buffer = rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian).map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::SerializationError,
                format!("Failed to serialize SPDP message: {:?}", e),
            )
        })?;

        self.send_to_all_participants_metatraffic_locators(&buffer, "SPDP Termination")?;

        Ok(())
    }

    fn send_to_participant_metatraffic_locators(
        &self,
        buffer: &[u8],
        remote_guid: Guid,
        message_type: &str,
    ) -> RtpsResult<bool> {
        let participant = self.get_upgraded_participant()?;

        let Some(remote_participant_data) =
            participant.find_remote_participant_proxy_data(remote_guid.prefix())
        else {
            debug!(
                "[{}] SEDP Logic: Remote participant data not found for GUID prefix: {}",
                message_type,
                Guid::guid_prefix_to_string(&remote_guid.prefix())
            );
            return Ok(false);
        };
        for locator in remote_participant_data.metatraffic_unicast_locator_list() {
            if let Err(e) = self.send_to_single_locator(buffer, locator.clone(), message_type) {
                debug!("[{}] send to {:?} skipped: {}", message_type, locator, e);
            }
        }
        Ok(true)
    }

    fn send_to_all_participants_metatraffic_locators(
        &self,
        buffer: &[u8],
        message_type: &str,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let remote_participant_datas = participant.remote_participant_proxy_datas();
        let remote_participant_datas_guard = remote_participant_datas
            .lock()
            .map_err(|_| RtpsError::new(RtpsErrorCode::LockError, None))?;

        for remote_participant_data in remote_participant_datas_guard.iter() {
            for locator in remote_participant_data.metatraffic_unicast_locator_list() {
                if let Err(e) = self.send_to_single_locator(buffer, locator.clone(), message_type) {
                    debug!("[{}] fan-out to {:?} skipped: {}", message_type, locator, e);
                }
            }
        }
        drop(remote_participant_datas_guard);

        Ok(())
    }

    fn send_to_single_locator(
        &self,
        buffer: &[u8],
        locator: Locator,
        message_type: &str,
    ) -> RtpsResult<()> {
        self.transport.send(buffer, &SendTarget::SEDPDiscovery(&locator)).map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::NotSent,
                format!("[{}] SEDP Logic: Failed to send message: {}", message_type, e),
            )
        })?;
        debug!("[{}] SEDP Logic: message sent via transport plugin", message_type);
        Ok(())
    }
}

/// Utilities
impl SedpLogic {
    fn register_preemptive_acknack_timer(
        &self,
        stateful_reader: &StatefulReader,
        remote_writer_guid: Guid,
    ) -> RtpsResult<()> {
        let stateful_reader_id = stateful_reader.guid().entity_id();
        let timer_id =
            TimerId::PreemptiveAcknack { entity_id: stateful_reader_id, remote_writer_guid };

        let participant = self.get_upgraded_participant()?;

        let callback = move || {
            if let Some(sending_handler) =
                SendingHandler::get_instance_by_participant_guid(participant.guid())
            {
                sending_handler.push_message_and_wake(MessageType::UserAcknack(
                    stateful_reader_id,
                    remote_writer_guid,
                    false, // final_flag
                    true,  // is_preemptive
                ));
            }
        };

        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(
                timer_id,
                stateful_reader.preemptive_acknack_delay().to_std_duration(),
                false,
                callback,
            );
        }

        Ok(())
    }

    fn register_preemptive_heartbeat_timer(
        &self,
        stateful_writer: &StatefulWriter,
        remote_reader_guid: Guid,
    ) -> RtpsResult<()> {
        let stateful_writer_id = stateful_writer.guid().entity_id();
        let timer_id =
            TimerId::PreemptiveHeartbeat { entity_id: stateful_writer_id, remote_reader_guid };

        let participant = self.get_upgraded_participant()?;

        let callback = move || {
            if let Some(sending_handler) =
                SendingHandler::get_instance_by_participant_guid(participant.guid())
            {
                sending_handler.push_message_and_wake(MessageType::UserHeartbeatToOne(
                    stateful_writer_id,
                    remote_reader_guid,
                    true,
                ));
            }
        };

        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(
                timer_id,
                stateful_writer.initial_heartbeat_delay().to_std_duration(),
                false,
                callback,
            );
        }

        Ok(())
    }

    /// Apply content filter signature to the most recently added ReaderProxy
    ///
    /// This is a helper function to keep the main logic clean.
    /// It finds the ReaderProxy for the given reader_guid and sets its content filter signatures.
    fn apply_content_filter_to_reader_proxy(
        &self,
        writer: &StatefulWriter,
        reader_guid: Guid,
        content_filter: &ContentFilterProperty,
    ) {
        use crate::rtps::builtin::data::content_filtered_topic::calculate_filter_signature;

        let signature = calculate_filter_signature(&content_filter.filter_expression);

        if let Ok(mut reader_proxies) = writer.reader_proxies().lock() {
            if let Some(reader_proxy) =
                reader_proxies.iter_mut().find(|rp| rp.remote_reader_guid() == reader_guid)
            {
                reader_proxy.set_content_filter_signatures(Some(vec![signature]));
                // debug!("Applied content filter signature to ReaderProxy: {:?}", reader_guid);
            }
        }
    }

    /// Check if the message is a SEDP Publication message.
    fn is_sedp_publication_data(&self, data: &Data) -> bool {
        (data.reader_id == EntityId::SEDP_BUILTIN_PUBLICATIONS_READER
            || data.reader_id == EntityId::UNKNOWN)
            && data.writer_id == EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER
    }

    /// Check if the message is a SEDP Subscription message.
    fn is_sedp_subscription_data(&self, data: &Data) -> bool {
        (data.reader_id == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER
            || data.reader_id == EntityId::UNKNOWN)
            && data.writer_id == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER
    }

    fn is_participant_data(&self, data: &Data) -> bool {
        (data.reader_id == EntityId::SPDP_BUILTIN_PARTICIPANT_READER
            || data.reader_id == EntityId::UNKNOWN)
            && data.writer_id == EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER
    }

    /// Updates the writer proxy when data is received from a matched writer.
    /// Marks the change as received and increments the expected sequence number.
    pub(crate) fn mark_as_received_in_writer_proxy(
        &self,
        reader: &Arc<StatefulReader>,
        writer_guid: Guid,
        seq_num: SequenceNumber,
    ) -> RtpsResult<()> {
        let writer_proxies = reader.writer_proxies();
        let mut matched_writers = writer_proxies.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("[data] Failed to acquire matched_writers lock: {}", e),
            )
        })?;

        let writer_proxy = matched_writers
            .iter_mut()
            .find(|proxy| proxy.remote_writer_guid() == writer_guid)
            .ok_or_else(|| {
                RtpsError::new(
                    RtpsErrorCode::BuiltinEndpointNotFound,
                    format!(
                        "[data] SPDP Message may have not been received, cause builtin reader not matched with remote guid: {}",
                        writer_guid
                    ),
                )
            })?;

        writer_proxy.mark_change_received(seq_num, None);
        writer_proxy.increment_expected_sn();
        Ok(())
    }
}

impl_participant_accessor!(SedpLogic);
impl_unicast_thread_handler!(SedpLogic);
impl_multicast_thread_handler!(SedpLogic);

impl ParticipantMessageProcessor for SedpLogic {}
impl JoinAllThread for SedpLogic {}

impl UnicastMessageProcessor for SedpLogic {
    fn handle_data_message(
        &mut self,
        rtps_header: &Header,
        submessage_header: &SubmessageHeader,
        data: &Data,
        message_receiver: &MessageReceiver,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        if data.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
            debug!("[SedpLogic] P2P data, skipping in sedp_logic");
            return Ok(());
        }

        if Self::is_type_lookup_data(data) {
            return self.handle_type_lookup_data(rtps_header, data);
        }

        debug!("[data] entity id - reader: {}, writer: {}", data.reader_id, data.writer_id);

        let is_big_endian = submessage_header
            .endianness_flag()
            .is_some_and(|endianness| endianness == Endianness::BigEndian);

        let inline_qos_params = data.inline_qos();

        if self.is_sedp_publication_data(data) || self.is_sedp_subscription_data(data) {
            debug!("SEDP endpoint data received");

            let builtin_endpoint_pair = BuiltinEndpointPair::reader_writer_from_entity_id(
                data.writer_id,
                participant.clone(),
            )?;

            // Check if the builtin reader is matched with the remote writer GUID
            let builtin_endpoint_pair = builtin_endpoint_pair.ok_or_else(|| {
                RtpsError::new(
                    RtpsErrorCode::BuiltinEndpointNotFound,
                    "[data] Failed to get builtin endpoint pair",
                )
            })?;

            let writer_guid = Guid::new(rtps_header.guid_prefix(), data.writer_id);

            self.mark_as_received_in_writer_proxy(
                &builtin_endpoint_pair.reader(),
                writer_guid,
                data.writer_sn,
            )?;

            let payload = data.serialized_data();

            let store_wire_in_cache = |endpoint_guid: Guid| {
                let instance_handle = InstanceHandle::from_guid(&endpoint_guid);
                let reader = builtin_endpoint_pair.reader();
                if let Ok(mut cache_guard) = reader.reader_cache().lock() {
                    let mut cache_change = cache_guard.acquire_change();
                    cache_change.reset(
                        ChangeKind::Alive,
                        writer_guid,
                        instance_handle,
                        data.writer_sn,
                        message_receiver.get_source_timestamp(),
                    );
                    cache_change.set_owned_payload(payload.as_ref().to_vec());
                    let _ = cache_guard.add_change(cache_change, false);
                }
            };

            if data.writer_id == EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER {
                let is_termination = inline_qos_params
                    .as_ref()
                    .and_then(|qos| qos.get_status_info())
                    .is_some_and(|status| status.disposed() || status.unregistered());
                if is_termination && payload.is_empty() {
                    if let Some(terminated_writer_guid) =
                        inline_qos_params.as_ref().and_then(|qos| qos.get_key_hash())
                    {
                        participant.cleanup_remote_writer_by_guid(InstanceHandle::to_guid(
                            &terminated_writer_guid,
                        ))?;
                        return Ok(());
                    }
                }

                let writer_data = SEDPMessage::<DiscoveredWriterData>::from_serialized_payload(
                    payload.as_ref(),
                    is_big_endian,
                )
                .map_err(|e| {
                    RtpsError::new(
                        RtpsErrorCode::DeserializationError,
                        format!("Failed to parse DiscoveredWriterData: {}", e),
                    )
                })?;

                store_wire_in_cache(writer_data.publication_builtin_topic_data.endpoint_guid());

                debug!("SEDP Logic: DiscoveredWriterData: {:?}", writer_data);
                return self.handle_publication_builtin_topic_data(
                    writer_data.publication_builtin_topic_data,
                    inline_qos_params,
                );
            } else if data.writer_id == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER {
                let is_termination = inline_qos_params
                    .as_ref()
                    .and_then(|qos| qos.get_status_info())
                    .is_some_and(|status| status.disposed() || status.unregistered());
                if is_termination && payload.is_empty() {
                    if let Some(terminated_reader_guid) =
                        inline_qos_params.as_ref().and_then(|qos| qos.get_key_hash())
                    {
                        participant.cleanup_remote_reader_by_guid(InstanceHandle::to_guid(
                            &terminated_reader_guid,
                        ))?;
                        return Ok(());
                    }
                }

                let reader_data = SEDPMessage::<DiscoveredReaderData>::from_serialized_payload(
                    payload.as_ref(),
                    is_big_endian,
                )
                .map_err(|e| {
                    RtpsError::new(
                        RtpsErrorCode::DeserializationError,
                        format!("Failed to parse DiscoveredReaderData: {}", e),
                    )
                })?;

                store_wire_in_cache(reader_data.subscription_builtin_topic_data.endpoint_guid());

                debug!("SEDP Logic: DiscoveredReaderData: {:?}", reader_data);
                return self.handle_subscription_builtin_topic_data(
                    reader_data.subscription_builtin_topic_data,
                    inline_qos_params,
                    reader_data.content_filter,
                );
            }
        } else if self.is_participant_data(data) {
            let (participant_proxy_data, inline_qos_params) = message_receiver
                .extract_participant_proxy_data(participant.domain_id())
                .ok_or_else(|| {
                    RtpsError::new(
                        RtpsErrorCode::DeserializationError,
                        "Failed to parse RTPS message",
                    )
                })?;

            let is_termination = inline_qos_params
                .as_ref()
                .and_then(|qos| qos.get_status_info())
                .is_some_and(|status| status.disposed() || status.unregistered());

            if is_termination {
                let terminated_participant_guid =
                    inline_qos_params.as_ref().and_then(|qos| qos.get_key_hash()).unwrap_or_else(
                        || InstanceHandle::from_guid(&participant_proxy_data.participant_guid()),
                    );

                let _ = participant
                    .unmatch_with_remote_participant(&terminated_participant_guid.to_guid());
            } else {
                return self.handle_discovered_participant_data(participant_proxy_data.clone());
            }
        }

        Ok(())
    }

    fn handle_heartbeat_message(
        &mut self,
        rtps_header: &Header,
        submessage_header: &SubmessageHeader,
        heartbeat: &Heartbeat,
    ) -> RtpsResult<()> {
        // ManualByTopic liveliness: user writer sends HEARTBEAT with liveliness_flag
        // via metatraffic channel. Delegate to WLP logic for liveliness renewal.
        let is_liveliness_heartbeat = submessage_header.liveliness_flag().unwrap_or(false);
        if is_liveliness_heartbeat && !heartbeat.writer_id.entity_kind().is_built_in() {
            let participant = self.get_upgraded_participant()?;
            if let Some(wlp) = participant.wlp_logic() {
                let _ = wlp.handle_heartbeat_message_inner(
                    heartbeat,
                    Guid::new(rtps_header.guid_prefix(), heartbeat.writer_id),
                    submessage_header.final_flag().unwrap_or(false),
                    true,
                );
            }
            return Ok(());
        }

        if heartbeat.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
            debug!("[heartbeat] P2P heartbeat, skipping in sedp_logic");
            return Ok(());
        }

        debug!(
            "[heartbeat] entity id - reader: {}, writer: {}",
            heartbeat.reader_id, heartbeat.writer_id
        );

        let participant = self.get_upgraded_participant()?;

        let builtin_endpoint_pair = match BuiltinEndpointPair::reader_writer_from_entity_id(
            heartbeat.writer_id,
            participant.clone(),
        )? {
            Some(proxy) => proxy,
            None => {
                return Err(RtpsError::new(
                    RtpsErrorCode::BuiltinEndpointNotFound,
                    format!(
                    "[heartbeat] No matching SEDP builtin reader found for writer entity ID: {:?}",
                    heartbeat.writer_id
                ),
                ));
            }
        };

        let local_reader = builtin_endpoint_pair.reader();
        let remote_writer_guid = Guid::new(rtps_header.guid_prefix(), heartbeat.writer_id);
        let final_flag = submessage_header.final_flag().unwrap_or(false);

        if !local_reader.matched_writer_is_matched(remote_writer_guid) {
            debug!("[heartbeat] SPDP Message may have not been received, cause builtin reader not matched with remote guid: {}", remote_writer_guid);
            return Ok(());
        }

        let writer_proxies = local_reader.writer_proxies();
        let mut writer_proxies_guard =
            writer_proxies.lock().map_err(|_| RtpsError::new(RtpsErrorCode::LockError, None))?;

        let writer_proxy = writer_proxies_guard
            .iter_mut()
            .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::RtpsEntityNotFound, None))?;

        // Check for duplicate Heartbeat. A non-increasing count is accepted
        // once enough time has passed (peer assumed to have reset).
        let now = Instant::now();
        if !should_accept_count(
            "[SEDP] [Heartbeat]",
            heartbeat.count,
            writer_proxy.last_heartbeat_count(),
            writer_proxy.last_heartbeat_at(),
            false,
            now,
        ) {
            return Ok(());
        }
        writer_proxy.set_last_heartbeat_count(heartbeat.count);
        writer_proxy.set_last_heartbeat_at(now);

        let missing_changes = writer_proxy.process_heartbeat(heartbeat.first_sn, heartbeat.last_sn);
        let bitmap_base = writer_proxy.expected_sn();

        // Check Heartbeat's final flag
        let requires_response = !final_flag;

        // Send AckNack if there are missing changes or if response is required
        if !missing_changes.is_empty() || requires_response {
            debug!(
                "Sending AckNack - missing changes: [{}], requires response: {}",
                missing_changes.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", "),
                requires_response
            );

            writer_proxy.increase_acknack_count();
            let acknack_count = writer_proxy.acknack_count();
            self.send_sedp_acknack_message(
                remote_writer_guid,
                local_reader.guid().entity_id(),
                heartbeat.writer_id,
                missing_changes,
                acknack_count,
                bitmap_base,
            )
        } else {
            Ok(())
        }
    }

    fn handle_acknack_message(
        &mut self,
        rtps_header: &Header,
        _submessage_header: &SubmessageHeader,
        acknack: &AckNack,
    ) -> RtpsResult<()> {
        if acknack.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
            debug!("[AckNack] P2P acknack, skipping in sedp_logic");
            return Ok(());
        }

        debug!(
            "[acknack] entity id - reader: {}, writer: {}",
            acknack.reader_id, acknack.writer_id
        );

        let participant = self.get_upgraded_participant()?;
        let remote_reader_guid = Guid::new(rtps_header.guid_prefix(), acknack.reader_id);

        let builtin_endpoint_pair = match BuiltinEndpointPair::reader_writer_from_entity_id(
            acknack.writer_id,
            participant.clone(),
        )? {
            Some(proxy) => proxy,
            None => {
                return Err(RtpsError::new(
                    RtpsErrorCode::BuiltinEndpointNotFound,
                    format!(
                    "[acknack] No matching SEDP builtin reader found for reader entity ID: {:?}",
                    acknack.reader_id
                ),
                ));
            }
        };

        let local_writer = builtin_endpoint_pair.writer();

        if !local_writer.matched_reader_is_matched(remote_reader_guid) {
            return Err(RtpsError::new(
                RtpsErrorCode::RtpsEntityNotFound,
                "Failed to find matched SEDP reader for AckNack",
            ));
        }

        // Check for duplicate AckNack
        let stateful_writer =
            local_writer.as_any().downcast_ref::<StatefulWriter>().ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::DowncastError, "Failed to downcast to StatefulWriter")
            })?;
        let reader_proxies = stateful_writer.reader_proxies();
        let mut reader_proxies_guard =
            reader_proxies.lock().map_err(|_| RtpsError::new(RtpsErrorCode::LockError, None))?;
        let reader_proxy = reader_proxies_guard
            .iter_mut()
            .find(|rp| rp.remote_reader_guid() == remote_reader_guid)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

        // Preemptive ACKNACK (RTPS 8.4.12.2) carries seqbase < 1 as an explicit
        // reset signal and bypasses the count/debounce check.
        let is_preemptive = acknack.reader_sn_state.bitmap_base().to_i64() < 1;
        let now = Instant::now();
        if !should_accept_count(
            "[SEDP] [AckNack]",
            acknack.count,
            reader_proxy.last_acknack_count(),
            reader_proxy.last_acknack_at(),
            is_preemptive,
            now,
        ) {
            return Ok(());
        }
        reader_proxy.set_last_acknack_count(acknack.count);
        reader_proxy.set_last_acknack_at(now);
        drop(reader_proxies_guard);

        let missing_sequence_numbers = acknack.reader_sn_state.extract_numbers();

        debug!(
            "Missing sequence numbers [{}] from remote: {}",
            missing_sequence_numbers.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", "),
            remote_reader_guid
        );

        if missing_sequence_numbers.is_empty() {
            return Ok(());
        }

        let mut missing_changes = Vec::new();
        let mut gap_sns: Vec<SequenceNumber> = Vec::new();
        let writer_cache = local_writer.writer_cache();

        {
            let cache_guard = writer_cache.lock().unwrap();
            for seq_num in missing_sequence_numbers {
                match cache_guard.get_change(seq_num) {
                    Some(change) => missing_changes.push(change),
                    None => gap_sns.push(seq_num),
                }
            }
        }

        if !gap_sns.is_empty() {
            debug!(
                "[SEDP] GAP for {} missing SN(s) not in cache, remote: {}",
                gap_sns.len(),
                remote_reader_guid
            );
            self.send_sedp_gap_for_vec(
                remote_reader_guid,
                acknack.reader_id,
                acknack.writer_id,
                gap_sns,
            )?;
        }

        if !missing_changes.is_empty() {
            debug!(
                "Retransmitting {} missing changes from remote: {}",
                missing_changes.len(),
                remote_reader_guid
            );

            for change in missing_changes {
                self.send_sedp_data_message(
                    change,
                    remote_reader_guid,
                    acknack.reader_id,
                    acknack.writer_id,
                )?;
            }
        }

        Ok(())
    }

    fn handle_gap_message(&mut self, rtps_header: &Header, gap: &Gap) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let builtin_endpoint_pair =
            BuiltinEndpointPair::reader_writer_from_entity_id(gap.writer_id, participant.clone())?
                .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

        let local_reader = builtin_endpoint_pair.reader();
        let remote_writer_guid = Guid::new(rtps_header.guid_prefix(), gap.writer_id);

        let stateful_reader =
            local_reader.as_any().downcast_ref::<StatefulReader>().ok_or_else(|| {
                RtpsError::new(
                    RtpsErrorCode::DowncastError,
                    "Failed to downcast Builtin SEDP reader",
                )
            })?;

        let writer_proxies_arc = stateful_reader.writer_proxies();
        let mut writer_proxies = writer_proxies_arc.lock().map_err(|_| {
            RtpsError::new(RtpsErrorCode::LockError, "Failed to acquire writer proxies lock")
        })?;

        let writer_proxy = writer_proxies
            .iter_mut()
            .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

        let capacity =
            (gap.gap_list.bitmap_base().to_i64() - gap.gap_start.to_i64()).max(0) as usize;
        let mut irrelevant_changes = Vec::with_capacity(capacity);

        for sn in gap.gap_start.to_i64()..gap.gap_list.bitmap_base().to_i64() {
            irrelevant_changes.push(SequenceNumber::from_i64(sn));
        }

        irrelevant_changes.extend(gap.gap_list.extract_numbers().iter());

        debug!(
            "[SEDP GAP] Marking changes as irrelevant from SN {:?}, to {:?}",
            irrelevant_changes.first(),
            irrelevant_changes.last()
        );

        if irrelevant_changes.last().unwrap_or(&SequenceNumber::new(0, 0))
            > &writer_proxy.expected_sn()
        {
            writer_proxy.set_expected_sn(SequenceNumber::from_i64(
                irrelevant_changes.last().unwrap().to_i64() + 1,
            ));
        }

        for seq_num in irrelevant_changes {
            writer_proxy.irrelevant_change_set(seq_num);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // TODO: Tests need updating to use TransportPlugin + MessageSource pattern.
    // Previous tests used removed Socket methods (create_socket, sender, discovery_multicast_listener, etc.)
    // These will be updated when DcpsBridge migration (Phase 5+) is complete.
}
