//! RTPS participant implementation.
//!
//! This module implements the `Participant` which represents a DDS domain participant
//! at the RTPS layer. Participants manage readers, writers, discovery endpoints, and
//! communication with remote participants in the DDS domain.

#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    collections::HashMap,
    fmt::Debug,
    net::Ipv4Addr,
    str::FromStr,
    sync::{atomic::AtomicBool, Arc, Mutex, OnceLock},
};

use arc_swap::ArcSwap;
use dashmap::DashMap;

use log::{debug, info};

use crate::{
    common::{
        builtin::topic::{
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    infrastructure::{
        liveliness_monitor::LivelinessMonitor,
        status::{StatusInfo, StatusKind},
    },
    rtps::{
        builtin::{
            builtin_endpoints::BuiltinEndpoints,
            data::{
                builtin_endpoint_set::{BuiltinEndpointFlag, BuiltinEndpointSet},
                spdp_discovered_participant_data::SPDPDiscoveredParticipantData,
            },
            spdp_builtin_participant_reader::SPDPBuiltinParticipantReader,
            spdp_builtin_participant_writer::SPDPbuiltinParticipantWriter,
        },
        common::{
            entity_id::EntityId,
            entity_kind::EntityKind,
            guid::{Guid, GuidPrefix},
            locator::Locator,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            time::RtpsTime,
            types::{ChangeKind, DomainId, ParticipantId},
        },
        entities::{
            entity::Entity,
            history::history_cache::HistoryCache,
            reader::{Reader, ReaderStore, StatefulReader, StatelessReader},
            wire_buffer_pool::WireBufferPool,
            writer::{StatefulWriter, StatelessWriter, Writer, WriterStore},
        },
        logic::{
            sedp_logic::SedpLogic, spdp_logic::SpdpLogic, user_logic::UserLogic,
            wlp_logic::WlpLogic,
        },
        task::sending_handler::{MessageType, SendingHandler},
        transport::{
            get_transport_type, port_manager::PortManager, TransportSender, TransportType,
        },
    },
    utils::timer::timer_handler::TimerHandler,
};

#[derive(Clone)]
pub struct Participant {
    guid: Guid,
    domain_id: DomainId,
    participant_id: ParticipantId,
    builtin_endpoints: Arc<BuiltinEndpoints>,
    // My data
    local_participant_proxy_data: Arc<SPDPDiscoveredParticipantData>,
    // Remote participant data
    remote_participant_proxy_datas: Arc<Mutex<Vec<SPDPDiscoveredParticipantData>>>,

    #[allow(clippy::type_complexity)]
    callback:
        Arc<ArcSwap<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>,

    // RTPS data reader/writer matched with DCPS r/w
    rtps_reader_store: Arc<ReaderStore>,
    rtps_writer_store: Arc<WriterStore>,

    spdp_logic: OnceLock<Arc<Option<SpdpLogic>>>,
    sedp_logic: OnceLock<Arc<Option<SedpLogic>>>,
    user_logic: OnceLock<Arc<Option<UserLogic>>>,
    wlp_logic: Arc<OnceLock<WlpLogic>>,

    // Entity ID used each time RTPS data reader/writer is added (incremented by one)
    current_entity_id: Arc<Mutex<[u8; 3]>>,

    remote_publications: Arc<DashMap<String, HashMap<Guid, PublicationBuiltinTopicData>>>,
    remote_subscriptions: Arc<DashMap<String, HashMap<Guid, SubscriptionBuiltinTopicData>>>,

    liveliness_monitor: Arc<Mutex<Option<LivelinessMonitor>>>,
    working_ips: Vec<String>,
    terminated: Arc<AtomicBool>,
    wire_buffer_pool: Arc<Mutex<WireBufferPool>>,
}
impl Debug for Participant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Participant: {:?}", self.guid)
    }
}

impl Entity for Participant {
    fn guid(&self) -> Guid {
        self.guid
    }

    fn update_status(&self, status: StatusKind, info: Option<Arc<dyn StatusInfo>>) {
        let callback = self.callback.load();
        if let Some(callback) = callback.as_ref() {
            callback(status, info);
        }
    }

    fn set_update_status(
        &self,
        f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    ) {
        self.callback.store(Arc::new(Some(f)));
    }
}

impl Participant {
    pub(crate) fn new(
        domain_id: DomainId,
        participant_id: ParticipantId,
        working_ips: Vec<String>,
    ) -> Self {
        let guid = Guid::new(Guid::generate_unique_guid_prefix(), EntityId::PARTICIPANT);

        let mut local_participant_proxy_data = SPDPDiscoveredParticipantData::new(
            domain_id,
            guid.prefix(),
            Participant::init_builtin_endpoints(),
        );

        Self::init_locators(
            &working_ips,
            &mut local_participant_proxy_data,
            domain_id,
            participant_id,
        );

        let local_participant_proxy_data = Arc::new(local_participant_proxy_data);
        let builtin_endpoints = Arc::new(BuiltinEndpoints::new(guid));

        Self {
            guid,
            domain_id,
            participant_id,
            builtin_endpoints,
            local_participant_proxy_data,
            remote_participant_proxy_datas: Arc::new(Mutex::new(vec![])),
            callback: Arc::new(ArcSwap::new(Arc::new(None))),
            rtps_reader_store: Arc::new(ReaderStore::new()),
            rtps_writer_store: Arc::new(WriterStore::new()),
            spdp_logic: OnceLock::new(),
            sedp_logic: OnceLock::new(),
            user_logic: OnceLock::new(),
            wlp_logic: Arc::new(OnceLock::new()),
            current_entity_id: Arc::new(Mutex::new([0, 0, 0])),
            remote_publications: Arc::new(DashMap::new()),
            remote_subscriptions: Arc::new(DashMap::new()),
            working_ips,
            terminated: Arc::new(AtomicBool::new(false)),
            liveliness_monitor: Arc::new(Mutex::new(None)),
            wire_buffer_pool: Arc::new(Mutex::new(WireBufferPool::new())),
        }
    }

