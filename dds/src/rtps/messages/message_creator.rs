//! RTPS message creation utilities.
//!
//! This module provides functions for creating RTPS messages from submessages,
//! including headers, submessage composition, and serialization for transmission
//! over the network.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use log::{debug, info};
use speedy::{Endianness, Writable};

use crate::rtps::{
    builtin::data::content_filtered_topic::ContentFilterInfo,
    common::{
        entity_id::EntityId,
        guid::Guid,
        parameters::{Parameter, ParameterId, ParameterList, StatusInfo},
        rtps_error_code::RtpsResult,
        sequence::{FragmentNumberSet, SequenceNumber},
        types::{ChangeKind, SerializedData},
    },
    entities::{entity::Entity, history::cache_change::CacheChange, participant::Participant},
    messages::{
        header::Header,
        rtps_messages::RtpsMessage,
        spdp_message::SpdpMessage,
        submessage::Submessage,
        submessage_body::SubmessageBody,
        submessage_creator::SubmessageCreator,
        submessage_header::SubmessageHeader,
        submessage_header_flag::{SubmessageFlagType, SubmessageHeaderFlag},
        submessage_id::SubmessageId,
        submessages::{data::Data, data_frag::DataFrag},
    },
};

pub(crate) struct MessageCreator {}

impl MessageCreator {
    pub(crate) fn create_spdp_msg(participant: Arc<Participant>) -> RtpsResult<Arc<RtpsMessage>> {
        let (participant_guid, _) = {
            let local_participant_data = participant.local_participant_proxy_data();
            (
                local_participant_data.participant_guid(),
                local_participant_data.available_builtin_endpoints(),
            )
        };

        info!("Creating basic SPDP message for participant: {:?}", participant_guid);

        let spdp_message = SpdpMessage::new(participant, None)?;
        let rtps_message = spdp_message.rtps_message();

        Ok(rtps_message)
    }

