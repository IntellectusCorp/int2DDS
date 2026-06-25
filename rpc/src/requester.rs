//! Requester — the request-reply style component that sends requests and receives replies.
//!
//! Owns a DDS DataWriter (for requests) and DataReader (for replies), correlating
//! replies back to their originating request via SampleIdentity.

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
use int2dds::dcps::publication::qos::DataWriterQos;
use int2dds::dcps::subscription::data_reader::DataReader;
use int2dds::dcps::subscription::data_reader_listener::DataReaderListener;
use int2dds::dcps::subscription::qos::DataReaderQos;
use int2dds::dcps::subscription::query_condition::QueryCondition;
use int2dds::dcps::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
use int2dds::dcps::topic::qos::TopicQos;
use int2dds::dcps::topic::type_support::DdsType;

use crate::entity::{RpcEntity, ServiceProxy};
use crate::error::{DdsRpcError, DdsRpcResult};
use crate::listener::{RequesterListener, SimpleRequesterListener};
use crate::params::RequesterParams;
use crate::sample::Sample;
use crate::topic_name::TopicNameConfig;
use crate::types::{DdsRpcType, InstanceName, Request, SampleIdentity};

/// Low-level request-reply endpoint on the client side.
/// Sends requests via a DDS DataWriter and receives correlated replies
/// through a DDS DataReader, using SampleIdentity for correlation.
pub struct Requester<TReq, TRep> {
    request_writer: Option<Arc<DataWriter<Request<TReq>>>>,
    reply_reader: Option<Arc<DataReader<crate::types::Reply<TRep>>>>,
    bound_instance: Option<InstanceName>,
    closed: bool,
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> Requester<TReq, TRep> {
    pub fn new(params: RequesterParams) -> DdsRpcResult<Self> {
        let topic_config = TopicNameConfig {
            interface_name: params.interface_name.clone(),
            service_name: params.service_name.clone(),
            request_topic_override: params.request_topic_name.clone(),
            reply_topic_override: params.reply_topic_name.clone(),
            request_type_override: None,
            reply_type_override: None,
        };

        let request_topic_name = topic_config.request_topic();
        let reply_topic_name = topic_config.reply_topic();
        let request_type_name =
            topic_config.request_type().unwrap_or_else(|| Request::<TReq>::get_type_name());
        let reply_type_name = topic_config
            .reply_type()
            .unwrap_or_else(|| crate::types::Reply::<TRep>::get_type_name());

        let request_topic = params.participant.create_topic::<Request<TReq>>(
            &request_topic_name,
            &request_type_name,
            TopicQos::default(),
            None,
            StatusMask::default(),
        )?;

        let reply_topic = params.participant.create_topic::<crate::types::Reply<TRep>>(
            &reply_topic_name,
            &reply_type_name,
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
            request_writer: Some(Arc::new(request_writer)),
            reply_reader: Some(Arc::new(reply_reader)),
            bound_instance: None,
            closed: false,
        })
    }

    fn writer(&self) -> DdsRpcResult<&DataWriter<Request<TReq>>> {
        self.request_writer.as_deref().ok_or(DdsError::AlreadyDeleted.into())
    }

    fn reader(&self) -> DdsRpcResult<&DataReader<crate::types::Reply<TRep>>> {
        self.reply_reader.as_deref().ok_or(DdsError::AlreadyDeleted.into())
    }

    /// Send a request. The middleware fills in `RequestHeader.requestId`
    /// before writing, and returns it for reply correlation.
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

    /// Send a request and return a Future for the correlated reply.
    pub fn send_request_async(&self, data: &TReq) -> DdsRpcResult<Future<TRep>> {
        let identity = self.send_request(data)?;
        let condition = self.create_correlation_condition(&identity)?;
        let reader = self.reader()?.clone();
        Ok(Future { reader, condition })
    }

