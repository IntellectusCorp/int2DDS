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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use bytes::Bytes;

    use super::*;
    use crate::rtps::common::entity_kind::EntityKind;
    use crate::rtps::messages::message_receiver::{MessageReceiver, TypedSubmessage};

    const LOCAL_PREFIX: GuidPrefix = [1; 12];
    const DST_PREFIX: GuidPrefix = [2; 12];

    fn reply(reader: u8, writer: u8, missing: Vec<i64>) -> AckNackRequest {
        AckNackRequest {
            reader_entity_id: EntityId::new(
                [0, 0, reader],
                EntityKind::USER_DEFINED_READER_WITH_KEY,
            ),
            writer_entity_id: EntityId::new(
                [0, 0, writer],
                EntityKind::USER_DEFINED_WRITER_WITH_KEY,
            ),
            missing_changes: missing.into_iter().map(SequenceNumber::from_i64).collect(),
            acknack_count: 1,
            bitmap_base: SequenceNumber::from_i64(1),
            is_preemptive: false,
        }
    }

    // Several replies must survive one message intact: a peer reads them back as
    // separate ACKNACKs, each still addressed to its own reader/writer pair.
    #[test]
    fn batched_acknacks_round_trip_as_separate_submessages() {
        let local = Guid::new(LOCAL_PREFIX, EntityId::PARTICIPANT);
        let dst_prefix = DST_PREFIX;
        let replies = vec![
            reply(0x10, 0xA0, vec![1, 2]),
            reply(0x11, 0xA1, vec![]),
            reply(0x12, 0xA2, vec![7]),
        ];

        let buffer = MessageCreator::create_acknack_msg_multi(local, dst_prefix, &replies).unwrap();

        let addr = "127.0.0.1:7400".parse().unwrap();
        let mut receiver = MessageReceiver::new(dst_prefix, &addr);
        receiver.init(&Bytes::copy_from_slice(&buffer)).unwrap();

        let parsed: Vec<_> = receiver
            .parse_submessages()
            .into_iter()
            .filter_map(|submessage| match submessage {
                TypedSubmessage::AckNack(_, acknack) => Some(acknack.reader_id),
                _ => None,
            })
            .collect();

        assert_eq!(parsed, replies.iter().map(|reply| reply.reader_entity_id).collect::<Vec<_>>(),);
        assert!(receiver.is_dst_me(dst_prefix), "INFO_DST must address the writer's participant");
    }

    // A single reply is the ordinary case and must not gain anything from the batched
    // builder - one ACKNACK in, one ACKNACK out.
    #[test]
    fn a_lone_acknack_still_produces_one_submessage() {
        let local = Guid::new(LOCAL_PREFIX, EntityId::PARTICIPANT);
        let dst_prefix = DST_PREFIX;

        let buffer = MessageCreator::create_acknack_msg_multi(
            local,
            dst_prefix,
            &[reply(0x10, 0xA0, vec![1])],
        )
        .unwrap();

        let addr = "127.0.0.1:7400".parse().unwrap();
        let mut receiver = MessageReceiver::new(dst_prefix, &addr);
        receiver.init(&Bytes::copy_from_slice(&buffer)).unwrap();

        let count = receiver
            .parse_submessages()
            .iter()
            .filter(|submessage| matches!(submessage, TypedSubmessage::AckNack(..)))
            .count();

        assert_eq!(count, 1);
    }

    // Env is process-global: serialize every INT2DDS_MAX_MESSAGE_SIZE mutation.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // get_max_message_size resolves the env var, clamps to 1..=65000, and defaults to 65000.
    #[test]
    fn max_message_size_resolves_and_clamps() {
        let _guard = lock_env();
        let key = "INT2DDS_MAX_MESSAGE_SIZE";

        unsafe { std::env::remove_var(key) };
        assert_eq!(crate::common::env::get_max_message_size(), 65_000);

        unsafe { std::env::set_var(key, "14720") };
        assert_eq!(crate::common::env::get_max_message_size(), 14_720);

        unsafe { std::env::set_var(key, "0") };
        assert_eq!(crate::common::env::get_max_message_size(), 65_000);

        unsafe { std::env::set_var(key, "70000") };
        assert_eq!(crate::common::env::get_max_message_size(), 65_000);

        unsafe { std::env::set_var(key, "not-a-number") };
        assert_eq!(crate::common::env::get_max_message_size(), 65_000);

        unsafe { std::env::remove_var(key) };
    }

    // Every batched GAP and NACK_FRAG datagram stays within INT2DDS_MAX_MESSAGE_SIZE,
    // headers included, once the batch is large enough to span several datagrams.
    #[test]
    fn batched_datagrams_stay_within_max_message_size() {
        let _guard = lock_env();
        let key = "INT2DDS_MAX_MESSAGE_SIZE";
        let max: usize = 400;
        unsafe { std::env::set_var(key, max.to_string()) };

        let local = Guid::new(LOCAL_PREFIX, EntityId::PARTICIPANT);
        let remote = Guid::new(DST_PREFIX, EntityId::PARTICIPANT);
        let reader_entity_id =
            EntityId::new([0, 0, 0x10], EntityKind::USER_DEFINED_READER_WITH_KEY);
        let writer_entity_id =
            EntityId::new([0, 0, 0xA0], EntityKind::USER_DEFINED_WRITER_WITH_KEY);

        // Scatter beyond 256 apart so each gap needs its own window instead of
        // collapsing into one contiguous GAP submessage.
        let mut gap_list: Vec<SequenceNumber> =
            (0..80).map(|i| SequenceNumber::from_i64(i * 500 + 1)).collect();
        let gap_msgs = MessageCreator::create_multiple_gap_msgs(
            local,
            remote,
            reader_entity_id,
            writer_entity_id,
            &mut gap_list,
        )
        .unwrap();

        assert!(gap_msgs.len() > 1, "gap batch must span several datagrams");
        for datagram in &gap_msgs {
            assert!(datagram.len() <= max, "gap datagram {} exceeds {}", datagram.len(), max);
        }

        let mut missing_fragments: Vec<u32> = (0..80).map(|i| i * 500 + 1).collect();
        let (nackfrag_msgs, _) = MessageCreator::create_multiple_nackfrag_msgs(
            local,
            remote,
            reader_entity_id,
            writer_entity_id,
            SequenceNumber::from_i64(1),
            &mut missing_fragments,
            0,
        )
        .unwrap();

        assert!(nackfrag_msgs.len() > 1, "nackfrag batch must span several datagrams");
        for datagram in &nackfrag_msgs {
            assert!(datagram.len() <= max, "nackfrag datagram {} exceeds {}", datagram.len(), max);
        }

        unsafe { std::env::remove_var(key) };
    }
}

