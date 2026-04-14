//! Cache change representation for history caches.
//!
//! This module defines `CacheChange` which represents a single data sample change
//! stored in reader or writer history caches. Changes include the sequence number,
//! data payload, instance handle, and metadata.

use std::collections::HashSet;

use bytes::Bytes;

use crate::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    rtps::common::{
        guid::Guid,
        // parameters::ParameterList,
        sequence::SequenceNumber,
        time::RtpsTime,
        types::ChangeKind,
    },
};

/// Data payload of a CacheChange.
///
/// `Owned` is used on the writer side (mutable, capacity-reusable via pool)
/// and in cases where the receiver owns a fresh buffer.
/// `Shared` holds a `Bytes` slice of the original socket buffer (or a
/// fragment-assembled buffer), allowing multiple readers to receive the
/// same payload with only refcount increments.
#[derive(Debug)]
pub(crate) enum DataPayload {
    Owned(Vec<u8>),
    Shared(Bytes),
}

impl Clone for DataPayload {
    fn clone(&self) -> Self {
        match self {
            DataPayload::Owned(v) => DataPayload::Owned(v.clone()),
            DataPayload::Shared(b) => DataPayload::Shared(b.clone()),
        }
    }
}

impl PartialEq for DataPayload {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for DataPayload {}

impl DataPayload {
    pub(crate) fn as_slice(&self) -> &[u8] {
        match self {
            DataPayload::Owned(v) => v,
            DataPayload::Shared(b) => b,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheChange {
    kind: ChangeKind,
    // From writer perspective: created local writer guid
    // From reader perspective: created remote writer guid
    writer_guid: Guid,
    pub(crate) sequence_number: SequenceNumber,
    data_payload: DataPayload,
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
        data_value: Vec<u8>,
        source_timestamp: Option<RtpsTime>,
    ) -> Self {
        Self {
            kind,
            writer_guid,
            writer_ownership_strength: None,
            instance_handle,
            data_payload: DataPayload::Owned(data_value),
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

    /// Create an empty CacheChange (for pool pre-allocation)
    pub(crate) fn empty() -> Self {
        Self {
            kind: ChangeKind::Alive,
            writer_guid: Guid::UNKNOWN,
            writer_ownership_strength: None,
            instance_handle: InstanceHandle::default(),
            data_payload: DataPayload::Owned(Vec::new()),
            sequence_number: SequenceNumber::UNKNOWN,
            source_timestamp: None,
            reception_timestamp: None,
            fragmented: false,
            fragment_set: HashSet::new(),
            total_fragments: 0,
            fragment_size: 0,
            lifespan_duration: None,
        }
    }

    /// Reset metadata for reuse, preserving data_value capacity for Owned payloads.
    pub(crate) fn reset(
        &mut self,
        kind: ChangeKind,
        writer_guid: Guid,
        instance_handle: InstanceHandle,
        sequence_number: SequenceNumber,
        source_timestamp: Option<RtpsTime>,
    ) {
        self.kind = kind;
        self.writer_guid = writer_guid;
        self.writer_ownership_strength = None;
        self.instance_handle = instance_handle;
        match &mut self.data_payload {
            DataPayload::Owned(v) => v.clear(),
            DataPayload::Shared(_) => {
                self.data_payload = DataPayload::Owned(Vec::new());
            }
        }
        self.sequence_number = sequence_number;
        self.source_timestamp = source_timestamp;
        self.reception_timestamp = None;
        self.fragmented = false;
        self.fragment_set.clear();
        self.total_fragments = 0;
        self.fragment_size = 0;
        self.lifespan_duration = None;
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
        self.data_payload.as_slice()
    }

    /// Mutable access to the owned Vec buffer.
    /// Used by writer serialization and non-fragmented reader reception.
    pub(crate) fn data_mut(&mut self) -> &mut Vec<u8> {
        match &mut self.data_payload {
            DataPayload::Owned(v) => v,
            DataPayload::Shared(_) => unreachable!("data_mut called on shared payload"),
        }
    }

    /// Replace payload with an owned Vec (fragment assembly single-reader move).
    pub(crate) fn set_owned_payload(&mut self, data: Vec<u8>) {
        self.data_payload = DataPayload::Owned(data);
    }

    /// Set a shared payload for zero-copy multi-reader delivery.
    pub(crate) fn set_shared_payload(&mut self, data: Bytes) {
        self.data_payload = DataPayload::Shared(data);
    }

    /// Return the shared backing for zero-copy deserialization of byte sequences.
    ///
    /// TEMPORARILY DISABLED (always returns `None`).
    ///
    /// The original purpose is to let `DdsBytes` fields reference a sub-slice
    /// of the `CacheChange` payload without copying. The downstream plumbing
    /// (`TypeSupport::deserialize_with_backing`, derive macro codegen,
    /// `CdrDeserializer::set_shared_backing`, `DdsBytes::Shared`) is still wired
    /// to `Option<Arc<Vec<u8>>>`, but `DataPayload::Shared` now holds
    /// `bytes::Bytes` (changed to make non-fragmented / fragmented receive
    /// paths zero-copy for `Vec<u8>` users). Since `DdsBytes` is scheduled to
    /// be replaced wholesale by `bytes::Bytes` in a follow-up change, rather
    /// than cascading the type through the old plumbing here we disconnect it
    /// and let `DdsBytes` fall back to its owned/copy path until then.
    ///
    /// When the `DdsBytes` → `bytes::Bytes` migration lands, rewire this to
    /// return the backing `Bytes` directly and thread it through the new
    /// pipeline.
    pub(crate) fn shared_backing(&self) -> Option<std::sync::Arc<Vec<u8>>> {
        None
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

    // Apply fragmentation metadata based on the current `data_value` length.
    pub(crate) fn apply_fragmentation(&mut self, max_payload_size: usize) {
        self.fragment_set.clear();
        let data_len = self.data_value().len();
        if max_payload_size > 0 && data_len > max_payload_size {
            self.fragmented = true;
            self.fragment_size = max_payload_size as u32;
            let num_fragments = data_len.div_ceil(max_payload_size);
            self.total_fragments = num_fragments as u32;
            for i in 1..=num_fragments {
                self.fragment_set.insert(i as u32);
            }
        } else {
            self.fragmented = false;
            self.total_fragments = 0;
            self.fragment_size = 0;
        }
    }

    pub(crate) fn get_fragment_data(&self, fragment_num: u32) -> Option<&[u8]> {
        if !self.fragmented || fragment_num == 0 || fragment_num > self.total_fragments {
            return None;
        }

        let data = self.data_value();
        let start = (fragment_num - 1) * self.fragment_size;
        let end = std::cmp::min(start + self.fragment_size, data.len() as u32);

        Some(&data[start as usize..end as usize])
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
            payload.to_vec(),
            source_timestamp,
        );

        change.apply_fragmentation(max_payload_size);
        change
    }
}
