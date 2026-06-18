//! TopicRelay: Bidirectional forwarding of a single topic between two participants.
//!
//! A TopicRelay holds two pairs of dynamic Reader/Writer:
//! - LocalNode reader → RemoteNode writer  (LAN → WAN)
//! - RemoteNode reader → LocalNode writer  (WAN → LAN)
//!
//! Data flows in both directions automatically using DynamicData,
//! so the actual topic type does not need to be known at compile time.

use std::sync::Arc;

use crate::{
    common::instance_handle::InstanceHandle,
    core::error::DdsResult,
    domain::domain_participant::DomainParticipant,
    infrastructure::{
        qos_policy::{HistoryQosPolicy, HistoryQosPolicyKind},
        status::StatusMask,
    },
    publication::{
        data_writer::DataWriter,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
    xtypes::{DynamicData, TypeObject},
};

/// Built-in fallback DataReaderQos used when the gateway config supplies no
/// override. `KEEP_ALL` is preferred over the library default `KEEP_LAST(1)`
/// so a momentary relay stall does not silently drop samples; every other
/// policy stays on its type default.
pub fn default_relay_reader_qos() -> DataReaderQos {
    DataReaderQos {
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
        ..Default::default()
    }
}

/// Built-in fallback DataWriterQos. See [`default_relay_reader_qos`].
pub fn default_relay_writer_qos() -> DataWriterQos {
    DataWriterQos {
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
        ..Default::default()
    }
}

/// The four QoS objects a [`TopicRelay`] installs on its endpoints.
///
/// Each side of the relay is an independent DDS endpoint and takes its own
/// QoS; SEDP matching with local pub/sub happens per side. Reader-side and
/// writer-side QoS can differ freely (this mirrors the RTI Routing Service
/// `<input>` / `<output>` model).
#[derive(Debug, Clone)]
pub struct TopicRelayQos {
    pub local_reader: DataReaderQos,
    pub local_writer: DataWriterQos,
    pub remote_reader: DataReaderQos,
    pub remote_writer: DataWriterQos,
}

impl Default for TopicRelayQos {
    fn default() -> Self {
        Self {
            local_reader: default_relay_reader_qos(),
            local_writer: default_relay_writer_qos(),
            remote_reader: default_relay_reader_qos(),
            remote_writer: default_relay_writer_qos(),
        }
    }
}

/// Bidirectional relay for a single topic between LocalNode and RemoteNode.
///
/// Both participants must already be created. The TopicRelay creates
/// dynamic topics, readers, and writers for the given topic name on both
/// participants using the supplied TypeObject.
///
/// Use [`TopicRelay::forward_local_to_remote`] and
/// [`TopicRelay::forward_remote_to_local`] to drive forwarding manually,
/// or [`TopicRelay::forward_once`] to run a single bidirectional pass.
pub struct TopicRelay {
    topic_name: String,
    local_reader: Arc<DataReader<DynamicData>>,
    local_writer: Arc<DataWriter<DynamicData>>,
    remote_reader: Arc<DataReader<DynamicData>>,
    remote_writer: Arc<DataWriter<DynamicData>>,
    // Own writer handles on each side. Used to drop self-reflected samples:
    // a participant's writer on a given topic self-matches that participant's
    // reader on the same topic, so without filtering, every forwarded sample
    // loops back and bounces across the gateway indefinitely.
    local_writer_handle: InstanceHandle,
    remote_writer_handle: InstanceHandle,
}

impl TopicRelay {
    /// Create a new TopicRelay for `topic_name` between `local` and `remote`.
    ///
    /// Both participants will get a dynamic topic + reader + writer for the
    /// same TypeObject, enabling bidirectional forwarding.
    pub fn new(
        local: &DomainParticipant,
        remote: &DomainParticipant,
        topic_name: &str,
        type_object: TypeObject,
    ) -> DdsResult<Self> {
        Self::new_with_qos(local, remote, topic_name, type_object, TopicRelayQos::default())
    }

    /// Create a TopicRelay with explicit per-endpoint QoS. Use this when
    /// local pub/sub requires non-default policies (RELIABLE, TRANSIENT_LOCAL,
    /// ownership, deadline, etc.) so SEDP actually matches.
    pub fn new_with_qos(
        local: &DomainParticipant,
        remote: &DomainParticipant,
        topic_name: &str,
        type_object: TypeObject,
        qos: TopicRelayQos,
    ) -> DdsResult<Self> {
        let local_type_support =
            Arc::new(local.create_dynamic_type_from_type_object(type_object.clone())?);
        let remote_type_support =
            Arc::new(remote.create_dynamic_type_from_type_object(type_object)?);

        let local_topic = local.create_topic_dynamic(
            topic_name,
            local_type_support.clone(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )?;
        let remote_topic = remote.create_topic_dynamic(
            topic_name,
            remote_type_support.clone(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )?;

        let local_subscriber =
            local.create_subscriber(SubscriberQos::default(), None, StatusMask::default())?;
        let local_publisher =
            local.create_publisher(PublisherQos::default(), None, StatusMask::default())?;
        let remote_subscriber =
            remote.create_subscriber(SubscriberQos::default(), None, StatusMask::default())?;
        let remote_publisher =
            remote.create_publisher(PublisherQos::default(), None, StatusMask::default())?;

        let local_reader = Arc::new(local_subscriber.create_datareader_dynamic(
            &local_topic,
            local_type_support.clone(),
            qos.local_reader,
            None,
            StatusMask::default(),
        )?);
        let local_writer = Arc::new(local_publisher.create_datawriter_dynamic(
            &local_topic,
            local_type_support,
            qos.local_writer,
            None,
            StatusMask::default(),
        )?);
        let remote_reader = Arc::new(remote_subscriber.create_datareader_dynamic(
            &remote_topic,
            remote_type_support.clone(),
            qos.remote_reader,
            None,
            StatusMask::default(),
        )?);
        let remote_writer = Arc::new(remote_publisher.create_datawriter_dynamic(
            &remote_topic,
            remote_type_support,
            qos.remote_writer,
            None,
            StatusMask::default(),
        )?);

        let local_writer_handle = local_writer.get_instance_handle()?;
        let remote_writer_handle = remote_writer.get_instance_handle()?;

        Ok(Self {
            topic_name: topic_name.to_string(),
            local_reader,
            local_writer,
            remote_reader,
            remote_writer,
            local_writer_handle,
            remote_writer_handle,
        })
    }

    /// Topic name handled by this relay.
    pub fn topic_name(&self) -> &str {
        &self.topic_name
    }

    /// Take all available samples from `from` and write them to `to`,
    /// dropping any sample produced by `sibling_writer_handle` — that is,
    /// the writer living in the same participant as `from`. Without this
    /// filter, the bidirectional relay would re-forward every sample it
    /// just emitted, because a participant's writer on a given topic
    /// self-matches that same participant's reader.
    ///
    /// Log output (enable via `--verbose` for debug lines):
    ///   info : per-batch summary whenever `take` returned >0 samples
    ///   debug: per-sample forwarding detail
    ///   warn : take/write errors
    fn forward(
        from: &DataReader<DynamicData>,
        to: &DataWriter<DynamicData>,
        sibling_writer_handle: InstanceHandle,
        topic_name: &str,
        direction: &str,
    ) -> DdsResult<usize> {
        let t_take = std::time::Instant::now();
        let samples = match from.take(
            i32::MAX,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        ) {
            Ok(s) => s,
            Err(crate::core::error::DdsError::NoData) => return Ok(0),
            Err(e) => {
                log::warn!("[relay:{} {}] take error: {:?}", topic_name, direction, e);
                return Err(e);
            }
        };
        let take_elapsed = t_take.elapsed();
        let took = samples.len();

        let t_write = std::time::Instant::now();
        let mut count = 0;
        let mut filtered = 0;
        let mut data_err = 0;
        for sample in samples.iter() {
            if sample.sample_info().publication_handle == sibling_writer_handle {
                filtered += 1;
                continue;
            }
            match sample.data() {
                Ok(data) => match to.write(&data, InstanceHandle::NIL) {
                    Ok(()) => {
                        count += 1;
                        log::debug!(
                            "[relay:{} {}] fwd #{} pub_handle={:?}",
                            topic_name,
                            direction,
                            count,
                            sample.sample_info().publication_handle,
                        );
                    }
                    Err(e) => {
                        log::warn!(
                            "[relay:{} {}] write error at {}/{}: {:?}",
                            topic_name,
                            direction,
                            count + 1,
                            took,
                            e,
                        );
                        return Err(e);
                    }
                },
                Err(_) => data_err += 1,
            }
        }
        let write_elapsed = t_write.elapsed();

        if took > 0 {
            log::info!(
                "[relay:{} {}] took={} fwd={} filtered={} data_err={} t_take={:?} t_write={:?}",
                topic_name,
                direction,
                took,
                count,
                filtered,
                data_err,
                take_elapsed,
                write_elapsed,
            );
        }
        Ok(count)
    }

    /// Forward all available LocalNode samples to RemoteNode.
    pub fn forward_local_to_remote(&self) -> DdsResult<usize> {
        Self::forward(
            &self.local_reader,
            &self.remote_writer,
            self.local_writer_handle,
            &self.topic_name,
            "L->R",
        )
    }

    /// Forward all available RemoteNode samples to LocalNode.
    pub fn forward_remote_to_local(&self) -> DdsResult<usize> {
        Self::forward(
            &self.remote_reader,
            &self.local_writer,
            self.remote_writer_handle,
            &self.topic_name,
            "R->L",
        )
    }

    /// Run one bidirectional forwarding pass.
    /// Returns (local_to_remote_count, remote_to_local_count).
    pub fn forward_once(&self) -> DdsResult<(usize, usize)> {
        let l2r = self.forward_local_to_remote()?;
        let r2l = self.forward_remote_to_local()?;
        Ok((l2r, r2l))
    }
}
