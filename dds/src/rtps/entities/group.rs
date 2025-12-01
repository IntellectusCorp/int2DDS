//! Group trait for Publishers and Subscribers.
//!
//! This module defines the `Group` trait representing RTPS Publishers and Subscribers.
//! Groups contain collections of endpoints (readers or writers) and map to DDS Publishers
//! and Subscribers.

#![allow(dead_code)]
#![allow(unused_variables)]

use std::sync::Arc;

use crate::{
    infrastructure::status::{StatusInfo, StatusKind},
    rtps::common::guid::Guid,
};

use super::{entity::Entity, reader::Reader, writer::Writer};

pub(crate) trait Group: Entity {
    /*
    • The RTPS Publisher contains RTPS Writer endpoints. The RTPS Publisher maps to a DDS
    Publisher.
    • The RTPS Subscriber contains RTPS Reader endpoints. The RTPS Subscriber maps to a DDS
    Subscriber.
     */
}

pub(crate) struct Publisher {
    guid: Guid,
    writers: Vec<Box<dyn Writer>>,
}

impl Group for Publisher {}

impl Entity for Publisher {
    fn guid(&self) -> Guid {
        self.guid
    }

    fn set_update_status(
        &self,
        f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    ) {
        todo!();
    }

    fn update_status(&self, status: StatusKind, info: Option<Arc<dyn StatusInfo>>) {
        todo!()
    }
}

pub(crate) struct Subscriber {
    guid: Guid,
    readers: Vec<Box<dyn Reader>>,
}

impl Group for Subscriber {}

impl Entity for Subscriber {
    fn guid(&self) -> Guid {
        self.guid
    }

    fn set_update_status(
        &self,
        f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    ) {
        todo!();
    }

    fn update_status(&self, status: StatusKind, info: Option<Arc<dyn StatusInfo>>) {
        todo!()
    }
}
