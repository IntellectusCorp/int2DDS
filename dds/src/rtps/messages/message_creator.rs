//! RTPS message creation utilities.
//!
//! This module provides functions for creating RTPS messages from submessages,
//! including headers, submessage composition, and serialization for transmission
//! over the network.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use log::{debug, info};
use smallvec::SmallVec;
use speedy::{Endianness, Writable};

use crate::rtps::{
    builtin::data::content_filtered_topic::ContentFilterInfo,
    common::{
        entity_id::EntityId,
        guid::{Guid, GuidPrefix},
        parameters::{Parameter, ParameterId, ParameterList, StatusInfo},
        rtps_error_code::RtpsResult,
        sequence::SequenceNumber,
        types::{ChangeKind, SubmessagePayload},
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
use crate::serialize::pl_cdr::InlineQosParameters;

pub(crate) struct MessageCreator {}

impl MessageCreator {
    pub(crate) fn create_spdp_msg(
        participant: Arc<Participant>,
    ) -> RtpsResult<Arc<RtpsMessage<'static>>> {
        let (participant_guid, _) = {
            let local_participant_data = participant.local_participant_proxy_data();
            (
                local_participant_data.participant_guid(),
                local_participant_data.available_builtin_endpoints(),
            )
        };

        info!("Creating basic SPDP message for participant: {}", participant_guid);

        let spdp_message = SpdpMessage::new(participant, None)?;
        let rtps_message = spdp_message.rtps_message();

        Ok(rtps_message)
    }

    pub(crate) fn create_spdp_msg_with_inline_qos(
        participant: Arc<Participant>,
    ) -> RtpsResult<Arc<RtpsMessage<'static>>> {
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
        local_guid_prefix: GuidPrefix,
        target_guid_prefix: GuidPrefix,
        heartbeat_count: u32,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
        final_flag: bool,
        liveliness_flag: bool,
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>> {
        let mut rtps_message = RtpsMessage::new(Header::new(local_guid_prefix));

        rtps_message
            .add_submessage(SubmessageCreator::create_info_dst_submessage(target_guid_prefix));
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
        acknack_count: u32,
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
        cache_change: &CacheChange,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        heartbeat_info: Option<(u32, SequenceNumber, SequenceNumber, bool, bool)>,
        use_inline_qos: bool,
        content_filter_info: Option<ContentFilterInfo>,
        send_buffer: &mut Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Creating RTPS message from cache change: {}", cache_change);

        let mut rtps_message = RtpsMessage::new(Header::new(cache_change.writer_guid().prefix()));

        rtps_message
            .add_submessage(SubmessageCreator::create_info_dst_submessage(remote_guid.prefix()));
        let timestamp = Utc::now();
        rtps_message.add_submessage(SubmessageCreator::create_info_ts_submessage(timestamp));

        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::DATA);
        match cache_change.kind() {
            ChangeKind::Alive | ChangeKind::AliveFiltered => {
                // Payload-less Alive changes (coherent set end markers) carry no DataFlag.
                if !cache_change.data_value().is_empty() {
                    data_header_flag.add_flag(SubmessageFlagType::DataFlag, SubmessageId::DATA);
                }
            }
            ChangeKind::NotAliveDisposed
            | ChangeKind::NotAliveUnregistered
            | ChangeKind::NotAliveDisposedUnregistered => {
                if !cache_change.data_value().is_empty() {
                    data_header_flag.add_flag(SubmessageFlagType::KeyFlag, SubmessageId::DATA);
                }
            }
        }

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

            // Attach per-sample coherent/group presentation metadata.
            let inline = cache_change.presentation_info();
            if let Some(sn) = inline.coherent_set {
                param_list.set_coherent_set(sn);
            }
            if let Some(sn) = inline.group_seq_num {
                param_list.set_group_seq_num(sn);
            }
            if let Some(sn) = inline.group_coherent_set {
                param_list.set_group_coherent_set(sn);
            }

            if !param_list.parameters().is_empty() {
                data_header_flag.add_flag(SubmessageFlagType::InlineQosFlag, SubmessageId::DATA);
                data.set_inline_qos_list(param_list);
            }
        }

        data.add_serialized_data(SubmessagePayload::Borrowed(cache_change.data_value()));
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

        // Serialize directly into the reusable Vec via its `Write` impl.
        // Avoids the `bytes_needed` pre-pass and the zero-fill from `resize(_, 0)`.
        send_buffer.clear();
        rtps_message.write_to_stream_with_ctx(Endianness::LittleEndian, &mut *send_buffer)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_data_frag_msg(
        cache_change: &CacheChange,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        fragment_starting_num: u32,
        fragments_in_submessage: u16,
        fragment_size: u16,
        sample_size: u32,
        fragment_data: &[u8],
        heartbeat_info: Option<(u32, SequenceNumber, SequenceNumber, bool, bool)>,
        timestamp: DateTime<Utc>,
        send_buffer: &mut Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        data_frag.add_serialized_data(SubmessagePayload::Borrowed(fragment_data));

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

        // Serialize directly into the reusable Vec via its `Write` impl.
        send_buffer.clear();
        rtps_message.write_to_stream_with_ctx(Endianness::LittleEndian, &mut *send_buffer)?;
        Ok(())
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
        gap_list.sort();
        if gap_list.is_empty() {
            return Ok(Vec::new());
        }

        // submessageId + flags + submessageLength, ahead of the body length that
        // submessage_length() reports.
        const SUBMESSAGE_HEADER_LEN: usize = 4;

        // INT2DDS_DATA_FRAG_SIZE bounds the serialized payload packed into one datagram.
        let max_fragment_payload = crate::common::env::get_data_frag_size_override()
            .filter(|&size| (1..=65000).contains(&size))
            .unwrap_or(65000) as usize;

        let open_datagram = || {
            let mut rtps_message = RtpsMessage::new(Header::new(local_participant_guid.prefix()));
            rtps_message.add_submessage(SubmessageCreator::create_info_dst_submessage(
                remote_guid.prefix(),
            ));
            rtps_message
        };

        let mut gap_rtps_messages = Vec::new();
        let mut rtps_message = open_datagram();
        let mut packed_len = 0;

        while !gap_list.is_empty() {
            // Build the next 256-window of gap sequence numbers.
            let submessage = SubmessageCreator::create_gap_submessage(
                reader_entity_id,
                writer_entity_id,
                gap_list,
            )?;
            let submessage_len =
                SUBMESSAGE_HEADER_LEN + submessage.header.submessage_length() as usize;

            // Start a fresh datagram before it would exceed the payload budget.
            if packed_len > 0 && packed_len + submessage_len > max_fragment_payload {
                gap_rtps_messages
                    .push(Arc::new(rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?));
                rtps_message = open_datagram();
                packed_len = 0;
            }

            rtps_message.add_submessage(submessage);
            packed_len += submessage_len;
        }

        // Send the last datagram.
        gap_rtps_messages
            .push(Arc::new(rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?));

        Ok(gap_rtps_messages)
    }

    // Split missing fragments into 256-wide windows and pack as many NACK_FRAG
    // submessages as fit under INT2DDS_DATA_FRAG_SIZE into each datagram.
    pub(crate) fn create_multiple_nackfrag_msgs(
        reader_guid: Guid,
        writer_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        writer_sn: SequenceNumber,
        missing_fragments: &mut Vec<u32>,
        first_nackfrag_count: u32,
    ) -> Result<Vec<Arc<Vec<u8>>>, Box<dyn std::error::Error>> {
        missing_fragments.sort_unstable();
        missing_fragments.dedup();
        if missing_fragments.is_empty() {
            return Ok(Vec::new());
        }

        // submessageId + flags + submessageLength, ahead of the body length that
        // submessage_length() reports.
        const SUBMESSAGE_HEADER_LEN: usize = 4;

        // INT2DDS_DATA_FRAG_SIZE bounds the serialized payload packed into one datagram.
        let max_fragment_payload = crate::common::env::get_data_frag_size_override()
            .filter(|&size| (1..=65000).contains(&size))
            .unwrap_or(65000) as usize;

        let open_datagram = || {
            let mut rtps_message = RtpsMessage::new(Header::new(reader_guid.prefix()));
            rtps_message.add_submessage(SubmessageCreator::create_info_dst_submessage(
                writer_guid.prefix(),
            ));
            rtps_message.add_submessage(SubmessageCreator::create_info_ts_submessage(Utc::now()));
            rtps_message
        };

        let mut messages = Vec::new();
        let mut nackfrag_count = first_nackfrag_count;
        let mut rtps_message = open_datagram();
        let mut packed_len = 0;

        while !missing_fragments.is_empty() {
            // Build the next 256-window of missing fragments into a NACK_FRAG.
            let fragment_number_state =
                SubmessageCreator::calculate_nackfrag_fns_from_vec(missing_fragments);
            let submessage = SubmessageCreator::create_nackfrag_submessage(
                reader_entity_id,
                writer_entity_id,
                writer_sn,
                fragment_number_state,
                nackfrag_count,
            )?;
            nackfrag_count = nackfrag_count.wrapping_add(1);
            let submessage_len =
                SUBMESSAGE_HEADER_LEN + submessage.header.submessage_length() as usize;

            // Start a fresh datagram before it would exceed the payload budget.
            if packed_len > 0 && packed_len + submessage_len > max_fragment_payload {
                messages
                    .push(Arc::new(rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?));
                rtps_message = open_datagram();
                packed_len = 0;
            }

            rtps_message.add_submessage(submessage);
            packed_len += submessage_len;
        }

        // Send the last datagram.
        messages.push(Arc::new(rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?));

        Ok(messages)
    }

    /// Create KeyHash inline QoS parameter
    pub(crate) fn create_key_hash_parameter(key_hash: &[u8; 16]) -> Parameter {
        Parameter::new(ParameterId::PidKeyHash, SmallVec::from_slice(key_hash))
    }

    /// Create StatusInfo inline QoS parameter
    pub(crate) fn create_status_info_parameter(flags: &[u8; 4]) -> Parameter {
        Parameter::new(ParameterId::PidStatusInfo, SmallVec::from_slice(flags))
    }
}
