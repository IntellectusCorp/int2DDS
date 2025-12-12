use serde::{Deserialize, Serialize};

use crate::config::types::entity_qos::{
    DataReaderQosSeq, DataWriterQosSeq, DomainParticipantQosSeq, PublisherQosSeq, SubscriberQosSeq,
    TopicQosSeq,
};

#[derive(Serialize, Deserialize)]
pub(crate) struct QosProfile {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) domain_participant_qos: Option<DomainParticipantQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic_qos: Option<TopicQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) publisher_qos: Option<PublisherQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subscriber_qos: Option<SubscriberQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datawriter_qos: Option<DataWriterQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datareader_qos: Option<DataReaderQosSeq>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct QosLibrary {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) qos_profiles: Option<Vec<QosProfile>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) domain_participant_qos: Option<DomainParticipantQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic_qos: Option<TopicQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) publisher_qos: Option<PublisherQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subscriber_qos: Option<SubscriberQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datawriter_qos: Option<DataWriterQosSeq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datareader_qos: Option<DataReaderQosSeq>,
}