/// One reader's reply to one remote writer, worked out but not yet on the wire.
///
/// Holding the reply as data rather than a finished datagram is what lets several of
/// them share one message - see [`MessageCreator::create_acknack_msg_multi`].
pub(crate) struct AckNackRequest {
    pub(crate) reader_entity_id: EntityId,
    pub(crate) writer_entity_id: EntityId,
    pub(crate) missing_changes: Vec<SequenceNumber>,
    pub(crate) acknack_count: u32,
    pub(crate) bitmap_base: SequenceNumber,
    pub(crate) is_preemptive: bool,
}

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

    /// One RTPS message carrying several ACKNACKs to the **same** remote participant.
    ///
    /// INFO_DST addresses a participant, so every writer behind one GuidPrefix can share
    /// a single header + INFO_DST and differ only in its ACKNACK submessage. This is the
    /// return path of a batched DATA heartbeat: every reader behind this participant
    /// answers it, and without this the replies would go back one datagram at a time.
    pub(crate) fn create_acknack_msg_multi(
        local_participant_guid: Guid,
        dst_prefix: GuidPrefix,
        acknacks: &[AckNackRequest],
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>> {
        let mut rtps_message = RtpsMessage::new(Header::new(local_participant_guid.prefix()));
        rtps_message.add_submessage(SubmessageCreator::create_info_dst_submessage(dst_prefix));

        for acknack in acknacks {
            rtps_message.add_submessage(SubmessageCreator::create_acknack_submessage(
                acknack.reader_entity_id,
                acknack.writer_entity_id,
                acknack.missing_changes.clone(),
                acknack.acknack_count,
                acknack.bitmap_base,
                acknack.is_preemptive,
            )?);
        }

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

        let data_submessage = Self::build_data_submessage(
            cache_change,
            reader_entity_id,
            writer_entity_id,
            use_inline_qos,
            content_filter_info,
        );

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

    // Pack several DATA changes bound for the same reader into as few datagrams as fit under
    // INT2DDS_MAX_MESSAGE_SIZE. Each datagram is Header, INFO_DST, INFO_TS, then DATA submessages.
    pub(crate) fn create_multiple_data_msgs(
        local_participant_guid: Guid,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        cache_changes: &[Arc<CacheChange>],
        use_inline_qos: bool,
    ) -> Result<Vec<Arc<Vec<u8>>>, Box<dyn std::error::Error>> {
        if cache_changes.is_empty() {
            return Ok(Vec::new());
        }

        // submessageId + flags + submessageLength, ahead of the body length that
        // submessage_length() reports.
        const SUBMESSAGE_HEADER_LEN: usize = 4;

        let max_message_size = crate::common::env::get_max_message_size();

        let open_datagram = || {
            let mut rtps_message = RtpsMessage::new(Header::new(local_participant_guid.prefix()));
            rtps_message.add_submessage(SubmessageCreator::create_info_dst_submessage(
                remote_guid.prefix(),
            ));
            rtps_message.add_submessage(SubmessageCreator::create_info_ts_submessage(Utc::now()));
            rtps_message
        };

        // RTPS Header (20) + INFO_DST (4 + 12) + INFO_TS (4 + 8) that open_datagram writes
        // before the batched DATA submessages.
        const DATAGRAM_BASE_LEN: usize =
            20 + SUBMESSAGE_HEADER_LEN + 12 + SUBMESSAGE_HEADER_LEN + 8;

        let mut messages = Vec::new();
        let mut rtps_message = open_datagram();
        let mut packed_len = DATAGRAM_BASE_LEN;

        for cache_change in cache_changes {
            let data = Self::build_data_submessage(
                cache_change,
                reader_entity_id,
                writer_entity_id,
                use_inline_qos,
                None,
            );
            let data_len = SUBMESSAGE_HEADER_LEN + data.header.submessage_length() as usize;

            // Start a fresh datagram before this DATA would exceed the message-size budget.
            // A single change larger than the budget still emits one oversized datagram,
            // matching create_data_msg. SEDP has no DATA_FRAG path.
            if packed_len > DATAGRAM_BASE_LEN && packed_len + data_len > max_message_size {
                messages
                    .push(Arc::new(rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?));
                rtps_message = open_datagram();
                packed_len = DATAGRAM_BASE_LEN;
            }

            rtps_message.add_submessage(data);
            packed_len += data_len;
        }

        messages.push(Arc::new(rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?));

        Ok(messages)
    }

    /// The DATA submessage for one reader.
    fn build_data_submessage<'a>(
        cache_change: &'a CacheChange,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        use_inline_qos: bool,
        content_filter_info: Option<ContentFilterInfo>,
    ) -> Submessage<'a> {
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
        Submessage {
            header: SubmessageHeader::new(
                SubmessageId::DATA,
                data_header_flag.flag,
                data.octets_to_next_header(),
            ),
            body: SubmessageBody::Data(data),
        }
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

        // INT2DDS_MAX_MESSAGE_SIZE bounds the datagram that batched submessages are packed into.
        let max_message_size = crate::common::env::get_max_message_size();

        let open_datagram = || {
            let mut rtps_message = RtpsMessage::new(Header::new(local_participant_guid.prefix()));
            rtps_message.add_submessage(SubmessageCreator::create_info_dst_submessage(
                remote_guid.prefix(),
            ));
            rtps_message
        };

        // RTPS Header (20) + INFO_DST (4 + 12) that open_datagram writes before the batched submessages.
        const DATAGRAM_BASE_LEN: usize = 20 + SUBMESSAGE_HEADER_LEN + 12;

        let mut gap_rtps_messages = Vec::new();
        let mut rtps_message = open_datagram();
        let mut packed_len = DATAGRAM_BASE_LEN;

        while !gap_list.is_empty() {
            // Build the next 256-window of gap sequence numbers.
            let submessage = SubmessageCreator::create_gap_submessage(
                reader_entity_id,
                writer_entity_id,
                gap_list,
            )?;
            let submessage_len =
                SUBMESSAGE_HEADER_LEN + submessage.header.submessage_length() as usize;

            // Start a fresh datagram before it would exceed the message-size budget.
            if packed_len > DATAGRAM_BASE_LEN && packed_len + submessage_len > max_message_size {
                gap_rtps_messages
                    .push(Arc::new(rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?));
                rtps_message = open_datagram();
                packed_len = DATAGRAM_BASE_LEN;
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
    // submessages as fit under INT2DDS_MAX_MESSAGE_SIZE into each datagram.
    pub(crate) fn create_multiple_nackfrag_msgs(
        reader_guid: Guid,
        writer_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        writer_sn: SequenceNumber,
        missing_fragments: &mut Vec<u32>,
        first_nackfrag_count: u32,
    ) -> Result<(Vec<Arc<Vec<u8>>>, u32), Box<dyn std::error::Error>> {
        missing_fragments.sort_unstable();
        missing_fragments.dedup();
        if missing_fragments.is_empty() {
            return Ok((Vec::new(), 0));
        }

        // submessageId + flags + submessageLength, ahead of the body length that
        // submessage_length() reports.
        const SUBMESSAGE_HEADER_LEN: usize = 4;

        // INT2DDS_MAX_MESSAGE_SIZE bounds the datagram that batched submessages are packed into.
        let max_message_size = crate::common::env::get_max_message_size();

        let open_datagram = || {
            let mut rtps_message = RtpsMessage::new(Header::new(reader_guid.prefix()));
            rtps_message.add_submessage(SubmessageCreator::create_info_dst_submessage(
                writer_guid.prefix(),
            ));
            rtps_message.add_submessage(SubmessageCreator::create_info_ts_submessage(Utc::now()));
            rtps_message
        };

        // RTPS Header (20) + INFO_DST (4 + 12) + INFO_TS (4 + 8) that open_datagram writes first.
        const DATAGRAM_BASE_LEN: usize =
            20 + (SUBMESSAGE_HEADER_LEN + 12) + (SUBMESSAGE_HEADER_LEN + 8);

        let mut messages = Vec::new();
        let mut nackfrag_count = first_nackfrag_count;
        let mut rtps_message = open_datagram();
        let mut packed_len = DATAGRAM_BASE_LEN;

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

            // Start a fresh datagram before it would exceed the message-size budget.
            if packed_len > DATAGRAM_BASE_LEN && packed_len + submessage_len > max_message_size {
                messages
                    .push(Arc::new(rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?));
                rtps_message = open_datagram();
                packed_len = DATAGRAM_BASE_LEN;
            }

            rtps_message.add_submessage(submessage);
            packed_len += submessage_len;
        }

        // Send the last datagram.
        messages.push(Arc::new(rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian)?));

        // Each NACK_FRAG submessage consumed one count; the caller advances its
        // counter by this amount so the next request stays monotonic.
        let consumed_count = nackfrag_count.wrapping_sub(first_nackfrag_count);

        Ok((messages, consumed_count))
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
