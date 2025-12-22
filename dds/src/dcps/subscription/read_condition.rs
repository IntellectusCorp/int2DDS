//! ReadCondition - Condition for filtering data availability by sample states.
//!
//! A `ReadCondition` is a condition object that triggers when a `DataReader` has samples
//! available that match specified sample, view, and instance state criteria. ReadConditions
//! allow applications to wait for specific types of data using a `WaitSet`.
//!
//! ReadConditions are created with `DataReader::create_readcondition()` and filter samples
//! based on:
//! - Sample state (READ/NOT_READ)
//! - View state (NEW/NOT_NEW)
//! - Instance state (ALIVE/DISPOSED/NO_WRITERS)
//!
//! Unlike listeners which push notifications, ReadConditions enable pull-based data access
//! with state filtering, useful for event-driven architectures.

use std::{
    any::Any,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
};

use super::sample_info::*;
use crate::{
    core::error::{DdsError, DdsResult},
    infrastructure::condition::{
        impl_dds_condition, impl_dds_condition_impl, Condition, ConditionInternal,
    },
    subscription::{
        data_reader::{DataReader, DataReaderInternal},
        qos::DataReaderQos,
    },
};

#[allow(private_bounds)]
pub trait ReadConditionTrait: ReadConditionBase {
    // fn get_datareader<Foo: 'static + Clone + Debug>(&self) -> DdsResult<DataReader<Foo>>;
    // type Foo: 'static + Clone;
    // fn get_datareader(&self) -> DdsResult<DataReader<Self::Foo>>;
    fn get_sample_state_mask(&self) -> &[SampleStateKind];
    fn get_view_state_mask(&self) -> &[ViewStateKind];
    fn get_instance_state_mask(&self) -> &[InstanceStateKind];
}
pub(crate) trait ReadConditionBase: Condition + Any + Send + Sync {
    fn set_trigger_value(&self, trigger_value: bool);
}
macro_rules! impl_dds_read_condition_impl {
    ($type:ty) => {
        impl ReadConditionTrait for $type {
            fn get_instance_state_mask(&self) -> &[InstanceStateKind] {
                &self.instance_state_mask
            }

            fn get_sample_state_mask(&self) -> &[SampleStateKind] {
                &self.sample_state_mask
            }

            fn get_view_state_mask(&self) -> &[ViewStateKind] {
                &self.view_status_mask
            }
        }

        impl ReadConditionBase for $type {
            fn set_trigger_value(&self, trigger_value: bool) {
                log::trace!("set trigger value: {:?}", trigger_value);
                self.trigger_value.store(trigger_value, Ordering::Release);
                if trigger_value {
                    if let Ok(callback) = self.waitset_callback.lock() {
                        if let Some(callback) = callback.as_ref() {
                            callback(); // Always true (state is activated)
                        }
                    }
                }
            }
        }

        // Public wrapper methods - can be called directly without importing trait
        impl $type {
            #[inline]
            pub fn get_instance_state_mask(&self) -> &[InstanceStateKind] {
                <Self as ReadConditionTrait>::get_instance_state_mask(self)
            }

            #[inline]
            pub fn get_sample_state_mask(&self) -> &[SampleStateKind] {
                <Self as ReadConditionTrait>::get_sample_state_mask(self)
            }

            #[inline]
            pub fn get_view_state_mask(&self) -> &[ViewStateKind] {
                <Self as ReadConditionTrait>::get_view_state_mask(self)
            }
        }
    };
}
macro_rules! impl_dds_read_condition {
    ($type:ty) => {
        impl_dds_read_condition_impl!($type);
    };
}
pub(crate) use impl_dds_read_condition;
pub(crate) use impl_dds_read_condition_impl;

#[derive(Clone)]
pub struct ReadCondition {
    trigger_value: Arc<AtomicBool>,
    view_status_mask: Vec<ViewStateKind>,
    instance_state_mask: Vec<InstanceStateKind>,
    sample_state_mask: Vec<SampleStateKind>,
    datareader: Option<Weak<dyn DataReaderInternal<Qos = DataReaderQos>>>,
    waitset_callback: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl Debug for ReadCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadCondition")
            .field("view_status_mask", &self.view_status_mask)
            .field("instance_state_mask", &self.instance_state_mask)
            .field("sample_state_mask", &self.sample_state_mask)
            .field("datareader", &self.datareader.as_ref())
            .field(
                "waitset_callback",
                &self.waitset_callback.lock().unwrap().as_ref().map(|_| "Arc<dyn Fn(bool)>"),
            )
            .finish()
    }
}

