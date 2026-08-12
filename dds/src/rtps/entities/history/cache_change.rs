//! Cache change representation for history caches.
//!
//! This module defines `CacheChange` which represents a single data sample change
//! stored in reader or writer history caches. Changes include the sequence number,
//! data payload, instance handle, and metadata.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use bytes::Bytes;
use smallvec::SmallVec;

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
/// `Chained` holds the fragment chunks as-is (scatter-gather receive path),
/// avoiding a per-sample contiguous reassembly. `cached` is a shared lazy slot:
/// the first caller that needs contiguous bytes (`as_slice`) materializes once
/// and every reader of the same sample borrows that result, so materialization
/// is at most once per sample globally.
#[derive(Debug)]
pub(crate) enum DataPayload {
    Owned(Vec<u8>),
    Shared(Bytes),
    Chained { chunks: SmallVec<[Bytes; 16]>, cached: Arc<OnceLock<Bytes>> },
}

impl Clone for DataPayload {
    fn clone(&self) -> Self {
        match self {
            DataPayload::Owned(v) => DataPayload::Owned(v.clone()),
            DataPayload::Shared(b) => DataPayload::Shared(b.clone()),
            DataPayload::Chained { chunks, cached } => {
                DataPayload::Chained { chunks: chunks.clone(), cached: cached.clone() }
            }
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
            // Materialize once into the shared cache, then borrow it. Callers
            // that need contiguous bytes (key extraction, read_serialized) pay
            // this at most once per sample across all readers.
            DataPayload::Chained { chunks, cached } => {
                cached.get_or_init(|| concat_chunks(chunks)).as_ref()
            }
        }
    }

    // Emptiness check that never materializes a chained payload. A single
    // non-empty chunk is enough to answer without contiguous reassembly.
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            DataPayload::Owned(v) => v.is_empty(),
            DataPayload::Shared(b) => b.is_empty(),
            DataPayload::Chained { chunks, .. } => chunks.iter().all(|c| c.is_empty()),
        }
    }
}

// Concatenate chunks into a contiguous Bytes without zero-filling.
fn concat_chunks(chunks: &[Bytes]) -> Bytes {
    let total: usize = chunks.iter().map(|c| c.len()).sum();
    let mut buf = Vec::with_capacity(total);
    for c in chunks {
        buf.extend_from_slice(c);
    }
    Bytes::from(buf)
}

