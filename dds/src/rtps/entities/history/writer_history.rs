use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Weak,
};
use std::time::Instant;

use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode};
use crate::{
    common::instance_handle::InstanceHandle,
    rtps::{
        builtin::builtin_endpoints::BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY,
        common::{entity_id::EntityId, rtps_error_code::RtpsResult, sequence::SequenceNumber},
        entities::entity::Entity,
        entities::history::{cache_change::CacheChange, history_cache::HistoryCache},
        entities::participant::Participant,
        entities::writer::Writer,
        task::sending_handler::{MessageType, SendingHandler},
    },
};

static RTPS_WRITER_HISTORY_PROFILE_COUNT: AtomicU64 = AtomicU64::new(0);
static RTPS_WRITER_HISTORY_PROFILE_INSERT_US: AtomicU64 = AtomicU64::new(0);
static RTPS_WRITER_HISTORY_PROFILE_HIGHEST_SN_US: AtomicU64 = AtomicU64::new(0);
static RTPS_WRITER_HISTORY_PROFILE_SEND_UNSENT_US: AtomicU64 = AtomicU64::new(0);
static RTPS_WRITER_HISTORY_PROFILE_TOTAL_US: AtomicU64 = AtomicU64::new(0);

fn rtps_writer_history_profile_enabled() -> bool {
    std::env::var_os("RMW_INT2DDS_PROFILE").is_some()
}

fn elapsed_us(start: Instant, end: Instant) -> u64 {
    end.duration_since(start).as_micros() as u64
}

fn record_rtps_writer_history_profile(
    insert_us: u64,
    highest_sn_us: u64,
    send_unsent_us: u64,
    total_us: u64,
) {
    let n = RTPS_WRITER_HISTORY_PROFILE_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    RTPS_WRITER_HISTORY_PROFILE_INSERT_US.fetch_add(insert_us, Ordering::Relaxed);
    RTPS_WRITER_HISTORY_PROFILE_HIGHEST_SN_US.fetch_add(highest_sn_us, Ordering::Relaxed);
    RTPS_WRITER_HISTORY_PROFILE_SEND_UNSENT_US.fetch_add(send_unsent_us, Ordering::Relaxed);
    RTPS_WRITER_HISTORY_PROFILE_TOTAL_US.fetch_add(total_us, Ordering::Relaxed);

    if n % 300 == 0 {
        let divisor = n as f64;
        eprintln!(
            "INT2DDS_RTPS_WRITER_HISTORY_PROFILE count={} total_avg_us={:.3} insert_avg_us={:.3} highest_sn_avg_us={:.3} send_unsent_avg_us={:.3}",
            n,
            RTPS_WRITER_HISTORY_PROFILE_TOTAL_US.load(Ordering::Relaxed) as f64 / divisor,
            RTPS_WRITER_HISTORY_PROFILE_INSERT_US.load(Ordering::Relaxed) as f64 / divisor,
            RTPS_WRITER_HISTORY_PROFILE_HIGHEST_SN_US.load(Ordering::Relaxed) as f64 / divisor,
            RTPS_WRITER_HISTORY_PROFILE_SEND_UNSENT_US.load(Ordering::Relaxed) as f64 / divisor,
        );
    }
}

#[derive(Debug)]
pub struct WriterHistoryCache {
    participant: Weak<Participant>,
    owner_id: EntityId,
    changes: BTreeMap<SequenceNumber, Arc<CacheChange>>,
    highest_sn: SequenceNumber, // Highest sequence number ever added to this cache
}

impl HistoryCache for WriterHistoryCache {
    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> RtpsResult<()> {
        self.changes.remove(&a_change.sequence_number());

        if let Some(participant) = self.participant.upgrade() {
            if let Some(handler) =
                SendingHandler::get_instance_by_participant_guid(participant.guid())
            {
                handler.push_message_and_wake(MessageType::OnUserCacheChangeRemoval(
                    true,
                    a_change.sequence_number(),
                    a_change.writer_guid().entity_id(),
                ));
            }
        }

        Ok(())
    }

