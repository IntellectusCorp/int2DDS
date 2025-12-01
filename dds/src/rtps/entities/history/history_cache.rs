use std::sync::Arc;

use crate::{
    common::instance_handle::InstanceHandle,
    rtps::{
        common::{rtps_error_code::RtpsResult, sequence::SequenceNumber},
        entities::history::cache_change::CacheChange,
    },
};

pub(crate) trait HistoryCache {
    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> RtpsResult<()>;
    fn is_builtin(&self) -> bool;
    fn get_changes(&self) -> Vec<Arc<CacheChange>>;
    fn get_change_from_instance_handle(
        &self,
        instance_handle: InstanceHandle,
    ) -> Vec<Arc<CacheChange>>;

    fn get_seq_num_min(&self) -> SequenceNumber {
        let changes = self.get_changes();
        let min_change = changes.iter().min_by_key(|change| change.sequence_number());
        match min_change {
            Some(change) => change.sequence_number(),
            None => SequenceNumber::new(0, 1),
        }
    }

    fn get_seq_num_max(&self) -> SequenceNumber {
        let changes = self.get_changes();
        let max_change = changes.iter().max_by_key(|change| change.sequence_number());
        match max_change {
            Some(change) => change.sequence_number(),
            None => SequenceNumber::new(0, 0),
        }
    }
}
