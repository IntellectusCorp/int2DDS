#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use log::debug;

use crate::{
    common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
    core::time::Duration,
    infrastructure::status::{StatusInfo, StatusKind},
    rtps::{
        common::{
            entity_id::EntityId,
            guid::Guid,
            locator::Locator,
            sequence::{FragmentNumberSet, SequenceNumber},
        },
        entities::history::cache_change::CacheChange,
    },
};

#[derive(Clone)]
pub(crate) struct WriterProxy {
    remote_writer_guid: Guid,
    remote_group_entity_id: EntityId,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    data_max_size_serialized: u32,
    changes_from_writer: BTreeMap<SequenceNumber, ChangeFromWriter>,
    acknack_count: u32,
    nackfrag_count: u32,
    expected_sn: SequenceNumber, // Expected next sequence number from writer
    last_heartbeat_count: Option<u32>,
    buffered_change: BTreeSet<CacheChange>, // Changes that reader has not processed yet
    publication_builtin_topic_data: PublicationBuiltinTopicData,
    #[allow(clippy::type_complexity)]
    status_callback:
        Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>,
}

impl PartialEq for WriterProxy {
    fn eq(&self, other: &Self) -> bool {
        self.remote_writer_guid == other.remote_writer_guid
    }
}

impl WriterProxy {
    #[allow(clippy::type_complexity)]
    pub(crate) fn new(
        remote_writer_guid: Guid,
        remote_group_entity_id: EntityId,
        unicast_locator_list: Vec<Locator>,
        multicast_locator_list: Vec<Locator>,
        data_max_size_serialized: u32,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
        status_callback: Arc<
            Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>,
        >,
    ) -> Self {
        Self {
            remote_writer_guid,
            remote_group_entity_id,
            unicast_locator_list,
            multicast_locator_list,
            data_max_size_serialized,
            changes_from_writer: BTreeMap::new(), // Initialize with empty vector
            acknack_count: 0,
            nackfrag_count: 0,
            expected_sn: SequenceNumber::UNKNOWN,
            last_heartbeat_count: None,
            buffered_change: BTreeSet::new(),
            publication_builtin_topic_data,
            status_callback,
        }
    }

    pub(crate) fn increase_acknack_count(&mut self) {
        self.acknack_count = self.acknack_count.wrapping_add(1);
    }

    pub(crate) fn acknack_count(&self) -> u32 {
        self.acknack_count
    }

    pub(crate) fn increase_nackfrag_count(&mut self) {
        self.nackfrag_count = self.nackfrag_count.wrapping_add(1);
    }

    pub(crate) fn nackfrag_count(&self) -> u32 {
        self.nackfrag_count
    }

    pub(crate) fn expected_sn(&self) -> SequenceNumber {
        self.expected_sn
    }

    pub(crate) fn last_heartbeat_count(&self) -> Option<u32> {
        self.last_heartbeat_count
    }

    pub(crate) fn set_last_heartbeat_count(&mut self, count: u32) {
        self.last_heartbeat_count = Some(count);
    }

    pub(crate) fn add_new_changes_from_writer(&mut self, change_from_writer: ChangeFromWriter) {
        self.changes_from_writer.insert(change_from_writer.sequence_number, change_from_writer);
    }

    pub(crate) fn add_buffered_change(&mut self, change: CacheChange) {
        self.buffered_change.insert(change);
    }

    pub(crate) fn flush_buffered_changes(&mut self) -> Vec<CacheChange> {
        let mut flushed_changes = Vec::new();

        while let Some(change) = self.buffered_change.first() {
            if change.sequence_number <= self.expected_sn {
                let change = self.buffered_change.pop_first().unwrap();
                flushed_changes.push(change);
                self.increment_expected_sn();
            } else {
                break;
            }
        }

        flushed_changes
    }

    pub(crate) fn publication_builtin_topic_data(&self) -> PublicationBuiltinTopicData {
        self.publication_builtin_topic_data.clone()
    }

    pub(crate) fn set_publication_builtin_topic_data(&mut self, data: PublicationBuiltinTopicData) {
        self.publication_builtin_topic_data = data;
    }

    pub(crate) fn get_ownership_strength(&self) -> i32 {
        self.publication_builtin_topic_data.ownership_strength().value
    }

    pub(crate) fn get_lifespan_duration(&self) -> Duration {
        self.publication_builtin_topic_data.lifespan().duration
    }

