//! Participant Discovery Handler trait for SPDP message processing.
//!
//! This module provides a trait for handling participant discovery messages
//! that can be implemented by both SPDP and SEDP logic components.

use log::debug;
use speedy::{Endianness, Writable};
use std::sync::{Arc, Mutex};

use crate::{
    common::builtin::topic::{
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    },
    core::time::Duration,
    infrastructure::liveliness_monitor::LivelinessMonitor,
    rtps::{
        builtin::{
            data::{
                builtin_endpoint_set::BuiltinEndpointFlag,
                spdp_discovered_participant_data::SPDPDiscoveredParticipantData,
            },
            spdp_builtin_participant_writer::SPDPbuiltinParticipantWriter,
        },
        common::{
            entity_id::EntityId,
            guid::Guid,
            locator::{
                Locator, LOCATOR_KIND_TCP_V4, LOCATOR_KIND_TCP_V6, LOCATOR_KIND_UDP_V4,
                LOCATOR_KIND_UDP_V6,
            },
            rtps_error_code::RtpsResult,
            sequence::SequenceNumber,
            time::RtpsDuration,
        },
        entities::{
            entity::Entity,
            reader::{Reader as _, StatefulReader, WriterProxy},
            writer::{
                has_reader_locator::HasReaderLocator as _, reader_locator::ReaderLocator,
                reader_proxy::ReaderProxy, StatefulWriter, Writer as _,
            },
        },
        logic::common::ParticipantAccessor,
        messages::message_creator::MessageCreator,
        task::sending_handler::{MessageType, SendingHandler},
    },
};

