#![allow(dead_code)]

use std::{
    cmp::max,
    collections::{BTreeMap, HashMap, HashSet},
};

use log::debug;

use crate::rtps::{
    common::{
        entity_id::EntityId,
        guid::Guid,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::SequenceNumber,
    },
    entities::history::cache_change::CacheChange,
    messages::submessages::{gap, heartbeat},
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
}

// Holds the samples a GROUP access scope Subscriber may not hand over yet, ordered per remote
// Publisher by group sequence number.
#[derive(Debug)]
pub(crate) struct SubscriberHistoryCache {
    publishers: HashMap<Guid, PublisherProxy>,
}

impl SubscriberHistoryCache {
    pub(crate) fn new() -> Self {
        Self { publishers: HashMap::new() }
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
        let mut cache = SubscriberHistoryCache::new();
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
        let mut cache = SubscriberHistoryCache::new();

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
}
