//! Discovered entity data structures for SEDP.
//!
//! This module defines data structures representing discovered DataReaders, DataWriters,
//! and Topics in the SEDP (Simple Endpoint Discovery Protocol). These structures carry
//! QoS and configuration information for discovered entities.

#![allow(dead_code)]

use crate::topic::qos::TopicQos;
use crate::{
    common::builtin::topic::{
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        topic_builtin_topic_data::TopicBuiltinTopicData,
    },
    rtps::{
        builtin::data::content_filtered_topic::ContentFilterProperty, common::guid::Guid,
        entities::participant::Participant,
    },
};
#[derive(Debug)]
pub(crate) struct DiscoveredWriterData {
    pub publication_builtin_topic_data: PublicationBuiltinTopicData,
}

impl DiscoveredWriterData {
    pub(crate) fn new(
        _participant: Participant,
        _publication_guid: Guid,
        _writer_guid: Guid,
    ) -> Self {
        let publication_builtin_topic_data = PublicationBuiltinTopicData::default();
        Self { publication_builtin_topic_data }
    }
}
#[derive(Debug)]
pub(crate) struct DiscoveredReaderData {
    pub subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    pub content_filter: Option<ContentFilterProperty>,
}

impl DiscoveredReaderData {
    pub(crate) fn new(
        _participant: Participant,
        _subscription_guid: Guid,
        _reader_guid: Guid,
    ) -> Self {
        let subscription_builtin_topic_data = SubscriptionBuiltinTopicData::default();
        Self { subscription_builtin_topic_data, content_filter: None }
    }
}

//8.5.4.3 SEDP As mentioned in the DDS specification, Topic propagation is optional ... ?
pub(crate) struct DiscoveredTopicData {
    pub topic_builtin_topic_data: TopicBuiltinTopicData,
}

impl DiscoveredTopicData {
    pub(crate) fn new(topic_guid: Guid, topic_name: String, type_name: String) -> Self {
        let topic_builtin_topic_data =
            TopicBuiltinTopicData::new(topic_guid, topic_name, type_name, TopicQos::default());
        Self { topic_builtin_topic_data }
    }
}
