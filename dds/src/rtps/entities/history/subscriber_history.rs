#![allow(dead_code)]

use std::{
    cmp::max,
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use dashmap::DashMap;
use log::debug;

use crate::{
    common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
    rtps::{
        common::{
            entity_id::EntityId,
            guid::{GroupDigest, Guid},
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
        },
        entities::history::cache_change::CacheChange,
        messages::submessages::{gap, heartbeat},
    },
};

// A sample held for one reader until its group sequence number is released.
#[derive(Debug)]
pub(crate) struct PendingSample {
    pub(crate) reader_id: EntityId,
    pub(crate) change: CacheChange,
    pub(crate) apply_filter: bool,
}

// Group ordering state of one remote writer belonging to a remote Publisher.
#[derive(Debug)]
pub(crate) struct WriterGsnInfo {
    reader_ids: HashSet<EntityId>,
    is_alive: bool,
    max_committed_gsn: Option<SequenceNumber>,
    heartbeat_group_info: Option<heartbeat::GroupInfo>,
    highest_gap_end_gsn: Option<SequenceNumber>,
}

impl WriterGsnInfo {
    fn new() -> Self {
        Self {
            reader_ids: HashSet::new(),
            is_alive: true,
            max_committed_gsn: None,
            heartbeat_group_info: None,
            highest_gap_end_gsn: None,
        }
    }

    // The writer left the group position before the given one behind: it committed a sample at or
    // after it, declared the range unavailable, or announced it never held that position.
    fn has_passed_group_seq_num(&self, group_seq_num: SequenceNumber) -> bool {
        let previous = group_seq_num.previous();

        // Advanced past the GSN-1 by committing a Data sample with groupSequenceNumber >= GSN
        let has_committed_beyond =
            self.max_committed_gsn.is_some_and(|committed| committed >= group_seq_num);
        // Or a Gap message with Gap.gapEndGSN.value >= GSN-1
        let has_declared_unavailable =
            self.highest_gap_end_gsn.is_some_and(|gap_end| gap_end >= previous);
        // Or a Heartbeat with Heartbeat.currentGSN.value >= GSN and GSN-1 not in [firstGSN, lastGSN]
        let has_never_held = self.heartbeat_group_info.is_some_and(|info| {
            info.current_gsn >= group_seq_num
                && (previous < info.first_gsn || previous > info.last_gsn)
        });

        has_committed_beyond || has_declared_unavailable || has_never_held
    }

    // The latest Heartbeat of this writer puts the group at or past the given position, and the
    // writer set it carries is the one we discovered.
    fn does_heartbeat_cover_group_seq_num(
        &self,
        group_seq_num: SequenceNumber,
        discovered_writer_set: GroupDigest,
    ) -> bool {
        // A Heartbeat from one of the DataWriters with Heartbeat.currentGSN.value >= GSN and the
        // Heartbeat.writerSet matching the set of discovered DataWriters
        self.heartbeat_group_info.is_some_and(|info| {
            info.current_gsn >= group_seq_num && info.writer_set == discovered_writer_set
        })
    }
}

// Group ordering state of one remote Publisher: its writers and the samples held for it.
#[derive(Debug)]
pub(crate) struct PublisherProxy {
    last_committed_gsn: Option<SequenceNumber>,
    pending: BTreeMap<SequenceNumber, Vec<PendingSample>>,
    ready: Vec<PendingSample>,
    writers: HashMap<Guid, WriterGsnInfo>,
}

impl PublisherProxy {
    fn new() -> Self {
        Self {
            last_committed_gsn: None,
            pending: BTreeMap::new(),
            ready: Vec::new(),
            writers: HashMap::new(),
        }
    }

    fn lowest_pending_group_seq_num(&self) -> Option<SequenceNumber> {
        self.pending.first_key_value().map(|(group_seq_num, _)| *group_seq_num)
    }

    // The position right after the last one released, or the very first one of the group.
    fn is_next_in_group_order(&self, group_seq_num: SequenceNumber) -> bool {
        // GSN-1 has already been committed. The first position has no predecessor, so it counts
        // as if committed.
        match self.last_committed_gsn {
            Some(last_committed) => last_committed.next() == group_seq_num,
            None => group_seq_num == SequenceNumber::INIT,
        }
    }

    // Hands over the samples held under one group sequence number and records how far the group
    // and each of their writers got.
    fn release_group_seq_num(&mut self, group_seq_num: SequenceNumber) -> Vec<PendingSample> {
        // Every copy under this position leaves together, one per reader.
        let Some(samples) = self.pending.remove(&group_seq_num) else {
            return Vec::new();
        };

        // The next position is judged against this one.
        self.last_committed_gsn = Some(group_seq_num);

        // Each writer now counts as advanced past this position, and a late copy of it from the
        // same writer is let through without a judgement.
        for sample in &samples {
            if let Some(writer) = self.writers.get_mut(&sample.change.writer_guid()) {
                writer.max_committed_gsn = Some(group_seq_num);
            }
        }

        debug!(
            "SHC released group sequence number {} with {} samples",
            group_seq_num.to_i64(),
            samples.len()
        );

        samples
    }

    // No alive writer of this Publisher will ever fill the position before the given one. One
    // Heartbeat must cover that position, and every alive writer must have left it behind.
    fn is_group_seq_num_absent_from_all_writers(
        &self,
        group_seq_num: SequenceNumber,
        discovered_writer_set: GroupDigest,
    ) -> bool {
        // Only DataWriters that have not lost their liveliness are taken into consideration.
        let mut alive_writers = self.writers.values().filter(|writer| writer.is_alive).peekable();

        // With nobody alive there is no one to vouch that the position will stay empty.
        if alive_writers.peek().is_none() {
            return false;
        }

        // None of the remote DataWriters have GSN-1 when one Heartbeat covers the position and
        // every DataWriter has left it behind.
        let mut is_covered_by_a_heartbeat = false;
        let mut has_every_writer_passed = true;

        for writer in alive_writers {
            is_covered_by_a_heartbeat |=
                writer.does_heartbeat_cover_group_seq_num(group_seq_num, discovered_writer_set);
            has_every_writer_passed &= writer.has_passed_group_seq_num(group_seq_num);
        }

        is_covered_by_a_heartbeat && has_every_writer_passed
    }
}

// Holds the samples a GROUP access scope Subscriber may not hand over yet, ordered per remote
// Publisher by group sequence number.
#[derive(Debug)]
pub(crate) struct SubscriberHistoryCache {
    remote_publications: Arc<DashMap<String, HashMap<Guid, PublicationBuiltinTopicData>>>,
    publishers: HashMap<Guid, PublisherProxy>,
}

impl SubscriberHistoryCache {
    pub(crate) fn new(
        remote_publications: Arc<DashMap<String, HashMap<Guid, PublicationBuiltinTopicData>>>,
    ) -> Self {
        Self { remote_publications, publishers: HashMap::new() }
    }

    // Takes one sample destined for a reader. A sample whose group position the writer already
    // passed goes out on the next flush, the rest wait under their group sequence number.
    // A writer offering GROUP access scope owes both a group sequence number and a group GUID,
    // and a sample missing either cannot be ordered.
    pub(crate) fn add_change(
        &mut self,
        reader_id: EntityId,
        change: CacheChange,
        apply_filter: bool,
    ) -> RtpsResult<()> {
        let writer_guid = change.writer_guid();

        let group_seq_num = change.presentation_info().group_seq_num.ok_or_else(|| {
            RtpsError::new(
                RtpsErrorCode::GroupSequenceNumberNotSet,
                format!(
                    "Writer {} sent sample {} without a group sequence number",
                    writer_guid.to_hex_string(),
                    change.sequence_number().to_i64()
                ),
            )
        })?;

        let proxy = self.find_publisher_proxy_of_writer_mut(writer_guid)?;

        let has_writer_passed_this_gsn = proxy
            .writers
            .get(&writer_guid)
            .and_then(|writer| writer.max_committed_gsn)
            .is_some_and(|committed| committed >= group_seq_num);

        let sample = PendingSample { reader_id, change, apply_filter };

        if has_writer_passed_this_gsn {
            proxy.ready.push(sample);
            return Ok(());
        }

        proxy.pending.entry(group_seq_num).or_default().push(sample);
        Ok(())
    }

    // Takes out everything the gate now lets through, across every remote Publisher. The caller
    // holds one lock over the event that changed the state and this call.
    pub(crate) fn flush_pending_changes(&mut self) -> Vec<PendingSample> {
        let mut released = Vec::new();

        for publisher_guid in self.publishers.keys().copied().collect::<Vec<Guid>>() {
            released.extend(self.flush_publisher_pending_changes(publisher_guid));
        }

        released
    }

    // Walks one Publisher's held positions from the lowest up and stops at the first one the gate
    // keeps.
    fn flush_publisher_pending_changes(&mut self, publisher_guid: Guid) -> Vec<PendingSample> {
        // Late copies of positions already released go out first, without a judgement.
        let mut released = match self.publishers.get_mut(&publisher_guid) {
            Some(proxy) => std::mem::take(&mut proxy.ready),
            None => return Vec::new(),
        };

        // Only a hole needs the discovered writer set, so it is computed at the first hole and
        // reused for the rest of this walk.
        let mut discovered_writer_set: Option<GroupDigest> = None;

        loop {
            let Some(proxy) = self.publishers.get(&publisher_guid) else {
                break;
            };

            // The lowest held position blocks every position above it.
            let Some(group_seq_num) = proxy.lowest_pending_group_seq_num() else {
                break;
            };

            // Samples arriving in order pass here and never touch the discovered writer set.
            if !proxy.is_next_in_group_order(group_seq_num) {
                let writer_set = *discovered_writer_set
                    .get_or_insert_with(|| self.discovered_writer_set(publisher_guid));

                let Some(proxy) = self.publishers.get(&publisher_guid) else {
                    break;
                };

                // Nobody has shown the hole will stay empty, so the position stays held until a
                // later Heartbeat, Gap, liveliness or matching event changes that.
                if !proxy.is_group_seq_num_absent_from_all_writers(group_seq_num, writer_set) {
                    break;
                }
            }

            let Some(proxy) = self.publishers.get_mut(&publisher_guid) else {
                break;
            };

            // Releasing this position may make the next one pass on order alone.
            released.extend(proxy.release_group_seq_num(group_seq_num));
        }

        released
    }

    // The announced range moves as the writer's history rolls, so the latest one replaces it.
    pub(crate) fn record_heartbeat_group_info(
        &mut self,
        writer_guid: Guid,
        group_info: heartbeat::GroupInfo,
    ) -> RtpsResult<()> {
        let writer = self.find_writer_mut(writer_guid)?;

        let previous = writer.heartbeat_group_info.map(|info| {
            (info.current_gsn.to_i64(), info.first_gsn.to_i64(), info.last_gsn.to_i64())
        });
        writer.heartbeat_group_info = Some(group_info);

        debug!(
            "SHC writer {} heartbeat group info (current, first, last) {:?} -> ({}, {}, {})",
            writer_guid.to_hex_string(),
            previous,
            group_info.current_gsn.to_i64(),
            group_info.first_gsn.to_i64(),
            group_info.last_gsn.to_i64()
        );

        Ok(())
    }

    // A later Gap can cover a shorter range, and what an earlier one declared gone stays gone.
    pub(crate) fn record_gap_group_info(
        &mut self,
        writer_guid: Guid,
        group_info: gap::GroupInfo,
    ) -> RtpsResult<()> {
        let writer = self.find_writer_mut(writer_guid)?;

        let previous = writer.highest_gap_end_gsn;
        let highest = match previous {
            Some(previous) => max(previous, group_info.gap_end_gsn),
            None => group_info.gap_end_gsn,
        };

        writer.highest_gap_end_gsn = Some(highest);

        debug!(
            "SHC writer {} highest gap end group sequence number {:?} -> {}, Gap declared [{}, {}]",
            writer_guid.to_hex_string(),
            previous.map(|gsn| gsn.to_i64()),
            highest.to_i64(),
            group_info.gap_start_gsn.to_i64(),
            group_info.gap_end_gsn.to_i64()
        );

        Ok(())
    }

    // Announced to every Subscriber of the participant, so a writer this one never matched is
    // not an error.
    pub(crate) fn set_writer_liveliness(&mut self, writer_guid: Guid, is_alive: bool) {
        for proxy in self.publishers.values_mut() {
            let Some(writer) = proxy.writers.get_mut(&writer_guid) else {
                continue;
            };

            let previous = writer.is_alive;
            writer.is_alive = is_alive;

            debug!(
                "SHC writer {} liveliness {} -> {}",
                writer_guid.to_hex_string(),
                previous,
                is_alive
            );
        }
    }

    // Registers a matched writer under the Publisher it announced. A writer that announced no
    // Publisher stays unregistered and its samples are dropped in add_change.
    pub(crate) fn add_matched_writer(
        &mut self,
        reader_id: EntityId,
        writer_guid: Guid,
        publisher_guid: Guid,
    ) -> RtpsResult<()> {
        if publisher_guid.entity_id() == EntityId::UNKNOWN {
            return Err(RtpsError::new(
                RtpsErrorCode::DataNotSet,
                format!(
                    "Writer {} announced no owning Publisher and cannot join group ordering",
                    writer_guid.to_hex_string()
                ),
            ));
        }

        debug!(
            "SHC registers writer {} of Publisher {} for reader {}",
            writer_guid.to_hex_string(),
            publisher_guid.to_hex_string(),
            reader_id
        );

        self.publishers
            .entry(publisher_guid)
            .or_insert_with(PublisherProxy::new)
            .writers
            .entry(writer_guid)
            .or_insert_with(WriterGsnInfo::new)
            .reader_ids
            .insert(reader_id);

        Ok(())
    }

    // The writer is gone for every reader at once, so it leaves the group ordering whole. Its
    // held samples stay and go out when their group sequence number is released.
    pub(crate) fn remove_matched_writer(&mut self, writer_guid: Guid) {
        for (publisher_guid, proxy) in self.publishers.iter_mut() {
            if proxy.writers.remove(&writer_guid).is_none() {
                continue;
            }

            debug!(
                "SHC dropped writer {} of Publisher {} from group ordering",
                writer_guid.to_hex_string(),
                publisher_guid.to_hex_string()
            );
        }
    }

    // Drops everything held for a deleted reader. Its share can no longer be committed.
    pub(crate) fn remove_reader(&mut self, reader_id: EntityId) {
        for (publisher_guid, proxy) in self.publishers.iter_mut() {
            let held_sample_count = Self::held_sample_count(proxy);
            let writer_count = proxy.writers.len();

            proxy.ready.retain(|sample| sample.reader_id != reader_id);

            for samples in proxy.pending.values_mut() {
                samples.retain(|sample| sample.reader_id != reader_id);
            }
            proxy.pending.retain(|_, samples| !samples.is_empty());

            proxy.writers.retain(|_, writer| {
                writer.reader_ids.remove(&reader_id);
                !writer.reader_ids.is_empty()
            });

            let dropped_sample_count = held_sample_count - Self::held_sample_count(proxy);
            let dropped_writer_count = writer_count - proxy.writers.len();

            if dropped_sample_count > 0 || dropped_writer_count > 0 {
                debug!(
                    "SHC removed reader {} from Publisher {}: dropped {} samples and {} writers",
                    reader_id,
                    publisher_guid.to_hex_string(),
                    dropped_sample_count,
                    dropped_writer_count
                );
            }
        }
    }

    fn held_sample_count(proxy: &PublisherProxy) -> usize {
        proxy.ready.len() + proxy.pending.values().map(Vec::len).sum::<usize>()
    }

    // The digest a writer of this Publisher announces covers every writer attached to it, so the
    // comparison set is every discovered writer of that Publisher, not only the matched ones.
    fn discovered_writer_set(&self, publisher_guid: Guid) -> GroupDigest {
        let mut entity_ids = Vec::new();

        for topic in self.remote_publications.iter() {
            for (writer_guid, publication) in topic.value() {
                if publication.group_guid() == Some(publisher_guid) {
                    entity_ids.push(writer_guid.entity_id());
                }
            }
        }

        GroupDigest::from_entity_ids(&entity_ids)
    }

    fn find_publisher_proxy_of_writer_mut(
        &mut self,
        writer_guid: Guid,
    ) -> RtpsResult<&mut PublisherProxy> {
        self.publishers
            .values_mut()
            .find(|proxy| proxy.writers.contains_key(&writer_guid))
            .ok_or_else(|| {
                RtpsError::new(
                    RtpsErrorCode::NotInitialized,
                    format!(
                        "Writer {} is not registered under any Publisher",
                        writer_guid.to_hex_string()
                    ),
                )
            })
    }

    fn find_writer_mut(&mut self, writer_guid: Guid) -> RtpsResult<&mut WriterGsnInfo> {
        self.publishers
            .values_mut()
            .find_map(|proxy| proxy.writers.get_mut(&writer_guid))
            .ok_or_else(|| {
                RtpsError::new(
                    RtpsErrorCode::NotInitialized,
                    format!(
                        "Writer {} is not registered under any Publisher",
                        writer_guid.to_hex_string()
                    ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::instance_handle::InstanceHandle,
        rtps::{
            common::{entity_kind::EntityKind, guid::GroupDigest, types::ChangeKind},
            entities::history::cache_change::PresentationInfo,
        },
    };

    const PREFIX: [u8; 12] = [1; 12];

    fn publisher_guid() -> Guid {
        Guid::new(PREFIX, EntityId::new([0, 1, 0], EntityKind::USER_DEFINED_WRITER_GROUP))
    }

    fn other_publisher_guid() -> Guid {
        Guid::new(PREFIX, EntityId::new([0, 3, 0], EntityKind::USER_DEFINED_WRITER_GROUP))
    }

    fn writer_guid(key: u8) -> Guid {
        Guid::new(PREFIX, EntityId::new([0, 1, key], EntityKind::USER_DEFINED_WRITER_WITH_KEY))
    }

    fn reader_id(key: u8) -> EntityId {
        EntityId::new([0, 2, key], EntityKind::USER_DEFINED_READER_WITH_KEY)
    }

    fn change(writer: Guid, seq: i64, group_seq_num: Option<i64>) -> CacheChange {
        let mut change = CacheChange::new(
            ChangeKind::Alive,
            writer,
            InstanceHandle::default(),
            SequenceNumber::from_i64(seq),
            vec![0u8; 4],
            None,
        );
        change.set_presentation_info(PresentationInfo {
            group_seq_num: group_seq_num.map(SequenceNumber::from_i64),
            ..PresentationInfo::default()
        });

        change
    }

    // One matched writer of one Publisher, seen by one reader.
    fn cache_with_one_matched_writer() -> SubscriberHistoryCache {
        let mut cache = SubscriberHistoryCache::new(Arc::new(DashMap::new()));
        cache
            .add_matched_writer(reader_id(1), writer_guid(1), publisher_guid())
            .expect("registered");

        cache
    }

    fn pending_group_sequence_numbers(cache: &SubscriberHistoryCache) -> Vec<i64> {
        cache.publishers[&publisher_guid()].pending.keys().map(SequenceNumber::to_i64).collect()
    }

    #[test]
    fn holds_a_sample_under_its_group_sequence_number() {
        let mut cache = cache_with_one_matched_writer();

        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(7)), true).expect("accepted");

        assert_eq!(pending_group_sequence_numbers(&cache), vec![7]);
        assert!(cache.publishers[&publisher_guid()].ready.is_empty());
    }

    // The same sample is copied per reader, and both copies must leave together.
    #[test]
    fn holds_both_reader_copies_under_one_group_sequence_number() {
        let mut cache = cache_with_one_matched_writer();
        cache
            .add_matched_writer(reader_id(2), writer_guid(1), publisher_guid())
            .expect("registered");

        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(7)), true).expect("accepted");
        cache.add_change(reader_id(2), change(writer_guid(1), 1, Some(7)), true).expect("accepted");

        let held = &cache.publishers[&publisher_guid()].pending[&SequenceNumber::from_i64(7)];
        assert_eq!(held.len(), 2);
        assert_eq!(held[0].reader_id, reader_id(1));
        assert_eq!(held[1].reader_id, reader_id(2));
    }

    // A late copy of a position the writer already passed cannot be judged again, condition a
    // will never hold for it a second time.
    #[test]
    fn sends_a_position_the_writer_passed_straight_to_ready() {
        let mut cache = cache_with_one_matched_writer();
        cache.find_writer_mut(writer_guid(1)).expect("writer registered").max_committed_gsn =
            Some(SequenceNumber::from_i64(7));

        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(7)), true).expect("accepted");

        assert_eq!(cache.publishers[&publisher_guid()].ready.len(), 1);
        assert!(pending_group_sequence_numbers(&cache).is_empty());
    }

    #[test]
    fn drops_a_sample_without_a_group_sequence_number() {
        let mut cache = cache_with_one_matched_writer();

        let result = cache.add_change(reader_id(1), change(writer_guid(1), 1, None), true);

        assert_eq!(result.expect_err("rejected").code, RtpsErrorCode::GroupSequenceNumberNotSet);
        assert!(pending_group_sequence_numbers(&cache).is_empty());
        assert!(cache.publishers[&publisher_guid()].ready.is_empty());
    }

    // A writer that announced no Publisher cannot take part in group ordering, so holding its
    // samples would leave a hole no event can fill.
    #[test]
    fn drops_a_sample_from_an_unregistered_writer() {
        let mut cache = cache_with_one_matched_writer();

        let result = cache.add_change(reader_id(1), change(writer_guid(2), 1, Some(7)), true);

        assert_eq!(result.expect_err("rejected").code, RtpsErrorCode::NotInitialized);
        assert!(pending_group_sequence_numbers(&cache).is_empty());
    }

    #[test]
    fn refuses_a_writer_that_announced_no_publisher() {
        let mut cache = SubscriberHistoryCache::new(Arc::new(DashMap::new()));

        let result = cache.add_matched_writer(reader_id(1), writer_guid(1), Guid::UNKNOWN);

        assert_eq!(result.expect_err("rejected").code, RtpsErrorCode::DataNotSet);
        assert!(cache.publishers.is_empty());
    }

    // The samples it already delivered keep their place, only the writer leaves the gate.
    #[test]
    fn drops_an_unmatched_writer_and_keeps_its_held_samples() {
        let mut cache = cache_with_one_matched_writer();
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(7)), true).expect("accepted");

        cache.remove_matched_writer(writer_guid(1));

        assert!(cache.publishers[&publisher_guid()].writers.is_empty());
        assert_eq!(pending_group_sequence_numbers(&cache), vec![7]);
    }

    // A writer stays while another reader of this Subscriber still matches it.
    #[test]
    fn keeps_a_writer_whose_other_reader_still_matches_it() {
        let mut cache = cache_with_one_matched_writer();
        cache
            .add_matched_writer(reader_id(2), writer_guid(1), publisher_guid())
            .expect("registered");
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(7)), true).expect("accepted");
        cache.add_change(reader_id(2), change(writer_guid(1), 1, Some(7)), true).expect("accepted");

        cache.remove_reader(reader_id(1));

        assert!(cache.publishers[&publisher_guid()].writers.contains_key(&writer_guid(1)));

        let held = &cache.publishers[&publisher_guid()].pending[&SequenceNumber::from_i64(7)];
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].reader_id, reader_id(2));
    }

    #[test]
    fn drops_the_group_sequence_number_a_removed_reader_alone_held() {
        let mut cache = cache_with_one_matched_writer();
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(7)), true).expect("accepted");

        cache.remove_reader(reader_id(1));

        assert!(pending_group_sequence_numbers(&cache).is_empty());
        assert!(cache.publishers[&publisher_guid()].writers.is_empty());
    }

    fn heartbeat_group_info(
        current_gsn: i64,
        first_gsn: i64,
        last_gsn: i64,
    ) -> heartbeat::GroupInfo {
        heartbeat::GroupInfo {
            current_gsn: SequenceNumber::from_i64(current_gsn),
            first_gsn: SequenceNumber::from_i64(first_gsn),
            last_gsn: SequenceNumber::from_i64(last_gsn),
            writer_set: GroupDigest::EMPTY,
            secure_writer_set: GroupDigest::EMPTY,
        }
    }

    #[test]
    fn reports_a_record_for_a_writer_that_is_not_registered() {
        let mut cache = cache_with_one_matched_writer();

        let result =
            cache.record_heartbeat_group_info(writer_guid(2), heartbeat_group_info(5, 1, 5));

        assert_eq!(result.expect_err("rejected").code, RtpsErrorCode::NotInitialized);
    }

    // firstGSN and lastGSN move forward as the writer's history rolls, so keeping the highest
    // pair would claim the writer still holds samples it has already dropped.
    #[test]
    fn records_the_group_info_of_the_latest_heartbeat() {
        let mut cache = cache_with_one_matched_writer();

        cache
            .record_heartbeat_group_info(writer_guid(1), heartbeat_group_info(5, 1, 5))
            .expect("writer registered");
        cache
            .record_heartbeat_group_info(writer_guid(1), heartbeat_group_info(9, 4, 9))
            .expect("writer registered");

        let writer = cache.find_writer_mut(writer_guid(1)).expect("writer registered");
        assert_eq!(writer.heartbeat_group_info, Some(heartbeat_group_info(9, 4, 9)));
    }

    // A Gap declares its range gone for good, and a later Gap can cover a shorter one. Replacing
    // the value would lose the proof that the group sequence numbers up to 7 will never arrive.
    #[test]
    fn keeps_the_furthest_gap_end_group_sequence_number() {
        let mut cache = cache_with_one_matched_writer();

        cache
            .record_gap_group_info(
                writer_guid(1),
                gap::GroupInfo {
                    gap_start_gsn: SequenceNumber::from_i64(3),
                    gap_end_gsn: SequenceNumber::from_i64(7),
                },
            )
            .expect("writer registered");
        cache
            .record_gap_group_info(
                writer_guid(1),
                gap::GroupInfo {
                    gap_start_gsn: SequenceNumber::from_i64(4),
                    gap_end_gsn: SequenceNumber::from_i64(4),
                },
            )
            .expect("writer registered");

        let writer = cache.find_writer_mut(writer_guid(1)).expect("writer registered");
        assert_eq!(writer.highest_gap_end_gsn, Some(SequenceNumber::from_i64(7)));
    }

    fn gap_group_info(gap_start_gsn: i64, gap_end_gsn: i64) -> gap::GroupInfo {
        gap::GroupInfo {
            gap_start_gsn: SequenceNumber::from_i64(gap_start_gsn),
            gap_end_gsn: SequenceNumber::from_i64(gap_end_gsn),
        }
    }

    fn set_last_committed_group_seq_num(cache: &mut SubscriberHistoryCache, last_committed: i64) {
        cache
            .publishers
            .get_mut(&publisher_guid())
            .expect("Publisher registered")
            .last_committed_gsn = Some(SequenceNumber::from_i64(last_committed));
    }

    fn released_group_seq_nums(released: &[PendingSample]) -> Vec<i64> {
        released
            .iter()
            .map(|sample| {
                sample
                    .change
                    .presentation_info()
                    .group_seq_num
                    .unwrap_or(SequenceNumber::UNKNOWN)
                    .to_i64()
            })
            .collect()
    }

    // Condition a with nothing committed yet: the group starts at 1, so 1 has no predecessor.
    #[test]
    fn releases_the_first_group_sequence_number() {
        let mut cache = cache_with_one_matched_writer();
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(1)), true).expect("accepted");

        let released = cache.flush_pending_changes();

        assert_eq!(released_group_seq_nums(&released), vec![1]);
        assert!(pending_group_sequence_numbers(&cache).is_empty());
    }

    #[test]
    fn releases_a_run_of_group_sequence_numbers_in_order() {
        let mut cache = cache_with_one_matched_writer();
        for group_seq_num in 1..=3 {
            cache
                .add_change(
                    reader_id(1),
                    change(writer_guid(1), group_seq_num, Some(group_seq_num)),
                    true,
                )
                .expect("accepted");
        }

        let released = cache.flush_pending_changes();

        assert_eq!(released_group_seq_nums(&released), vec![1, 2, 3]);
    }

    // Every reader's copy of one position leaves in the same flush.
    #[test]
    fn releases_the_copies_of_one_group_sequence_number_together() {
        let mut cache = cache_with_one_matched_writer();
        cache
            .add_matched_writer(reader_id(2), writer_guid(1), publisher_guid())
            .expect("registered");
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(1)), true).expect("accepted");
        cache.add_change(reader_id(2), change(writer_guid(1), 1, Some(1)), true).expect("accepted");

        let released = cache.flush_pending_changes();

        assert_eq!(released_group_seq_nums(&released), vec![1, 1]);
        assert_eq!(released[0].reader_id, reader_id(1));
        assert_eq!(released[1].reader_id, reader_id(2));
    }

    // Condition a alone cannot cross a hole, and no writer has said anything about it yet.
    #[test]
    fn holds_a_group_sequence_number_behind_a_hole() {
        let mut cache = cache_with_one_matched_writer();
        set_last_committed_group_seq_num(&mut cache, 4);
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(6)), true).expect("accepted");

        let released = cache.flush_pending_changes();

        assert!(released.is_empty());
        assert_eq!(pending_group_sequence_numbers(&cache), vec![6]);
    }

    // Two alive writers, group sequence number 5 lost. Writer 1 announces it never held 5 and
    // writer 2 declared it unavailable, so nothing will ever fill it.
    fn cache_with_two_writers_that_left_five_behind() -> SubscriberHistoryCache {
        let mut cache = cache_with_one_matched_writer();
        cache
            .add_matched_writer(reader_id(1), writer_guid(2), publisher_guid())
            .expect("registered");
        set_last_committed_group_seq_num(&mut cache, 4);

        cache
            .record_heartbeat_group_info(writer_guid(1), heartbeat_group_info(6, 6, 6))
            .expect("writer registered");
        cache
            .record_gap_group_info(writer_guid(2), gap_group_info(5, 5))
            .expect("writer registered");

        cache
    }

    #[test]
    fn releases_a_hole_every_alive_writer_left_behind() {
        let mut cache = cache_with_two_writers_that_left_five_behind();
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(6)), true).expect("accepted");

        let released = cache.flush_pending_changes();

        assert_eq!(released_group_seq_nums(&released), vec![6]);
    }

    // The announced writer set covers writers we have not discovered, so the Heartbeat does not
    // speak for the whole group.
    #[test]
    fn holds_when_the_announced_writer_set_does_not_match() {
        let mut cache = cache_with_two_writers_that_left_five_behind();
        let unknown_writer_set = GroupDigest::from_entity_ids(&[writer_guid(9).entity_id()]);
        let mut group_info = heartbeat_group_info(6, 6, 6);
        group_info.writer_set = unknown_writer_set;
        cache.record_heartbeat_group_info(writer_guid(1), group_info).expect("writer registered");
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(6)), true).expect("accepted");

        let released = cache.flush_pending_changes();

        assert!(released.is_empty());
    }

    // Both writers declared 5 gone, but without a Heartbeat nobody vouches that the group is at
    // or past 6 with a fully discovered writer set.
    #[test]
    fn holds_when_no_heartbeat_covers_the_hole() {
        let mut cache = cache_with_one_matched_writer();
        cache
            .add_matched_writer(reader_id(1), writer_guid(2), publisher_guid())
            .expect("registered");
        set_last_committed_group_seq_num(&mut cache, 4);
        cache
            .record_gap_group_info(writer_guid(1), gap_group_info(5, 5))
            .expect("writer registered");
        cache
            .record_gap_group_info(writer_guid(2), gap_group_info(5, 5))
            .expect("writer registered");
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(6)), true).expect("accepted");

        let released = cache.flush_pending_changes();

        assert!(released.is_empty());
    }

    // The only covering Heartbeat came from a writer that lost liveliness, and a dead writer does
    // not speak for the group.
    #[test]
    fn holds_when_only_a_dead_writer_covers_the_hole() {
        let mut cache = cache_with_two_writers_that_left_five_behind();
        cache.add_change(reader_id(1), change(writer_guid(2), 1, Some(6)), true).expect("accepted");

        cache.set_writer_liveliness(writer_guid(1), false);

        assert!(cache.flush_pending_changes().is_empty());
    }

    // Writer 1 covers 6 and never held 5. Writer 2 is alive but has said nothing, so 5 might still
    // be on its way from writer 2 and 6 is held.
    fn cache_with_a_silent_second_writer() -> SubscriberHistoryCache {
        let mut cache = cache_with_one_matched_writer();
        cache
            .add_matched_writer(reader_id(1), writer_guid(2), publisher_guid())
            .expect("registered");
        set_last_committed_group_seq_num(&mut cache, 4);
        cache
            .record_heartbeat_group_info(writer_guid(1), heartbeat_group_info(6, 6, 6))
            .expect("writer registered");
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(6)), true).expect("accepted");

        cache
    }

    #[test]
    fn holds_when_one_alive_writer_has_not_left_the_hole_behind() {
        let mut cache = cache_with_a_silent_second_writer();

        let released = cache.flush_pending_changes();

        assert!(released.is_empty());
        assert_eq!(pending_group_sequence_numbers(&cache), vec![6]);
    }

    // The held position is judged again on the next event, not only when it arrives.
    #[test]
    fn releases_a_hole_once_the_silent_writer_declares_it_gone() {
        let mut cache = cache_with_a_silent_second_writer();
        assert!(cache.flush_pending_changes().is_empty());

        cache
            .record_gap_group_info(writer_guid(2), gap_group_info(5, 5))
            .expect("writer registered");

        assert_eq!(released_group_seq_nums(&cache.flush_pending_changes()), vec![6]);
    }

    // An unmatched writer will send nothing more, so it stops being waited for.
    #[test]
    fn releases_a_hole_once_the_silent_writer_is_unmatched() {
        let mut cache = cache_with_a_silent_second_writer();
        assert!(cache.flush_pending_changes().is_empty());

        cache.remove_matched_writer(writer_guid(2));

        assert_eq!(released_group_seq_nums(&cache.flush_pending_changes()), vec![6]);
    }

    // A writer that lost liveliness is not waited for.
    #[test]
    fn releases_a_hole_when_the_silent_writer_lost_liveliness() {
        let mut cache = cache_with_a_silent_second_writer();

        cache.set_writer_liveliness(writer_guid(2), false);

        assert_eq!(released_group_seq_nums(&cache.flush_pending_changes()), vec![6]);
    }

    // Once the writer is alive again its silence counts again.
    #[test]
    fn holds_again_when_the_silent_writer_regains_liveliness() {
        let mut cache = cache_with_a_silent_second_writer();
        cache.set_writer_liveliness(writer_guid(2), false);
        assert_eq!(released_group_seq_nums(&cache.flush_pending_changes()), vec![6]);

        cache.set_writer_liveliness(writer_guid(2), true);
        cache
            .record_heartbeat_group_info(writer_guid(1), heartbeat_group_info(8, 8, 8))
            .expect("writer registered");
        cache.add_change(reader_id(1), change(writer_guid(1), 2, Some(8)), true).expect("accepted");

        assert!(cache.flush_pending_changes().is_empty());
        assert_eq!(pending_group_sequence_numbers(&cache), vec![8]);

        // The silent writer finally delivers 7, and 8 follows it in order.
        cache.add_change(reader_id(1), change(writer_guid(2), 1, Some(7)), true).expect("accepted");

        assert_eq!(released_group_seq_nums(&cache.flush_pending_changes()), vec![7, 8]);
    }

    // The release records how far each writer got, which is what a late copy of that position
    // is measured against.
    #[test]
    fn releases_a_late_copy_without_judging_it() {
        let mut cache = cache_with_one_matched_writer();
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(1)), true).expect("accepted");
        assert_eq!(released_group_seq_nums(&cache.flush_pending_changes()), vec![1]);

        cache.add_change(reader_id(2), change(writer_guid(1), 1, Some(1)), true).expect("accepted");
        cache.add_change(reader_id(1), change(writer_guid(1), 2, Some(3)), true).expect("accepted");

        let released = cache.flush_pending_changes();

        assert_eq!(released_group_seq_nums(&released), vec![1]);
        assert_eq!(pending_group_sequence_numbers(&cache), vec![3]);
    }

    // A hole in one Publisher does not hold back another Publisher's in-order samples.
    #[test]
    fn keeps_a_hole_in_one_publisher_from_blocking_another() {
        let mut cache = cache_with_one_matched_writer();
        cache
            .add_matched_writer(reader_id(1), writer_guid(5), other_publisher_guid())
            .expect("registered");
        set_last_committed_group_seq_num(&mut cache, 4);
        cache.add_change(reader_id(1), change(writer_guid(1), 1, Some(6)), true).expect("accepted");
        cache.add_change(reader_id(1), change(writer_guid(5), 1, Some(1)), true).expect("accepted");

        let released = cache.flush_pending_changes();

        assert_eq!(released_group_seq_nums(&released), vec![1]);
        assert_eq!(released[0].change.writer_guid(), writer_guid(5));
        assert_eq!(pending_group_sequence_numbers(&cache), vec![6]);
    }

    // Writers of the Publisher are collected across topics. Writers of another Publisher and
    // writers with no Publisher are left out even when they share a topic.
    #[test]
    fn collects_a_publisher_writers_across_topics_and_skips_the_rest() {
        let remote_publications = Arc::new(DashMap::new());
        let mut first_topic = HashMap::new();
        first_topic.insert(writer_guid(1), publication_of(publisher_guid()));
        first_topic.insert(writer_guid(3), publication_of(Guid::UNKNOWN));
        first_topic.insert(writer_guid(4), publication_of(other_publisher_guid()));
        let mut second_topic = HashMap::new();
        second_topic.insert(writer_guid(2), publication_of(publisher_guid()));
        remote_publications.insert("first".to_string(), first_topic);
        remote_publications.insert("second".to_string(), second_topic);

        let cache = SubscriberHistoryCache::new(remote_publications);

        assert_eq!(
            cache.discovered_writer_set(publisher_guid()),
            GroupDigest::from_entity_ids(&[writer_guid(1).entity_id(), writer_guid(2).entity_id()])
        );
    }

    fn publication_of(publisher_guid: Guid) -> PublicationBuiltinTopicData {
        let mut publication = PublicationBuiltinTopicData::default();

        if publisher_guid != Guid::UNKNOWN {
            publication.set_group_guid(publisher_guid);
        }

        publication
    }
}
