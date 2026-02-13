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
        common::{
            guid::Guid, rtps_error_code::RtpsResult, time::RtpsDuration,
        },
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
    fn matched_writer_is_matched(&self, writer_guid: Guid) -> bool;
    fn matched_writers_guids(&self) -> Vec<Guid>;
    fn on_change(&self, change: Arc<CacheChange>);

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn set_datareader_cache(
        &mut self,
        datareader_cache: Weak<
            Mutex<dyn HistoryCache<CacheChangeInputType = Arc<Mutex<CacheChange>>> + Send + Sync>,
        >,
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
}
