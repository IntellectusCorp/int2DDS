//! Cache change representation for history caches.
//!
//! This module defines `CacheChange` which represents a single data sample change
//! stored in reader or writer history caches. Changes include the sequence number,
//! data payload, instance handle, and metadata.

use std::{collections::HashSet, sync::Arc};
use uuid::Uuid;

use crate::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    rtps::common::{
        guid::Guid,
        // parameters::ParameterList,
        sequence::SequenceNumber,
        time::RtpsTime,
        types::{ChangeKind, SerializedData},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheChange {
    uuid: Uuid,
    kind: ChangeKind,
    // From writer perspective: created local writer guid
    // From reader perspective: created remote writer guid
    writer_guid: Guid,
    pub(crate) sequence_number: SequenceNumber,
    data_value: SerializedData,
    // inline_qos is RTPS version 2.5
    // inline_qos: ParameterList
    instance_handle: InstanceHandle,
    source_timestamp: Option<RtpsTime>,
    reception_timestamp: Option<RtpsTime>,
    // Fragmentation
    fragmented: bool,
    fragment_set: HashSet<u32>,
    total_fragments: u32,
    fragment_size: u32,
    writer_ownership_strength: Option<i32>, // Set only when it's writer cache && Ownership QoS is EXCLUSIVE, CacheChange should be split to reader & writer cache in the future
    lifespan_duration: Option<Duration>,
}

impl PartialOrd for CacheChange {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CacheChange {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sequence_number.cmp(&other.sequence_number)
    }
}

impl CacheChange {
    pub(crate) fn new(
        kind: ChangeKind,
        writer_guid: Guid,
        instance_handle: InstanceHandle,
        sequence_number: SequenceNumber,
        data_value: SerializedData,
        // inline_qos: ParameterList,
        source_timestamp: Option<RtpsTime>,
    ) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            kind,
            writer_guid,
            writer_ownership_strength: None,
            instance_handle,
            data_value,
            sequence_number,
            source_timestamp,
            reception_timestamp: None,
            fragmented: false,
            fragment_set: HashSet::new(),
            total_fragments: 0,
            fragment_size: 0,
            lifespan_duration: None,
        }
    }

    pub(crate) fn kind(&self) -> ChangeKind {
        self.kind
    }

    pub(crate) fn set_kind(&mut self, kind: ChangeKind) {
        self.kind = kind;
    }

    pub(crate) fn set_ownership_strength(&mut self, strength: Option<i32>) {
        self.writer_ownership_strength = strength;
    }

    pub(crate) fn ownership_strength(&self) -> Option<i32> {
        self.writer_ownership_strength
    }

    pub(crate) fn writer_guid(&self) -> Guid {
        self.writer_guid
    }

    pub(crate) fn instance_handle(&self) -> InstanceHandle {
        self.instance_handle
    }

    pub(crate) fn set_instance_handle(&mut self, instance_handle: InstanceHandle) {
        self.instance_handle = instance_handle;
    }

    pub(crate) fn sequence_number(&self) -> SequenceNumber {
        self.sequence_number
    }
    pub(crate) fn data_value(&self) -> &[u8] {
        &self.data_value
    }
    pub(crate) fn data_value_arc(&self) -> Arc<[u8]> {
        Arc::clone(&self.data_value)
    }

    pub(crate) fn source_timestamp(&self) -> Option<RtpsTime> {
        self.source_timestamp
    }

    pub(crate) fn reception_timestamp(&self) -> Option<RtpsTime> {
        self.reception_timestamp
    }

    pub(crate) fn set_reception_timestamp(&mut self, reception_timestamp: RtpsTime) {
        self.reception_timestamp = Some(reception_timestamp);
    }

    pub(crate) fn set_lifespan_duration(&mut self, lifespan_duration: Option<Duration>) {
        self.lifespan_duration = lifespan_duration;
    }

    pub(crate) fn lifespan_duration(&self) -> Option<Duration> {
        self.lifespan_duration
    }

    pub(crate) fn is_fragmented(&self) -> bool {
        self.fragmented
    }

    pub(crate) fn total_fragments(&self) -> u32 {
        self.total_fragments
    }

    pub(crate) fn fragment_size(&self) -> u32 {
        self.fragment_size
    }

    pub(crate) fn get_fragment_data(&self, fragment_num: u32) -> Option<SerializedData> {
        if !self.fragmented || fragment_num == 0 || fragment_num > self.total_fragments {
            return None;
        }

        let start = (fragment_num - 1) * self.fragment_size;
        let end = std::cmp::min(start + self.fragment_size, self.data_value.len() as u32);

        // Convert slice directly to Arc<[u8]> (SerializedData)
        // This still requires one copy (to_vec), but avoids intermediate Vec wrapper
        Some(Arc::from(&self.data_value[start as usize..end as usize]))
    }

    // Create fragmented cache change from payload
    pub(crate) fn create_fragmented(
        kind: ChangeKind,
        writer_guid: Guid,
        instance_handle: InstanceHandle,
        sequence_number: SequenceNumber,
        payload: &[u8],
        source_timestamp: Option<RtpsTime>,
        max_payload_size: usize,
    ) -> Self {
        let mut change = Self::new(
            kind,
            writer_guid,
            instance_handle,
            sequence_number,
            Arc::from(payload),
            source_timestamp,
        );

        if payload.len() > max_payload_size {
            change.fragmented = true;

            let fragment_size = max_payload_size;
            let num_fragments = payload.len().div_ceil(fragment_size);
            change.total_fragments = num_fragments as u32;
            change.fragment_size = fragment_size as u32;

            for i in 1..=num_fragments {
                change.fragment_set.insert(i as u32);
            }
        } else {
            change.fragmented = false;
        }

        change
    }
}
