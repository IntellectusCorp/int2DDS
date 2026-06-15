//! Builtin endpoint creation and management.
//!
//! This module provides functions for creating and configuring the builtin discovery
//! endpoints (SPDP and SEDP readers/writers) used in the RTPS discovery protocol.
//! These endpoints handle participant, publication, and subscription discovery.

use std::sync::{Arc, Mutex, Weak};

use log::debug;

use crate::{
    common::builtin::topic::{
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    },
    infrastructure::qos_policy::ReliabilityQosPolicyKind,
    rtps::{
        builtin::{
            spdp_builtin_participant_reader::SPDPBuiltinParticipantReader,
            spdp_builtin_participant_writer::SPDPbuiltinParticipantWriter,
        },
        common::{
            entity_id::EntityId,
            guid::{Guid, GuidPrefix},
            time::RtpsDuration,
            types::TopicKind,
        },
        entities::{reader::StatefulReader, writer::StatefulWriter},
    },
};

pub const BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY: usize = 2000;

/// Default heartbeat period for SPDP
const DEFAULT_HEARTBEAT_PERIOD_SECONDS: f64 = 2.0;

#[derive(Debug, Clone)]
pub struct BuiltinEndpoints {
    //SPDP
    pub spdp_builtin_participant_writer: Arc<Mutex<SPDPbuiltinParticipantWriter>>,
    pub spdp_builtin_participant_reader: Arc<SPDPBuiltinParticipantReader>,

    //SEDP
    pub sedp_builtin_publications_writer: Arc<StatefulWriter>,
    pub sedp_builtin_publications_reader: Arc<StatefulReader>,
    pub sedp_builtin_subscriptions_writer: Arc<StatefulWriter>,
    pub sedp_builtin_subscriptions_reader: Arc<StatefulReader>,

    //SEDP Topics
    pub sedp_builtin_topics_writer: Arc<StatefulWriter>,
    pub sedp_builtin_topics_reader: Arc<StatefulReader>,

    // WLP (Liveliness)
    pub builtin_participant_message_writer: Arc<StatefulWriter>,
    pub builtin_participant_message_reader: Arc<StatefulReader>,

    // TypeLookup
    pub type_lookup_request_writer: Arc<StatefulWriter>,
    pub type_lookup_request_reader: Arc<StatefulReader>,
    pub type_lookup_reply_writer: Arc<StatefulWriter>,
    pub type_lookup_reply_reader: Arc<StatefulReader>,
}

impl BuiltinEndpoints {
    fn get_heartbeat_period() -> RtpsDuration {
        let period = crate::common::int2dds_feature_ffi::get_heartbeat_period_seconds(
            DEFAULT_HEARTBEAT_PERIOD_SECONDS,
        );

        RtpsDuration::from_seconds_f64(period)
    }

