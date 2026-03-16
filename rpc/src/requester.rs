//! Requester<TReq, TRep> — sends requests and receives replies (7.11.1.4.3)

use std::fmt::Debug;
use std::sync::Arc;
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
use int2dds::dcps::subscription::data_reader_listener::DataReaderListener;
use int2dds::dcps::subscription::qos::{DataReaderQos, DATAREADER_QOS_DEFAULT};
use int2dds::dcps::subscription::query_condition::QueryCondition;
use int2dds::dcps::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
use int2dds::dcps::topic::qos::TopicQos;
use int2dds::dcps::topic::type_support::DdsType;
use int2dds::serialize::cdr::{CdrDeserialize, CdrSerialize, XcdrDeserialize, XcdrSerialize};

use crate::entity::{RpcEntity, ServiceProxy};
use crate::error::{DdsRpcError, DdsRpcResult};
use crate::listener::{RequesterListener, SimpleRequesterListener};
use crate::params::RequesterParams;
use crate::sample::Sample;
use crate::topic_name::TopicNameConfig;
use crate::types::{InstanceName, Request, SampleIdentity};

pub struct Requester<TReq, TRep> {
    request_writer: Option<DataWriter<Request<TReq>>>,
    reply_reader: Option<DataReader<crate::types::Reply<TRep>>>,
    bound_instance: Option<InstanceName>,
    closed: bool,
}

