//! Replier — the request-reply style component that receives requests and sends replies.
//!
//! Owns a DDS DataReader (for requests) and DataWriter (for replies), correlating
//! each reply to its original request via SampleIdentity.

use std::sync::Arc;
use std::time::Duration;

use int2dds::common::instance_handle::InstanceHandle;
use int2dds::dcps::core::error::DdsError;
use int2dds::dcps::infrastructure::qos_policy::{
    DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy, HistoryQosPolicyKind,
    ReliabilityQosPolicy, ReliabilityQosPolicyKind,
};
use int2dds::dcps::infrastructure::status::StatusMask;
use int2dds::dcps::publication::data_writer::DataWriter;
use int2dds::dcps::publication::qos::DataWriterQos;
use int2dds::dcps::subscription::data_reader::DataReader;
use int2dds::dcps::subscription::data_reader_listener::DataReaderListener;
use int2dds::dcps::subscription::qos::DataReaderQos;
use int2dds::dcps::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
use int2dds::dcps::topic::qos::TopicQos;
use int2dds::dcps::topic::type_support::DdsType;

use crate::entity::RpcEntity;
use crate::error::{DdsRpcError, DdsRpcResult};
use crate::listener::{ReplierListener, SimpleReplierListener};
use crate::params::ReplierParams;
use crate::sample::Sample;
use crate::topic_name::TopicNameConfig;
use crate::types::{DdsRpcType, RemoteExceptionCode, Reply, ReplyHeader, Request, SampleIdentity};

