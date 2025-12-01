//! Base entity trait for RTPS entities.
//!
//! This module defines the `Entity` trait which is the base abstraction for all
//! RTPS entities (participants, readers, writers). All entities have a GUID and
//! can report status changes.

#![allow(dead_code)]
#![allow(unused_variables)]

use std::sync::{Arc, Mutex};

use crate::{
    infrastructure::status::{StatusInfo, StatusKind},
    rtps::{common::guid::Guid, entities::history::cache_change::CacheChange},
};

// Entity's attribute: guid(Guid_t)
pub(crate) trait Entity {
    fn guid(&self) -> Guid;
    fn set_update_status(
        &self,
        f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    );
    fn update_status(&self, status: StatusKind, info: Option<Arc<dyn StatusInfo>>);
    fn set_update_change(&self, f: Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>) {}
    fn get_update_status_callback(
        &self,
    ) -> Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>
    {
        Arc::new(Mutex::new(None))
    }
}