    /// Install a SimpleRequesterListener. The middleware takes each arriving reply
    /// and dispatches it to `process_reply`.
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
    /// when replies arrive; the user must call `take_reply` manually.
    /// Pass `None` to remove an existing listener.
    pub fn set_requester_listener(
        &self,
        listener: Option<Arc<dyn RequesterListener<TReq, TRep>>>,
    ) -> DdsRpcResult<()> {
        let reader = self.reader()?;
        match listener {
            Some(l) => {
                // Proxy shares the same DDS entities via Arc — no dependency on
                // DataReader/DataWriter Clone semantics.
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

impl<TReq: DdsRpcType, TRep: DdsRpcType> RpcEntity for Requester<TReq, TRep> {
    fn close(&mut self) -> DdsRpcResult<()> {
        // Detach listener first to release any adapter-held Arc references
        if let Some(ref reader) = self.reply_reader {
            let _ = reader.set_listener(None, StatusMask::default());
        }
        if let Some(writer_arc) = self.request_writer.take() {
            // delete_datawriter/reader requires owned value, not Arc
            if let Ok(writer) = Arc::try_unwrap(writer_arc) {
                let publisher = writer.get_publisher()?;
                publisher.delete_datawriter(writer)?;
            }
        }
        if let Some(reader_arc) = self.reply_reader.take() {
            if let Ok(reader) = Arc::try_unwrap(reader_arc) {
                let subscriber = reader.get_subscriber()?;
                subscriber.delete_datareader(reader)?;
            }
        }
        self.closed = true;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> ServiceProxy for Requester<TReq, TRep> {
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
        let condition = self.writer()?.get_statuscondition()?;
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED)?;
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition)?;
        wait_set.wait(int2dds::dcps::core::time::Duration::infinite())?;

        let condition = self.reader()?.get_statuscondition()?;
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED)?;
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition)?;
        wait_set.wait(int2dds::dcps::core::time::Duration::infinite())?;

        Ok(())
    }

    fn wait_for_service_timeout(&self, timeout: Duration) -> DdsRpcResult<()> {
        let dds_timeout = int2dds::dcps::core::time::Duration::try_from(timeout)?;

        let condition = self.writer()?.get_statuscondition()?;
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED)?;
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition)?;
        wait_set.wait(dds_timeout)?;

        let condition = self.reader()?.get_statuscondition()?;
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED)?;
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition)?;
        wait_set.wait(dds_timeout)?;
        Ok(())
    }
}

/// Future-based reply reception.
/// Wraps a WaitSet + QueryCondition to wait only for the correlated reply.
pub struct Future<TRep: DdsRpcType> {
    reader: DataReader<crate::types::Reply<TRep>>,
    condition: QueryCondition,
}

impl<TRep: DdsRpcType> Future<TRep> {
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

/// DDS DataReaderListener adapter for SimpleRequesterListener.
/// Takes all available replies and dispatches each to `process_reply`.
struct SimpleRequesterDdsAdapter<TRep: Send + Sync> {
    listener: Arc<dyn SimpleRequesterListener<TRep>>,
}

impl<TRep: DdsRpcType> DataReaderListener for SimpleRequesterDdsAdapter<TRep> {
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
/// Notifies `on_reply_available` with a proxy that shares the same DDS entities via Arc.
struct RequesterDdsAdapter<TReq: Send + Sync, TRep: Send + Sync> {
    listener: Arc<dyn RequesterListener<TReq, TRep>>,
    proxy: Requester<TReq, TRep>,
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> DataReaderListener for RequesterDdsAdapter<TReq, TRep> {
    type Foo = crate::types::Reply<TRep>;

    fn on_data_available(&self, _reader: &DataReader<Self::Foo>) {
        self.listener.on_reply_available(&self.proxy);
    }
}

// RPC default QoS: RELIABLE, KEEP_ALL, VOLATILE
fn rpc_datawriter_qos() -> DataWriterQos {
    let mut qos = DataWriterQos::default();
    qos.reliability =
        ReliabilityQosPolicy { kind: ReliabilityQosPolicyKind::Reliable, ..qos.reliability };
    qos.history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true };
    qos.durability = DurabilityQosPolicy { kind: DurabilityQosPolicyKind::Volatile };
    qos
}

fn rpc_datareader_qos() -> DataReaderQos {
    let mut qos = DataReaderQos::default();
    qos.reliability =
        ReliabilityQosPolicy { kind: ReliabilityQosPolicyKind::Reliable, ..qos.reliability };
    qos.history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true };
    qos.durability = DurabilityQosPolicy { kind: DurabilityQosPolicyKind::Volatile };
    qos
}
