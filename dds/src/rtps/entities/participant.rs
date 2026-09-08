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
    net::SocketAddr,
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
            locator::{is_same_host, Locator},
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            time::RtpsTime,
            types::{ChangeKind, DomainId, ParticipantId},
        },
        entities::{
            entity::Entity,
            history::{cache_change::CacheChange, history_cache::HistoryCache},
            reader::{Reader, ReaderCallbackLease, ReaderStore, StatefulReader, StatelessReader},
            wire_buffer_pool::WireBufferPool,
            writer::{StatefulWriter, StatelessWriter, Writer, WriterStore},
        },
        logic::{
            sedp_logic::SedpLogic, spdp_logic::SpdpLogic, user_logic::UserLogic,
            wlp_logic::WlpLogic,
        },
        transport::plugin::TransportPlugin,
    },
    utils::timer::{timer_handler::TimerHandler, timer_id::TimerId},
    xtypes::{new_shared_registry, SharedTypeRegistry},
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

    // Callback for remote reader/writer discovery events.
    #[allow(clippy::type_complexity)]
    endpoint_discovery_cb: Arc<ArcSwap<Option<Arc<dyn Fn(&EndpointDiscoveryEvent) + Send + Sync>>>>,

    // RTPS data reader/writer matched with DCPS r/w
    rtps_reader_store: Arc<ReaderStore>,
    rtps_writer_store: Arc<WriterStore>,

    spdp_logic: OnceLock<Arc<Option<SpdpLogic>>>,
    sedp_logic: OnceLock<Arc<Option<SedpLogic>>>,
    user_logic: OnceLock<Arc<Option<UserLogic>>>,
    wlp_logic: Arc<OnceLock<WlpLogic>>,

    transport: OnceLock<Arc<dyn TransportPlugin>>,

    // Entity ID used each time RTPS data reader/writer is added (incremented by one)
    current_entity_id: Arc<Mutex<[u8; 3]>>,

    remote_publications: Arc<DashMap<String, HashMap<Guid, PublicationBuiltinTopicData>>>,
    remote_subscriptions: Arc<DashMap<String, HashMap<Guid, SubscriptionBuiltinTopicData>>>,

    // Dynamic-type registry populated from discovered TypeObjects.
    type_registry: SharedTypeRegistry,

    liveliness_monitor: Arc<Mutex<Option<LivelinessMonitor>>>,
    working_ips: Vec<String>,
    remote_same_host: Arc<DashMap<GuidPrefix, bool>>,
    terminated: Arc<AtomicBool>,
    wire_buffer_pool: Arc<Mutex<WireBufferPool>>,
}
impl Debug for Participant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Participant: {}", self.guid)
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

/// A remote endpoint (SEDP) discovery event.
/// `*Alive` carries the endpoint's builtin-topic data; `*Disposed` carries only its GUID.
pub enum EndpointDiscoveryEvent {
    WriterAlive(PublicationBuiltinTopicData),
    WriterDisposed(Guid),
    ReaderAlive(SubscriptionBuiltinTopicData),
    ReaderDisposed(Guid),
}

impl Participant {
    /// Create a new Participant.
    ///
    /// `metatraffic_unicast_locators` / `default_unicast_locators` are the
    /// locators this participant advertises to peers over SPDP. The caller
    /// (typically `DcpsBridge`) obtains them from the owning `TransportPlugin`
    /// so that locator generation stays encapsulated in the transport layer —
    /// `Participant` intentionally has no knowledge of transport types.
    pub(crate) fn new(
        domain_id: DomainId,
        participant_id: ParticipantId,
        working_ips: Vec<String>,
        metatraffic_unicast_locators: Vec<Locator>,
        default_unicast_locators: Vec<Locator>,
    ) -> Self {
        Self::with_guid_prefix(
            Guid::generate_unique_guid_prefix(),
            domain_id,
            participant_id,
            working_ips,
            metatraffic_unicast_locators,
            default_unicast_locators,
        )
    }