impl<TReq, TRep> Requester<TReq, TRep>
where
    TReq: DdsType + Clone + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
    TRep: DdsType + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
{
    pub fn new(params: RequesterParams) -> DdsRpcResult<Self> {
        let topic_config = TopicNameConfig {
            interface_name: None, // request-reply style: no interface name (7.4.1)
            service_name: params.service_name.clone(),
            request_topic_override: params.request_topic_name.clone(),
            reply_topic_override: params.reply_topic_name.clone(),
        };

        let request_topic_name = topic_config.request_topic();
        let reply_topic_name = topic_config.reply_topic();

        let request_topic = params.participant.create_topic::<Request<TReq>>(
            &request_topic_name,
            &Request::<TReq>::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )?;

        let reply_topic = params.participant.create_topic::<crate::types::Reply<TRep>>(
            &reply_topic_name,
            &crate::types::Reply::<TRep>::get_type_name(),
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
        let request_writer = publisher.create_datawriter::<Request<TReq>>(
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
        let reply_reader = subscriber.create_datareader::<crate::types::Reply<TRep>>(
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

    fn writer(&self) -> DdsRpcResult<&DataWriter<Request<TReq>>> {
        self.request_writer.as_ref().ok_or(DdsError::AlreadyDeleted.into())
    }

    fn reader(&self) -> DdsRpcResult<&DataReader<crate::types::Reply<TRep>>> {
        self.reply_reader.as_ref().ok_or(DdsError::AlreadyDeleted.into())
    }

    /// Send a request. The middleware fills in `RequestHeader.requestId`
    /// before writing, and returns it for reply correlation. (7.8.1)
    pub fn send_request(&self, data: &TReq) -> DdsRpcResult<SampleIdentity> {
        let mut request =
            Request { header: crate::types::RequestHeader::default(), data: data.clone() };
        if let Some(name) = &self.bound_instance {
            request.header.instance_name = name.clone();
        }
        let writer = self.writer()?;
        let (guid, seq) = writer.write_and_obtain_sample_identity(
            &mut request,
            InstanceHandle::NIL,
            |req, guid, seq| {
                req.header.request_id =
                    SampleIdentity { writer_guid: guid, sequence_number: seq.into() };
            },
        )?;
        Ok(SampleIdentity { writer_guid: guid, sequence_number: seq.into() })
    }

    pub fn receive_reply(
        &self,
        timeout: Duration,
    ) -> DdsRpcResult<Sample<crate::types::Reply<TRep>>> {
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

    pub fn take_reply(
        &self,
        related_id: &SampleIdentity,
    ) -> DdsRpcResult<Option<Sample<crate::types::Reply<TRep>>>> {
        let qc = self.create_correlation_condition(related_id)?;
        let samples = self.reader()?.take_w_condition(1, qc)?;
        Ok(samples.into_iter().next())
    }

    pub fn take_replies(
        &self,
        max_count: i32,
    ) -> DdsRpcResult<Vec<Sample<crate::types::Reply<TRep>>>> {
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
    ) -> DdsRpcResult<Vec<Sample<crate::types::Reply<TRep>>>> {
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

    pub fn get_request_datawriter(&self) -> DdsRpcResult<&DataWriter<Request<TReq>>> {
        self.writer()
    }

    pub fn get_reply_datareader(&self) -> DdsRpcResult<&DataReader<crate::types::Reply<TRep>>> {
        self.reader()
    }
}

impl<TReq, TRep> Requester<TReq, TRep>
where
    TReq: DdsType + Clone + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
    TRep: DdsType + Clone + Debug + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
{
    /// Send a request and return a Future for the correlated reply.
    /// No manual correlation needed — the Future handles it internally. (7.11.1.4.3)
    pub fn send_request_async(&self, data: &TReq) -> DdsRpcResult<Future<TRep>> {
        let identity = self.send_request(data)?;
        let condition = self.create_correlation_condition(&identity)?;
        let reader = self.reader()?.clone();
        Ok(Future { reader, condition })
    }
}

impl<TReq, TRep> RpcEntity for Requester<TReq, TRep>
where
    TReq: DdsType + Clone + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
    TRep: DdsType + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
{
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

impl<TReq, TRep> ServiceProxy for Requester<TReq, TRep>
where
    TReq: DdsType + Clone + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
    TRep: DdsType + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
{
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

/// Future-based reply reception.
/// Wraps a WaitSet + QueryCondition to wait only for the correlated reply. (7.11.1.4.3)
pub struct Future<TRep: 'static + Clone + Debug> {
    reader: DataReader<crate::types::Reply<TRep>>,
    condition: QueryCondition,
}

impl<TRep> Future<TRep>
where
    TRep: DdsType + Clone + Debug + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
{
    /// Block until the correlated reply arrives.
    pub fn get(self) -> DdsRpcResult<Sample<crate::types::Reply<TRep>>> {
        let wait_set = WaitSet::new();
        wait_set.attach_condition(self.condition.clone())?;
        wait_set.wait(int2dds::dcps::core::time::Duration::infinite())?;
        let samples = self.reader.take_w_condition(1, self.condition)?;
        samples.into_iter().next().ok_or(DdsRpcError::Timeout)
    }

    /// Block until the correlated reply arrives or timeout expires.
    pub fn get_timeout(self, timeout: Duration) -> DdsRpcResult<Sample<crate::types::Reply<TRep>>> {
        let wait_set = WaitSet::new();
        wait_set.attach_condition(self.condition.clone())?;
        let dds_timeout = int2dds::dcps::core::time::Duration::try_from(timeout)?;
        wait_set.wait(dds_timeout)?;
        let samples = self.reader.take_w_condition(1, self.condition)?;
        samples.into_iter().next().ok_or(DdsRpcError::Timeout)
    }
}

// Listener-based reply reception (7.11.1.4.9, 7.11.1.4.10) --
impl<TReq, TRep> Requester<TReq, TRep>
where
    TReq: DdsType + Clone + Debug + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
    TRep: DdsType + Clone + Debug + CdrSerialize + CdrDeserialize + XcdrSerialize + XcdrDeserialize,
{
    /// Install a SimpleRequesterListener. The middleware takes each arriving reply
    /// and dispatches it to `process_reply`. (7.11.1.4.9)
    /// Pass `None` to remove an existing listener.
    pub fn set_simple_requester_listener(
        &self,
        listener: Option<Arc<dyn SimpleRequesterListener<TRep>>>,
    ) -> DdsRpcResult<()> {
        let reader = self.reader()?;
        match listener {
            Some(l) => {
                let adapter: Arc<dyn DataReaderListener<Foo = crate::types::Reply<TRep>>> =
                    Arc::new(SimpleRequesterDdsAdapter { listener: l });
                reader.set_listener(Some(adapter), StatusMask::DATA_AVAILABLE)?;
            }
            None => {
                reader.set_listener(None, StatusMask::default())?;
            }
        }
        Ok(())
    }

    /// Install a RequesterListener. The middleware calls `on_reply_available`
    /// when replies arrive; the user must call `take_reply` manually. (7.11.1.4.10)
    /// Pass `None` to remove an existing listener.
    pub fn set_requester_listener(
        &self,
        listener: Option<Arc<dyn RequesterListener<TReq, TRep>>>,
    ) -> DdsRpcResult<()> {
        let reader = self.reader()?;
        match listener {
            Some(l) => {
                // Build a lightweight proxy that shares the same DDS entities.
                let proxy = Requester {
                    request_writer: self.request_writer.clone(),
                    reply_reader: self.reply_reader.clone(),
                    bound_instance: self.bound_instance.clone(),
                    closed: false,
                };
                let adapter: Arc<dyn DataReaderListener<Foo = crate::types::Reply<TRep>>> =
                    Arc::new(RequesterDdsAdapter { listener: l, proxy });
                reader.set_listener(Some(adapter), StatusMask::DATA_AVAILABLE)?;
            }
            None => {
                reader.set_listener(None, StatusMask::default())?;
            }
        }
        Ok(())
    }
}

/// DDS DataReaderListener adapter for SimpleRequesterListener.
/// Takes all available replies and dispatches each to `process_reply`.
struct SimpleRequesterDdsAdapter<TRep> {
    listener: Arc<dyn SimpleRequesterListener<TRep>>,
}

// Safety: listener is Send + Sync (trait bound), no other mutable state.
unsafe impl<TRep> Send for SimpleRequesterDdsAdapter<TRep> {}
unsafe impl<TRep> Sync for SimpleRequesterDdsAdapter<TRep> {}

impl<TRep> DataReaderListener for SimpleRequesterDdsAdapter<TRep>
where
    TRep: 'static
        + Clone
        + Debug
        + DdsType
        + CdrSerialize
        + CdrDeserialize
        + XcdrSerialize
        + XcdrDeserialize,
{
    type Foo = crate::types::Reply<TRep>;

    fn on_data_available(&self, reader: &DataReader<Self::Foo>) {
        if let Ok(samples) = reader.take(
            i32::MAX,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        ) {
            for sample in &samples {
                if let Ok(data) = sample.data() {
                    self.listener.process_reply(sample, &data.header.related_request_id);
                }
            }
        }
    }
}

/// DDS DataReaderListener adapter for RequesterListener.
/// Notifies `on_reply_available` with a proxy that shares the same DDS entities.
struct RequesterDdsAdapter<TReq, TRep> {
    listener: Arc<dyn RequesterListener<TReq, TRep>>,
    proxy: Requester<TReq, TRep>,
}

unsafe impl<TReq, TRep> Send for RequesterDdsAdapter<TReq, TRep> {}
unsafe impl<TReq, TRep> Sync for RequesterDdsAdapter<TReq, TRep> {}

impl<TReq, TRep> DataReaderListener for RequesterDdsAdapter<TReq, TRep>
where
    TReq: 'static
        + Clone
        + Debug
        + DdsType
        + CdrSerialize
        + CdrDeserialize
        + XcdrSerialize
        + XcdrDeserialize,
    TRep: 'static
        + Clone
        + Debug
        + DdsType
        + CdrSerialize
        + CdrDeserialize
        + XcdrSerialize
        + XcdrDeserialize,
{
    type Foo = crate::types::Reply<TRep>;

    fn on_data_available(&self, _reader: &DataReader<Self::Foo>) {
        self.listener.on_reply_available(&self.proxy);
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
