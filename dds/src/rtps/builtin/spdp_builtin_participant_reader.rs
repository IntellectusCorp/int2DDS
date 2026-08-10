//! SPDP builtin participant reader implementation.
//!
//! This module implements the SPDP (Simple Participant Discovery Protocol) reader
//! that receives and processes participant announcements from remote participants
//! in the DDS domain.

#![allow(dead_code)]

use crate::{
    common::builtin::topic::{
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    },
    infrastructure::{
        history_cache::HistoryCache as dcps_history_cache,
        qos_policy::ReliabilityQosPolicyKind,
        status::{StatusInfo, StatusKind},
    },
    rtps::{
        common::{
            entity_id::EntityId,
            guid::Guid,
            locator::Locator,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            time::RtpsDuration,
            types::TopicKind,
        },
        entities::{
            endpoint::Endpoint,
            entity::Entity,
            history::{cache_change::CacheChange, reader_history::ReaderHistoryCache},
            reader::Reader,
        },
        messages::spdp_message::SpdpMessage,
    },
};
use std::{
    any::Any,
    sync::{Arc, Mutex, Weak},
};

#[derive(Debug)]
pub(crate) struct SPDPBuiltinParticipantReader {
    guid: Guid,
    topic_kind: TopicKind,
    reliability_level: ReliabilityQosPolicyKind,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    endpoint_id: EntityId,
    expects_inline_qos: bool,
    heartbeat_response_delay: RtpsDuration,
    heartbeat_suppression_duration: RtpsDuration,
    reader_cache: Arc<Mutex<ReaderHistoryCache>>,
}

impl SPDPBuiltinParticipantReader {
    pub(crate) fn new(
        guid: Guid,
        topic_kind: TopicKind,
        reliability_level: ReliabilityQosPolicyKind,
        unicast_locator_list: Vec<Locator>,
        multicast_locator_list: Vec<Locator>,
        endpoint_id: EntityId,
        expects_inline_qos: bool,
    ) -> Self {
        Self {
            guid,
            topic_kind,
            reliability_level,
            unicast_locator_list,
            multicast_locator_list,
            endpoint_id,
            expects_inline_qos,
            heartbeat_response_delay: RtpsDuration::new(0, 500 * 1000 * 1000),
            heartbeat_suppression_duration: RtpsDuration::new(0, 0),
            reader_cache: Arc::new(Mutex::new(ReaderHistoryCache::new(endpoint_id, None))),
        }
    }

    pub(crate) fn process_message(&mut self, _message: &SpdpMessage) {
        // let spdp_discovered_participant_data = SPDPDiscoveredParticipantData::new(message);
        // self.add_change(spdp_discovered_participant_data);
    }

    fn add_change(&mut self, a_change: CacheChange) {
        let _ = self.reader_cache.lock().unwrap().add_change(a_change, false);
    }
}

impl Entity for SPDPBuiltinParticipantReader {
    fn guid(&self) -> Guid {
        self.guid
    }

    fn set_update_status(
        &self,
        _f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    ) {
        // Builtin entities are internal to DDS and do not expose status callbacks to user applications
    }

    fn update_status(&self, _status: StatusKind, _info: Option<Arc<dyn StatusInfo>>) {
        // Builtin entities are internal to DDS and do not expose status callbacks to user applications
    }
}

impl Endpoint for SPDPBuiltinParticipantReader {
    fn endpoint_id(&self) -> EntityId {
        self.endpoint_id
    }
    fn multicast_locator_list(&self) -> Vec<Locator> {
        self.multicast_locator_list.clone()
    }
    fn reliability_level(&self) -> ReliabilityQosPolicyKind {
        self.reliability_level
    }
    fn topic_kind(&self) -> TopicKind {
        self.topic_kind
    }
    fn unicast_locator_list(&self) -> Vec<Locator> {
        self.unicast_locator_list.clone()
    }
}

impl Reader for SPDPBuiltinParticipantReader {
    fn reader_cache(&self) -> Arc<Mutex<ReaderHistoryCache>> {
        Arc::clone(&self.reader_cache)
    }
    // For operations like read_instance, take_instance to find specific instances: exact: true
    fn available_changes(
        &self, /* , handle: InstanceHandle, exact: bool */
    ) -> Vec<Arc<CacheChange>> {
        Vec::new()
    }
    fn expects_inline_qos(&self) -> bool {
        self.expects_inline_qos
    }
    fn heartbeat_response_delay(&self) -> RtpsDuration {
        self.heartbeat_response_delay
    }
    fn heartbeat_suppression_duration(&self) -> RtpsDuration {
        self.heartbeat_suppression_duration
    }
    fn matched_writer_is_matched(&self, _writer_guid: Guid) -> bool {
        false
    }
    fn matched_writers_guids(&self) -> Vec<Guid> {
        Vec::new()
    }
    fn on_change(&self, _change: Arc<CacheChange>) {
        // SPDP builtin reader handles changes through internal discovery logic, not user callbacks
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn set_datareader_cache(
        &mut self,
        datareader_cache: Weak<Mutex<dyn dcps_history_cache + Send + Sync>>,
    ) -> RtpsResult<()> {
        let cache_guard = self.reader_cache.lock();
        match cache_guard {
            Ok(mut guard) => {
                guard.set_datareader_cache(datareader_cache);
                Ok(())
            }
            Err(e) => Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
        }
    }
    fn set_subscription_builtin_topic_data(
        &self,
        _subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> RtpsResult<()> {
        Ok(())
    }

    fn get_subscription_builtin_topic_data(&self) -> RtpsResult<SubscriptionBuiltinTopicData> {
        Err(RtpsError::new(RtpsErrorCode::Unknown, "UnSupported"))
    }

    fn get_matched_publication_data(
        &self,
        _writer_guid: Guid,
    ) -> RtpsResult<PublicationBuiltinTopicData> {
        Err(RtpsError::new(RtpsErrorCode::Unknown, "UnSupported"))
    }

    fn remove_matched_writer_and_update_status(&self, _writer_guid: Guid) -> RtpsResult<bool> {
        Ok(false)
    }
}