    /// As `new`, but adopting a prefix the caller has already handed to
    /// something constructed before this participant.
    pub(crate) fn with_guid_prefix(
        guid_prefix: GuidPrefix,
        domain_id: DomainId,
        participant_id: ParticipantId,
        working_ips: Vec<String>,
        metatraffic_unicast_locators: Vec<Locator>,
        default_unicast_locators: Vec<Locator>,
    ) -> Self {
        let guid = Guid::new(guid_prefix, EntityId::PARTICIPANT);

        let mut local_participant_proxy_data = SPDPDiscoveredParticipantData::new(
            domain_id,
            guid.prefix(),
            Participant::init_builtin_endpoints(),
        );

        for locator in metatraffic_unicast_locators {
            local_participant_proxy_data.add_metatraffic_unicast_locator(locator);
        }
        for locator in default_unicast_locators {
            local_participant_proxy_data.add_default_unicast_locator(locator);
        }

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
            endpoint_discovery_cb: Arc::new(ArcSwap::new(Arc::new(None))),
            rtps_reader_store: Arc::new(ReaderStore::new()),
            rtps_writer_store: Arc::new(WriterStore::new()),
            spdp_logic: OnceLock::new(),
            sedp_logic: OnceLock::new(),
            user_logic: OnceLock::new(),
            wlp_logic: Arc::new(OnceLock::new()),
            transport: OnceLock::new(),
            current_entity_id: Arc::new(Mutex::new([0, 0, 0])),
            remote_publications: Arc::new(DashMap::new()),
            remote_subscriptions: Arc::new(DashMap::new()),
            type_registry: new_shared_registry(),
            working_ips,
            remote_same_host: Arc::new(DashMap::new()),
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

    pub(crate) fn builtin_endpoints(&self) -> Arc<BuiltinEndpoints> {
        self.builtin_endpoints.clone()
    }

    pub(crate) fn local_participant_proxy_data(&self) -> Arc<SPDPDiscoveredParticipantData> {
        self.local_participant_proxy_data.clone()
    }

    /// Record this participant's own receive-buffer size for SPDP to advertise.
    /// Called once, right after construction, while the proxy Arc is still
    /// uniquely owned - `Arc::get_mut` fails silently (a no-op) otherwise.
    pub(crate) fn set_local_receive_buffer_size(&mut self, size: Option<usize>) {
        match Arc::get_mut(&mut self.local_participant_proxy_data) {
            Some(proxy) => proxy.set_receive_buffer_size(size),
            None => log::warn!(
                "local_participant_proxy_data already shared; receive buffer size not recorded"
            ),
        }
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

    pub(crate) fn set_endpoint_discovery_cb(
        &self,
        f: Arc<dyn Fn(&EndpointDiscoveryEvent) + Send + Sync>,
    ) {
        self.endpoint_discovery_cb.store(Arc::new(Some(f)));
    }

    pub(crate) fn fire_endpoint_discovery(&self, event: &EndpointDiscoveryEvent) {
        let cb = self.endpoint_discovery_cb.load();
        if let Some(cb) = cb.as_ref() {
            cb(event);
        }
    }

    pub(crate) fn remote_subscriptions(
        &self,
    ) -> Arc<DashMap<String, HashMap<Guid, SubscriptionBuiltinTopicData>>> {
        self.remote_subscriptions.clone()
    }

    pub(crate) fn type_registry(&self) -> SharedTypeRegistry {
        self.type_registry.clone()
    }

    pub(crate) fn register_local_type_objects(
        &self,
        objects: &[(crate::xtypes::TypeIdentifier, crate::xtypes::TypeObject)],
    ) {
        if objects.is_empty() {
            return;
        }
        if let Ok(mut registry) = self.type_registry.write() {
            registry.register_closure(objects);
        }
    }

    pub(crate) fn fetch_type_via_lookup(
        &self,
        remote_prefix: GuidPrefix,
        type_id: crate::xtypes::TypeIdentifier,
    ) {
        if let Some(sedp_logic) = self.sedp_logic.get() {
            if let Some(sedp_logic) = sedp_logic.as_ref().as_ref() {
                let _ = sedp_logic.request_get_types(remote_prefix, vec![type_id]);
            }
        }
    }

    /// Whether the peer behind `remote_prefix` runs on this host. A source
    /// address decides and keeps the verdict, `None` only reads one, so the
    /// answer does not depend on which announcement arrived first.
    pub(crate) fn remote_is_same_host(
        &self,
        remote_prefix: GuidPrefix,
        from_addr: Option<SocketAddr>,
    ) -> bool {
        match from_addr {
            Some(from_addr) => *self
                .remote_same_host
                .entry(remote_prefix)
                .or_insert_with(|| is_same_host(from_addr)),
            None => self.remote_same_host.get(&remote_prefix).is_some_and(|held| *held),
        }
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

    /// The receive-buffer size a discovered participant advertised, or `None` when it did not
    /// advertise a usable one. Reads under the lock without cloning the whole proxy, because
    /// the send path asks once per destination on every fragmented send.
    pub(crate) fn remote_receive_buffer_size(&self, guid_prefix: GuidPrefix) -> Option<usize> {
        match self.remote_participant_proxy_datas.lock() {
            Ok(datas) => datas
                .iter()
                .find(|data| data.guid_prefix() == guid_prefix)
                .and_then(|data| data.receive_buffer_size()),
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
                    debug!("Removed remote participant proxy data for GUID: {}", participant_guid);
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

    pub(crate) fn type_lookup_request_writer(&self) -> Arc<StatefulWriter> {
        self.builtin_endpoints.type_lookup_request_writer.clone()
    }

    pub(crate) fn type_lookup_request_reader(&self) -> Arc<StatefulReader> {
        self.builtin_endpoints.type_lookup_request_reader.clone()
    }

    pub(crate) fn type_lookup_reply_writer(&self) -> Arc<StatefulWriter> {
        self.builtin_endpoints.type_lookup_reply_writer.clone()
    }

    pub(crate) fn type_lookup_reply_reader(&self) -> Arc<StatefulReader> {
        self.builtin_endpoints.type_lookup_reply_reader.clone()
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
                let _ = wlp_logic.register_asserting_writer(writer.guid(), liveliness);
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
        self.rtps_reader_store.get_reader(entity_id)
    }

    // Callback-producing lookup: the returned lease keeps the reader's in-flight count raised
    // until dropped, so deletion drains it instead of racing the delivery.
    pub(crate) fn find_reader_callback_lease_from_entity_id(
        &self,
        entity_id: EntityId,
    ) -> Option<ReaderCallbackLease> {
        self.rtps_reader_store.get_reader_callback_lease(entity_id)
    }

    // Callback-producing variant of `find_readers_matched_with_remote_writer`. Matching reuses the
    // plain lookup, then each match is re-fetched through the shard lock to raise its in-flight
    // count. A reader removed in between is dropped from the result.
    pub(crate) fn find_reader_callback_leases_matched_with_remote_writer(
        &self,
        writer_guid: Guid,
    ) -> RtpsResult<Vec<ReaderCallbackLease>> {
        let matched = self.find_readers_matched_with_remote_writer(writer_guid)?;
        let mut leases = Vec::with_capacity(matched.len());

        for reader in matched {
            if let Some(lease) =
                self.rtps_reader_store.get_reader_callback_lease(reader.guid().entity_id())
            {
                leases.push(lease);
            }
        }

        Ok(leases)
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
                    let _ = wlp_logic.deregister_asserting_writer(writer.guid());
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
                // Bare dispose: instance is identified by PID_KEY_HASH inline QoS only
                let sedp_writer = self.sedp_builtin_publications_writer();
                let a_cache_change = Arc::new(sedp_writer.new_change(
                    ChangeKind::NotAliveDisposedUnregistered,
                    Vec::new(),
                    InstanceHandle::from_guid(&writer_guid),
                    Some(RtpsTime::now()),
                ));

                // The dispose takes the alive announcement's place in the history, and it does so
                // before it goes on the wire.
                //
                // Keeping it is what makes it repairable. This builtin writer is RELIABLE, and a
                // peer can only ask for a sequence number that a heartbeat still covers -- so a
                // dispose that exists nowhere but in one already-sent datagram cannot be
                // recovered, and a peer that missed it keeps the endpoint matched for good.
                //
                // Dropping it also stranded the sequence number it consumed. `new_change` moves
                // the counter whether or not anything is stored, so deleting every endpoint used
                // to empty the cache while the counter kept climbing, leaving the writer
                // advertising `firstSN = lastSN + 1` over a range it could not serve.
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
                        let _ = writer_cache
                            .add_change_builtin(a_cache_change.clone(), sedp_writer.as_ref());
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to lock sedp_builtin_publications_writer's cache: {:?}",
                            e
                        );
                    }
                }

                self.sync_send_sedp_terminate_endpoint(
                    self.sedp_builtin_publications_writer().guid(),
                    a_cache_change,
                )?;

                log::info!("Remote writer with GUID {} terminated", writer_guid);
            }
        }

        // Remove from the store first, before unmatching intra-participant readers below.
        // While the readers are being unmatched, a just-sent in-flight sample from this writer
        // then observes the writer as already gone (find_writer_from_entity_id == None), which
        // lets the delivery path deliver the already-accepted sample instead of dropping it.
        // No duplicate can result: the writer is gone, so no reliable retransmit can occur.
        self.rtps_writer_store.remove(&topic_name, entity_id);

        // Unmatch with intra participant readers
        self.cleanup_resources_for_remote_writer(
            Guid::new(self.guid().prefix(), entity_id),
            &topic_name,
        )?;

        Ok(())
    }

