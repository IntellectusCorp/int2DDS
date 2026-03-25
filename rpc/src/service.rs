//! Service — the callee-side handler for function-call style RPC.
//!
//! Each generated service wraps a Replier, dispatching incoming requests to
//! the user's trait implementation and sending back serialized replies.

use std::sync::Arc;

use int2dds::dcps::core::error::DdsError;
use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::publication::data_writer::DataWriter;
use int2dds::dcps::publication::publisher::Publisher;
use int2dds::dcps::publication::qos::{DataWriterQos, PublisherQos};
use int2dds::dcps::subscription::data_reader::DataReader;
use int2dds::dcps::subscription::qos::{DataReaderQos, SubscriberQos};
use int2dds::dcps::subscription::subscriber::Subscriber;

use crate::entity::RpcEntity;
use crate::error::DdsRpcResult;
use crate::params::ReplierParams;
use crate::replier::Replier;
use crate::server::Dispatchable;
use crate::types::{DdsRpcType, RemoteExceptionCode, Reply, Request};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Closed,
    Paused,
    Running,
}

/// Configuration for constructing a Service (function_call.h ServiceParams)
pub struct ServiceParams {
    pub(crate) participant: DomainParticipant,
    pub(crate) service_name: Option<String>,
    pub(crate) interface_name: Option<String>,
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

impl ServiceParams {
    pub fn new(participant: DomainParticipant) -> Self {
        Self {
            participant,
            service_name: None,
            interface_name: None,
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

    /// Convert into ReplierParams for internal Replier construction.
    pub fn into_replier_params(self) -> ReplierParams {
        let mut params = ReplierParams::new(self.participant);
        params.service_name = self.service_name;
        params.interface_name = self.interface_name;
        params.instance_name = self.instance_name;
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

pub trait ServiceEndpoint: RpcEntity {
    type TReq;
    type TRep;

    fn get_request_datareader(&self) -> DdsRpcResult<&DataReader<Request<Self::TReq>>>;
    fn get_reply_datawriter(&self) -> DdsRpcResult<&DataWriter<Reply<Self::TRep>>>;
    fn pause(&mut self);
    fn resume(&mut self);
    fn status(&self) -> ServiceStatus;
}

/// Request dispatch abstraction.
/// IDL code generators produce per-interface implementations of this trait.
/// Returns reply data + RemoteExceptionCode for the reply header.
pub trait RequestHandler<TReq, TRep>: Send + 'static {
    fn handle_request(&self, request: &TReq) -> (TRep, RemoteExceptionCode);
}

/// Manages a Replier and dispatches incoming requests to a RequestHandler.
pub struct Service<TReq, TRep, H> {
    replier: Replier<TReq, TRep>,
    handler: Arc<H>,
    status: ServiceStatus,
}

impl<TReq: DdsRpcType, TRep: DdsRpcType, H: RequestHandler<TReq, TRep>> Service<TReq, TRep, H> {
    pub fn new(params: ServiceParams, handler: H) -> DdsRpcResult<Self> {
        let replier = Replier::new(params.into_replier_params())?;
        Ok(Self { replier, handler: Arc::new(handler), status: ServiceStatus::Running })
    }

    /// Try to dispatch one pending request.
    /// Returns Ok(true) if a request was processed, Ok(false) if none available
    /// or service is not running.
    pub(crate) fn try_dispatch_one(&self) -> DdsRpcResult<bool> {
        if self.status != ServiceStatus::Running {
            return Ok(false);
        }

        let sample = match self.replier.take_request() {
            Ok(Some(s)) => s,
            Ok(None) => return Ok(false),
            // No data available is not an error (replier.rs:170 pattern)
            Err(crate::error::DdsRpcError::Dds(DdsError::NoData)) => return Ok(false),
            Err(e) => return Err(e),
        };

        let data = sample.data().map_err(|e| DdsError::Error(e.to_string()))?;
        let request_id = data.header.request_id;
        let (reply_data, remote_ex) = self.handler.handle_request(&data.data);
        self.replier.send_reply_with_exception_code(&reply_data, &request_id, remote_ex)?;
        Ok(true)
    }
}

impl<TReq: DdsRpcType, TRep: DdsRpcType, H: RequestHandler<TReq, TRep>> RpcEntity
    for Service<TReq, TRep, H>
{
    fn close(&mut self) -> DdsRpcResult<()> {
        self.replier.close()?;
        self.status = ServiceStatus::Closed;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.status == ServiceStatus::Closed
    }
}

impl<TReq: DdsRpcType, TRep: DdsRpcType, H: RequestHandler<TReq, TRep>> ServiceEndpoint
    for Service<TReq, TRep, H>
{
    type TReq = TReq;
    type TRep = TRep;

    fn get_request_datareader(&self) -> DdsRpcResult<&DataReader<Request<TReq>>> {
        self.replier.get_request_datareader()
    }

    fn get_reply_datawriter(&self) -> DdsRpcResult<&DataWriter<Reply<TRep>>> {
        self.replier.get_reply_datawriter()
    }

    fn pause(&mut self) {
        if self.status == ServiceStatus::Running {
            self.status = ServiceStatus::Paused;
        }
    }

    fn resume(&mut self) {
        if self.status == ServiceStatus::Paused {
            self.status = ServiceStatus::Running;
        }
    }

    fn status(&self) -> ServiceStatus {
        self.status
    }
}

impl<TReq, TRep, H> Dispatchable for Service<TReq, TRep, H>
where
    TReq: DdsRpcType + Send,
    TRep: DdsRpcType + Send,
    H: RequestHandler<TReq, TRep> + Send + Sync,
{
    fn try_dispatch_one(&self) -> DdsRpcResult<bool> {
        Service::try_dispatch_one(self)
    }
}
