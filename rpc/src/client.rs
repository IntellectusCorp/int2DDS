//! Function-call style Client abstraction (7.11.1.5.4, 7.11.1.5.5)

use std::time::Duration;

use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::publication::data_writer::DataWriter;
use int2dds::dcps::publication::publisher::Publisher;
use int2dds::dcps::publication::qos::{DataWriterQos, PublisherQos};
use int2dds::dcps::subscription::data_reader::DataReader;
use int2dds::dcps::subscription::qos::{DataReaderQos, SubscriberQos};
use int2dds::dcps::subscription::subscriber::Subscriber;

use crate::entity::{RpcEntity, ServiceProxy};
use crate::error::DdsRpcResult;
use crate::params::RequesterParams;
use crate::requester::{Future, Requester};
use crate::sample::Sample;
use crate::types::{DdsRpcType, InstanceName, Reply, Request, SampleIdentity};

/// Configuration for constructing a Client (function_call.h ClientParams)
pub struct ClientParams {
    pub(crate) participant: DomainParticipant,
    pub(crate) service_name: Option<String>,
    pub(crate) instance_name: Option<String>,
    pub(crate) request_topic_name: Option<String>,
    pub(crate) reply_topic_name: Option<String>,
    pub(crate) datawriter_qos: Option<DataWriterQos>,
    pub(crate) datareader_qos: Option<DataReaderQos>,
    pub(crate) publisher: Option<Publisher>,
    pub(crate) subscriber: Option<Subscriber>,
    pub(crate) publisher_qos: Option<PublisherQos>,
    pub(crate) subscriber_qos: Option<SubscriberQos>,
}

impl ClientParams {
    pub fn new(participant: DomainParticipant) -> Self {
        Self {
            participant,
            service_name: None,
            instance_name: None,
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

    /// Convert into RequesterParams for internal Requester construction.
    pub fn into_requester_params(self) -> RequesterParams {
        let mut params = RequesterParams::new(self.participant);
        params.service_name = self.service_name;
        params.request_topic_name = self.request_topic_name;
        params.reply_topic_name = self.reply_topic_name;
        params.datawriter_qos = self.datawriter_qos;
        params.datareader_qos = self.datareader_qos;
        params.publisher = self.publisher;
        params.subscriber = self.subscriber;
        params.publisher_qos = self.publisher_qos;
        params.subscriber_qos = self.subscriber_qos;
        params
    }
}

/// 7.11.1.5.5
pub trait ClientEndpoint: ServiceProxy {
    type TReq;
    type TRep;

    fn get_request_datawriter(&self) -> DdsRpcResult<&DataWriter<Request<Self::TReq>>>;
    fn get_reply_datareader(&self) -> DdsRpcResult<&DataReader<Reply<Self::TRep>>>;
}

/// 7.11.1.5.4
pub struct Client<TReq, TRep> {
    requester: Requester<TReq, TRep>,
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> Client<TReq, TRep> {
    pub fn new(params: ClientParams) -> DdsRpcResult<Self> {
        let requester = Requester::new(params.into_requester_params())?;
        Ok(Self { requester })
    }

    pub fn send_request(&self, data: &TReq) -> DdsRpcResult<SampleIdentity> {
        self.requester.send_request(data)
    }

    pub fn receive_reply(&self, timeout: Duration) -> DdsRpcResult<Sample<Reply<TRep>>> {
        self.requester.receive_reply(timeout)
    }

    pub fn take_reply(
        &self,
        related_id: &SampleIdentity,
    ) -> DdsRpcResult<Option<Sample<Reply<TRep>>>> {
        self.requester.take_reply(related_id)
    }

    pub fn send_request_async(&self, data: &TReq) -> DdsRpcResult<Future<TRep>> {
        self.requester.send_request_async(data)
    }
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> RpcEntity for Client<TReq, TRep> {
    fn close(&mut self) -> DdsRpcResult<()> {
        self.requester.close()
    }

    fn is_closed(&self) -> bool {
        self.requester.is_closed()
    }
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> ServiceProxy for Client<TReq, TRep> {
    fn bind_instance(&mut self, instance_name: InstanceName) -> DdsRpcResult<()> {
        self.requester.bind_instance(instance_name)
    }

    fn unbind(&mut self) -> DdsRpcResult<()> {
        self.requester.unbind()
    }

    fn get_bound_instance_name(&self) -> Option<&str> {
        self.requester.get_bound_instance_name()
    }

    fn wait_for_service(&self) -> DdsRpcResult<()> {
        self.requester.wait_for_service()
    }

    fn wait_for_service_timeout(&self, timeout: Duration) -> DdsRpcResult<()> {
        self.requester.wait_for_service_timeout(timeout)
    }
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> ClientEndpoint for Client<TReq, TRep> {
    type TReq = TReq;
    type TRep = TRep;

    fn get_request_datawriter(&self) -> DdsRpcResult<&DataWriter<Request<TReq>>> {
        self.requester.get_request_datawriter()
    }

    fn get_reply_datareader(&self) -> DdsRpcResult<&DataReader<Reply<TRep>>> {
        self.requester.get_reply_datareader()
    }
}
