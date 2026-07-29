//! Configuration parameters for Requester, Replier, Client, and Service.
//!
//! Bundles a DomainParticipant with optional QoS, Publisher/Subscriber, and
//! topic name overrides needed to create RPC endpoints.

use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::publication::publisher::Publisher;
use int2dds::dcps::publication::qos::DataWriterQos;
use int2dds::dcps::publication::qos::PublisherQos;
use int2dds::dcps::subscription::qos::DataReaderQos;
use int2dds::dcps::subscription::qos::SubscriberQos;
use int2dds::dcps::subscription::subscriber::Subscriber;

/// Configuration for constructing a Requester
pub struct RequesterParams {
    pub(crate) participant: DomainParticipant, // not Arc, already Arc-wrapped internally
    pub(crate) publisher: Option<Publisher>,
    pub(crate) subscriber: Option<Subscriber>,
    pub(crate) service_name: Option<String>,
    pub(crate) interface_name: Option<String>,
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
            interface_name: None,
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

    pub fn interface_name(mut self, name: impl Into<String>) -> Self {
        self.interface_name = Some(name.into());
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

/// Configuration for constructing a Replier
pub struct ReplierParams {
    pub(crate) participant: DomainParticipant,
    pub(crate) publisher: Option<Publisher>,
    pub(crate) subscriber: Option<Subscriber>,
    pub(crate) service_name: Option<String>,
    pub(crate) interface_name: Option<String>,
    pub(crate) instance_name: Option<String>,
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
            interface_name: None,
            instance_name: None,
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

    pub fn interface_name(mut self, name: impl Into<String>) -> Self {
        self.interface_name = Some(name.into());
        self
    }

    pub fn instance_name(mut self, name: impl Into<String>) -> Self {
        self.instance_name = Some(name.into());
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

#[cfg(test)]
mod tests {
    use super::*;
    use int2dds::dcps::domain::domain_participant_factory::DomainParticipantFactory;
    use int2dds::dcps::domain::qos::DomainParticipantQos;
    use int2dds::dcps::infrastructure::status::StatusMask;

    fn test_participant() -> DomainParticipant {
        use std::sync::atomic::{AtomicI32, Ordering};
        static DOMAIN_ID: AtomicI32 = AtomicI32::new(300);
        let factory = DomainParticipantFactory::get_instance();
        factory
            .create_participant(
                DOMAIN_ID.fetch_add(1, Ordering::SeqCst),
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap()
    }

    #[test]
    fn requester_params_defaults() {
        let p = test_participant();
        let params = RequesterParams::new(p);
        assert!(params.service_name.is_none());
        assert!(params.request_topic_name.is_none());
        assert!(params.reply_topic_name.is_none());
        assert!(params.datawriter_qos.is_none());
        assert!(params.datareader_qos.is_none());
        assert!(params.publisher.is_none());
        assert!(params.subscriber.is_none());
        assert!(params.publisher_qos.is_none());
        assert!(params.subscriber_qos.is_none());

        params.participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(params.participant).unwrap();
    }

    #[test]
    fn requester_params_builder_sets_fields() {
        let p = test_participant();
        let params = RequesterParams::new(p)
            .service_name("MySvc")
            .request_topic_name("ReqTopic")
            .reply_topic_name("RepTopic");
        assert_eq!(params.service_name.as_deref(), Some("MySvc"));
        assert_eq!(params.request_topic_name.as_deref(), Some("ReqTopic"));
        assert_eq!(params.reply_topic_name.as_deref(), Some("RepTopic"));

        params.participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(params.participant).unwrap();
    }

    #[test]
    fn requester_params_qos_setters() {
        use int2dds::dcps::publication::qos::DataWriterQos;
        use int2dds::dcps::subscription::qos::DataReaderQos;

        let p = test_participant();
        let params = RequesterParams::new(p)
            .datawriter_qos(DataWriterQos::default())
            .datareader_qos(DataReaderQos::default())
            .publisher_qos(Default::default())
            .subscriber_qos(Default::default());
        assert!(params.datawriter_qos.is_some());
        assert!(params.datareader_qos.is_some());
        assert!(params.publisher_qos.is_some());
        assert!(params.subscriber_qos.is_some());

        params.participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(params.participant).unwrap();
    }

    #[test]
    fn replier_params_defaults() {
        let p = test_participant();
        let params = ReplierParams::new(p);
        assert!(params.service_name.is_none());
        assert!(params.instance_name.is_none());
        assert!(params.request_topic_name.is_none());
        assert!(params.reply_topic_name.is_none());
        assert!(params.datawriter_qos.is_none());
        assert!(params.datareader_qos.is_none());
        assert!(params.publisher.is_none());
        assert!(params.subscriber.is_none());
        assert!(params.publisher_qos.is_none());
        assert!(params.subscriber_qos.is_none());

        params.participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(params.participant).unwrap();
    }

    #[test]
    fn replier_params_builder_sets_fields() {
        let p = test_participant();
        let params = ReplierParams::new(p)
            .service_name("MySvc")
            .instance_name("Instance1")
            .request_topic_name("ReqTopic")
            .reply_topic_name("RepTopic");
        assert_eq!(params.service_name.as_deref(), Some("MySvc"));
        assert_eq!(params.instance_name.as_deref(), Some("Instance1"));
        assert_eq!(params.request_topic_name.as_deref(), Some("ReqTopic"));
        assert_eq!(params.reply_topic_name.as_deref(), Some("RepTopic"));

        params.participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(params.participant).unwrap();
    }

    #[test]
    fn replier_params_qos_setters() {
        use int2dds::dcps::publication::qos::DataWriterQos;
        use int2dds::dcps::subscription::qos::DataReaderQos;

        let p = test_participant();
        let params = ReplierParams::new(p)
            .datawriter_qos(DataWriterQos::default())
            .datareader_qos(DataReaderQos::default())
            .publisher_qos(Default::default())
            .subscriber_qos(Default::default());
        assert!(params.datawriter_qos.is_some());
        assert!(params.datareader_qos.is_some());
        assert!(params.publisher_qos.is_some());
        assert!(params.subscriber_qos.is_some());

        params.participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(params.participant).unwrap();
    }
}