impl PartialEq for ReadCondition {
    fn eq(&self, other: &Self) -> bool {
        self.view_status_mask == other.view_status_mask
            && self.instance_state_mask == other.instance_state_mask
            && self.view_status_mask == other.view_status_mask
    }
}
impl Eq for ReadCondition {}

impl From<ReadCondition> for Arc<dyn Condition + Send + Sync>
where
    ReadCondition: Condition + Send + Sync + 'static,
{
    fn from(condition: ReadCondition) -> Self {
        Arc::new(condition) as Arc<dyn Condition + Send + Sync>
    }
}

impl From<ReadCondition> for Arc<dyn ReadConditionTrait + Send + Sync>
where
    ReadCondition: ReadConditionTrait + Send + Sync + 'static,
{
    fn from(condition: ReadCondition) -> Self {
        Arc::new(condition) as Arc<dyn ReadConditionTrait + Send + Sync>
    }
}

impl_dds_condition!(ReadCondition);
impl_dds_read_condition!(ReadCondition);
impl Condition for ReadCondition {
    fn get_trigger_value(&self) -> DdsResult<bool> {
        Ok(self.trigger_value.load(Ordering::Acquire))
    }
}

impl ReadCondition {
    pub(crate) fn new(
        view_status_mask: &[ViewStateKind],
        instance_state_mask: &[InstanceStateKind],
        sample_state_mask: &[SampleStateKind],
        datareader: &Arc<dyn DataReaderInternal<Qos = DataReaderQos>>,
    ) -> Self {
        Self {
            trigger_value: Arc::new(AtomicBool::new(false)),
            view_status_mask: view_status_mask.to_vec(),
            instance_state_mask: instance_state_mask.to_vec(),
            sample_state_mask: sample_state_mask.to_vec(),
            datareader: Some(Arc::downgrade(datareader)),
            waitset_callback: Arc::new(Mutex::new(None)),
        }
    }
    pub fn get_datareader<Foo: 'static + Clone + Debug>(&self) -> DdsResult<DataReader<Foo>> {
        {
            if let Some(datareader) = self.datareader.as_ref() {
                if let Some(datareader_arc) = datareader.upgrade() {
                    if let Some(typed_reader) =
                        datareader_arc.as_any().downcast_ref::<DataReader<Foo>>()
                    {
                        return Ok(typed_reader.clone());
                    }
                }
            }

            Err(DdsError::Error("DataReader reference is invalid or expired".to_string()))
        }
    }
}

impl From<ReadCondition> for Arc<dyn ReadConditionTrait> {
    fn from(val: ReadCondition) -> Self {
        Arc::new(val)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        common::instance_handle::InstanceHandle,
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy,
                ReliabilityQosPolicyKind,
            },
            status::StatusMask,
            wait_set::WaitSet,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        test_utils::unique_domain_id,
        topic::{qos::TopicQos, type_support::DdsType},
    };

    #[derive(DdsType)]
    struct HelloWorldType {
        index: u32,
        message: String,
    }

    #[test]
    fn test_waitset_with_readcondition() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorldType>(
                "read_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader_qos = DataReaderQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let reader = subscriber
            .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();

        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(10)).unwrap();
        wait_set.detach_condition(condition).unwrap();

        // Create ReadCondition: NOT_READ samples only
        let read_condition = reader
            .create_readcondition(
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap();

        // Connect ReadCondition to WaitSet
        wait_set.attach_condition(read_condition.clone()).unwrap();

        writer
            .write(
                &HelloWorldType { index: 0, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 1, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();

        // Wait for all data to be acknowledged by reader
        writer.wait_for_acknowledgments(Duration::from_seconds(10)).unwrap();
        // Delay to ensure data is in reader cache
        std::thread::sleep(std::time::Duration::from_secs(1));

        let samples = reader.read_w_condition(10, read_condition.clone()).unwrap();
        assert!(!samples.is_empty());

        for sample in samples.iter() {
            if sample.sample_info().valid_data {
                log::info!("Received: {:?}", sample.data());
            }
        }

        assert_eq!(read_condition.get_trigger_value(), Ok(false));
    }
}