    pub fn new(participant_guid: Guid) -> Self {
        //SPDP
        let heartbeat_period = Self::get_heartbeat_period();
        let spdp_writer = SPDPbuiltinParticipantWriter::new(
            Guid::new(participant_guid.prefix(), EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER),
            ReliabilityQosPolicyKind::BestEffort,
            TopicKind::WithKey,
            EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER,
            false,
            heartbeat_period,
            1024,
        );
        let spdp_reader = SPDPBuiltinParticipantReader::new(
            Guid::new(participant_guid.prefix(), EntityId::SPDP_BUILTIN_PARTICIPANT_READER),
            TopicKind::WithKey,
            ReliabilityQosPolicyKind::BestEffort,
            Vec::new(),
            Vec::new(),
            EntityId::SPDP_BUILTIN_PARTICIPANT_READER,
            false,
        );

        //SEDP
        let sedp_publications_writer = StatefulWriter::new(
            Guid::new(participant_guid.prefix(), EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::WithKey,
            EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
            1024,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        );
        let sedp_publications_reader = StatefulReader::new(
            Guid::new(participant_guid.prefix(), EntityId::SEDP_BUILTIN_PUBLICATIONS_READER),
            TopicKind::WithKey,
            ReliabilityQosPolicyKind::Reliable,
            Vec::new(),
            Vec::new(),
            EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
            false,
            None,
            None,
            SubscriptionBuiltinTopicData::default(),
            participant_guid,
        );
        let sedp_subscriptions_writer = StatefulWriter::new(
            Guid::new(participant_guid.prefix(), EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::WithKey,
            EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
            1024,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        );
        let sedp_subscriptions_reader = StatefulReader::new(
            Guid::new(participant_guid.prefix(), EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER),
            TopicKind::WithKey,
            ReliabilityQosPolicyKind::Reliable,
            Vec::new(),
            Vec::new(),
            EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER,
            false,
            None,
            None,
            SubscriptionBuiltinTopicData::default(),
            participant_guid,
        );

        //SEDP Topics
        let sedp_topics_writer = StatefulWriter::new(
            Guid::new(participant_guid.prefix(), EntityId::SEDP_BUILTIN_TOPICS_WRITER),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::WithKey,
            EntityId::SEDP_BUILTIN_TOPICS_WRITER,
            1024,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        );
        let sedp_topics_reader = StatefulReader::new(
            Guid::new(participant_guid.prefix(), EntityId::SEDP_BUILTIN_TOPICS_READER),
            TopicKind::WithKey,
            ReliabilityQosPolicyKind::Reliable,
            Vec::new(),
            Vec::new(),
            EntityId::SEDP_BUILTIN_TOPICS_READER,
            false,
            None,
            None,
            SubscriptionBuiltinTopicData::default(),
            participant_guid,
        );

        // WLP
        let builtin_participant_message_writer = StatefulWriter::new(
            Guid::new(participant_guid.prefix(), EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::WithKey,
            // HistoryQosPolicyKind::KeepLast(1),
            // ResourceLimitsQosPolicy::default(),
            EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
            1024,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        );
        let builtin_participant_message_reader = StatefulReader::new(
            Guid::new(participant_guid.prefix(), EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER),
            TopicKind::WithKey,
            ReliabilityQosPolicyKind::Reliable, // BestEffort possible?
            // HistoryQosPolicyKind::KeepLast(1),
            // ResourceLimitsQosPolicy::default(),
            Vec::new(),
            Vec::new(),
            EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
            false,
            None,
            None,
            SubscriptionBuiltinTopicData::default(),
            participant_guid,
        );

        // TypeLookup service (keyless, Reliable RPC endpoints)
        let type_lookup_request_writer = StatefulWriter::new(
            Guid::new(participant_guid.prefix(), EntityId::TYPE_LOOKUP_REQUEST_WRITER),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::NoKey,
            EntityId::TYPE_LOOKUP_REQUEST_WRITER,
            1024,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        );
        let type_lookup_request_reader = StatefulReader::new(
            Guid::new(participant_guid.prefix(), EntityId::TYPE_LOOKUP_REQUEST_READER),
            TopicKind::NoKey,
            ReliabilityQosPolicyKind::Reliable,
            Vec::new(),
            Vec::new(),
            EntityId::TYPE_LOOKUP_REQUEST_READER,
            false,
            None,
            None,
            SubscriptionBuiltinTopicData::default(),
            participant_guid,
        );
        let type_lookup_reply_writer = StatefulWriter::new(
            Guid::new(participant_guid.prefix(), EntityId::TYPE_LOOKUP_REPLY_WRITER),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::NoKey,
            EntityId::TYPE_LOOKUP_REPLY_WRITER,
            1024,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        );
        let type_lookup_reply_reader = StatefulReader::new(
            Guid::new(participant_guid.prefix(), EntityId::TYPE_LOOKUP_REPLY_READER),
            TopicKind::NoKey,
            ReliabilityQosPolicyKind::Reliable,
            Vec::new(),
            Vec::new(),
            EntityId::TYPE_LOOKUP_REPLY_READER,
            false,
            None,
            None,
            SubscriptionBuiltinTopicData::default(),
            participant_guid,
        );

        Self {
            spdp_builtin_participant_writer: Arc::new(Mutex::new(spdp_writer)),
            spdp_builtin_participant_reader: Arc::new(spdp_reader),
            sedp_builtin_publications_writer: Arc::new(sedp_publications_writer),
            sedp_builtin_publications_reader: Arc::new(sedp_publications_reader),
            sedp_builtin_subscriptions_writer: Arc::new(sedp_subscriptions_writer),
            sedp_builtin_subscriptions_reader: Arc::new(sedp_subscriptions_reader),
            sedp_builtin_topics_writer: Arc::new(sedp_topics_writer),
            sedp_builtin_topics_reader: Arc::new(sedp_topics_reader),
            builtin_participant_message_writer: Arc::new(builtin_participant_message_writer),
            builtin_participant_message_reader: Arc::new(builtin_participant_message_reader),
            type_lookup_request_writer: Arc::new(type_lookup_request_writer),
            type_lookup_request_reader: Arc::new(type_lookup_request_reader),
            type_lookup_reply_writer: Arc::new(type_lookup_reply_writer),
            type_lookup_reply_reader: Arc::new(type_lookup_reply_reader),
        }
    }

    /// Remove all remote Built-in Endpoints that were matched when remote Participant no longer exists or has died
    pub fn remove_unmatched_endpoint(&self, terminated_participant_guid_prefix: GuidPrefix) {
        //SPDP
        if let Ok(spdp_builtin_participant_writer) = self.spdp_builtin_participant_writer.lock() {
            spdp_builtin_participant_writer
                .reader_locator_remove_by_guid_prefix(terminated_participant_guid_prefix);
        } else {
            debug!("Failed to lock SPDP builtin participant writer for unmatched removal");
        }

        //SEDP
        if let Ok(mut reader_proxies) =
            self.sedp_builtin_publications_writer.reader_proxies().lock()
        {
            debug!(
                "SEDP builtin publications writer had {} reader proxies before remove {:?}",
                reader_proxies.len(),
                terminated_participant_guid_prefix
            );
            reader_proxies.retain(|reader_proxy| {
                reader_proxy.remote_reader_guid().prefix() != terminated_participant_guid_prefix
            });
            debug!(
                "SEDP builtin publications writer now has {} reader proxies",
                reader_proxies.len()
            );
        } else {
            debug!("Failed to lock SEDP builtin publications writer reader proxies");
        }

        if let Ok(mut writer_proxies) =
            self.sedp_builtin_publications_reader.writer_proxies().lock()
        {
            debug!(
                "SEDP builtin publications reader had {} writer proxies before remove {:?}",
                writer_proxies.len(),
                terminated_participant_guid_prefix
            );
            writer_proxies.retain(|writer_proxy| {
                writer_proxy.remote_writer_guid().prefix() != terminated_participant_guid_prefix
            });
            debug!(
                "SEDP builtin publications reader now has {} writer proxies",
                writer_proxies.len()
            );
        } else {
            debug!("Failed to lock SEDP builtin publications reader writer proxies");
        }

        if let Ok(mut reader_proxies) =
            self.sedp_builtin_subscriptions_writer.reader_proxies().lock()
        {
            debug!(
                "SEDP builtin subscriptions writer had {} reader proxies before remove {:?}",
                reader_proxies.len(),
                terminated_participant_guid_prefix
            );
            reader_proxies.retain(|reader_proxy| {
                reader_proxy.remote_reader_guid().prefix() != terminated_participant_guid_prefix
            });
            debug!(
                "SEDP builtin subscriptions writer now has {} reader proxies",
                reader_proxies.len()
            );
        } else {
            debug!("Failed to lock SEDP builtin subscriptions writer reader proxies");
        }

        if let Ok(mut writer_proxies) =
            self.sedp_builtin_subscriptions_reader.writer_proxies().lock()
        {
            debug!(
                "SEDP builtin subscriptions reader had {} writer proxies before remove {:?}",
                writer_proxies.len(),
                terminated_participant_guid_prefix
            );
            writer_proxies.retain(|writer_proxy| {
                writer_proxy.remote_writer_guid().prefix() != terminated_participant_guid_prefix
            });
            debug!(
                "SEDP builtin subscriptions reader now has {} writer proxies",
                writer_proxies.len()
            );
        } else {
            debug!("Failed to lock SEDP builtin subscriptions reader writer proxies");
        }

        //SEDP Topics
        if let Ok(mut reader_proxies) = self.sedp_builtin_topics_writer.reader_proxies().lock() {
            debug!(
                "SEDP builtin topics writer had {} reader proxies before remove {:?}",
                reader_proxies.len(),
                terminated_participant_guid_prefix
            );
            reader_proxies.retain(|reader_proxy| {
                reader_proxy.remote_reader_guid().prefix() != terminated_participant_guid_prefix
            });
            debug!("SEDP builtin topics writer now has {} reader proxies", reader_proxies.len());
        } else {
            debug!("Failed to lock SEDP builtin topics writer reader proxies");
        }

        if let Ok(mut writer_proxies) = self.sedp_builtin_topics_reader.writer_proxies().lock() {
            debug!(
                "SEDP builtin topics reader had {} writer proxies before remove {:?}",
                writer_proxies.len(),
                terminated_participant_guid_prefix
            );
            writer_proxies.retain(|writer_proxy| {
                writer_proxy.remote_writer_guid().prefix() != terminated_participant_guid_prefix
            });
            debug!("SEDP builtin topics reader now has {} writer proxies", writer_proxies.len());
        } else {
            debug!("Failed to lock SEDP builtin topics reader writer proxies");
        }

        debug!(
            "Removed all unmatched built-in endpoints from terminated participant: {:?}",
            terminated_participant_guid_prefix
        );

        // WLP
        if let Ok(mut reader_proxies) =
            self.builtin_participant_message_writer.reader_proxies().lock()
        {
            debug!(
                "SEDP builtin participant message writer had {} reader locators before remove {:?}",
                reader_proxies.len(),
                terminated_participant_guid_prefix
            );
            reader_proxies.retain(|reader_proxy| {
                reader_proxy.remote_reader_guid().prefix() != terminated_participant_guid_prefix
            });
            debug!(
                "SEDP builtin participant message writer now has {} reader locators",
                reader_proxies.len()
            );
        } else {
            debug!("Failed to lock SEDP builtin participant message writer reader proxies");
        }

        if let Ok(mut writer_proxies) =
            self.builtin_participant_message_reader.writer_proxies().lock()
        {
            debug!(
                "SEDP builtin participant message reader had {} writer proxies before remove {:?}",
                writer_proxies.len(),
                terminated_participant_guid_prefix
            );
            writer_proxies.retain(|writer_proxy| {
                writer_proxy.remote_writer_guid().prefix() != terminated_participant_guid_prefix
            });
            debug!(
                "SEDP builtin participant message reader now has {} writer proxies",
                writer_proxies.len()
            );
        } else {
            debug!("Failed to lock SEDP builtin participant message reader writer proxies");
        }

        // TypeLookup service
        for writer in [&self.type_lookup_request_writer, &self.type_lookup_reply_writer] {
            if let Ok(mut reader_proxies) = writer.reader_proxies().lock() {
                reader_proxies.retain(|reader_proxy| {
                    reader_proxy.remote_reader_guid().prefix() != terminated_participant_guid_prefix
                });
            } else {
                debug!("Failed to lock TypeLookup writer reader proxies");
            }
        }
        for reader in [&self.type_lookup_request_reader, &self.type_lookup_reply_reader] {
            if let Ok(mut writer_proxies) = reader.writer_proxies().lock() {
                writer_proxies.retain(|writer_proxy| {
                    writer_proxy.remote_writer_guid().prefix() != terminated_participant_guid_prefix
                });
            } else {
                debug!("Failed to lock TypeLookup reader writer proxies");
            }
        }
    }
}