    /// Method to remove RTPS Reader
    pub(crate) fn remove_reader(&self, topic_name: String, entity_id: EntityId) -> RtpsResult<()> {
        // Send Data[r(UD)]
        let reader_arc = self.rtps_reader_store.get_reader(entity_id);

        if let Some(reader) = reader_arc.as_ref() {
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
                // Bare dispose: instance is identified by PID_KEY_HASH inline QoS only
                let sedp_writer = self.sedp_builtin_subscriptions_writer();
                let a_cache_change = Arc::new(sedp_writer.new_change(
                    ChangeKind::NotAliveDisposedUnregistered,
                    Vec::new(),
                    InstanceHandle::from_guid(&reader_guid),
                    Some(RtpsTime::now()),
                ));

                // Same reasoning as the publications side in `remove_writer`: the dispose has to
                // stay in the history to be repairable, and to keep the advertised range in step
                // with the sequence number it consumed.
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
                        let _ = writer_cache
                            .add_change_builtin(a_cache_change.clone(), sedp_writer.as_ref());
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to lock sedp_builtin_subscriptions_writer's cache: {:?}",
                            e
                        );
                    }
                }

                self.sync_send_sedp_terminate_endpoint(
                    self.sedp_builtin_subscriptions_writer().guid(),
                    a_cache_change,
                )?;
            }
        }

        // Remove from store
        self.rtps_reader_store.remove(&topic_name, entity_id);

        // Drain in-flight delivery/listener callbacks so none runs against this reader after the
        // delete call returns. Store removal above stops new callbacks from starting; this waits
        // out the ones already past the shard lock. A reentrant delete from inside a callback is
        // refused earlier, but guard the wait too: blocking on our own count would deadlock.
        if let Some(reader) = reader_arc.as_ref() {
            if !crate::utils::notify::in_listener_callback() {
                let drain_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while reader.in_flight_callbacks() > 0 {
                    if std::time::Instant::now() >= drain_deadline {
                        log::warn!(
                            "remove_reader: {} callback(s) still in flight for {} after drain timeout",
                            reader.in_flight_callbacks(),
                            entity_id
                        );
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_micros(50));
                }
            }
        }

        // The reader is gone, so any reassembly still addressed to it can never complete and
        // will not be reached by the normal completion path -- only by cap-driven eviction,
        // which may not run again for a long time.
        if let Some(user_logic_arc) = self.user_logic_if_set() {
            if let Some(user_logic) = user_logic_arc.as_ref() {
                user_logic.forget_fragment_buffers_for_reader(entity_id);
            }
        }

        // Unmatch with intra participant writers
        self.cleanup_resources_for_remote_reader(
            Guid::new(self.guid().prefix(), entity_id),
            &topic_name,
        )?;

        Ok(())
    }

    pub(crate) fn cleanup_resources_for_remote_reader(
        &self,
        reader_guid: Guid,
        topic_name: &str,
    ) -> RtpsResult<()> {
        // Drop reader-side proxies and fire PUBLICATION_MATCHED(-1).
        self.remove_unmatched_reader_from_writer(reader_guid)?;

        // Forget the discovery entry; drop the topic bucket if it became empty.
        if let Some(mut entry) = self.remote_subscriptions().get_mut(topic_name) {
            entry.value_mut().remove(&reader_guid);

            if entry.value().is_empty() {
                drop(entry);
                self.remote_subscriptions().remove(topic_name);
            }
        }

        Ok(())
    }

    pub(crate) fn cleanup_remote_reader_by_guid(&self, reader_guid: Guid) -> RtpsResult<()> {
        self.remove_unmatched_reader_from_writer(reader_guid)?;
        self.remove_remote_subscription_by_guid(reader_guid);

        Ok(())
    }

    pub(crate) fn cleanup_resources_for_remote_writer(
        &self,
        writer_guid: Guid,
        topic_name: &str,
    ) -> RtpsResult<()> {
        // The writer is gone, so no further DATA_FRAG can ever complete or evict a reassembly
        // still waiting on it. Also reached per-writer from `unmatch_with_remote_participant`
        // via `remove_all_unmatched_endpoint_from_terminated_participant`, so that path is
        // covered too without a separate hook there.
        if let Some(user_logic_arc) = self.user_logic_if_set() {
            if let Some(user_logic) = user_logic_arc.as_ref() {
                user_logic.forget_fragment_buffers_for_writer(writer_guid);
            }
        }

        // Fire LIVELINESS_CHANGED first; it iterates reader's writer_proxies to find matches.
        if writer_guid.entity_id().entity_kind().is_user_defined() {
            if let Some(wlp_logic) = self.wlp_logic() {
                let _ = wlp_logic.deregister_monitored_writer(writer_guid);
            }
        }

        // Drop reader-side proxies and fire SUBSCRIPTION_MATCHED(-1).
        self.remove_unmatched_writer_from_reader(writer_guid)?;

        // Forget the discovery entry; drop the topic bucket if it became empty.
        if let Some(mut entry) = self.remote_publications().get_mut(topic_name) {
            entry.value_mut().remove(&writer_guid);

            if entry.value().is_empty() {
                drop(entry);
                self.remote_publications().remove(topic_name);
            }
        }

        Ok(())
    }

    pub(crate) fn cleanup_remote_writer_by_guid(&self, writer_guid: Guid) -> RtpsResult<()> {
        if writer_guid.entity_id().entity_kind().is_user_defined() {
            if let Some(wlp_logic) = self.wlp_logic() {
                let _ = wlp_logic.deregister_monitored_writer(writer_guid);
            }
        }

        self.remove_unmatched_writer_from_reader(writer_guid)?;
        self.remove_remote_publication_by_guid(writer_guid);

        Ok(())
    }

    fn remove_remote_publication_by_guid(&self, writer_guid: Guid) {
        self.remote_publications().retain(|_, endpoints| {
            endpoints.remove(&writer_guid);
            !endpoints.is_empty()
        });
    }

    fn remove_remote_subscription_by_guid(&self, reader_guid: Guid) {
        self.remote_subscriptions().retain(|_, endpoints| {
            endpoints.remove(&reader_guid);
            !endpoints.is_empty()
        });
    }

    /// Iterate through all Readers in the Participant to find Readers matched with the Writer, then remove Writer Proxy
    fn remove_unmatched_writer_from_reader(&self, writer_guid: Guid) -> RtpsResult<()> {
        for reader in self.rtps_reader_store.iter_all() {
            if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                stateful_reader.remove_matched_writer_and_update_status(writer_guid)?;
            } else if let Some(stateless_reader) = reader.as_any().downcast_ref::<StatelessReader>()
            {
                stateless_reader.remove_matched_writer_and_update_status(writer_guid)?;
            }
        }

        Ok(())
    }

    /// Iterate through all Writers in the Participant to find Writers matched with the Reader, then remove Reader Locator or Reader Proxy
    fn remove_unmatched_reader_from_writer(&self, reader_guid: Guid) -> RtpsResult<()> {
        for writer in self.rtps_writer_store.iter_all() {
            if let Some(stateful_writer) = writer.as_any().downcast_ref::<StatefulWriter>() {
                stateful_writer.remove_matched_reader_and_update_status(reader_guid)?;
            } else if let Some(stateless_writer) = writer.as_any().downcast_ref::<StatelessWriter>()
            {
                stateless_writer.remove_matched_reader_and_update_status(reader_guid)?;
            }
        }

        Ok(())
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

            self.sync_send_sedp_terminate_endpoint(
                self.sedp_builtin_subscriptions_writer().guid(),
                Arc::new(a_cache_change),
            )?;
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

            self.sync_send_sedp_terminate_endpoint(
                self.sedp_builtin_publications_writer().guid(),
                Arc::new(a_cache_change),
            )?;
        }

        // Send Data(p[UD]) messages
        self.sync_send_spdp_terminate_participant()?;

        Ok(())
    }

    // Send a SEDP dispose synchronously; the event-loop path can race participant teardown
    pub fn sync_send_sedp_terminate_endpoint(
        &self,
        builtin_writer_guid: Guid,
        cache_change: Arc<CacheChange>,
    ) -> RtpsResult<()> {
        if let Some(sedp_logic) = self.sedp_logic.get().and_then(|a| a.as_ref().as_ref()) {
            sedp_logic.send_endpoint_termination_message(builtin_writer_guid, cache_change)?;
        }
        Ok(())
    }

    // Send SPDP/SEDP participant dispose synchronously on shutdown
    pub fn sync_send_spdp_terminate_participant(&self) -> RtpsResult<()> {
        let spdp_logic = self
            .spdp_logic
            .get()
            .and_then(|a| a.as_ref().as_ref())
            .ok_or(RtpsError::new(RtpsErrorCode::NotInitialized, "SpdpLogic is not initialized"))?;
        spdp_logic.send_participant_termination_message_multicast()?;
        let sedp_logic = self
            .sedp_logic
            .get()
            .and_then(|a| a.as_ref().as_ref())
            .ok_or(RtpsError::new(RtpsErrorCode::NotInitialized, "SedpLogic is not initialized"))?;
        sedp_logic.send_participant_termination_message_unicast()?;
        Ok(())
    }

    /// Method to remove all information about remote participant
    pub(crate) fn unmatch_with_remote_participant(
        &self,
        terminated_participant_guid: &Guid,
    ) -> RtpsResult<()> {
        // Capture the peer's locators before removing its proxy, so the transport
        // connections to it can be closed after the DDS-level cleanup below.
        let peer_locators = self
            .find_remote_participant_proxy_data(terminated_participant_guid.prefix())
            .map(|data| {
                let mut locs = data.metatraffic_unicast_locator_list().clone();
                locs.extend(data.default_unicast_locator_list().iter().cloned());
                locs
            });

        // Remove participant proxy
        if !self.remove_remote_participant_proxy_data(*terminated_participant_guid) {
            debug!("Remote participant not found, participant may have already been unmatched");
            return Ok(());
        }

        // Remove proxies from built-in endpoint
        self.builtin_endpoints().remove_unmatched_endpoint(terminated_participant_guid.prefix());

        // Remove proxies from endpoint
        self.remove_all_unmatched_endpoint_from_terminated_participant(
            terminated_participant_guid.prefix(),
        )?;

        // Remove SEDP periodic timers tied to this remote
        let remote_prefix = terminated_participant_guid.prefix();
        if let Ok(handler) = TimerHandler::get_instance(self.guid().prefix()).lock() {
            for writer_entity_id in [
                EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER,
                EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
                EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
                EntityId::SEDP_BUILTIN_TOPICS_WRITER,
            ] {
                handler.remove_timer(TimerId::SedpScheduledMessage {
                    remote_prefix,
                    writer_entity_id,
                });
            }
        }

        self.remote_same_host.remove(&remote_prefix);

        // The peer is gone, so it will never answer to release what is charged against it, and
        // the entry would sit there until the backstop -- or for good, since a peer that
        // reconnects comes back under a new prefix.
        if let Some(user_logic_arc) = self.user_logic_if_set() {
            if let Some(user_logic) = user_logic_arc.as_ref() {
                user_logic.forget_send_credit_for_participant(remote_prefix);
            }
        }

        // Close transport connections to the now-unmatched peer so its per-peer
        // resources are released promptly, rather than lingering until OS keepalive.
        if let (Some(transport), Some(locators)) = (self.transport.get(), peer_locators) {
            transport.disconnect_peer(&locators);
        }

        info!("Successfully unmatched with remote participant: {}", terminated_participant_guid);
        Ok(())
    }

    /// Function to remove all Remote Endpoints with the given GuidPrefix when Remote Participant terminates
    pub(crate) fn remove_all_unmatched_endpoint_from_terminated_participant(
        &self,
        terminated_participant_guid_prefix: GuidPrefix,
    ) -> RtpsResult<()> {
        // Get all writers with the same GuidPrefix
        let writers: Vec<(String, Guid)> = self
            .remote_publications()
            .iter()
            .flat_map(|e| {
                let topic = e.key().clone();
                e.value()
                    .iter()
                    .filter(|(guid, _)| guid.prefix() == terminated_participant_guid_prefix)
                    .map(|(guid, _)| (topic.clone(), *guid))
                    .collect::<Vec<_>>()
            })
            .collect();

        // Clean up resources related to each writer
        for (topic, writer_guid) in writers {
            self.cleanup_resources_for_remote_writer(writer_guid, &topic)?;
        }

        // Get all readers with the same GuidPrefix
        let readers: Vec<(String, Guid)> = self
            .remote_subscriptions()
            .iter()
            .flat_map(|e| {
                let topic = e.key().clone();
                e.value()
                    .iter()
                    .filter(|(guid, _)| guid.prefix() == terminated_participant_guid_prefix)
                    .map(|(guid, _)| (topic.clone(), *guid))
                    .collect::<Vec<_>>()
            })
            .collect();

        // Clean up resources related to each reader
        for (topic, reader_guid) in readers {
            self.cleanup_resources_for_remote_reader(reader_guid, &topic)?;
        }

        Ok(())
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
    /// This creates SPDP, SEDP, User, and WLP logic instances using the provided transport.
    /// Point every builtin writer history at this participant: they are built before the owning
    /// `Arc` exists, so until this runs anything the cache reaches through it silently does nothing.
    fn wire_builtin_writer_histories(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        let builtin = &self.builtin_endpoints;
        for writer in [
            &builtin.sedp_builtin_publications_writer,
            &builtin.sedp_builtin_subscriptions_writer,
            &builtin.sedp_builtin_topics_writer,
            &builtin.builtin_participant_message_writer,
            &builtin.type_lookup_request_writer,
            &builtin.type_lookup_reply_writer,
        ] {
            match writer.writer_cache().lock() {
                Ok(mut cache) => cache.set_participant(weak.clone()),
                Err(e) => log::error!("Failed to wire builtin writer history: {}", e),
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn wire_builtin_writer_histories_for_test(self: &Arc<Self>) {
        self.wire_builtin_writer_histories();
    }

    pub(crate) fn init_logics(
        self: &Arc<Self>,
        transport: Arc<dyn TransportPlugin>,
        property: &crate::infrastructure::qos_policy::PropertyQosPolicy,
    ) {
        // Prefer initial_peers from PropertyQosPolicy; fall back to env var.
        let initial_peers = property
            .find_property("int2dds.initial_peers")
            .map(crate::common::env::parse_initial_peers)
            .unwrap_or_else(crate::common::env::get_initial_peers);
        if !initial_peers.is_empty() {
            log::info!("Configured initial peers for SPDP: {:?}", initial_peers);
        }

        // Keep a handle to the transport so unmatch can close per-peer connections.
        let _ = self.transport.set(transport.clone());

        self.wire_builtin_writer_histories();

        // Create SPDP logic
        let spdp_logic =
            Arc::new(Some(SpdpLogic::new(self.clone(), transport.clone(), initial_peers)));
        let _ = self.spdp_logic.set(spdp_logic);

        // Create SEDP logic
        let sedp_logic = Arc::new(Some(SedpLogic::new(self.clone(), transport.clone())));
        let _ = self.sedp_logic.set(sedp_logic);

        // Create User logic
        let user_logic = Arc::new(Some(UserLogic::new(self.clone(), transport.clone())));
        let _ = self.user_logic.set(user_logic);

        // Create WLP logic
        let wlp_logic = WlpLogic::new(self.clone(), transport);
        let _ = self.wlp_logic.set(wlp_logic);
    }

    pub(crate) fn set_wlp_logic(&self, wlp_logic: WlpLogic) {
        let _ = self.wlp_logic.set(wlp_logic);
    }

    pub(crate) fn wlp_logic(&self) -> Option<WlpLogic> {
        self.wlp_logic.get().cloned()
    }

    /// `get_logics` panics before the logics are installed, and builtin writers can take a change
    /// that early.
    pub(crate) fn user_logic_if_set(&self) -> Option<Arc<Option<UserLogic>>> {
        self.user_logic.get().cloned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::qos_policy::ReliabilityQosPolicyKind;
    use crate::rtps::common::{entity_kind::EntityKind, types::TopicKind};

    fn participant_with_one_writer() -> (Arc<Participant>, EntityId) {
        let participant = Arc::new(Participant::new(0, 0, Vec::new(), Vec::new(), Vec::new()));
        let entity_id = EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY);

        let writer = Arc::new(StatefulWriter::new(
            Guid::new(participant.guid().prefix(), entity_id),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::WithKey,
            entity_id,
            1024,
            None,
            PublicationBuiltinTopicData::default(),
            Arc::downgrade(&participant),
        ));

        participant.add_writer("a_topic", writer).expect("the writer must register");

        (participant, entity_id)
    }

    fn participant_with_one_reader() -> (Arc<Participant>, EntityId) {
        let participant = Arc::new(Participant::new(0, 0, Vec::new(), Vec::new(), Vec::new()));
        let entity_id = EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_READER_WITH_KEY);

        let reader = Arc::new(StatefulReader::new(
            Guid::new(participant.guid().prefix(), entity_id),
            TopicKind::WithKey,
            ReliabilityQosPolicyKind::Reliable,
            Vec::new(),
            Vec::new(),
            entity_id,
            false,
            None,
            None,
            SubscriptionBuiltinTopicData::default(),
            participant.guid(),
        ));

        participant.add_reader("a_topic", reader);

        (participant, entity_id)
    }

    /// The subscriptions writer has the same defect as the publications one.
    #[test]
    fn deleting_a_reader_leaves_its_dispose_in_the_sedp_history() {
        let (participant, entity_id) = participant_with_one_reader();
        let sedp_writer = participant.sedp_builtin_subscriptions_writer();

        participant.remove_reader("a_topic".to_string(), entity_id).expect("removal must succeed");

        let dispose_sn = sedp_writer.last_change_sequence_number();
        let cache_arc = sedp_writer.writer_cache();
        let cache = cache_arc.lock().expect("cache lock");

        assert!(
            cache.get_change(dispose_sn).is_some(),
            "the dispose took sequence number {dispose_sn} but left no change behind, so nothing \
             can retransmit it"
        );
        assert_eq!(cache.get_seq_num_max(), Some(dispose_sn));
    }

    /// The SEDP dispose is what tells peers an endpoint is gone, and the builtin writer that
    /// carries it is RELIABLE. Sending it once and dropping it on the floor leaves no copy to
    /// retransmit: the heartbeat range never covers that sequence number, so a peer that missed
    /// the single datagram cannot even ask for it and keeps the endpoint matched forever.
    ///
    /// It also strands the sequence number the dispose consumed. Deleting every endpoint then
    /// leaves the writer advertising `firstSN = lastSN + 1` -- an empty range over a counter that
    /// kept climbing.
    #[test]
    fn deleting_a_writer_leaves_its_dispose_in_the_sedp_history() {
        let (participant, entity_id) = participant_with_one_writer();
        let sedp_writer = participant.sedp_builtin_publications_writer();

        participant.remove_writer("a_topic".to_string(), entity_id).expect("removal must succeed");

        let dispose_sn = sedp_writer.last_change_sequence_number();
        let cache_arc = sedp_writer.writer_cache();
        let cache = cache_arc.lock().expect("cache lock");

        assert!(
            cache.get_change(dispose_sn).is_some(),
            "the dispose took sequence number {dispose_sn} but left no change behind, so nothing \
             can retransmit it"
        );
        assert_eq!(
            cache.get_seq_num_max(),
            Some(dispose_sn),
            "the advertised range has to reach the dispose"
        );
    }
}
