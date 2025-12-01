//! Writer trait and implementation for RTPS DataWriters.
//!
//! This module defines the `Writer` trait representing RTPS DataWriters that publish
//! data samples to DataReaders. Writers manage history caches, reader proxies, heartbeat
//! timers, and implement reliable or best-effort communication protocols.

#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    any::Any,
    fmt::Debug,
    sync::{Arc, Mutex},
};

use crate::{
    common::{
        builtin::topic::{
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    core::time::Duration as DcpsDuration,
    infrastructure::qos_policy::LivelinessQosPolicy,
    rtps::{
        common::{
            guid::Guid,
            rtps_error_code::RtpsResult,
            sequence::SequenceNumber,
            time::{RtpsDuration, RtpsTime},
            types::{ChangeKind, SerializedData},
        },
        entities::{
            endpoint::Endpoint,
            entity::Entity,
            history::{cache_change::CacheChange, writer_history::WriterHistoryCache},
        },
    },
};

pub(crate) trait Writer: Entity + Endpoint + Debug + Any {
    // Based on Figure 8.6 state diagram
    // in: state Flag, actual Data, inline QoS, InstanceHandle
    // out: CacheChange
    fn new_change(
        &self,
        kind: ChangeKind,
        data: SerializedData,
        // inline_qos: ParameterList,
        handle: InstanceHandle,
        source_timestamp: Option<RtpsTime>,
    ) -> CacheChange;

    fn push_mode(&self) -> bool;
    fn heartbeat_period(&self) -> RtpsDuration;
    fn nack_response_delay(&self) -> RtpsDuration;
    fn nack_suppression_duration(&self) -> RtpsDuration;
    fn last_change_sequence_number(&self) -> SequenceNumber;
    fn data_max_size_serialized(&self) -> i32;
    fn wait_for_all_acked(&self, max_wait: DcpsDuration) -> bool;
    fn matched_reader_is_matched(&self, reader_guid: Guid) -> bool;
    fn matched_readers_guids(&self) -> Vec<Guid>;
    fn writer_cache(&self) -> Arc<Mutex<WriterHistoryCache>>;

    fn liveliness(&self) -> RtpsResult<LivelinessQosPolicy>;
    fn assert_liveliness(&self) -> bool;
    fn get_publication_builtin_topic_data(&self)
        -> RtpsResult<Option<PublicationBuiltinTopicData>>; // TODO: Contain real qos rather than this data type
    fn set_publication_builtin_topic_data(
        &self,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> RtpsResult<()>;
    // For Dcps DataWriter::get_matched_subscription_data
    fn get_matched_subscription_data(
        &self,
        reader_guid: Guid,
    ) -> RtpsResult<SubscriptionBuiltinTopicData>;

    //any
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