// Per-sample inline QoS metadata carried in a Data submessage's inline QoS list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PresentationInfo {
    pub coherent_set: Option<SequenceNumber>, // PID_COHERENT_SET (writer's first member seq)
    pub group_seq_num: Option<SequenceNumber>, // PID_GROUP_SEQ_NUM (sample's own group seq)
    pub group_coherent_set: Option<SequenceNumber>, // PID_GROUP_COHERENT_SET (group set's first seq)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheChange {
    kind: ChangeKind,
    // From writer perspective: created local writer guid
    // From reader perspective: created remote writer guid
    writer_guid: Guid,
    pub(crate) sequence_number: SequenceNumber,
    data_payload: DataPayload,
    presentation_info: PresentationInfo,
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
impl std::fmt::Display for CacheChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CacheChange {{ kind: {:?}, writer_guid: {}, sequence_number: {}, \
             data_payload_len: {}, instance_handle: {}, source_timestamp: {:?}, \
             reception_timestamp: {:?}, fragmented: {}, fragment_set: {:?}, \
             total_fragments: {}, fragment_size: {}, writer_ownership_strength: {:?}, \
             lifespan_duration: {:?}, presentation_info: {:?} }}",
            self.kind,
            self.writer_guid,
            self.sequence_number,
            self.data_payload.as_slice().len(),
            self.instance_handle,
            self.source_timestamp,
            self.reception_timestamp,
            self.fragmented,
            self.fragment_set,
            self.total_fragments,
            self.fragment_size,
            self.writer_ownership_strength,
            self.lifespan_duration,
            self.presentation_info
        )
    }
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
            presentation_info: PresentationInfo::default(),
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
            presentation_info: PresentationInfo::default(),
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
            DataPayload::Shared(_) | DataPayload::Chained { .. } => {
                self.data_payload = DataPayload::Owned(Vec::new());
            }
        }
        self.presentation_info = PresentationInfo::default();
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

    pub(crate) fn presentation_info(&self) -> &PresentationInfo {
        &self.presentation_info
    }

    pub(crate) fn set_presentation_info(&mut self, presentation_info: PresentationInfo) {
        self.presentation_info = presentation_info;
    }

    pub(crate) fn sequence_number(&self) -> SequenceNumber {
        self.sequence_number
    }

    pub(crate) fn data_value(&self) -> &[u8] {
        self.data_payload.as_slice()
    }

    // A payload-less Alive Data carrying no coherent set id (or UNKNOWN) closes
    // the writer's open coherent set.
    pub(crate) fn is_coherent_end_marker(&self) -> bool {
        let coherent_set = self.presentation_info.coherent_set;
        (coherent_set.is_none() || coherent_set == Some(SequenceNumber::UNKNOWN))
            && self.kind == ChangeKind::Alive
            && self.data_payload.is_empty()
    }

    pub(crate) fn data_bytes(&self) -> Bytes {
        match &self.data_payload {
            // Shared payload: refcount bump, no allocation.
            DataPayload::Shared(b) => b.clone(),
            // Owned payload: copy into a fresh Bytes (writer-side path).
            DataPayload::Owned(v) => Bytes::copy_from_slice(v),
            // Chained: materialize once into the shared cache, then refcount bump.
            DataPayload::Chained { chunks, cached } => {
                cached.get_or_init(|| concat_chunks(chunks)).clone()
            }
        }
    }

    // Fragment chunks and their shared materialization cache, when this change
    // holds a scatter-gather payload. Lets the subscription layer deserialize
    // directly across chunks without a contiguous reassembly.
    pub(crate) fn data_chunks(&self) -> Option<(&[Bytes], &Arc<OnceLock<Bytes>>)> {
        match &self.data_payload {
            DataPayload::Chained { chunks, cached } => Some((chunks, cached)),
            _ => None,
        }
    }

    /// Mutable access to the owned Vec buffer.
    /// Used by writer serialization and non-fragmented reader reception.
    pub(crate) fn data_mut(&mut self) -> &mut Vec<u8> {
        match &mut self.data_payload {
            DataPayload::Owned(v) => v,
            DataPayload::Shared(_) => unreachable!("data_mut called on shared payload"),
            DataPayload::Chained { .. } => unreachable!("data_mut called on chained payload"),
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

    // Set a scatter-gather payload: fragment chunks plus a shared materialization
    // cache. All readers of one sample share `cached`, so any contiguous fallback
    // happens at most once globally.
    pub(crate) fn set_chained_payload(
        &mut self,
        chunks: SmallVec<[Bytes; 16]>,
        cached: Arc<OnceLock<Bytes>>,
    ) {
        self.data_payload = DataPayload::Chained { chunks, cached };
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

    // Fragment when the payload exceeds max_message_size, cutting into fragment_size chunks.
    pub(crate) fn apply_fragmentation(&mut self, max_message_size: usize, fragment_size: usize) {
        self.fragment_set.clear();
        let data_len = self.data_value().len();
        if max_message_size > 0 && fragment_size > 0 && data_len > max_message_size {
            self.fragmented = true;
            self.fragment_size = fragment_size as u32;
            let num_fragments = data_len.div_ceil(fragment_size);
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
        self.get_fragment_range_data(fragment_num, 1)
    }

    /// Payload for `count` consecutive fragments starting at `fragment_num`, as one slice.
    /// The last fragment is short, so the slice is clamped to the payload end.
    pub(crate) fn get_fragment_range_data(&self, fragment_num: u32, count: u16) -> Option<&[u8]> {
        if !self.fragmented
            || fragment_num == 0
            || fragment_num > self.total_fragments
            || count == 0
        {
            return None;
        }

        let data = self.data_value();
        let start = (fragment_num - 1) as usize * self.fragment_size as usize;
        let span = count as usize * self.fragment_size as usize;
        let end = std::cmp::min(start.saturating_add(span), data.len());

        Some(&data[start..end])
    }

    /// How many fragments fit in one DATA_FRAG submessage under the datagram budget.
    /// At least 1, so a budget below one fragment still makes progress.
    pub(crate) fn fragments_per_submessage(&self, max_message_size: usize) -> u16 {
        if self.fragment_size == 0 {
            return 1;
        }
        (max_message_size / self.fragment_size as usize).clamp(1, u16::MAX as usize) as u16
    }

    // Create fragmented cache change from payload
    pub(crate) fn create_fragmented(
        kind: ChangeKind,
        writer_guid: Guid,
        instance_handle: InstanceHandle,
        sequence_number: SequenceNumber,
        payload: &[u8],
        source_timestamp: Option<RtpsTime>,
        max_message_size: usize,
        fragment_size: usize,
    ) -> Self {
        let mut change = Self::new(
            kind,
            writer_guid,
            instance_handle,
            sequence_number,
            payload.to_vec(),
            source_timestamp,
        );

        change.apply_fragmentation(max_message_size, fragment_size);
        change
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change_with_payload(len: usize) -> CacheChange {
        CacheChange::new(
            ChangeKind::Alive,
            Guid::UNKNOWN,
            InstanceHandle::default(),
            SequenceNumber::from_i64(1),
            vec![0u8; len],
            None,
        )
    }

    // A payload over fragment_size but at or below max_message_size still goes as a
    // single DATA: the trigger is max_message_size, not fragment_size.
    #[test]
    fn payload_within_max_is_not_fragmented() {
        let mut change = change_with_payload(10_000);
        change.apply_fragmentation(14_720, 1_344);
        assert!(!change.is_fragmented());
        assert_eq!(change.total_fragments(), 0);
        assert_eq!(change.fragment_size(), 0);
    }

    // A payload over max_message_size is cut into fragment_size chunks.
    #[test]
    fn payload_above_max_fragments_by_fragment_size() {
        let mut change = change_with_payload(20_000);
        change.apply_fragmentation(14_720, 1_344);
        assert!(change.is_fragmented());
        assert_eq!(change.fragment_size(), 1_344);
        assert_eq!(change.total_fragments(), 20_000u32.div_ceil(1_344));
    }

    // Equal knobs reproduce the single-size behavior: trigger and chunk are the same value.
    #[test]
    fn equal_max_and_fragment_size_matches_single_knob() {
        let mut change = change_with_payload(100_000);
        change.apply_fragmentation(65_000, 65_000);
        assert!(change.is_fragmented());
        assert_eq!(change.fragment_size(), 65_000);
        assert_eq!(change.total_fragments(), 2);
    }

    // Zero on either knob disables fragmentation.
    #[test]
    fn zero_knob_disables_fragmentation() {
        let mut change = change_with_payload(100_000);
        change.apply_fragmentation(0, 1_344);
        assert!(!change.is_fragmented());

        let mut other = change_with_payload(100_000);
        other.apply_fragmentation(14_720, 0);
        assert!(!other.is_fragmented());
    }

    // The last fragment carries the remainder, shorter than fragment_size.
    #[test]
    fn last_fragment_holds_remainder() {
        let mut change = change_with_payload(20_000);
        change.apply_fragmentation(14_720, 1_344);

        let total = change.total_fragments();
        let expected_last = 20_000 - 1_344 * (total as usize - 1);

        assert_eq!(change.get_fragment_data(1).unwrap().len(), 1_344);
        assert_eq!(change.get_fragment_data(total).unwrap().len(), expected_last);
    }

    // A change fragmented at 1344 bytes over a payload that needs 781 fragments.
    // The payload is patterned, not zeroed, so a slice taken at the wrong offset is visible.
    fn packed_change() -> CacheChange {
        let payload: Vec<u8> = (0..1_048_576u32).map(|i| (i % 251) as u8).collect();
        let mut change = CacheChange::new(
            ChangeKind::Alive,
            Guid::UNKNOWN,
            InstanceHandle::default(),
            SequenceNumber::from_i64(1),
            payload,
            None,
        );
        change.apply_fragmentation(65_000, 1_344);
        change
    }

    #[test]
    fn fragment_range_returns_consecutive_fragments_as_one_slice() {
        let change = packed_change();
        assert_eq!(change.total_fragments(), 781);

        let one = change.get_fragment_range_data(1, 1).unwrap();
        assert_eq!(one.len(), 1_344, "a count of one matches a single fragment");

        let ten = change.get_fragment_range_data(1, 10).unwrap();
        assert_eq!(ten.len(), 13_440, "ten fragments concatenate");
        assert_eq!(&ten[..1_344], one, "the range starts at the same offset");

        let from_eleven = change.get_fragment_range_data(11, 10).unwrap();
        assert_eq!(from_eleven, &change.data_value()[13_440..26_880]);
    }

    #[test]
    fn fragment_range_clamps_to_the_payload_end() {
        let change = packed_change();
        let total = change.total_fragments();
        // 781 * 1344 = 1_049_664, which is 1088 past the 1_048_576-byte payload.
        let last = change.get_fragment_range_data(total, 1).unwrap();
        assert_eq!(last.len(), 1_048_576 - 780 * 1_344);

        let overrun = change.get_fragment_range_data(total - 4, 100).unwrap();
        assert_eq!(overrun.len(), 1_048_576 - (total as usize - 5) * 1_344);
    }

    #[test]
    fn fragment_range_rejects_out_of_range_input() {
        let change = packed_change();
        assert!(change.get_fragment_range_data(0, 1).is_none(), "fragment numbers are 1-based");
        assert!(change.get_fragment_range_data(782, 1).is_none(), "past the last fragment");
        assert!(change.get_fragment_range_data(1, 0).is_none(), "a count of zero has no payload");

        let mut unfragmented = packed_change();
        unfragmented.apply_fragmentation(65_000, 0);
        assert!(unfragmented.get_fragment_range_data(1, 1).is_none());
    }

    #[test]
    fn get_fragment_data_is_unchanged_by_the_range_accessor() {
        let change = packed_change();
        for fragment_num in [1u32, 2, 400, 780, 781] {
            assert_eq!(
                change.get_fragment_data(fragment_num),
                change.get_fragment_range_data(fragment_num, 1),
                "single-fragment access must stay identical"
            );
        }
        assert!(change.get_fragment_data(0).is_none());
        assert!(change.get_fragment_data(782).is_none());
    }

    #[test]
    fn fragments_per_submessage_divides_the_datagram_budget() {
        let change = packed_change();
        assert_eq!(change.fragments_per_submessage(65_000), 48, "65000 / 1344");
        assert_eq!(change.fragments_per_submessage(14_720), 10, "14720 / 1344");
        assert_eq!(change.fragments_per_submessage(1_472), 1, "budget below two fragments");
        assert_eq!(change.fragments_per_submessage(0), 1, "never returns zero");
        assert_eq!(change.fragments_per_submessage(usize::MAX), u16::MAX, "clamped to u16");

        // The default fragment size is the default budget, so packing is a no-op there.
        let mut default_size = packed_change();
        default_size.apply_fragmentation(65_000, 65_000);
        assert_eq!(default_size.fragments_per_submessage(65_000), 1, "defaults do not pack");
    }
}
