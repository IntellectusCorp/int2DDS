use serde::{Deserialize, Serialize};

use crate::config::types::entity_qos::{
    DataReaderQos, DataReaderQosSeq, DataWriterQos, DataWriterQosSeq, DomainParticipantQos,
    DomainParticipantQosSeq, PublisherQos, PublisherQosSeq, SubscriberQos, SubscriberQosSeq,
    TopicQos, TopicQosSeq,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum SingleOrSeq<T, N> {
    Single(T),
    Seq(N),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct QosProfile {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    /// When true, this profile is used as the default when an entity is created
    /// without an explicit profile path (mirrors RTI Connext `is_default_qos`
    /// and FastDDS `is_default_profile`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) is_default_profile: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) domain_participant_qos:
        Option<SingleOrSeq<DomainParticipantQos, DomainParticipantQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic_qos: Option<SingleOrSeq<TopicQos, TopicQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) publisher_qos: Option<SingleOrSeq<PublisherQos, PublisherQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subscriber_qos: Option<SingleOrSeq<SubscriberQos, SubscriberQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datawriter_qos: Option<SingleOrSeq<DataWriterQos, DataWriterQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datareader_qos: Option<SingleOrSeq<DataReaderQos, DataReaderQosSeq>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct QosLibrary {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) qos_profiles: Option<Vec<QosProfile>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) domain_participant_qos:
        Option<SingleOrSeq<DomainParticipantQos, DomainParticipantQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic_qos: Option<SingleOrSeq<TopicQos, TopicQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) publisher_qos: Option<SingleOrSeq<PublisherQos, PublisherQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subscriber_qos: Option<SingleOrSeq<SubscriberQos, SubscriberQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datawriter_qos: Option<SingleOrSeq<DataWriterQos, DataWriterQosSeq>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datareader_qos: Option<SingleOrSeq<DataReaderQos, DataReaderQosSeq>>,
}