/// Trait for handling participant discovery operations.
///
/// This trait provides default implementations for common participant discovery
/// operations that can be used by both SPDP and SEDP logic components.
pub(crate) trait ParticipantMessageProcessor: ParticipantAccessor {
    /// Handle discovered participant data (renamed from handle_multicast_spdp_message)
    fn handle_discovered_participant_data(
        &self,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
    ) -> RtpsResult<()> {
        if spdp_discovered_participant_data.participant_guid().entity_id() != EntityId::PARTICIPANT
        {
            debug!(
                "Received SPDP message with non-participant entity ID: {}. Ignoring.",
                spdp_discovered_participant_data.participant_guid()
            );
            return Ok(());
        }

        let participant = self.get_upgraded_participant()?;

        let participant_guid = spdp_discovered_participant_data.participant_guid();
        log::debug!(
            "Start handling DiscoveredParticipantData {} / [{}]",
            participant_guid,
            spdp_discovered_participant_data
                .metatraffic_unicast_locator_list()
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Start monitoring liveliness for the discovered participant
        self.register_remote_participant_liveliness(
            &spdp_discovered_participant_data.participant_guid(),
            spdp_discovered_participant_data.lease_duration().into(),
        )?;

        // Check domain ID match
        if participant.domain_id() != spdp_discovered_participant_data.domain_id() {
            log::trace!(
                "SPDP message domain ID mismatch: local {}, remote {}. Ignoring.",
                participant.domain_id(),
                spdp_discovered_participant_data.domain_id()
            );
            return Ok(());
        }

        // Check if this participant data already exists
        let is_exist = participant.remote_participant_proxy_datas().lock().is_ok_and(
            |remote_participant_datas| {
                remote_participant_datas.iter().any(|remote_participant_data| {
                    if remote_participant_data.participant_guid() == participant_guid {
                        log::debug!(
                            "DiscoveredParticipantData already exist {} / [{}]",
                            participant_guid,
                            spdp_discovered_participant_data
                                .metatraffic_unicast_locator_list()
                                .iter()
                                .map(|l| l.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        true
                    } else {
                        false
                    }
                })
            },
        );

        if is_exist {
            return Ok(());
        }

        // Setup builtin endpoints based on available endpoints
        self.match_builtin_endpoints(&spdp_discovered_participant_data)?;

        // Add remote participant data
        self.get_upgraded_participant()?
            .add_remote_participant_proxy_data(spdp_discovered_participant_data.clone());

        // Trigger SEDP message
        self.trigger_send_sedp_message(Arc::new(spdp_discovered_participant_data.clone()))?;

        Ok(())
    }

    /// Setup builtin endpoints for discovered participant
    fn match_builtin_endpoints(
        &self,
        spdp_discovered_participant_data: &SPDPDiscoveredParticipantData,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        // Add reader_locator/reader_proxy to writer according to remote endpointset
        // Create reader proxy and reader locator for builtin writers
        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PARTICIPANT_DETECTOR)
        {
            self.add_reader_locator_to_spdp_builtin_participant_writer(
                participant.spdp_builtin_participant_writer(),
                spdp_discovered_participant_data.clone(),
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_READER)
        {
            self.add_reader_proxy_to_builtin_writer(
                participant.builtin_participant_message_writer(),
                spdp_discovered_participant_data.clone(),
                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_WRITER)
        {
            self.add_writer_proxy_to_builtin_reader(
                participant.builtin_participant_message_reader(),
                spdp_discovered_participant_data.clone(),
                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PUBLICATIONS_DETECTOR)
        {
            self.add_reader_proxy_to_builtin_writer(
                participant.sedp_builtin_publications_writer(),
                spdp_discovered_participant_data.clone(),
                EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PUBLICATIONS_ANNOUNCER)
        {
            self.add_writer_proxy_to_builtin_reader(
                participant.sedp_builtin_publications_reader(),
                spdp_discovered_participant_data.clone(),
                EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_DETECTOR)
        {
            self.add_reader_proxy_to_builtin_writer(
                participant.sedp_builtin_subscriptions_writer(),
                spdp_discovered_participant_data.clone(),
                EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_ANNOUNCER)
        {
            self.add_writer_proxy_to_builtin_reader(
                participant.sedp_builtin_subscriptions_reader(),
                spdp_discovered_participant_data.clone(),
                EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
            );
        }

        // TypeLookup service (DDS-XTypes 1.3 §7.6.3) - request/reply RPC pairs
        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REQUEST_DATA_READER)
        {
            self.add_reader_proxy_to_builtin_writer(
                participant.type_lookup_request_writer(),
                spdp_discovered_participant_data.clone(),
                EntityId::TYPE_LOOKUP_REQUEST_READER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REQUEST_DATA_WRITER)
        {
            self.add_writer_proxy_to_builtin_reader(
                participant.type_lookup_request_reader(),
                spdp_discovered_participant_data.clone(),
                EntityId::TYPE_LOOKUP_REQUEST_WRITER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REPLY_DATA_READER)
        {
            self.add_reader_proxy_to_builtin_writer(
                participant.type_lookup_reply_writer(),
                spdp_discovered_participant_data.clone(),
                EntityId::TYPE_LOOKUP_REPLY_READER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REPLY_DATA_WRITER)
        {
            self.add_writer_proxy_to_builtin_reader(
                participant.type_lookup_reply_reader(),
                spdp_discovered_participant_data.clone(),
                EntityId::TYPE_LOOKUP_REPLY_WRITER,
            );
        }

        Ok(())
    }

    /// Trigger SEDP message sending for a discovered participant
    fn trigger_send_sedp_message(
        &self,
        spdp_discovered_participant_data: Arc<SPDPDiscoveredParticipantData>,
    ) -> RtpsResult<()> {
        let heartbeat_period =
            match self.get_upgraded_participant()?.spdp_builtin_participant_writer().lock() {
                Ok(writer) => writer.heartbeat_period(),
                Err(e) => {
                    log::error!("Failed to acquire spdp builtin participant writer lock: {}", e);
                    RtpsDuration::from_seconds_f64(2.0)
                }
            };

        let participant = self.get_upgraded_participant()?;
        let sending_handler = SendingHandler::get_instance(participant.clone(), None);
        let remote_prefix = spdp_discovered_participant_data.guid_prefix();
        let period = heartbeat_period.to_std_duration();
        let spdp_payload = self.create_spdp_message()?;

        // Send each SPDP message to the discovered participant before registering periodic timers
        // since timer is not triggered immediately after registration
        sending_handler.push_message_and_wake(MessageType::PeriodicParticipantDataUnicast(
            None,
            period,
            spdp_discovered_participant_data.clone(),
            spdp_payload.clone(),
        ));

        sending_handler.push_message_and_wake(MessageType::PeriodicPublicationHeartbeat(
            None,
            period,
            Arc::new(remote_prefix),
        ));

        sending_handler.push_message_and_wake(MessageType::PeriodicSubscriptionHeartbeat(
            None,
            period,
            Arc::new(remote_prefix),
        ));

        sending_handler.push_message_and_wake(MessageType::P2pHeartbeat(Some(remote_prefix)));

        // sending_handler.push_message_and_wake(MessageType::PeriodicSedpTopicHeartbeat(
        //     None,
        //     heartbeat_period.to_std_duration().clone(),
        //     spdp_discovered_participant_data.clone(),
        // ));

        let (_, sedp_logic_arc, _) = participant.get_logics();
        if let Some(sedp_logic) = sedp_logic_arc.as_ref().as_ref() {
            // Announcements made before this peer existed are only in the history; the heartbeats
            // armed below advertise them but nothing pumps a builtin writer's unsent changes.
            if let Err(e) = sedp_logic.push_sedp_history_to_participant(remote_prefix) {
                log::warn!("Failed to push SEDP history to {:?}: {}", remote_prefix, e);
            }

            let _ = sedp_logic.register_periodic_send_timer(
                remote_prefix,
                EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER,
                period,
                MessageType::PeriodicParticipantDataUnicast(
                    None,
                    period,
                    spdp_discovered_participant_data.clone(),
                    spdp_payload,
                ),
            );
            let _ = sedp_logic.register_periodic_send_timer(
                remote_prefix,
                EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
                period,
                MessageType::PeriodicPublicationHeartbeat(None, period, Arc::new(remote_prefix)),
            );
            let _ = sedp_logic.register_periodic_send_timer(
                remote_prefix,
                EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
                period,
                MessageType::PeriodicSubscriptionHeartbeat(None, period, Arc::new(remote_prefix)),
            );
        }

        Ok(())
    }

    // Create extended discovery rtps message for SPDP
    fn create_spdp_message(&self) -> RtpsResult<Option<Arc<Vec<u8>>>> {
        let data = match MessageCreator::create_spdp_msg(self.get_upgraded_participant()?.clone()) {
            Ok(rtps_message) => {
                match rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian) {
                    Ok(data) => Some(Arc::new(data)),
                    Err(e) => {
                        log::error!("Failed to write SPDP message: {:?}", e);
                        None
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to create SPDP message: {:?}", e);
                None
            }
        };

        Ok(data)
    }

    /// Add reader locator to SPDP builtin participant writer
    fn add_reader_locator_to_spdp_builtin_participant_writer(
        &self,
        spdp_builtin_participant_writer: Arc<Mutex<SPDPbuiltinParticipantWriter>>,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
    ) {
        // Add reader locator to SPDP builtin participant writer
        // SPDP sends to everyone once the locator list is registered
        for locator in spdp_discovered_participant_data.metatraffic_unicast_locator_list() {
            if !self.is_builtin_reader_locator_matched(
                spdp_builtin_participant_writer.clone(),
                locator.clone(),
            ) {
                match spdp_builtin_participant_writer.lock() {
                    Ok(spdp_builtin_participant_writer) => {
                        if !(locator.kind() == LOCATOR_KIND_UDP_V4
                            || locator.kind() == LOCATOR_KIND_UDP_V6
                            || locator.kind() == LOCATOR_KIND_TCP_V4
                            || locator.kind() == LOCATOR_KIND_TCP_V6)
                        {
                            continue;
                        }
                        let reader_locator = ReaderLocator::new(
                            locator.clone(),
                            None,
                            false,
                            spdp_discovered_participant_data.guid_prefix(),
                            EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER,
                            SubscriptionBuiltinTopicData::default(),
                        );

                        spdp_builtin_participant_writer.reader_locator_add(reader_locator);
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to acquire spdp builtin participant writer lock: {}",
                            e
                        );
                    }
                }
            }
        }
    }

    /// Add reader proxy to writer for builtin endpoints
    fn add_reader_proxy_to_builtin_writer(
        &self,
        writer: Arc<StatefulWriter>,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
        remote_entity_id: EntityId,
    ) {
        let reader_guid = Guid::new(
            spdp_discovered_participant_data.participant_guid().prefix(),
            remote_entity_id,
        );

        if !writer.matched_reader_is_matched(reader_guid) {
            let reader_proxy = ReaderProxy::new(
                reader_guid,
                remote_entity_id,
                spdp_discovered_participant_data.metatraffic_unicast_locator_list().clone(),
                spdp_discovered_participant_data.metatraffic_multicast_locator_list().clone(),
                writer.last_change_sequence_number(),
                SequenceNumber::new(0, 0),
                false,
                true,
                // 8.5.4.1 According to the DDS specification, the reliability QoS for these built-in Entities is set to 'reliable.'
                SubscriptionBuiltinTopicData::default(),
                SequenceNumber::new(0, 0), // Built-in endpoints are not volatile
            );
            writer.matched_reader_add(reader_proxy);
        }
    }

    /// Add writer proxy to reader for builtin endpoints
    fn add_writer_proxy_to_builtin_reader(
        &self,
        reader: Arc<StatefulReader>,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
        remote_entity_id: EntityId,
    ) {
        let writer_guid = Guid::new(
            spdp_discovered_participant_data.participant_guid().prefix(),
            remote_entity_id,
        );

        if !reader.matched_writer_is_matched(writer_guid) {
            let reader_proxy = WriterProxy::new(
                writer_guid,
                remote_entity_id,
                spdp_discovered_participant_data.metatraffic_unicast_locator_list().clone(),
                spdp_discovered_participant_data.metatraffic_multicast_locator_list().clone(),
                0,
                PublicationBuiltinTopicData::default(),
                reader.get_update_status_callback(),
            );
            reader.matched_writer_add(reader_proxy);
        }
    }

    /// Check if reader locator exists in writer
    fn is_builtin_reader_locator_matched(
        &self,
        writer: Arc<Mutex<SPDPbuiltinParticipantWriter>>,
        locator: Locator,
    ) -> bool {
        if let Ok(writer) = writer.lock() {
            writer.reader_locator().iter().any(|reader_locator| reader_locator.locator() == locator)
        } else {
            false
        }
    }

    fn register_remote_participant_liveliness(
        &self,
        remote_participant_guid: &Guid,
        lease_duration: Duration,
    ) -> RtpsResult<()> {
        if let Ok(mut monitor) = self.get_upgraded_participant()?.liveliness_monitor().lock() {
            if monitor.is_none() {
                let participant = self.get_upgraded_participant()?;
                let callback = Arc::new(move |participant_guid: Guid| {
                    let _ = participant.unmatch_with_remote_participant(&participant_guid);
                    true
                });
                *monitor = Some(LivelinessMonitor::new(callback));
            }

            if let Some(monitor) = monitor.as_ref() {
                monitor.track_writer(remote_participant_guid, lease_duration);
            }
        }

        Ok(())
    }
}
