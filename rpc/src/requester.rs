//! Requester<TReq, TRep> — sends requests and receives replies (7.11.1.4.3)

use std::time::Duration;

use int2dds::common::instance_handle::InstanceHandle;
use int2dds::dcps::core::error::DdsError;
use int2dds::dcps::infrastructure::qos_policy::{
    DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy, HistoryQosPolicyKind,
    ReliabilityQosPolicy, ReliabilityQosPolicyKind,
};
use int2dds::dcps::infrastructure::status::StatusMask;
use int2dds::dcps::infrastructure::wait_set::WaitSet;
use int2dds::dcps::publication::data_writer::DataWriter;
use int2dds::dcps::publication::qos::{DataWriterQos, DATAWRITER_QOS_DEFAULT};
use int2dds::dcps::subscription::data_reader::DataReader;
use int2dds::dcps::subscription::qos::{DataReaderQos, DATAREADER_QOS_DEFAULT};
use int2dds::dcps::subscription::query_condition::QueryCondition;
use int2dds::dcps::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
use int2dds::dcps::topic::qos::TopicQos;
use int2dds::dcps::topic::type_support::DdsType;

use crate::entity::{RpcEntity, ServiceProxy};
use crate::error::{DdsRpcError, DdsRpcResult};
use crate::params::RequesterParams;
use crate::sample::Sample;
use crate::topic_name::TopicNameConfig;
use crate::types::{InstanceName, RpcRequest, SampleIdentity};

pub struct Requester<TReq, TRep> {
    request_writer: Option<DataWriter<TReq>>,
    reply_reader: Option<DataReader<TRep>>,
    bound_instance: Option<InstanceName>,
    closed: bool,
}

impl<TReq: DdsType + Clone + RpcRequest, TRep: DdsType> Requester<TReq, TRep> {
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
            request_writer: Some(request_writer),
            reply_reader: Some(reply_reader),
            bound_instance: None,
            closed: false,
        })
    }

    fn writer(&self) -> DdsRpcResult<&DataWriter<TReq>> {
        self.request_writer.as_ref().ok_or(DdsError::AlreadyDeleted.into())
    }

    fn reader(&self) -> DdsRpcResult<&DataReader<TRep>> {
        self.reply_reader.as_ref().ok_or(DdsError::AlreadyDeleted.into())
    }

    /// Send a request. The middleware fills in `RequestHeader.requestId`
    /// before writing, and returns it for reply correlation. (7.8.1)
    pub fn send_request(&self, data: &mut TReq) -> DdsRpcResult<SampleIdentity> {
        // Bind instance name if bound
        if let Some(name) = &self.bound_instance {
            data.header_mut().instance_name = name.clone();
        }
        let writer = self.writer()?;
        let (guid, seq) = writer.write_and_obtain_sample_identity(data, InstanceHandle::NIL)?;
        let identity = SampleIdentity { writer_guid: guid, sequence_number: seq.into() };
        data.header_mut().request_id = identity;
        Ok(identity)
    }

    pub fn receive_reply(&self, timeout: Duration) -> DdsRpcResult<Sample<TRep>> {
        let reader = self.reader()?;
        let start = std::time::Instant::now();
        loop {
            let samples = reader.take(
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

    pub fn take_reply(&self, related_id: &SampleIdentity) -> DdsRpcResult<Option<Sample<TRep>>> {
        let qc = self.create_correlation_condition(related_id)?;
        let samples = self.reader()?.take_w_condition(1, qc)?;
        Ok(samples.into_iter().next())
    }

    pub fn take_replies(&self, max_count: i32) -> DdsRpcResult<Vec<Sample<TRep>>> {
        let samples = self.reader()?.take(
            max_count,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )?;
        Ok(samples)
    }

    pub fn take_replies_for_request(
        &self,
        max_count: i32,
        related_id: &SampleIdentity,
    ) -> DdsRpcResult<Vec<Sample<TRep>>> {
        let qc = self.create_correlation_condition(related_id)?;
        let samples = self.reader()?.take_w_condition(max_count, qc)?;
        Ok(samples)
    }

    fn create_correlation_condition(
        &self,
        related_id: &SampleIdentity,
    ) -> DdsRpcResult<QueryCondition> {
        let qc = self.reader()?.create_querycondition(
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
            "header.related_request_id.writer_guid = %0 \
             AND header.related_request_id.sequence_number.high = %1 \
             AND header.related_request_id.sequence_number.low = %2",
            vec![
                related_id.writer_guid.to_hex_string(),
                related_id.sequence_number.high.to_string(),
                (related_id.sequence_number.low as i32).to_string(),
            ],
        )?;
        Ok(qc)
    }

    pub fn get_request_datawriter(&self) -> DdsRpcResult<&DataWriter<TReq>> {
        self.writer()
    }

    pub fn get_reply_datareader(&self) -> DdsRpcResult<&DataReader<TRep>> {
        self.reader()
    }
}

impl<TReq: DdsType + Clone + RpcRequest, TRep: DdsType> RpcEntity for Requester<TReq, TRep> {
    fn close(&mut self) -> DdsRpcResult<()> {
        if let Some(writer) = self.request_writer.take() {
            let publisher = writer.get_publisher()?;
            publisher.delete_datawriter(writer)?;
        }
        if let Some(reader) = self.reply_reader.take() {
            let subscriber = reader.get_subscriber()?;
            subscriber.delete_datareader(reader)?;
        }
        self.closed = true;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

impl<TReq: DdsType + Clone + RpcRequest, TRep: DdsType> ServiceProxy for Requester<TReq, TRep> {
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
        // Basic discovery: wait until request_writer has at least one matched subscription
        let mut condition = self.writer()?.get_statuscondition()?;
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED)?;
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition)?;
        wait_set.wait(int2dds::dcps::core::time::Duration::infinite())?;
        Ok(())
    }

    fn wait_for_service_timeout(&self, timeout: Duration) -> DdsRpcResult<()> {
        let mut condition = self.writer()?.get_statuscondition()?;
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED)?;
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition)?;
        let dds_timeout = int2dds::dcps::core::time::Duration::try_from(timeout)?;
        wait_set.wait(dds_timeout)?;
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
