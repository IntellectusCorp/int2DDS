//! Replier<TReq, TRep> — receives requests and sends replies (7.11.1.4.5)

use std::time::Duration;

use int2dds::common::instance_handle::InstanceHandle;
use int2dds::dcps::core::error::DdsError;
use int2dds::dcps::infrastructure::qos_policy::{
    DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy, HistoryQosPolicyKind,
    ReliabilityQosPolicy, ReliabilityQosPolicyKind,
};
use int2dds::dcps::infrastructure::status::StatusMask;
use int2dds::dcps::publication::data_writer::DataWriter;
use int2dds::dcps::publication::qos::{DataWriterQos, DATAWRITER_QOS_DEFAULT};
use int2dds::dcps::subscription::data_reader::DataReader;
use int2dds::dcps::subscription::qos::{DataReaderQos, DATAREADER_QOS_DEFAULT};
use int2dds::dcps::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
use int2dds::dcps::topic::qos::TopicQos;
use int2dds::dcps::topic::type_support::DdsType;

use crate::entity::RpcEntity;
use crate::error::{DdsRpcError, DdsRpcResult};
use crate::params::ReplierParams;
use crate::sample::Sample;
use crate::topic_name::TopicNameConfig;
use crate::types::{RemoteExceptionCode, RpcReply, SampleIdentity};

pub struct Replier<TReq, TRep> {
    request_reader: Option<DataReader<TReq>>,
    reply_writer: Option<DataWriter<TRep>>,
    closed: bool,
}

impl<TReq: DdsType, TRep: DdsType + Clone + RpcReply> Replier<TReq, TRep> {
    pub fn new(params: ReplierParams) -> DdsRpcResult<Self> {
        let topic_config = TopicNameConfig {
            interface_name: None,
            service_name: params.service_name.clone(),
            request_topic_override: params.request_topic_name.clone(),
            reply_topic_override: params.reply_topic_name.clone(),
        };

        let request_topic_name = topic_config.request_topic();
        let reply_topic_name = topic_config.reply_topic();

        let request_topic = params.participant.create_topic::<TReq>(
            &request_topic_name,
            &TReq::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )?;

        let reply_topic = params.participant.create_topic::<TRep>(
            &reply_topic_name,
            &TRep::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )?;

        let subscriber = match params.subscriber {
            Some(s) => s,
            None => params.participant.create_subscriber(
                params.subscriber_qos.unwrap_or_default(),
                None,
                StatusMask::default(),
            )?,
        };

        let reader_qos = params.datareader_qos.unwrap_or_else(rpc_datareader_qos);
        let request_reader = subscriber.create_datareader::<TReq>(
            &request_topic,
            reader_qos,
            None,
            StatusMask::default(),
        )?;

        let publisher = match params.publisher {
            Some(p) => p,
            None => params.participant.create_publisher(
                params.publisher_qos.unwrap_or_default(),
                None,
                StatusMask::default(),
            )?,
        };

        let writer_qos = params.datawriter_qos.unwrap_or_else(rpc_datawriter_qos);
        let reply_writer = publisher.create_datawriter::<TRep>(
            &reply_topic,
            writer_qos,
            None,
            StatusMask::default(),
        )?;

        Ok(Self {
            request_reader: Some(request_reader),
            reply_writer: Some(reply_writer),
            closed: false,
        })
    }

    fn reader(&self) -> DdsRpcResult<&DataReader<TReq>> {
        self.request_reader.as_ref().ok_or(DdsError::AlreadyDeleted.into())
    }

    fn writer(&self) -> DdsRpcResult<&DataWriter<TRep>> {
        self.reply_writer.as_ref().ok_or(DdsError::AlreadyDeleted.into())
    }

    /// Send a reply correlated with the given request identity. (7.8.1)
    pub fn send_reply(
        &self,
        data: &mut TRep,
        related_request_id: &SampleIdentity,
    ) -> DdsRpcResult<()> {
        data.header_mut().related_request_id = *related_request_id;
        data.header_mut().remote_ex = RemoteExceptionCode::Ok;
        self.writer()?.write(data, InstanceHandle::NIL)?;
        Ok(())
    }

    /// Take a single pending request (non-blocking).
    pub fn take_request(&self) -> DdsRpcResult<Option<Sample<TReq>>> {
        let samples = self.reader()?.take(
            1,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )?;
        Ok(samples.into_iter().next())
    }

    /// Take up to `max_count` pending requests (non-blocking).
    pub fn take_requests(&self, max_count: i32) -> DdsRpcResult<Vec<Sample<TReq>>> {
        let samples = self.reader()?.take(
            max_count,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )?;
        Ok(samples)
    }

    /// Block until a request arrives or timeout expires.
    pub fn receive_request(&self, timeout: Duration) -> DdsRpcResult<Sample<TReq>> {
        let reader = self.reader()?;
        let start = std::time::Instant::now();
        loop {
            match reader.take(
                1,
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
            ) {
                Ok(samples) => {
                    if let Some(sample) = samples.into_iter().next() {
                        return Ok(sample);
                    }
                }
                Err(DdsError::NoData) => {}
                Err(e) => return Err(e.into()),
            }
            if start.elapsed() >= timeout {
                return Err(DdsRpcError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn get_request_datareader(&self) -> DdsRpcResult<&DataReader<TReq>> {
        self.reader()
    }

    pub fn get_reply_datawriter(&self) -> DdsRpcResult<&DataWriter<TRep>> {
        self.writer()
    }
}

impl<TReq: DdsType, TRep: DdsType + Clone + RpcReply> RpcEntity for Replier<TReq, TRep> {
    fn close(&mut self) -> DdsRpcResult<()> {
        if let Some(reader) = self.request_reader.take() {
            let subscriber = reader.get_subscriber()?;
            subscriber.delete_datareader(reader)?;
        }
        if let Some(writer) = self.reply_writer.take() {
            let publisher = writer.get_publisher()?;
            publisher.delete_datawriter(writer)?;
        }
        self.closed = true;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

// RPC default QoS (7.10.2): RELIABLE, KEEP_ALL, VOLATILE
fn rpc_datawriter_qos() -> DataWriterQos {
    let mut qos = DATAWRITER_QOS_DEFAULT;
    qos.reliability =
        ReliabilityQosPolicy { kind: ReliabilityQosPolicyKind::Reliable, ..qos.reliability };
    qos.history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll };
    qos.durability = DurabilityQosPolicy { kind: DurabilityQosPolicyKind::Volatile };
    qos
}

fn rpc_datareader_qos() -> DataReaderQos {
    let mut qos = DATAREADER_QOS_DEFAULT;
    qos.reliability =
        ReliabilityQosPolicy { kind: ReliabilityQosPolicyKind::Reliable, ..qos.reliability };
    qos.history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll };
    qos.durability = DurabilityQosPolicy { kind: DurabilityQosPolicyKind::Volatile };
    qos
}
