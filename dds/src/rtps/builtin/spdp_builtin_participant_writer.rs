//! SPDP builtin participant writer implementation.
//!
//! This module implements the SPDP (Simple Participant Discovery Protocol) writer
//! that announces the local participant's presence to other participants in the
//! DDS domain through periodic participant data messages.

#![allow(dead_code)]
use std::sync::{Arc, Mutex};

use log::debug;

use crate::common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData;
use crate::common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData;
use crate::infrastructure::qos_policy::LivelinessQosPolicy;
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::entities::history::writer_history::WriterHistoryCache;
use crate::{
    common::instance_handle::InstanceHandle,
    core::time::Duration as DcpsDuration,
    infrastructure::{
        qos_policy::ReliabilityQosPolicyKind,
        status::{StatusInfo, StatusKind},
    },
    rtps::{
        common::{
            entity_id::EntityId,
            guid::{Guid, GuidPrefix},
            locator::Locator,
            sequence::SequenceNumber,
            time::{RtpsDuration, RtpsTime},
            types::{ChangeKind, TopicKind},
        },
        entities::{
            endpoint::Endpoint,
            entity::Entity,
            history::{cache_change::CacheChange, history_cache::HistoryCache},
            writer::{has_reader_locator::HasReaderLocator, reader_locator::ReaderLocator, Writer},
        },
    },
};

#[derive(Debug)]
pub(crate) struct SPDPbuiltinParticipantWriter {
    guid: Guid,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    reliability_level: ReliabilityQosPolicyKind,
    topic_kind: TopicKind,
    endpoint_id: EntityId,
    push_mode: bool,
    nack_suppression_duration: RtpsDuration,
    nack_response_delay: RtpsDuration,
    last_change_sequence_number: Arc<Mutex<SequenceNumber>>,
    heartbeat_period: RtpsDuration,
    data_max_size_serialized: i32,
    reader_locators: Arc<Mutex<Vec<ReaderLocator>>>,
    writer_cache: Arc<Mutex<WriterHistoryCache>>,
}

impl SPDPbuiltinParticipantWriter {
    pub(crate) fn new(
        guid: Guid,
        reliability_level: ReliabilityQosPolicyKind,
        topic_kind: TopicKind,
        endpoint_id: EntityId,
        push_mode: bool,
        heartbeat_period: RtpsDuration,
        data_max_size_serialized: i32,
    ) -> Self {
        // in:attribute_values
        // 8.4.7.1.1 & 8.4.7.1.2 & 8.4.7.2.1

        let nack_response_delay = RtpsDuration::new(0, 200 * 1000 * 1000); // 200 milliseconds
        let nack_suppression_duration = RtpsDuration::new(0, 0);

        Self {
            guid,
            unicast_locator_list: Vec::new(),
            multicast_locator_list: Vec::new(),
            reliability_level,
            topic_kind,
            endpoint_id,
            push_mode,
            nack_response_delay,
            nack_suppression_duration,
            last_change_sequence_number: Arc::new(Mutex::new(SequenceNumber::new(0, 0))), // Assuming SequenceNumber has a new method
            heartbeat_period,
            data_max_size_serialized,
            reader_locators: Arc::new(Mutex::new(Vec::new())),
            writer_cache: Arc::new(Mutex::new(WriterHistoryCache::new(
                std::sync::Weak::new(),
                endpoint_id,
            ))),
        }
    }

    pub(crate) fn unsent_changes_reset(&self) {
        // void
        match self.reader_locators.lock() {
            Ok(mut reader_locators) => {
                for reader_locator in reader_locators.iter_mut() {
                    reader_locator.set_highest_sent_change_sn(SequenceNumber::new(0, 0));
                }
            }
            Err(e) => {
                debug!("Failed to acquire reader_locators lock: {}", e);
            }
        }
    }

    pub(crate) fn remove_cache_change(&self, cache_change: Arc<CacheChange>) {
        match self.writer_cache.lock() {
            Ok(mut history_cache) => {
                let _ = history_cache.remove_change(cache_change);
            }
            Err(e) => {
                debug!("Failed to acquire writer cache lock: {}", e);
            }
        }
    }

    pub(crate) fn reader_locator_remove_by_guid_prefix(&self, guid_prefix: GuidPrefix) {
        match self.reader_locators.lock() {
            Ok(mut reader_locators) => {
                debug!(
                    "SPDP Builtin Participant Writer had {} reader locators before remove {:?}",
                    reader_locators.len(),
                    guid_prefix
                );
                reader_locators.retain(|x| x.guid_prefix() != guid_prefix);
                debug!(
                    "SPDP Builtin Participant Writer now has {} reader locators",
                    reader_locators.len()
                );
            }
            Err(e) => {
                debug!("Failed to acquire reader_locators lock: {}", e);
            }
        }
    }
}

