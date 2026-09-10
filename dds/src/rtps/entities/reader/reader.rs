//! Reader trait and implementation for RTPS DataReaders.
//!
//! This module defines the `Reader` trait representing RTPS DataReaders that receive
//! and process data samples from DataWriters. Readers manage history caches, writer
//! proxies, and implement reliable or best-effort communication protocols.

#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    any::Any,
    fmt::Debug,
    sync::{Arc, Mutex, Weak},
};

use crate::{
    common::builtin::topic::{
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    },
    infrastructure::history_cache::HistoryCache,
    rtps::{
        common::{guid::Guid, rtps_error_code::RtpsResult, time::RtpsDuration},
        entities::{
            endpoint::Endpoint,
            entity::Entity,
            history::{cache_change::CacheChange, reader_history::ReaderHistoryCache},
        },
    },
};

pub(crate) trait Reader: Entity + Endpoint + Debug + Any {
    fn reader_cache(&self) -> Arc<Mutex<ReaderHistoryCache>>;
    // For operations like read_instance and take_instance that search for specific instances, exact: true
    fn available_changes(&self) -> Vec<Arc<CacheChange>>;
    fn expects_inline_qos(&self) -> bool;
    fn heartbeat_response_delay(&self) -> RtpsDuration;
    fn heartbeat_suppression_duration(&self) -> RtpsDuration;
    /// Delay before the first NACK_FRAG for a sample's missing fragments.
    fn nack_frag_response_delay(&self) -> RtpsDuration;
    /// Delay before retrying a NACK_FRAG that got no reply.
    fn nack_frag_retry_delay(&self) -> RtpsDuration;
    /// Retries before a stalled fragment repair yields to the periodic heartbeat.
    fn nack_frag_max_retries(&self) -> u32;
    fn matched_writer_is_matched(&self, writer_guid: Guid) -> bool;
    fn matched_writers_guids(&self) -> Vec<Guid>;
    fn on_change(&self, change: Arc<CacheChange>);

    // Mark that a callback-producing access to this reader has started. Balanced by
    // `exit_callback`. `remove_reader` waits for `in_flight_callbacks` to reach zero so no
    // delivery or listener callback runs after deletion returns.
    fn enter_callback(&self);
    fn exit_callback(&self);
    fn in_flight_callbacks(&self) -> usize;

    // Set once `remove_reader` starts draining, so a callback path that raised the in-flight
    // count after the drain already passed can see the reader is gone and skip.
    fn mark_deleted(&self);
    fn is_deleted(&self) -> bool;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn set_datareader_cache(
        &mut self,
        datareader_cache: Weak<Mutex<dyn HistoryCache + Send + Sync>>,
    ) -> RtpsResult<()>;
    fn set_subscription_builtin_topic_data(
        &self,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> RtpsResult<()>;
    fn get_subscription_builtin_topic_data(&self) -> RtpsResult<SubscriptionBuiltinTopicData>;

    // For Dcps DataReader::get_matched_publication_data
    fn get_matched_publication_data(
        &self,
        writer_guid: Guid,
    ) -> RtpsResult<PublicationBuiltinTopicData>;

    fn remove_matched_writer_and_update_status(&self, writer_guid: Guid) -> RtpsResult<bool>;
}
