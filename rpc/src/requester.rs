//! Requester<TReq, TRep> — sends requests and receives replies (7.11.1.4.3)

use std::time::Duration;

use int2dds::common::instance_handle::InstanceHandle;
use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::infrastructure::qos_policy::{
    DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy, HistoryQosPolicyKind,
    ReliabilityQosPolicy, ReliabilityQosPolicyKind,
};
use int2dds::dcps::infrastructure::status::StatusMask;
use int2dds::dcps::publication::data_writer::DataWriter;
use int2dds::dcps::publication::qos::{DataWriterQos, DATAWRITER_QOS_DEFAULT};
use int2dds::dcps::subscription::data_reader::DataReader;
use int2dds::dcps::subscription::data_sample::DataSample;
use int2dds::dcps::subscription::qos::{DataReaderQos, DATAREADER_QOS_DEFAULT};
use int2dds::dcps::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
use int2dds::dcps::topic::qos::TopicQos;
use int2dds::dcps::topic::type_support::DdsType;

use crate::entity::{RpcEntity, ServiceProxy};
use crate::error::{DdsRpcError, DdsRpcResult};
use crate::params::RequesterParams;
use crate::topic_name::TopicNameConfig;
use crate::types::{InstanceName, SampleIdentity};

pub type Sample<T> = DataSample<T>;

pub struct Requester<TReq, TRep> {
    participant: DomainParticipant,
    request_writer: DataWriter<TReq>,
    reply_reader: DataReader<TRep>,
    bound_instance: Option<InstanceName>,
    closed: bool,
}

impl<TReq: DdsType + Clone, TRep: DdsType> Requester<TReq, TRep> {
    pub fn new(params: RequesterParams) -> DdsRpcResult<Self> {
        let topic_config = TopicNameConfig {
            interface_name: None, // request-reply style: no interface name (7.4.1)
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

        let publisher = match params.publisher {
            Some(p) => p,
            None => params.participant.create_publisher(
                params.publisher_qos.unwrap_or_default(),
                None,
                StatusMask::default(),
            )?,
        };

        let writer_qos = params.datawriter_qos.unwrap_or_else(rpc_datawriter_qos);
        let request_writer = publisher.create_datawriter::<TReq>(
            &request_topic,
            writer_qos,
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
        let reply_reader = subscriber.create_datareader::<TRep>(
            &reply_topic,
            reader_qos,
            None,
            StatusMask::default(),
        )?;

        Ok(Self {
            participant: params.participant,
            request_writer,
            reply_reader,
            bound_instance: None,
            closed: false,
        })
    }

    pub fn send_request(&self, data: &TReq) -> DdsRpcResult<SampleIdentity> {
        let (guid, seq) =
            self.request_writer.write_and_obtain_sample_identity(data, InstanceHandle::NIL)?;
        Ok(SampleIdentity { writer_guid: guid, sequence_number: seq })
    }

    pub fn receive_reply(&self, timeout: Duration) -> DdsRpcResult<Sample<TRep>> {
        // Poll with sleep until timeout
        let start = std::time::Instant::now();
        loop {
            let samples = self.reply_reader.take(
                1,
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
            )?;
            if let Some(sample) = samples.into_iter().next() {
                return Ok(sample);
            }
            if start.elapsed() >= timeout {
                return Err(DdsRpcError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn take_reply(&self, _related_id: &SampleIdentity) -> DdsRpcResult<Option<Sample<TRep>>> {
        // TODO: filter by related_request_id once correlation is implemented
        let samples = self.reply_reader.take(
            1,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )?;
        Ok(samples.into_iter().next())
    }

    pub fn take_replies(&self, max_count: i32) -> DdsRpcResult<Vec<Sample<TRep>>> {
        let samples = self.reply_reader.take(
            max_count,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )?;
        Ok(samples)
    }

    pub fn get_request_datawriter(&self) -> &DataWriter<TReq> {
        &self.request_writer
    }

    pub fn get_reply_datareader(&self) -> &DataReader<TRep> {
        &self.reply_reader
    }
}

impl<TReq: DdsType + Clone, TRep: DdsType> RpcEntity for Requester<TReq, TRep> {
    fn close(&mut self) -> DdsRpcResult<()> {
        self.closed = true;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

impl<TReq: DdsType + Clone, TRep: DdsType> ServiceProxy for Requester<TReq, TRep> {
    fn bind_instance(&mut self, instance_name: InstanceName) -> DdsRpcResult<()> {
        self.bound_instance = Some(instance_name);
        Ok(())
    }

    fn unbind(&mut self) -> DdsRpcResult<()> {
        self.bound_instance = None;
        Ok(())
    }

    fn get_bound_instance_name(&self) -> Option<&str> {
        self.bound_instance.as_deref()
    }

    fn wait_for_service(&self) -> DdsRpcResult<()> {
        // TODO: implement discovery-based service waiting
        Ok(())
    }

    fn wait_for_service_timeout(&self, _timeout: Duration) -> DdsRpcResult<()> {
        // TODO: implement discovery-based service waiting with timeout
        Ok(())
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