impl HasReaderLocator for SPDPbuiltinParticipantWriter {
    fn reader_locator(&self) -> Vec<ReaderLocator> {
        match self.reader_locators.lock() {
            Ok(reader_locators) => reader_locators.clone(),
            Err(e) => {
                debug!("Failed to acquire reader_locators lock: {}", e);
                Vec::new()
            }
        }
    }
    fn reader_locator_add(&self, a_locator: ReaderLocator) {
        match self.reader_locators.lock() {
            Ok(mut reader_locators) => {
                reader_locators.push(a_locator.clone());
            }
            Err(e) => {
                debug!("Failed to acquire reader_locators lock: {}", e);
            }
        }
    }

    fn reader_locator_remove(&self, a_locator: Locator) {
        // void
        match self.reader_locators.lock() {
            Ok(mut reader_locators) => {
                reader_locators.retain(|x| x.locator() != a_locator);
            }
            Err(e) => {
                debug!("Failed to acquire reader_locators lock: {}", e);
            }
        }
    }
}

impl Writer for SPDPbuiltinParticipantWriter {
    fn writer_cache(&self) -> Arc<Mutex<WriterHistoryCache>> {
        Arc::clone(&self.writer_cache)
    }
    fn new_change(
        &self,
        kind: ChangeKind,
        data: Vec<u8>,
        // Not present in RTPS
        // inline_qos: ParameterList,
        handle: InstanceHandle,
        source_timestamp: Option<RtpsTime>,
    ) -> CacheChange {
        // TODO: Add sequence number increment logic, varies depending on handle
        let last_change_sequence_number = match self.last_change_sequence_number.lock() {
            Ok(mut last_change_sequence_number) => {
                *last_change_sequence_number += 1;
                *last_change_sequence_number
            }
            Err(e) => {
                debug!("Failed to acquire last_change_sequence_number lock: {}", e);
                SequenceNumber::UNKNOWN
            }
        };

        CacheChange::new(
            kind,
            self.guid,
            handle,
            last_change_sequence_number,
            data,
            // inline_qos,
            source_timestamp,
        )
    }
    fn new_change_with_rpc_callback(
        &self,
        _kind: ChangeKind,
        _handle: InstanceHandle,
        _source_timestamp: Option<RtpsTime>,
        _data_fn: Box<dyn FnOnce(Guid, SequenceNumber) -> Vec<u8> + '_>,
    ) -> CacheChange {
        unimplemented!("SPDP writer does not support RPC callback")
    }

    fn data_max_size_serialized(&self) -> i32 {
        self.data_max_size_serialized
    }
    fn heartbeat_period(&self) -> RtpsDuration {
        self.heartbeat_period
    }
    fn last_change_sequence_number(&self) -> SequenceNumber {
        match self.last_change_sequence_number.lock() {
            Ok(last_change_sequence_number) => *last_change_sequence_number,
            Err(e) => {
                debug!("Failed to acquire last_change_sequence_number lock: {}", e);
                SequenceNumber::UNKNOWN
            }
        }
    }
    fn nack_response_delay(&self) -> RtpsDuration {
        self.nack_response_delay
    }
    fn nack_suppression_duration(&self) -> RtpsDuration {
        self.nack_suppression_duration
    }
    fn allocate_sequence_number(&self) -> SequenceNumber {
        match self.last_change_sequence_number.lock() {
            Ok(mut seq) => {
                *seq += 1;
                *seq
            }
            Err(e) => {
                log::error!("Failed to acquire last_change_sequence_number lock: {}", e);
                SequenceNumber::UNKNOWN
            }
        }
    }

    fn push_mode(&self) -> bool {
        self.push_mode
    }
    fn wait_for_all_acked(&self, _max_wait: DcpsDuration) -> bool {
        true
    }
    fn matched_reader_is_matched(&self, _reader_guid: Guid) -> bool {
        false
    }
    fn matched_readers_guids(&self) -> Vec<Guid> {
        Vec::new()
    }
    fn assert_liveliness(&self) -> bool {
        true
    }
    fn liveliness(&self) -> RtpsResult<LivelinessQosPolicy> {
        Ok(LivelinessQosPolicy::default())
    }

    //Any
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn get_publication_builtin_topic_data(
        &self,
    ) -> RtpsResult<Option<PublicationBuiltinTopicData>> {
        Ok(None)
    }
    fn set_publication_builtin_topic_data(
        &self,
        _publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> RtpsResult<()> {
        Ok(())
    }
    fn get_matched_subscription_data(
        &self,
        _reader_guid: Guid,
    ) -> RtpsResult<SubscriptionBuiltinTopicData> {
        Err(RtpsError::new(RtpsErrorCode::Unknown, "UnSupported"))
    }

    fn remove_matched_reader_and_update_status(&self, _reader_guid: Guid) -> RtpsResult<bool> {
        Ok(false)
    }

    fn remove_all_matched_readers_with_prefix_and_update_status(
        &self,
        _prefix: GuidPrefix,
    ) -> RtpsResult<usize> {
        Ok(0)
    }
}

impl Endpoint for SPDPbuiltinParticipantWriter {
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

impl Entity for SPDPbuiltinParticipantWriter {
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