    pub(crate) fn init_builtin_endpoints() -> BuiltinEndpointSet {
        let mut endpointset = BuiltinEndpointSet::new();
        endpointset.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PARTICIPANT_ANNOUNCER);
        endpointset.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PARTICIPANT_DETECTOR);

        endpointset.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PUBLICATIONS_ANNOUNCER);
        endpointset.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PUBLICATIONS_DETECTOR);
        endpointset.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_ANNOUNCER);
        endpointset.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_DETECTOR);

        endpointset.add(BuiltinEndpointFlag::BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_WRITER);
        endpointset.add(BuiltinEndpointFlag::BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_READER);

        endpointset.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_TOPICS_ANNOUNCER);
        endpointset.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_TOPICS_DETECTOR);
        endpointset
    }

    /// Initialize locators for participant proxy data based on transport type.
    /// Registers locators for all available NIC IPs.
    fn init_locators(
        working_ips: &Vec<String>,
        local_participant_proxy_data: &mut SPDPDiscoveredParticipantData,
        domain_id: DomainId,
        participant_id: ParticipantId,
    ) {
        let transport_type = get_transport_type();
        let metatraffic_port =
            PortManager::get_discovery_traffic_unicast_port(domain_id, participant_id) as u32;
        let user_port =
            PortManager::get_user_traffic_unicast_port(domain_id, participant_id) as u32;

        for working_ip in working_ips {
            let Ok(ip) = Ipv4Addr::from_str(working_ip) else {
                continue;
            };

            match transport_type {
                TransportType::UDP => {
                    local_participant_proxy_data.add_metatraffic_unicast_locator(
                        Locator::from_ip_v4_addr_and_port(&ip, metatraffic_port),
                    );
                    local_participant_proxy_data.add_default_unicast_locator(
                        Locator::from_ip_v4_addr_and_port(&ip, user_port),
                    );
                }
                TransportType::TCP => {
                    local_participant_proxy_data.add_metatraffic_unicast_locator(
                        Locator::from_tcp_v4(ip, metatraffic_port),
                    );
                    local_participant_proxy_data
                        .add_default_unicast_locator(Locator::from_tcp_v4(ip, user_port));
                }
                TransportType::Hybrid => {
                    // Add both UDP and TCP locators
                    local_participant_proxy_data.add_metatraffic_unicast_locator(
                        Locator::from_ip_v4_addr_and_port(&ip, metatraffic_port),
                    );
                    local_participant_proxy_data.add_default_unicast_locator(
                        Locator::from_ip_v4_addr_and_port(&ip, user_port),
                    );
                    local_participant_proxy_data.add_metatraffic_unicast_locator(
                        Locator::from_tcp_v4(ip, metatraffic_port),
                    );
                    local_participant_proxy_data
                        .add_default_unicast_locator(Locator::from_tcp_v4(ip, user_port));
                }
                TransportType::SHM => {
                    // metatraffic uses UDP, default uses SHM
                    local_participant_proxy_data.add_metatraffic_unicast_locator(
                        Locator::from_ip_v4_addr_and_port(&ip, metatraffic_port),
                    );
                    local_participant_proxy_data
                        .add_default_unicast_locator(Locator::from_shm(&ip, user_port));
                }
            }
        }
    }

    pub(crate) fn builtin_endpoints(&self) -> Arc<BuiltinEndpoints> {
        self.builtin_endpoints.clone()
    }

    pub(crate) fn local_participant_proxy_data(&self) -> Arc<SPDPDiscoveredParticipantData> {
        self.local_participant_proxy_data.clone()
    }

    pub(crate) fn remote_participant_proxy_datas(
        &self,
    ) -> Arc<Mutex<Vec<SPDPDiscoveredParticipantData>>> {
        self.remote_participant_proxy_datas.clone()
    }

    pub(crate) fn remote_publications(
        &self,
    ) -> Arc<DashMap<String, HashMap<Guid, PublicationBuiltinTopicData>>> {
        self.remote_publications.clone()
    }

    pub(crate) fn remote_subscriptions(
        &self,
    ) -> Arc<DashMap<String, HashMap<Guid, SubscriptionBuiltinTopicData>>> {
        self.remote_subscriptions.clone()
    }

    pub(crate) fn working_ips(&self) -> Vec<String> {
        self.working_ips.clone()
    }

    pub(crate) fn participant_id(&self) -> ParticipantId {
        self.participant_id
    }

    pub(crate) fn find_remote_participant_proxy_data(
        &self,
        guid_prefix: GuidPrefix,
    ) -> Option<SPDPDiscoveredParticipantData> {
        match self.remote_participant_proxy_datas.lock() {
            Ok(remote_participant_proxy_datas) => {
                for remote_participant_data in remote_participant_proxy_datas.iter() {
                    if remote_participant_data.guid_prefix() == guid_prefix {
                        return Some(remote_participant_data.clone());
                    }
                }
                None
            }
            Err(e) => {
                log::error!("Failed to lock remote_participant_proxy_datas: {:?}", e);
                None
            }
        }
    }

    pub(crate) fn add_remote_participant_proxy_data(
        &self,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
    ) {
        match self.remote_participant_proxy_datas.lock() {
            Ok(mut remote_participant_proxy_datas) => {
                remote_participant_proxy_datas.push(spdp_discovered_participant_data);
            }
            Err(e) => {
                log::error!("Failed to lock remote_participant_proxy_datas: {:?}", e);
            }
        }
    }

    pub(crate) fn remove_remote_participant_proxy_data(&self, participant_guid: Guid) -> bool {
        match self.remote_participant_proxy_datas.lock() {
            Ok(mut remote_participant_proxy_datas) => {
                let initial_len = remote_participant_proxy_datas.len();
                remote_participant_proxy_datas
                    .retain(|data| data.participant_guid() != participant_guid);
                let removed = initial_len != remote_participant_proxy_datas.len();
                if removed {
                    debug!(
                        "Removed remote participant proxy data for GUID: {:?}",
                        participant_guid
                    );
                }
                removed
            }
            Err(e) => {
                log::error!("Failed to lock remote_participant_proxy_datas: {:?}", e);
                false
            }
        }
    }

    pub(crate) fn assert_liveliness(&self) -> bool {
        if let Some(wlp_logic) = self.wlp_logic.get() {
            match wlp_logic.assert_participant_liveliness() {
                Ok(()) => return true,
                Err(_) => return false,
            }
        }
        false
    }

    pub(crate) fn spdp_builtin_participant_writer(
        &self,
    ) -> Arc<Mutex<SPDPbuiltinParticipantWriter>> {
        self.builtin_endpoints.spdp_builtin_participant_writer.clone()
    }

    pub(crate) fn spdp_builtin_participant_reader(&self) -> Arc<SPDPBuiltinParticipantReader> {
        self.builtin_endpoints.spdp_builtin_participant_reader.clone()
    }

    pub(crate) fn builtin_participant_message_writer(&self) -> Arc<StatefulWriter> {
        self.builtin_endpoints.builtin_participant_message_writer.clone()
    }

    pub(crate) fn builtin_participant_message_reader(&self) -> Arc<StatefulReader> {
        self.builtin_endpoints.builtin_participant_message_reader.clone()
    }

    pub(crate) fn sedp_builtin_publications_writer(&self) -> Arc<StatefulWriter> {
        self.builtin_endpoints.sedp_builtin_publications_writer.clone()
    }

    pub(crate) fn sedp_builtin_publications_reader(&self) -> Arc<StatefulReader> {
        self.builtin_endpoints.sedp_builtin_publications_reader.clone()
    }

    pub(crate) fn sedp_builtin_subscriptions_writer(&self) -> Arc<StatefulWriter> {
        self.builtin_endpoints.sedp_builtin_subscriptions_writer.clone()
    }

    pub(crate) fn sedp_builtin_subscriptions_reader(&self) -> Arc<StatefulReader> {
        self.builtin_endpoints.sedp_builtin_subscriptions_reader.clone()
    }

    pub(crate) fn sedp_builtin_topics_writer(&self) -> Arc<StatefulWriter> {
        self.builtin_endpoints.sedp_builtin_topics_writer.clone()
    }

    pub(crate) fn sedp_builtin_topics_reader(&self) -> Arc<StatefulReader> {
        self.builtin_endpoints.sedp_builtin_topics_reader.clone()
    }

    pub(crate) fn domain_id(&self) -> DomainId {
        self.domain_id
    }

    pub(crate) fn is_terminated(&self) -> bool {
        self.terminated.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn terminate(&self) {
        self.terminated.store(true, std::sync::atomic::Ordering::Release);
    }

    pub(crate) fn add_writer(
        &self,
        topic_name: &str,
        writer: Arc<dyn Writer + Send + Sync>,
    ) -> RtpsResult<()> {
        self.rtps_writer_store.add(topic_name, writer.clone());
        let liveliness = writer.liveliness()?;

        if writer.guid().entity_id().entity_kind().is_user_defined() {
            if let Some(wlp_logic) = self.wlp_logic.get() {
                let _ = wlp_logic.add_local_writer(writer.guid(), liveliness);
            }
        }
        Ok(())
    }

    pub(crate) fn find_writers_from_topic_name(
        &self,
        topic_name: &str,
    ) -> Vec<Arc<dyn Writer + Send + Sync>> {
        self.rtps_writer_store.get_by_topic(topic_name)
    }

    pub(crate) fn find_writer_from_entity_id(
        &self,
        entity_id: EntityId,
    ) -> Option<Arc<dyn Writer + Send + Sync>> {
        self.rtps_writer_store.get(entity_id)
    }

    pub(crate) fn find_readers_from_topic_name(
        &self,
        topic_name: &str,
    ) -> Vec<Arc<dyn Reader + Send + Sync>> {
        self.rtps_reader_store.get_by_topic(topic_name)
    }

    // pub(crate) fn find_history_cache(
    //     &self,
    //     topic_name: &str,
    //     entity_id: EntityId,
    // ) -> Option<Arc<Mutex<WriterHistoryCache>>> {
    //     self.rtps_writer_map
    //         .get(topic_name)
    //         .and_then(|map| map.get(&entity_id).cloned())
    //         .map(|writer| writer.writer_cache())
    // }

    pub(crate) fn add_reader(&self, topic_name: &str, reader: Arc<dyn Reader + Send + Sync>) {
        self.rtps_reader_store.add(topic_name, reader);
    }

    pub(crate) fn find_reader_from_entity_id(
        &self,
        entity_id: EntityId,
    ) -> Option<Arc<dyn Reader + Send + Sync>> {
        self.rtps_reader_store.get(entity_id)
    }

    /// Function to increment entity_key by 1
    fn increment_key(key: &mut [u8; 3]) {
        for i in (0..3).rev() {
            if key[i] < 0xFF {
                key[i] += 1;
                break;
            } else {
                key[i] = 0x00;
            }
        }
    }

    /// Function to find the next entity_key
    pub(crate) fn next_entity_key(&self, kind: EntityKind) -> EntityId {
        match self.current_entity_id.lock() {
            Ok(mut current_entity_id) => {
                Self::increment_key(&mut current_entity_id);
                EntityId::new(*current_entity_id, kind)
            }
            Err(e) => {
                log::error!("Failed to lock current_entity_id: {:?}", e);
                EntityId::UNKNOWN
            }
        }
    }

    /// Method to remove RTPS Writer
    pub(crate) fn remove_writer(&self, topic_name: String, entity_id: EntityId) -> RtpsResult<()> {
        // Send Data[w(UD)]
        let writer_arc = self.rtps_writer_store.get(entity_id);

        if let Some(writer) = writer_arc {
            if writer.guid().entity_id().entity_kind().is_user_defined() {
                if let Some(wlp_logic) = self.wlp_logic.get() {
                    let _ = wlp_logic.remove_local_writer(writer.guid());
                }
            }

            let writer_info = if let Some(stateful_writer) =
                writer.as_any().downcast_ref::<StatefulWriter>()
            {
                // Remove all timers for this writer (heartbeat, heartbeat delay, nack response)
                if let Ok(handler) = TimerHandler::get_instance(self.guid().prefix()).lock() {
                    handler.remove_timers_by_entity(entity_id);
                }
                stateful_writer.compare_and_set_heartbeat_timer_running(true, false)?;
                Some((stateful_writer.guid(), stateful_writer.publication_builtin_topic_data()?))
            } else {
                writer.as_any().downcast_ref::<StatelessWriter>().and_then(|stateless_writer| {
                    stateless_writer
                        .publication_builtin_topic_data()
                        .ok()
                        .map(|data| (stateless_writer.guid(), data))
                })
            };

            if let Some((writer_guid, _)) = writer_info {
                // Also unmatch local readers in this participant before removing the writer.
                self.remove_unmatched_writer_from_reader(writer_guid);
                // Bare dispose: instance is identified by PID_KEY_HASH inline QoS only
                let a_cache_change = self.sedp_builtin_publications_writer().new_change(
                    ChangeKind::NotAliveDisposedUnregistered,
                    Vec::new(),
                    InstanceHandle::from_guid(&writer_guid),
                    Some(RtpsTime::now()),
                );

                let handler = SendingHandler::get_instance(Arc::new(self.clone()), None, None);
                handler.push_message_and_wake(MessageType::SedpTerminateEndpoint(
                    self.sedp_builtin_publications_writer().guid(),
                    Arc::new(a_cache_change),
                ));
                log::info!("Remote writer with GUID {:?} terminated", writer_guid);

                // Remove builtin topic data from builtin endpoint
                match self.builtin_endpoints.sedp_builtin_publications_writer.writer_cache().lock()
                {
                    Ok(mut writer_cache) => {
                        for cache in writer_cache.get_changes() {
                            if let Ok(publication_builtin_topic_data) =
                                PublicationBuiltinTopicData::from_serialized_data(
                                    cache.data_value(),
                                )
                            {
                                if publication_builtin_topic_data.endpoint_guid() == writer_guid {
                                    let _ = writer_cache.remove_change(cache);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to lock sedp_builtin_publications_writer's cache: {:?}",
                            e
                        );
                    }
                }
            }
        }

        // Remove from store
        self.rtps_writer_store.remove(&topic_name, entity_id);
        if let Some(mut entry) = self.remote_publications().get_mut(&topic_name) {
            entry.value_mut().remove(&Guid::new(self.guid().prefix(), entity_id));

            if entry.value().is_empty() {
                drop(entry);
                self.remote_publications().remove(&topic_name);
            }
        }

        Ok(())
    }

    /// Method to remove RTPS Reader
    pub(crate) fn remove_reader(&self, topic_name: String, entity_id: EntityId) -> RtpsResult<()> {
        // Send Data[r(UD)]
        let reader_arc = self.rtps_reader_store.get(entity_id);

        if let Some(reader) = reader_arc {
            // Remove all timers for this reader (acknack, nackfrag)
            if let Ok(handler) = TimerHandler::get_instance(self.guid().prefix()).lock() {
                handler.remove_timers_by_entity(entity_id);
            }

            let reader_info = if let Some(stateful_reader) =
                reader.as_any().downcast_ref::<StatefulReader>()
            {
                Some((stateful_reader.guid(), stateful_reader.subscription_builtin_topic_data()?))
            } else {
                reader.as_any().downcast_ref::<StatelessReader>().and_then(|stateless_reader| {
                    stateless_reader
                        .subscription_builtin_topic_data()
                        .ok()
                        .map(|data| (stateless_reader.guid(), data))
                })
            };

            if let Some((reader_guid, _)) = reader_info {
                // Also unmatch local writers in this participant before removing the reader.
                self.remove_unmatched_reader_from_writer(reader_guid);
                // Bare dispose: instance is identified by PID_KEY_HASH inline QoS only
                let a_cache_change = self.sedp_builtin_subscriptions_writer().new_change(
                    ChangeKind::NotAliveDisposedUnregistered,
                    Vec::new(),
                    InstanceHandle::from_guid(&reader_guid),
                    Some(RtpsTime::now()),
                );

                let handler = SendingHandler::get_instance(Arc::new(self.clone()), None, None);
                handler.push_message_and_wake(MessageType::SedpTerminateEndpoint(
                    self.sedp_builtin_subscriptions_writer().guid(),
                    Arc::new(a_cache_change),
                ));

                // Remove builtin topic data from builtin endpoint
                match self.builtin_endpoints.sedp_builtin_subscriptions_writer.writer_cache().lock()
                {
                    Ok(mut writer_cache) => {
                        for cache in writer_cache.get_changes() {
                            if let Ok(subscription_builtin_topic_data) =
                                SubscriptionBuiltinTopicData::from_serialized_data(
                                    cache.data_value(),
                                )
                            {
                                if subscription_builtin_topic_data.endpoint_guid() == reader_guid {
                                    let _ = writer_cache.remove_change(cache);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to lock sedp_builtin_subscriptions_writer's cache: {:?}",
                            e
                        );
                    }
                }
            }
        }

        // Remove from store
        self.rtps_reader_store.remove(&topic_name, entity_id);
        if let Some(mut entry) = self.remote_subscriptions().get_mut(&topic_name) {
            entry.value_mut().remove(&Guid::new(self.guid().prefix(), entity_id));

            if entry.value().is_empty() {
                drop(entry);
                self.remote_subscriptions().remove(&topic_name);
            }
        }

        Ok(())
    }

    /// Iterate through all Readers in the Participant to find Readers matched with the Writer, then remove Writer Proxy
    pub(crate) fn remove_unmatched_writer_from_reader(&self, writer_guid: Guid) {
        for reader in self.rtps_reader_store.iter_all() {
            if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                if let Ok(mut writer_proxies) = stateful_reader.writer_proxies().lock() {
                    let len_before_unmatch = writer_proxies.len();
                    debug!(
                        "Before unmatching with writer, this reader had {:?} matched writer",
                        len_before_unmatch
                    );

                    writer_proxies
                        .retain(|writer_proxy| writer_proxy.remote_writer_guid() != writer_guid);

                    if len_before_unmatch == writer_proxies.len() + 1 {
                        stateful_reader.update_subscription_matched_status(
                            -1,
                            InstanceHandle::from_guid(&writer_guid),
                        );
                        info!("Unmatched with remote writer {:?}", writer_guid);
                        debug!("Current number of matched writer: {:?}", writer_proxies.len());
                    } else if len_before_unmatch == writer_proxies.len() {
                        debug!("No matching writer found to unmatch for GUID: {:?}", writer_guid);
                    } else {
                        log::error!(
                            "This is abnormal behavior, this reader had {:?} writer proxy of same guid",
                            len_before_unmatch - writer_proxies.len()
                        );
                    }
                }
            } else if let Some(stateless_reader) = reader.as_any().downcast_ref::<StatelessReader>()
            {
                if let Ok(mut remote_writer_info) = stateless_reader.remote_writer_infos().lock() {
                    let len_before_unmatch = remote_writer_info.len();
                    debug!(
                        "Before unmatching with writer, this reader had {:?} matched writer",
                        len_before_unmatch
                    );

                    remote_writer_info.retain(|info| info.remote_writer_guid() != writer_guid);

                    if len_before_unmatch == remote_writer_info.len() + 1 {
                        stateless_reader.update_subscription_matched_status(
                            -1,
                            InstanceHandle::from_guid(&writer_guid),
                        );
                        info!("Unmatched with remote writer {:?}", writer_guid);
                        debug!("Current number of matched writer: {:?}", remote_writer_info.len());
                    } else if len_before_unmatch == remote_writer_info.len() {
                        debug!("No matching writer found to unmatch for GUID: {:?}", writer_guid);
                    } else {
                        log::error!(
                            "This is abnormal behavior, this reader had {:?} writer locator of same guid",
                            len_before_unmatch - remote_writer_info.len()
                        );
                    }
                }
            }
        }
    }

    /// Iterate through all Writers in the Participant to find Writers matched with the Reader, then remove Reader Locator or Reader Proxy
    pub(crate) fn remove_unmatched_reader_from_writer(&self, reader_guid: Guid) {
        for writer in self.rtps_writer_store.iter_all() {
            if let Some(stateful_writer) = writer.as_any().downcast_ref::<StatefulWriter>() {
                if let Ok(mut reader_proxies) = stateful_writer.reader_proxies().lock() {
                    let len_before_unmatch = reader_proxies.len();
                    debug!(
                        "Before unmatching with reader, this writer had {:?} matched readers",
                        len_before_unmatch
                    );

                    reader_proxies
                        .retain(|reader_proxy| reader_proxy.remote_reader_guid() != reader_guid);

                    if len_before_unmatch == reader_proxies.len() + 1 {
                        stateful_writer.update_publication_matched_status(
                            -1,
                            InstanceHandle::from_guid(&reader_guid),
                        );
                        info!("Unmatched with remote reader {:?}", reader_guid);
                        debug!("Current number of matched reader: {:?}", reader_proxies.len());
                    } else if len_before_unmatch == reader_proxies.len() {
                        debug!("No matching reader found to unmatch for GUID: {:?}", reader_guid);
                    } else {
                        log::error!(
                            "This is abnormal behavior, this writer had {:?} reader proxy of same guid",
                            len_before_unmatch - reader_proxies.len()
                        );
                    }
                }
            } else if let Some(stateless_writer) = writer.as_any().downcast_ref::<StatelessWriter>()
            {
                if let Ok(mut reader_locator) = stateless_writer.reader_locator().lock() {
                    let len_before_unmatch = reader_locator.len();
                    debug!(
                        "Before unmatching with reader, this writer had {:?} matched readers",
                        len_before_unmatch
                    );

                    let matching_count = reader_locator
                        .iter()
                        .filter(|locator| {
                            locator.guid_prefix() == reader_guid.prefix()
                                && locator.remote_entity_id() == reader_guid.entity_id()
                        })
                        .count();

                    reader_locator.retain(|locator| {
                        locator.guid_prefix() != reader_guid.prefix()
                            || locator.remote_entity_id() != reader_guid.entity_id()
                    });

                    if matching_count > 0 {
                        stateless_writer.update_publication_matched_status(
                            -1,
                            InstanceHandle::from_guid(&reader_guid),
                        );
                        info!("Unmatched with remote reader {:?}", reader_guid);
                        debug!("Current number of matched reader: {:?}", reader_locator.len());
                    } else if len_before_unmatch == reader_locator.len() {
                        debug!("No matching reader found to unmatch for GUID: {:?}", reader_guid);
                    }
                }
            }
        }
    }

    pub(crate) fn find_readers_matched_with_local_writer(
        &self,
        writer_guid: &Guid,
    ) -> RtpsResult<Vec<Arc<dyn Reader + Send + Sync>>> {
        let writer = match self.find_writer_from_entity_id(writer_guid.entity_id()) {
            Some(w) => w,
            None => return Ok(Vec::new()),
        };

        let topic_name = writer
            .get_publication_builtin_topic_data()?
            .ok_or_else(|| {
                RtpsError::new(
                    RtpsErrorCode::NotInitialized,
                    format!("Publication builtin topic data not set for writer: {:?}", writer_guid),
                )
            })?
            .topic_name();

        Ok(self.find_readers_from_topic_name(&topic_name))
    }

    pub(crate) fn find_readers_matched_with_remote_writer(
        &self,
        writer_guid: Guid,
    ) -> RtpsResult<Vec<Arc<dyn Reader + Send + Sync>>> {
        let mut matched_readers: Vec<Arc<dyn Reader + Send + Sync>> = Vec::new();

        for reader in self.rtps_reader_store.iter_all() {
            if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                let writer_proxies_arc = stateful_reader.writer_proxies();
                let writer_proxies = writer_proxies_arc.lock().map_err(|_e| {
                    RtpsError::new(RtpsErrorCode::LockError, "Failed to acquire writer proxy lock")
                })?;

                if writer_proxies.iter().any(|wp| wp.remote_writer_guid() == writer_guid) {
                    matched_readers.push(reader.clone());
                }
            } else if let Some(stateless_reader) = reader.as_any().downcast_ref::<StatelessReader>()
            {
                let remote_writer_infos_arc = stateless_reader.remote_writer_infos();
                let remote_writer_infos = remote_writer_infos_arc.lock().map_err(|e| {
                    RtpsError::new(
                        RtpsErrorCode::LockError,
                        format!("Failed to acquire writer locator lock: {:?}", e),
                    )
                })?;

                if remote_writer_infos.iter().any(|w| w.remote_writer_guid() == writer_guid) {
                    matched_readers.push(reader.clone());
                }
            }
        }

        Ok(matched_readers)
    }

    /// Function to send Data(w[UD]), DATA(r[UD]) for all DataWriters and DataReaders in the Participant
    /// Later changed to send Data(p[UD]) only once
    pub(crate) fn send_termination_message_on_shutdown(&self) -> RtpsResult<()> {
        // Send Data(r[UD]) messages
        for reader in self.rtps_reader_store.iter_all() {
            // Bare dispose: instance is identified by PID_KEY_HASH inline QoS only
            let a_cache_change = self.sedp_builtin_subscriptions_writer().new_change(
                ChangeKind::NotAliveDisposedUnregistered,
                Vec::new(),
                InstanceHandle::from_guid(&reader.guid()),
                Some(RtpsTime::now()),
            );

            let handler = SendingHandler::get_instance(Arc::new(self.clone()), None, None);
            if let Some(sending_task) = handler.get_sending_task() {
                // Send messages synchronously without using event loop
                if let Ok(sending_task_guard) = sending_task.lock() {
                    // let join_handle = sending_task_guard.create_worker_thread(MessageType::SedpTerminateEndpoint(
                    //     self.sedp_builtin_subscriptions_writer().guid(),
                    //     Arc::new(a_cache_change),
                    // ));

                    // if let Err(e) = join_handle.join() {
                    //     log::error!("Failed to join sending task thread for SEDP Terminate endpoint task: {:?}", e);
                    // }
                    sending_task_guard.sync_sedp_terminate_endpoint_task(
                        self.sedp_builtin_subscriptions_writer().guid(),
                        Arc::new(a_cache_change),
                    );
                }
            }
        }

        // Send Data(w[UD]) messages
        for writer in self.rtps_writer_store.iter_all() {
            // Bare dispose: instance is identified by PID_KEY_HASH inline QoS only
            let a_cache_change = self.sedp_builtin_publications_writer().new_change(
                ChangeKind::NotAliveDisposedUnregistered,
                Vec::new(),
                InstanceHandle::from_guid(&writer.guid()),
                Some(RtpsTime::now()),
            );

            let handler = SendingHandler::get_instance(Arc::new(self.clone()), None, None);
            if let Some(sending_task) = handler.get_sending_task() {
                // Send messages synchronously without using event loop
                if let Ok(sending_task_guard) = sending_task.lock() {
                    // let join_handle = sending_task_guard.create_worker_thread(
                    //     MessageType::SedpTerminateEndpoint(
                    //         self.sedp_builtin_publications_writer().guid(),
                    //         Arc::new(a_cache_change),
                    //     ),
                    // );

                    // if let Err(e) = join_handle.join() {
                    //     log::error!("Failed to join sending task thread for SEDP Terminate endpoint task: {:?}", e);
                    // }

                    sending_task_guard.sync_sedp_terminate_endpoint_task(
                        self.sedp_builtin_publications_writer().guid(),
                        Arc::new(a_cache_change),
                    );
                }
            }
        }

        // Send Data(p[UD]) messages
        let handler = SendingHandler::get_instance(Arc::new(self.clone()), None, None);
        if let Some(sending_task) = handler.get_sending_task() {
            // Send messages synchronously without using event loop
            if let Ok(sending_task_guard) = sending_task.lock() {
                // let join_handle = sending_task_guard
                //     .create_worker_thread(MessageType::SpdpTerminateParticipant());

                // if let Err(e) = join_handle.join() {
                //     log::error!("Failed to join sending task thread for SPDP Terminate participant task: {:?}", e);
                // }

                sending_task_guard.sync_spdp_terminate_participant_task()?;
            }
        }

        Ok(())
    }

    /// Method to remove all information about remote participant
    pub(crate) fn unmatch_with_remote_participant(&self, terminated_participant_guid: &Guid) {
        // Remove participant proxy
        if !self.remove_remote_participant_proxy_data(*terminated_participant_guid) {
            debug!("Remote participant not found, participant may have already been unmatched");
            return;
        }

        // Remove discovered remote endpoint data used by graph/introspection paths.
        // Without this, a terminated participant can stay visible in discovery snapshots
        // even after its proxy/matching state has been removed.
        let terminated_prefix = terminated_participant_guid.prefix();
        self.remote_publications().retain(|topic_name, endpoints| {
            endpoints.retain(|endpoint_guid, _| endpoint_guid.prefix() != terminated_prefix);
            let keep_topic = !endpoints.is_empty();
            if !keep_topic {
                debug!(
                    "Removed all remote publications for terminated participant on topic '{}'",
                    topic_name
                );
            }
            keep_topic
        });
        self.remote_subscriptions().retain(|topic_name, endpoints| {
            endpoints.retain(|endpoint_guid, _| endpoint_guid.prefix() != terminated_prefix);
            let keep_topic = !endpoints.is_empty();
            if !keep_topic {
                debug!(
                    "Removed all remote subscriptions for terminated participant on topic '{}'",
                    topic_name
                );
            }
            keep_topic
        });

        // Remove proxies from built-in endpoint
        self.builtin_endpoints().remove_unmatched_endpoint(terminated_prefix);

        // Remove proxies from endpoint
        self.remove_all_unmatched_endpoint_from_terminated_participant(terminated_prefix);

        info!("Successfully unmatched with remote participant: {:?}", terminated_participant_guid);
    }

    pub(crate) fn remove_remote_publication_by_guid(&self, writer_guid: Guid) {
        self.remote_publications().retain(|topic_name, endpoints| {
            let removed = endpoints.remove(&writer_guid).is_some();
            let keep_topic = !endpoints.is_empty();
            if removed && !keep_topic {
                debug!(
                    "Removed final remote publication entry for topic '{}' after writer termination",
                    topic_name
                );
            }
            keep_topic
        });
    }

    pub(crate) fn remove_remote_subscription_by_guid(&self, reader_guid: Guid) {
        self.remote_subscriptions().retain(|topic_name, endpoints| {
            let removed = endpoints.remove(&reader_guid).is_some();
            let keep_topic = !endpoints.is_empty();
            if removed && !keep_topic {
                debug!(
                    "Removed final remote subscription entry for topic '{}' after reader termination",
                    topic_name
                );
            }
            keep_topic
        });
    }

    /// Function to remove all Remote Endpoints with the given GuidPrefix when Remote Participant terminates
    pub(crate) fn remove_all_unmatched_endpoint_from_terminated_participant(
        &self,
        terminated_participant_guid_prefix: GuidPrefix,
    ) {
        for reader in self.rtps_reader_store.iter_all() {
            if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                if let Ok(mut writer_proxies) = stateful_reader.writer_proxies().lock() {
                    debug!(
                        "Before unmatching with writer, this reader had {:?} matched writer",
                        writer_proxies.len()
                    );
                    for writer_proxy in writer_proxies.iter() {
                        if writer_proxy.remote_writer_guid().prefix()
                            == terminated_participant_guid_prefix
                        {
                            stateful_reader.update_subscription_matched_status(
                                -1,
                                InstanceHandle::from_guid(&writer_proxy.remote_writer_guid()),
                            );
                        }
                    }
                    writer_proxies.retain(|writer_proxy| {
                        writer_proxy.remote_writer_guid().prefix()
                            != terminated_participant_guid_prefix
                    });

                    debug!(
                        "Removed all unmatched remote writers from participant: {:?}",
                        terminated_participant_guid_prefix
                    );
                    debug!("Current number of matched writer: {:?}", writer_proxies.len());
                }
            } else if let Some(stateless_reader) = reader.as_any().downcast_ref::<StatelessReader>()
            {
                if let Ok(mut remote_writer_info) = stateless_reader.remote_writer_infos().lock() {
                    debug!(
                        "Before unmatching with writer, this reader had {:?} matched writer",
                        remote_writer_info.len()
                    );
                    for remote_writer_info in remote_writer_info.iter() {
                        if remote_writer_info.remote_writer_guid().prefix()
                            == terminated_participant_guid_prefix
                        {
                            stateless_reader.update_subscription_matched_status(
                                -1,
                                InstanceHandle::from_guid(&remote_writer_info.remote_writer_guid()),
                            );
                        }
                    }
                    remote_writer_info.retain(|remote_writer_info| {
                        remote_writer_info.remote_writer_guid().prefix()
                            != terminated_participant_guid_prefix
                    });

                    debug!(
                        "Removed all unmatched remote writers from participant: {:?}",
                        terminated_participant_guid_prefix
                    );
                    debug!("Current number of matched writer: {:?}", remote_writer_info.len());
                }
            }
        }

        for writer in self.rtps_writer_store.iter_all() {
            if let Some(stateful_writer) = writer.as_any().downcast_ref::<StatefulWriter>() {
                if let Ok(mut reader_proxies) = stateful_writer.reader_proxies().lock() {
                    debug!(
                        "Before unmatching with reader, this writer had {:?} matched readers",
                        reader_proxies.len()
                    );
                    for reader_proxy in reader_proxies.iter() {
                        if reader_proxy.remote_reader_guid().prefix()
                            == terminated_participant_guid_prefix
                        {
                            stateful_writer.update_publication_matched_status(
                                -1,
                                InstanceHandle::from_guid(&reader_proxy.remote_reader_guid()),
                            );
                        }
                    }
                    reader_proxies.retain(|reader_proxy| {
                        reader_proxy.remote_reader_guid().prefix()
                            != terminated_participant_guid_prefix
                    });
                    debug!(
                        "Removed all unmatched reader proxies from unmatched participant: {:?}",
                        terminated_participant_guid_prefix
                    );
                    debug!("Current number of matched reader: {:?}", reader_proxies.len());
                }
            } else if let Some(stateless_writer) = writer.as_any().downcast_ref::<StatelessWriter>()
            {
                if let Ok(mut reader_locator) = stateless_writer.reader_locator().lock() {
                    debug!(
                        "Before unmatching with reader, this writer had {:?} matched readers",
                        reader_locator.len()
                    );

                    // Collect unique entity IDs to avoid duplicate callbacks
                    let unique_entity_ids: std::collections::HashSet<_> = reader_locator
                        .iter()
                        .filter(|locator| {
                            locator.guid_prefix() == terminated_participant_guid_prefix
                        })
                        .map(|locator| locator.remote_entity_id())
                        .collect();

                    for entity_id in unique_entity_ids {
                        stateless_writer.update_publication_matched_status(
                            -1,
                            InstanceHandle::from_guid(&Guid::new(
                                terminated_participant_guid_prefix,
                                entity_id,
                            )),
                        );
                    }

                    reader_locator.retain(|locator| {
                        locator.guid_prefix() != terminated_participant_guid_prefix
                    });
                    debug!(
                        "Removed all unmatched reader locators from participant: {:?}",
                        terminated_participant_guid_prefix
                    );
                    debug!("Current number of matched reader: {:?}", reader_locator.len());
                }
            }
        }
    }

    pub(crate) fn on_reader_cache_change_removal(
        &self,
        entity_id: EntityId,
        sequence_number: SequenceNumber,
    ) {
        for reader in self.rtps_reader_store.iter_all() {
            if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                match stateful_reader.writer_proxies().lock() {
                    Ok(mut writer_proxies) => {
                        for writer_proxy in writer_proxies.iter_mut() {
                            if writer_proxy.remote_writer_guid().entity_id() == entity_id {
                                writer_proxy.remove_change_from_writer_by_sn(sequence_number);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to acquire writer_proxies lock: {}", e);
                    }
                };
            }
        }
    }

    pub(crate) fn set_spdp_logic(&self, spdp_logic: Arc<Option<SpdpLogic>>) {
        let _ = self.spdp_logic.set(spdp_logic);
    }

    pub(crate) fn set_sedp_logic(&self, sedp_logic: Arc<Option<SedpLogic>>) {
        let _ = self.sedp_logic.set(sedp_logic);
    }

    pub(crate) fn set_user_logic(&self, user_logic: Arc<Option<UserLogic>>) {
        let _ = self.user_logic.set(user_logic);
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn get_logics(
        &self,
    ) -> (Arc<Option<SpdpLogic>>, Arc<Option<SedpLogic>>, Arc<Option<UserLogic>>) {
        (
            self.spdp_logic.get().expect("spdp_logic not set").clone(),
            self.sedp_logic.get().expect("sedp_logic not set").clone(),
            self.user_logic.get().expect("user_logic not set").clone(),
        )
    }

    pub(crate) fn wire_buffer_pool(&self) -> &Mutex<WireBufferPool> {
        &self.wire_buffer_pool
    }

    /// Initialize all logic instances. Must be called immediately after creating Participant.
    /// This creates SPDP, SEDP, User, and WLP logic instances using the provided sender.
    pub(crate) fn init_logics(
        self: &Arc<Self>,
        sender: Arc<TransportSender>,
        tcp_sender: Option<Arc<TransportSender>>,
        shm_sender: Option<Arc<TransportSender>>,
    ) {
        // Get initial peers from environment for TCP/Hybrid discovery
        let initial_peers = crate::common::env::get_initial_peers();
        if !initial_peers.is_empty() {
            log::info!("Configured initial peers for SPDP: {:?}", initial_peers);
        }

        // Create SPDP logic
        let spdp_logic = Arc::new(Some(SpdpLogic::new(
            self.clone(),
            Some(sender.clone()),
            tcp_sender.clone(),
            initial_peers,
        )));
        let _ = self.spdp_logic.set(spdp_logic);

        // Create SEDP logic
        let sedp_logic = Arc::new(Some(SedpLogic::new(self.clone(), Some(sender.clone()))));
        let _ = self.sedp_logic.set(sedp_logic);

        // Create User logic
        let user_logic = Arc::new(Some(UserLogic::new(
            self.clone(),
            Some(sender.clone()),
            tcp_sender.clone(),
            shm_sender,
        )));
        let _ = self.user_logic.set(user_logic);

        // Create WLP logic
        let wlp_logic = WlpLogic::new(self.clone(), sender);
        let _ = self.wlp_logic.set(wlp_logic);
    }

    pub(crate) fn set_wlp_logic(&self, wlp_logic: WlpLogic) {
        let _ = self.wlp_logic.set(wlp_logic);
    }

    pub(crate) fn wlp_logic(&self) -> Option<WlpLogic> {
        self.wlp_logic.get().cloned()
    }

    /// Clear the WlpLogic sender reference to allow Arc cleanup during shutdown.
    pub(crate) fn clear_wlp_logic_sender(&self) {
        if let Some(wlp_logic) = self.wlp_logic.get() {
            wlp_logic.clear_sender();
        }
    }

    pub(crate) fn increase_manual_liveliness_count(&self) -> RtpsResult<()> {
        self.local_participant_proxy_data.increase_manual_liveliness_count()
    }

    pub(crate) fn liveliness_monitor(&self) -> Arc<Mutex<Option<LivelinessMonitor>>> {
        self.liveliness_monitor.clone()
    }

    pub(crate) fn shutdown_liveliness_monitor(&self) {
        if let Ok(mut monitor) = self.liveliness_monitor.lock() {
            if let Some(ref mut m) = *monitor {
                m.shutdown();
            }
            *monitor = None;
        }
    }
}