/// Low-level request-reply endpoint on the service side.
/// Receives incoming requests via a DDS DataReader and sends correlated
/// replies through a DDS DataWriter, using SampleIdentity for correlation.
pub struct Replier<TReq, TRep> {
    request_reader: Option<Arc<DataReader<Request<TReq>>>>,
    reply_writer: Option<Arc<DataWriter<Reply<TRep>>>>,
    closed: bool,
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> Replier<TReq, TRep> {
    pub fn new(params: ReplierParams) -> DdsRpcResult<Self> {
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
        let reply_type_name =
            topic_config.reply_type().unwrap_or_else(|| Reply::<TRep>::get_type_name());

        let request_topic = params.participant.create_topic::<Request<TReq>>(
            &request_topic_name,
            &request_type_name,
            TopicQos::default(),
            None,
            StatusMask::default(),
        )?;

        let reply_topic = params.participant.create_topic::<Reply<TRep>>(
            &reply_topic_name,
            &reply_type_name,
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
        let request_reader = subscriber.create_datareader::<Request<TReq>>(
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
        let reply_writer = publisher.create_datawriter::<Reply<TRep>>(
            &reply_topic,
            writer_qos,
            None,
            StatusMask::default(),
        )?;

        Ok(Self {
            request_reader: Some(Arc::new(request_reader)),
            reply_writer: Some(Arc::new(reply_writer)),
            closed: false,
        })
    }

    fn reader(&self) -> DdsRpcResult<&DataReader<Request<TReq>>> {
        self.request_reader.as_deref().ok_or(DdsError::AlreadyDeleted.into())
    }

    fn writer(&self) -> DdsRpcResult<&DataWriter<Reply<TRep>>> {
        self.reply_writer.as_deref().ok_or(DdsError::AlreadyDeleted.into())
    }

    /// Send a reply with explicit RemoteExceptionCode.
    pub fn send_reply_with_exception_code(
        &self,
        data: &TRep,
        related_request_id: &SampleIdentity,
        remote_ex: RemoteExceptionCode,
    ) -> DdsRpcResult<()> {
        let mut reply = Reply {
            header: ReplyHeader { related_request_id: *related_request_id, remote_ex },
            data: data.clone(),
        };
        self.writer()?.write(&mut reply, InstanceHandle::NIL)?;
        Ok(())
    }

    /// Send a reply correlated with the given request identity.
    pub fn send_reply(&self, data: &TRep, related_request_id: &SampleIdentity) -> DdsRpcResult<()> {
        self.send_reply_with_exception_code(data, related_request_id, RemoteExceptionCode::Ok)
    }

    /// Take a single pending request (non-blocking).
    pub fn take_request(&self) -> DdsRpcResult<Option<Sample<Request<TReq>>>> {
        let samples = self.reader()?.take(
            1,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )?;
        Ok(samples.into_iter().next())
    }

    /// Take up to `max_count` pending requests (non-blocking).
    pub fn take_requests(&self, max_count: i32) -> DdsRpcResult<Vec<Sample<Request<TReq>>>> {
        let samples = self.reader()?.take(
            max_count,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )?;
        Ok(samples)
    }

    /// Block until a request arrives or timeout expires.
    pub fn receive_request(&self, timeout: Duration) -> DdsRpcResult<Sample<Request<TReq>>> {
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

    pub fn get_request_datareader(&self) -> DdsRpcResult<&DataReader<Request<TReq>>> {
        self.reader()
    }

    pub fn get_reply_datawriter(&self) -> DdsRpcResult<&DataWriter<Reply<TRep>>> {
        self.writer()
    }

    /// Install a SimpleReplierListener. The middleware takes each arriving request,
    /// calls `process_request`, and automatically sends the returned reply.
    /// Pass `None` to remove an existing listener.
    pub fn set_simple_replier_listener(
        &self,
        listener: Option<Arc<dyn SimpleReplierListener<TReq, TRep>>>,
    ) -> DdsRpcResult<()> {
        let reader = self.reader()?;
        match listener {
            Some(l) => {
                let adapter: Arc<dyn DataReaderListener<Foo = Request<TReq>>> =
                    Arc::new(SimpleReplierDdsAdapter {
                        listener: l,
                        reply_writer: self.reply_writer.clone().ok_or(DdsError::AlreadyDeleted)?,
                    });
                reader.set_listener(Some(adapter), StatusMask::DATA_AVAILABLE)?;
            }
            None => {
                reader.set_listener(None, StatusMask::default())?;
            }
        }
        Ok(())
    }

    /// Install a ReplierListener. The middleware calls `on_request_available`
    /// when requests arrive; the user must call `take_request` / `send_reply`
    /// manually.
    /// Pass `None` to remove an existing listener.
    pub fn set_replier_listener(
        &self,
        listener: Option<Arc<dyn ReplierListener<TReq, TRep>>>,
    ) -> DdsRpcResult<()> {
        let reader = self.reader()?;
        match listener {
            Some(l) => {
                // Proxy shares the same DDS entities via Arc — no dependency on
                // DataReader/DataWriter Clone semantics.
                let proxy = Replier {
                    request_reader: self.request_reader.clone(),
                    reply_writer: self.reply_writer.clone(),
                    closed: false,
                };
                let adapter: Arc<dyn DataReaderListener<Foo = Request<TReq>>> =
                    Arc::new(ReplierDdsAdapter { listener: l, proxy });
                reader.set_listener(Some(adapter), StatusMask::DATA_AVAILABLE)?;
            }
            None => {
                reader.set_listener(None, StatusMask::default())?;
            }
        }
        Ok(())
    }
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> RpcEntity for Replier<TReq, TRep> {
    fn close(&mut self) -> DdsRpcResult<()> {
        // Detach listener first to release any adapter-held Arc references
        if let Some(ref reader) = self.request_reader {
            let _ = reader.set_listener(None, StatusMask::default());
        }
        // delete_datareader/writer requires owned value, not Arc
        if let Some(reader_arc) = self.request_reader.take() {
            if let Ok(reader) = Arc::try_unwrap(reader_arc) {
                let subscriber = reader.get_subscriber()?;
                subscriber.delete_datareader(reader)?;
            }
        }
        if let Some(writer_arc) = self.reply_writer.take() {
            if let Ok(writer) = Arc::try_unwrap(writer_arc) {
                let publisher = writer.get_publisher()?;
                publisher.delete_datawriter(writer)?;
            }
        }
        self.closed = true;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

/// DDS DataReaderListener adapter for SimpleReplierListener.
/// Takes all available requests, dispatches each to `process_request`,
/// and sends the returned reply automatically.
struct SimpleReplierDdsAdapter<TReq: Send + Sync, TRep: Send + Sync> {
    listener: Arc<dyn SimpleReplierListener<TReq, TRep>>,
    reply_writer: Arc<DataWriter<Reply<TRep>>>,
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> DataReaderListener
    for SimpleReplierDdsAdapter<TReq, TRep>
{
    type Foo = Request<TReq>;

    fn on_data_available(&self, reader: &DataReader<Self::Foo>) {
        if let Ok(samples) = reader.take(
            i32::MAX,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        ) {
            for sample in &samples {
                if let Ok(data) = sample.data() {
                    let request_id = data.header.request_id;
                    if let Some(reply_data) = self.listener.process_request(sample, &request_id) {
                        let mut reply = Reply {
                            header: ReplyHeader {
                                related_request_id: request_id,
                                remote_ex: RemoteExceptionCode::Ok,
                            },
                            data: reply_data,
                        };
                        let _ = self.reply_writer.write(&mut reply, InstanceHandle::NIL);
                    }
                }
            }
        }
    }
}

/// DDS DataReaderListener adapter for ReplierListener.
/// Notifies `on_request_available` with a proxy that shares the same DDS entities via Arc.
struct ReplierDdsAdapter<TReq: Send + Sync, TRep: Send + Sync> {
    listener: Arc<dyn ReplierListener<TReq, TRep>>,
    proxy: Replier<TReq, TRep>,
}

impl<TReq: DdsRpcType, TRep: DdsRpcType> DataReaderListener for ReplierDdsAdapter<TReq, TRep> {
    type Foo = Request<TReq>;

    fn on_data_available(&self, _reader: &DataReader<Self::Foo>) {
        self.listener.on_request_available(&self.proxy);
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