    fn is_builtin(&self) -> bool {
        self.owner_id.entity_kind().is_built_in()
    }

    fn get_changes(&self) -> Vec<Arc<CacheChange>> {
        self.changes.values().cloned().collect()
    }

    fn get_change_from_instance_handle(
        &self,
        instance_handle: InstanceHandle,
    ) -> Vec<Arc<CacheChange>> {
        self.changes
            .values()
            .filter(|change| change.instance_handle() == instance_handle)
            .cloned()
            .collect()
    }

    fn get_seq_num_min(&self) -> Option<SequenceNumber> {
        self.changes.keys().next().copied()
    }

    fn get_seq_num_max(&self) -> Option<SequenceNumber> {
        self.changes.keys().next_back().copied()
    }
}

impl WriterHistoryCache {
    pub(crate) fn new(participant: Weak<Participant>, owner_id: EntityId) -> Self {
        Self {
            participant,
            owner_id,
            changes: BTreeMap::new(),
            highest_sn: SequenceNumber::new(0, 0),
        }
    }

    pub(crate) fn get_change(&self, seq_num: SequenceNumber) -> Option<Arc<CacheChange>> {
        self.changes.get(&seq_num).cloned()
    }

    pub(crate) fn add_change(
        &mut self,
        a_change: Arc<CacheChange>,
        writer: &(dyn Writer + Send + Sync),
    ) -> RtpsResult<()> {
        let profile = rtps_writer_history_profile_enabled();
        let total_t0 = Instant::now();
        if self.is_builtin() {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidEntityKind,
                "add_change must not be called on a built-in writer history",
            ));
        }

        let sn = a_change.sequence_number();
        let insert_t0 = Instant::now();
        self.changes.insert(sn, a_change.clone());
        let insert_us = if profile { elapsed_us(insert_t0, Instant::now()) } else { 0 };

        let highest_t0 = Instant::now();
        if sn > self.highest_sn {
            self.highest_sn = sn;
        }
        let highest_us = if profile { elapsed_us(highest_t0, Instant::now()) } else { 0 };

        let send_t0 = Instant::now();
        if let Some(participant) = self.participant.upgrade() {
            let (_, _, user_logic_arc) = participant.get_logics();
            if let Some(user_logic) = user_logic_arc.as_ref() {
                user_logic.send_unsent_changes(writer, self)?
            }
        }
        let send_us = if profile { elapsed_us(send_t0, Instant::now()) } else { 0 };

        if profile {
            record_rtps_writer_history_profile(
                insert_us,
                highest_us,
                send_us,
                elapsed_us(total_t0, Instant::now()),
            );
        }

        Ok(())
    }

    pub(crate) fn add_change_builtin(&mut self, a_change: Arc<CacheChange>) -> RtpsResult<()> {
        if !self.is_builtin() {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidEntityKind,
                "add_change_builtin must not be called on a non-built-in writer history",
            ));
        }

        let sn = a_change.sequence_number();

        // Built-in endpoint not connected to DDS entity, Resource limits & History QoS not applied
        // Therefore arbitrarily limit size
        if self.changes.len() >= BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY {
            self.changes.pop_first();
        }

        self.changes.insert(sn, a_change.clone());

        if sn > self.highest_sn {
            self.highest_sn = sn;
        }

        Ok(())
    }

    pub(crate) fn highest_sn(&self) -> SequenceNumber {
        self.highest_sn
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.changes.len()
    }

    /// Returns the next change with a sequence number strictly greater than `sn`.
    /// Uses BTreeMap range query for O(log N) lookup.
    pub(crate) fn next_change_after(&self, sn: SequenceNumber) -> Option<Arc<CacheChange>> {
        self.changes
            .range((Bound::Excluded(sn), Bound::Unbounded))
            .next()
            .map(|(_, change)| change.clone())
    }
}