    pub(crate) fn create_spdp_msg_with_inline_qos(
        participant: Arc<Participant>,
    ) -> RtpsResult<Arc<RtpsMessage>> {
        let mut param_list = ParameterList::default();
        let key_hash = participant.guid().to_bytes();
        param_list.add_parameter(Self::create_key_hash_parameter(&key_hash));

        let mut flags = 0;

        flags |= StatusInfo::DISPOSED;
        flags |= StatusInfo::UNREGISTERED;
        let status_info_bytes = flags.to_be_bytes();
        param_list.add_parameter(Self::create_status_info_parameter(&status_info_bytes));

        let spdp_message = SpdpMessage::new(participant, Some(param_list))?;
        let rtps_message = spdp_message.rtps_message();

        Ok(rtps_message)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_heartbeat_message(
        local_participant_guid: Guid,
        target_participant_guid: Guid,
        heartbeat_count: i32,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
        final_flag: bool,
        liveliness_flag: bool,
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>> {
        let mut rtps_message = RtpsMessage::new(Header::new(local_participant_guid.prefix()));

        rtps_message.add_submessage(SubmessageCreator::create_info_dst_submessage(
            target_participant_guid.prefix(),
        ));
        rtps_message.add_submessage(SubmessageCreator::create_heartbeat_submessage(
            heartbeat_count,
            reader_entity_id,
            writer_entity_id,
            first_sn,
            last_sn,
            final_flag,
            liveliness_flag,
        )?);

        match rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian) {
            Ok(buffer) => Ok(Arc::new(buffer)),
            Err(e) => Err(Box::new(e)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_acknack_message(
        local_participant_guid: Guid,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        missing_changes: Vec<SequenceNumber>,
        acknack_count: i32,
        bitmap_base: SequenceNumber,
        is_preemptive: bool,
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>> {
        let mut rtps_message = RtpsMessage::new(Header::new(local_participant_guid.prefix()));

        rtps_message
            .add_submessage(SubmessageCreator::create_info_dst_submessage(remote_guid.prefix()));
        rtps_message.add_submessage(SubmessageCreator::create_acknack_submessage(
            reader_entity_id,
            writer_entity_id,
            missing_changes,
            acknack_count,
            bitmap_base,
            is_preemptive,
        )?);

        match rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian) {
            Ok(buffer) => Ok(Arc::new(buffer)),
            Err(e) => Err(Box::new(e)),
        }
    }

    pub(crate) fn create_data_msg(
        cache_change: Arc<CacheChange>,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        heartbeat_info: Option<(i32, SequenceNumber, SequenceNumber, bool, bool)>,
        use_inline_qos: bool, // TODO: Can be changed to Vec<Parameter> in the future
        content_filter_info: Option<ContentFilterInfo>,
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>> {
        debug!("Creating RTPS message from cache change: {:?}", cache_change);

        let mut rtps_message = RtpsMessage::new(Header::new(cache_change.writer_guid().prefix()));

        rtps_message
            .add_submessage(SubmessageCreator::create_info_dst_submessage(remote_guid.prefix()));
        let timestamp = Utc::now();
        rtps_message.add_submessage(SubmessageCreator::create_info_ts_submessage(timestamp));

        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::DATA);
        data_header_flag.add_flag(SubmessageFlagType::DataFlag, SubmessageId::DATA);

        let mut data =
            Data::new(reader_entity_id, writer_entity_id, cache_change.sequence_number());

        // Add inline QoS parameters if enabled
        if use_inline_qos {
            let mut param_list = ParameterList::default();
            if !cache_change.instance_handle().is_nil() {
                param_list.add_parameter(Self::create_key_hash_parameter(
                    cache_change.instance_handle().value(),
                ));
            }

            let mut flags = 0;
            match cache_change.kind() {
                ChangeKind::Alive => {}
                ChangeKind::AliveFiltered => {}
                ChangeKind::NotAliveDisposed => {
                    flags |= StatusInfo::DISPOSED;
                    let status_info_bytes = flags.to_be_bytes();
                    param_list
                        .add_parameter(Self::create_status_info_parameter(&status_info_bytes));
                }
                ChangeKind::NotAliveUnregistered => {
                    flags |= StatusInfo::UNREGISTERED;
                    let status_info_bytes = flags.to_be_bytes();
                    param_list
                        .add_parameter(Self::create_status_info_parameter(&status_info_bytes));
                }
                ChangeKind::NotAliveDisposedUnregistered => {
                    flags |= StatusInfo::DISPOSED;
                    flags |= StatusInfo::UNREGISTERED;
                    let status_info_bytes = flags.to_be_bytes();
                    param_list
                        .add_parameter(Self::create_status_info_parameter(&status_info_bytes));
                }
            }

            // Add ContentFilterInfo if present
            if let Some(filter_info) = content_filter_info {
                use crate::serialize::pl_cdr::InlineQosSerializer;
                let serializer = InlineQosSerializer::new(false); // little endian

                if let Ok(filter_bytes) = serializer.serialize_content_filter_info(&filter_info) {
                    param_list.add_parameter(Parameter::new(
                        ParameterId::PidContentFilterInfo,
                        filter_bytes,
                    ));
                    debug!("Added ContentFilterInfo to inline QoS");
                }
            }

            if !param_list.parameters().is_empty() {
                data_header_flag.add_flag(SubmessageFlagType::InlineQosFlag, SubmessageId::DATA);
                data.set_inline_qos_list(param_list);
            }
        }

        data.add_serialized_data(cache_change.data_value_arc());
        let data_submessage = Submessage {
            header: SubmessageHeader::new(
                SubmessageId::DATA,
                data_header_flag.flag,
                data.octets_to_next_header(),
            ),
            body: SubmessageBody::Data(data),
        };

        rtps_message.add_submessage(data_submessage);

        // Add heartbeat submessage if heartbeat info is provided
        if let Some((heartbeat_count, first_sn, last_sn, final_flag, liveliness_flag)) =
            heartbeat_info
        {
            let heartbeat_submessage = SubmessageCreator::create_heartbeat_submessage(
                heartbeat_count,
                reader_entity_id,
                writer_entity_id,
                first_sn,
                last_sn,
                final_flag,
                liveliness_flag,
            )?;
            rtps_message.add_submessage(heartbeat_submessage);
        }

        // Serialize the complete RTPS message
        match rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian) {
            Ok(buffer) => Ok(Arc::new(buffer)),
            Err(e) => Err(Box::new(e)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_data_frag_msg(
        cache_change: Arc<CacheChange>,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        fragment_starting_num: u32,
        fragments_in_submessage: u16,
        fragment_size: u16,
        sample_size: u32,
        fragment_data: SerializedData,
        heartbeat_info: Option<(i32, SequenceNumber, SequenceNumber, bool, bool)>,
        timestamp: DateTime<Utc>,
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>> {
        let mut rtps_message = RtpsMessage::new(Header::new(cache_change.writer_guid().prefix()));

        rtps_message
            .add_submessage(SubmessageCreator::create_info_dst_submessage(remote_guid.prefix()));
        rtps_message.add_submessage(SubmessageCreator::create_info_ts_submessage(timestamp));

        let mut data_frag_header_flag = SubmessageHeaderFlag::new();
        data_frag_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::DATA_FRAG);
        data_frag_header_flag.add_flag(SubmessageFlagType::DataFlag, SubmessageId::DATA_FRAG);

        let mut data_frag = DataFrag::new(
            reader_entity_id,
            writer_entity_id,
            cache_change.sequence_number(),
            fragment_starting_num,
            fragments_in_submessage,
            fragment_size,
            sample_size,
        );

        // Zero-copy: directly use the Arc<[u8]> fragment data
        data_frag.add_serialized_data(fragment_data);

        let data_frag_submessage = Submessage {
            header: SubmessageHeader::new(
                SubmessageId::DATA_FRAG,
                data_frag_header_flag.flag,
                data_frag.octets_to_next_header(),
            ),
            body: SubmessageBody::DataFrag(data_frag),
        };

        rtps_message.add_submessage(data_frag_submessage);

        // Add heartbeat submessage if heartbeat info is provided
        if let Some((heartbeat_count, first_sn, last_sn, final_flag, liveliness_flag)) =
            heartbeat_info
        {
            let heartbeat_submessage = SubmessageCreator::create_heartbeat_submessage(
                heartbeat_count,
                reader_entity_id,
                writer_entity_id,
                first_sn,
                last_sn,
                final_flag,
                liveliness_flag,
            )?;
            rtps_message.add_submessage(heartbeat_submessage);
        }

        // Serialize the complete RTPS message
        match rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian) {
            Ok(buffer) => Ok(Arc::new(buffer)),
            Err(e) => Err(Box::new(e)),
        }
    }

    pub(crate) fn create_gap_msg_consecutive(
        local_participant_guid: Guid,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        gap_start: SequenceNumber,
        gap_end: SequenceNumber,
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>> {
        let mut rtps_message = RtpsMessage::new(Header::new(local_participant_guid.prefix()));

        rtps_message
            .add_submessage(SubmessageCreator::create_info_dst_submessage(remote_guid.prefix()));
        rtps_message.add_submessage(SubmessageCreator::create_gap_submessage_consecutive(
            reader_entity_id,
            writer_entity_id,
            gap_start,
            gap_end,
        )?);

        let serialized_message = rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?;

        Ok(Arc::new(serialized_message))
    }

    pub(crate) fn create_multiple_gap_msgs(
        local_participant_guid: Guid,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        gap_list: &mut Vec<SequenceNumber>,
    ) -> Result<Vec<Arc<Vec<u8>>>, Box<dyn std::error::Error>> {
        let mut gap_rtps_messages = Vec::new();
        gap_list.sort();

        while !gap_list.is_empty() {
            let mut rtps_message = RtpsMessage::new(Header::new(local_participant_guid.prefix()));

            rtps_message.add_submessage(SubmessageCreator::create_info_dst_submessage(
                remote_guid.prefix(),
            ));
            rtps_message.add_submessage(SubmessageCreator::create_gap_submessage(
                reader_entity_id,
                writer_entity_id,
                gap_list,
            )?);

            let serialized_message =
                rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?;
            gap_rtps_messages.push(Arc::new(serialized_message));
        }

        Ok(gap_rtps_messages)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_nackfrag_msg(
        reader_guid: Guid,
        writer_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        writer_sn: SequenceNumber,
        fragment_number_state: FragmentNumberSet,
        nackfrag_count: i32,
        acknack_info: Option<(i32, SequenceNumber, Vec<SequenceNumber>)>,
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>> {
        let mut rtps_message = RtpsMessage::new(Header::new(reader_guid.prefix()));

        rtps_message
            .add_submessage(SubmessageCreator::create_info_dst_submessage(writer_guid.prefix()));
        let timestamp = Utc::now();
        rtps_message.add_submessage(SubmessageCreator::create_info_ts_submessage(timestamp));

        let nackfrag_submessage = SubmessageCreator::create_nackfrag_submessage(
            reader_entity_id,
            writer_entity_id,
            writer_sn,
            fragment_number_state,
            nackfrag_count,
        )?;

        rtps_message.add_submessage(nackfrag_submessage);

        if let Some((acknack_count, bitmap_base, missing_changes)) = acknack_info {
            let acknack_submessage = SubmessageCreator::create_acknack_submessage(
                reader_entity_id,
                writer_entity_id,
                missing_changes,
                acknack_count,
                bitmap_base,
                false,
            )?;
            rtps_message.add_submessage(acknack_submessage);
        }

        // Serialize the complete RTPS message
        match rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian) {
            Ok(rtps_message_bytes) => Ok(Arc::new(rtps_message_bytes)),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Create KeyHash inline QoS parameter
    pub(crate) fn create_key_hash_parameter(key_hash: &[u8; 16]) -> Parameter {
        Parameter::new(ParameterId::PidKeyHash, key_hash.to_vec())
    }

    /// Create StatusInfo inline QoS parameter
    pub(crate) fn create_status_info_parameter(flags: &[u8; 4]) -> Parameter {
        Parameter::new(ParameterId::PidStatusInfo, flags.to_vec())
    }
}
