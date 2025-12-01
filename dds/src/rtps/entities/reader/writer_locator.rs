#![allow(dead_code)]
#![allow(unused_variables)]

use crate::{
    common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
    rtps::common::{guid::Guid, locator::Locator},
};

#[derive(Debug, Clone)]
pub(crate) struct WriterLocator {
    remote_writer_guid: Guid,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,

    // Additional fields
    is_active: bool,
    publication_builtin_topic_data: PublicationBuiltinTopicData,
}

impl PartialEq for WriterLocator {
    fn eq(&self, other: &Self) -> bool {
        self.remote_writer_guid == other.remote_writer_guid
            && self.unicast_locator_list == other.unicast_locator_list
            && self.multicast_locator_list == other.multicast_locator_list
    }
}

impl Eq for WriterLocator {}

impl WriterLocator {
    pub(crate) fn new(
        remote_writer_guid: Guid,
        unicast_locator_list: Vec<Locator>,
        multicast_locator_list: Vec<Locator>,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> Self {
        Self {
            unicast_locator_list,

            multicast_locator_list,

            remote_writer_guid,

            is_active: false,
            publication_builtin_topic_data,
        }
    }

    pub(crate) fn remote_writer_guid(&self) -> Guid {
        self.remote_writer_guid
    }

    pub(crate) fn publication_builtin_topic_data(&self) -> PublicationBuiltinTopicData {
        self.publication_builtin_topic_data.clone()
    }

    pub(crate) fn set_publication_builtin_topic_data(&mut self, data: PublicationBuiltinTopicData) {
        self.publication_builtin_topic_data = data;
    }
}