    pub(crate) fn has_fragmented_changes(
        &self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> bool {
        if last_sn < first_sn {
            return false;
        }

        self.changes_from_writer.range(first_sn..=last_sn).any(|(_, change)| {
            change.fragment_info.as_ref().map(|info| !info.is_complete).unwrap_or(false)
        })
    }

    pub(crate) fn still_missing_fragments(&self, seq_num: SequenceNumber) -> bool {
        self.changes_from_writer
            .get(&seq_num)
            .and_then(|change| change.fragment_info.as_ref())
            .is_some_and(|info| !info.is_complete)
    }

    pub(crate) fn all_fragments_received(&self, seq_num: SequenceNumber) -> bool {
        self.changes_from_writer
            .get(&seq_num)
            .and_then(|change| change.fragment_info.as_ref())
            .is_some_and(|info| info.is_complete)
    }

    pub(crate) fn calculate_missing_fragments(
        &self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> Option<FragmentNumberSet> {
        if last_sn < first_sn {
            return None;
        }

        self.changes_from_writer.range(first_sn..=last_sn).find_map(|(_, change)| {
            change.fragment_info.as_ref().and_then(|info| {
                if info.is_complete {
                    return None;
                }

                let mut missing_fragments = Vec::new();
                let mut base_fragment = None;

                for fragment_num in 1..=info.total_fragments {
                    if !info.received_fragments.contains(&fragment_num) {
                        if base_fragment.is_none() {
                            base_fragment = Some(fragment_num);
                        }
                        missing_fragments.push(fragment_num);
                    }
                }

                if !missing_fragments.is_empty() {
                    base_fragment.map(|base| FragmentNumberSet::from_vec(base, missing_fragments))
                } else {
                    None
                }
            })
        })
    }

    pub(crate) fn remote_writer_guid(&self) -> Guid {
        self.remote_writer_guid
    }

    pub(crate) fn unicast_locator_list(&self) -> Vec<Locator> {
        self.unicast_locator_list.clone()
    }

    pub(crate) fn increment_expected_sn(&mut self) {
        self.expected_sn += 1;
    }

    pub(crate) fn set_expected_sn(&mut self, seq_num: SequenceNumber) {
        self.expected_sn = seq_num;
    }

    /// Get the maximum available sequence number from the writer, which indicates there are no missing or unknown changes before this sequence number.
    pub(crate) fn available_changes_max(&self) -> SequenceNumber {
        self.changes_from_writer
            .iter()
            .take_while(|(_, v)| {
                v.status == ChangeFromWriterStatusKind::Received
                    || v.status == ChangeFromWriterStatusKind::NotAvailable(NotAvailable::Removed)
            })
            .last()
            .map(|(k, _)| *k)
            .unwrap_or(SequenceNumber::UNKNOWN)
    }

    /// Get the maximum sequence number from changes_from_writer (regardless of status).
    pub(crate) fn changes_from_writer_max(&self) -> SequenceNumber {
        self.changes_from_writer
            .last_key_value()
            .map(|(k, _)| *k)
            .unwrap_or(SequenceNumber::UNKNOWN)
    }

    pub(crate) fn irrelevant_change_set(&mut self, a_seq_num: SequenceNumber) {
        let change = self.changes_from_writer.entry(a_seq_num).or_insert(ChangeFromWriter {
            sequence_number: a_seq_num,
            status: ChangeFromWriterStatusKind::Received,
            is_relevant: false,
            fragment_info: None,
        });

        change.status = ChangeFromWriterStatusKind::Received;
        change.is_relevant = false;
    }

    #[allow(clippy::unnecessary_filter_map)]
    pub(crate) fn lost_changes_update(&mut self, first_available_seq_num: SequenceNumber) {
        self.expected_sn = max(self.expected_sn, first_available_seq_num);

        let keys: Vec<SequenceNumber> = self
            .changes_from_writer
            .range(..first_available_seq_num)
            .filter_map(|(seq_num, change_from_writer)| {
                if change_from_writer.status == ChangeFromWriterStatusKind::Missing {
                    debug!("Sample Lost!: {:?}", change_from_writer.sequence_number);
                    self.on_sample_lost();
                }
                Some(*seq_num)
            })
            .collect();

        for key in keys {
            self.changes_from_writer.remove(&key);
        }
    }

    pub(crate) fn process_heartbeat(
        &mut self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> Vec<SequenceNumber> {
        self.lost_changes_update(first_sn);

        self.update_changes_for_heartbeat_range(first_sn, last_sn);

        self.missing_changes_for_heartbeat(first_sn, last_sn)
    }

    fn update_changes_for_heartbeat_range(
        &mut self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) {
        if last_sn < first_sn {
            return;
        }

        let mut current_sn = first_sn;
        while current_sn <= last_sn {
            let change = self.changes_from_writer.entry(current_sn).or_insert(ChangeFromWriter {
                sequence_number: current_sn,
                status: ChangeFromWriterStatusKind::Missing,
                is_relevant: true,
                fragment_info: None,
            });
            if change.status == ChangeFromWriterStatusKind::Unknown {
                change.status = ChangeFromWriterStatusKind::Missing;
            }
            current_sn = current_sn.next();
        }
    }

    pub(crate) fn missing_changes_for_heartbeat(
        &self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> Vec<SequenceNumber> {
        if last_sn < first_sn {
            return Vec::new();
        }

        let mut missing = Vec::new();
        let mut current_sn = first_sn;

        while current_sn <= last_sn {
            // Check change state for the corresponding sequence number
            match self.changes_from_writer.get(&current_sn) {
                Some(change) if change.status != ChangeFromWriterStatusKind::Missing => {}
                _ => {
                    missing.push(current_sn);
                }
            }
            current_sn = current_sn.next();
        }

        missing
    }

    /// Update change state to Received when Data submessage is received
    pub(crate) fn mark_change_received(
        &mut self,
        seq_num: SequenceNumber,
        fragment_info: Option<FragmentInfo>,
    ) {
        self.changes_from_writer
            .entry(seq_num)
            .or_insert_with(|| ChangeFromWriter {
                sequence_number: seq_num,
                status: ChangeFromWriterStatusKind::Received,
                is_relevant: true,
                fragment_info,
            })
            .status = ChangeFromWriterStatusKind::Received;
    }

    pub(crate) fn remove_change_from_writer_by_sn(&mut self, seq_num: SequenceNumber) {
        self.changes_from_writer.remove(&seq_num);
    }

    pub(crate) fn calculate_bitmap_base(&self) -> SequenceNumber {
        let last_received = self.available_changes_max();
        if last_received == SequenceNumber::UNKNOWN {
            SequenceNumber::new(0, 1)
        } else {
            last_received.next()
        }
    }

    pub(crate) fn mark_frag_received(
        &mut self,
        seq_num: SequenceNumber,
        total_fragments: u32,
        received_fragments: std::collections::HashSet<u32>,
    ) {
        let change = self.changes_from_writer.entry(seq_num).or_insert_with(|| ChangeFromWriter {
            sequence_number: seq_num,
            status: ChangeFromWriterStatusKind::Received,
            is_relevant: true,
            fragment_info: Some(FragmentInfo {
                total_fragments,
                received_fragments: std::collections::HashSet::new(),
                is_complete: false,
            }),
        });

        // Combine existing fragments with new received fragments
        if let Some(info) = &mut change.fragment_info {
            for &fragment in &received_fragments {
                info.received_fragments.insert(fragment);
            }

            // Update completion status
            info.is_complete = info.received_fragments.len() == total_fragments as usize;
        }
    }

    pub(crate) fn on_sample_lost(&self) {
        match self.status_callback.lock() {
            Ok(callback) => {
                if let Some(callback) = callback.as_ref() {
                    callback(StatusKind::SAMPLE_LOST, None);
                }
            }
            Err(e) => {
                log::error!("Failed to lock callback: {:?}", e);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeFromWriterStatusKind {
    Unknown,
    Missing,
    Received,
    NotAvailable(NotAvailable),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotAvailable {
    Filtered,
    Removed,
    Unspecified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeFromWriter {
    pub(crate) sequence_number: SequenceNumber,
    pub(crate) status: ChangeFromWriterStatusKind,
    pub(crate) is_relevant: bool,
    pub(crate) fragment_info: Option<FragmentInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FragmentInfo {
    pub(crate) total_fragments: u32,
    pub(crate) received_fragments: std::collections::HashSet<u32>,
    pub(crate) is_complete: bool,
}
