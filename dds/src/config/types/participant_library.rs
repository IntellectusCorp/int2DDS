use serde::{Deserialize, Serialize};

use crate::config::types::entity_qos::{
    DataReaderQos, DataWriterQos, DomainParticipantQos, PublisherQos, SubscriberQos,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ParticipantLibrary {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) domain_participants: Vec<ParticipantDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ParticipantDecl {
    pub(crate) name: String,
    /// `DomainLibrary::Domain` whose topics `topic_ref`s resolve against.
    pub(crate) domain_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) domain_participant_qos: Option<DomainParticipantQos>,
    #[serde(default)]
    pub(crate) publishers: Vec<PublisherDecl>,
    #[serde(default)]
    pub(crate) subscribers: Vec<SubscriberDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PublisherDecl {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) publisher_qos: Option<PublisherQos>,
    #[serde(default)]
    pub(crate) data_writers: Vec<DataWriterDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DataWriterDecl {
    pub(crate) name: String,
    pub(crate) topic_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datawriter_qos: Option<DataWriterQos>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SubscriberDecl {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subscriber_qos: Option<SubscriberQos>,
    #[serde(default)]
    pub(crate) data_readers: Vec<DataReaderDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DataReaderDecl {
    pub(crate) name: String,
    pub(crate) topic_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) datareader_qos: Option<DataReaderQos>,
}
