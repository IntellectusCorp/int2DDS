#![allow(dead_code)]
#![allow(unused_variables)]

use crate::{
    common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
    rtps::common::{guid::Guid, sequence::SequenceNumber},
};

#[derive(Debug, Clone)]
pub(crate) struct RemoteWriterInfo {
    remote_writer_guid: Guid,

    // Additional fields
    expected_sn: SequenceNumber, // Expected next sequence number from writer
    is_active: bool,
    publication_builtin_topic_data: PublicationBuiltinTopicData,
}

impl PartialEq for RemoteWriterInfo {
    fn eq(&self, other: &Self) -> bool {
        self.remote_writer_guid == other.remote_writer_guid
    }
}

impl Eq for RemoteWriterInfo {}

impl RemoteWriterInfo {
    pub(crate) fn new(
        remote_writer_guid: Guid,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> Self {
        Self {
            remote_writer_guid,
            is_active: false,
            publication_builtin_topic_data,
            expected_sn: SequenceNumber::UNKNOWN,
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

    pub(crate) fn expected_sn(&self) -> SequenceNumber {
        self.expected_sn
    }

    pub(crate) fn set_expected_sn(&mut self, sn: SequenceNumber) {
        self.expected_sn = sn;
    }
}
