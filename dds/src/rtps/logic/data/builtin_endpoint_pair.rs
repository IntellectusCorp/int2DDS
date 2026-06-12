use std::sync::Arc;

use crate::rtps::builtin::data::builtin_endpoint_set::BuiltinEndpointFlag;
use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::rtps_error_code::RtpsResult;
use crate::rtps::entities::participant::Participant;
use crate::rtps::entities::reader::StatefulReader;
use crate::rtps::entities::writer::StatefulWriter;

pub(crate) struct BuiltinEndpointPair {
    writer_proxy: Arc<StatefulReader>,
    reader_proxy: Arc<StatefulWriter>,
}

impl BuiltinEndpointPair {
    pub(crate) fn new(
        writer_proxy: Arc<StatefulReader>,
        reader_proxy: Arc<StatefulWriter>,
    ) -> Self {
        Self { writer_proxy, reader_proxy }
    }

    pub(crate) fn reader(&self) -> Arc<StatefulReader> {
        self.writer_proxy.clone()
    }

    pub(crate) fn writer(&self) -> Arc<StatefulWriter> {
        self.reader_proxy.clone()
    }

    pub(crate) fn reader_writer_from_entity_id(
        entity_id: EntityId,
        participant: Arc<Participant>,
    ) -> RtpsResult<Option<Self>> {
        let available_builtin_endpoints = {
            let local_participant_data = participant.local_participant_proxy_data();
            local_participant_data.available_builtin_endpoints()
        };
        match entity_id {
            EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER => {
                if available_builtin_endpoints
                    .contains(BuiltinEndpointFlag::BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_READER)
                {
                    return Ok(Some(BuiltinEndpointPair::new(
                        participant.builtin_participant_message_reader(),
                        participant.builtin_participant_message_writer(),
                    )));
                }
                Ok(None)
            }
            EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER => {
                if available_builtin_endpoints
                    .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PUBLICATIONS_DETECTOR)
                {
                    return Ok(Some(BuiltinEndpointPair::new(
                        participant.sedp_builtin_publications_reader(),
                        participant.sedp_builtin_publications_writer(),
                    )));
                }
                Ok(None)
            }
            EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER => {
                if available_builtin_endpoints
                    .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_DETECTOR)
                {
                    return Ok(Some(BuiltinEndpointPair::new(
                        participant.sedp_builtin_subscriptions_reader(),
                        participant.sedp_builtin_subscriptions_writer(),
                    )));
                }
                Ok(None)
            }
            EntityId::SEDP_BUILTIN_TOPICS_WRITER => {
                if available_builtin_endpoints
                    .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_TOPICS_DETECTOR)
                {
                    return Ok(Some(BuiltinEndpointPair::new(
                        participant.sedp_builtin_topics_reader(),
                        participant.sedp_builtin_topics_writer(),
                    )));
                }
                Ok(None)
            }
            EntityId::TYPE_LOOKUP_REQUEST_WRITER => {
                if available_builtin_endpoints.contains(
                    BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REQUEST_DATA_READER,
                ) {
                    return Ok(Some(BuiltinEndpointPair::new(
                        participant.type_lookup_request_reader(),
                        participant.type_lookup_request_writer(),
                    )));
                }
                Ok(None)
            }
            EntityId::TYPE_LOOKUP_REPLY_WRITER => {
                if available_builtin_endpoints.contains(
                    BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REPLY_DATA_READER,
                ) {
                    return Ok(Some(BuiltinEndpointPair::new(
                        participant.type_lookup_reply_reader(),
                        participant.type_lookup_reply_writer(),
                    )));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
