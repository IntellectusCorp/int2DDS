//! Configuration parameters for Requester and Replier (7.11.1.4.11, 7.11.1.4.12)

use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::publication::publisher::Publisher;
use int2dds::dcps::publication::qos::DataWriterQos;
use int2dds::dcps::publication::qos::PublisherQos;
use int2dds::dcps::subscription::qos::DataReaderQos;
use int2dds::dcps::subscription::qos::SubscriberQos;
use int2dds::dcps::subscription::subscriber::Subscriber;

/// Configuration for constructing a Requester (7.11.1.4.11)
pub struct RequesterParams {
    pub(crate) participant: DomainParticipant, // not Arc, already Arc-wrapped internally
    pub(crate) publisher: Option<Publisher>,
    pub(crate) subscriber: Option<Subscriber>,
    pub(crate) service_name: Option<String>,
    pub(crate) request_topic_name: Option<String>,
    pub(crate) reply_topic_name: Option<String>,
    pub(crate) datawriter_qos: Option<DataWriterQos>,
    pub(crate) datareader_qos: Option<DataReaderQos>,
    pub(crate) publisher_qos: Option<PublisherQos>,
    pub(crate) subscriber_qos: Option<SubscriberQos>,
}

impl RequesterParams {
    pub fn new(participant: DomainParticipant) -> Self {
        Self {
            participant,
            service_name: None,
            request_topic_name: None,
            reply_topic_name: None,
            datawriter_qos: None,
            datareader_qos: None,
            publisher: None,
            subscriber: None,
            publisher_qos: None,
            subscriber_qos: None,
        }
    }

    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    pub fn request_topic_name(mut self, name: impl Into<String>) -> Self {
        self.request_topic_name = Some(name.into());
        self
    }

    pub fn reply_topic_name(mut self, name: impl Into<String>) -> Self {
        self.reply_topic_name = Some(name.into());
        self
    }

    pub fn datawriter_qos(mut self, qos: DataWriterQos) -> Self {
        self.datawriter_qos = Some(qos);
        self
    }

    pub fn datareader_qos(mut self, qos: DataReaderQos) -> Self {
        self.datareader_qos = Some(qos);
        self
    }

    pub fn publisher(mut self, publisher: Publisher) -> Self {
        self.publisher = Some(publisher);
        self
    }

    pub fn subscriber(mut self, subscriber: Subscriber) -> Self {
        self.subscriber = Some(subscriber);
        self
    }

    pub fn publisher_qos(mut self, qos: PublisherQos) -> Self {
        self.publisher_qos = Some(qos);
        self
    }

    pub fn subscriber_qos(mut self, qos: SubscriberQos) -> Self {
        self.subscriber_qos = Some(qos);
        self
    }
}

/// Configuration for constructing a Replier (7.11.1.4.12)
pub struct ReplierParams {
    pub(crate) participant: DomainParticipant,
    pub(crate) publisher: Option<Publisher>,
    pub(crate) subscriber: Option<Subscriber>,
    pub(crate) service_name: Option<String>,
    pub(crate) request_topic_name: Option<String>,
    pub(crate) reply_topic_name: Option<String>,
    pub(crate) datawriter_qos: Option<DataWriterQos>,
    pub(crate) datareader_qos: Option<DataReaderQos>,
    pub(crate) publisher_qos: Option<PublisherQos>,
    pub(crate) subscriber_qos: Option<SubscriberQos>,
}

impl ReplierParams {
    pub fn new(participant: DomainParticipant) -> Self {
        Self {
            participant,
            publisher: None,
            subscriber: None,
            service_name: None,
            request_topic_name: None,
            reply_topic_name: None,
            datawriter_qos: None,
            datareader_qos: None,
            publisher_qos: None,
            subscriber_qos: None,
        }
    }

    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    pub fn request_topic_name(mut self, name: impl Into<String>) -> Self {
        self.request_topic_name = Some(name.into());
        self
    }

    pub fn reply_topic_name(mut self, name: impl Into<String>) -> Self {
        self.reply_topic_name = Some(name.into());
        self
    }

    pub fn datawriter_qos(mut self, qos: DataWriterQos) -> Self {
        self.datawriter_qos = Some(qos);
        self
    }

    pub fn datareader_qos(mut self, qos: DataReaderQos) -> Self {
        self.datareader_qos = Some(qos);
        self
    }

    pub fn publisher(mut self, publisher: Publisher) -> Self {
        self.publisher = Some(publisher);
        self
    }

    pub fn subscriber(mut self, subscriber: Subscriber) -> Self {
        self.subscriber = Some(subscriber);
        self
    }

    pub fn publisher_qos(mut self, qos: PublisherQos) -> Self {
        self.publisher_qos = Some(qos);
        self
    }

    pub fn subscriber_qos(mut self, qos: SubscriberQos) -> Self {
        self.subscriber_qos = Some(qos);
        self
    }
}
