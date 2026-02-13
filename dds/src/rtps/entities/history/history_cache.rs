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

    fn get_seq_num_min(&self) -> Option<SequenceNumber>;
    fn get_seq_num_max(&self) -> Option<SequenceNumber>;
}
