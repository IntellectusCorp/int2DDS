//! QueryCondition - ReadCondition with content-based filtering.
//!
//! A `QueryCondition` extends `ReadCondition` by adding SQL-like content filtering on top
//! of sample state filtering. It triggers when a `DataReader` has samples available that
//! match both the state criteria and the query expression.
//!
//! QueryConditions are created with `DataReader::create_querycondition()` and provide:
//! - All ReadCondition state filtering (sample/view/instance states)
//! - SQL-92 query expression filtering on sample content
//! - Parameter substitution for dynamic query modification
//!
//! This enables fine-grained, content-aware event handling without needing to read and
//! filter all samples in application code.

use std::{
    any::Any,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
};

use super::read_condition::ReadConditionTrait;
use crate::{
    core::error::{DdsError, DdsResult},
    infrastructure::condition::{
        impl_dds_condition, impl_dds_condition_impl, Condition, ConditionInternal,
    },
    subscription::{
        data_reader::{DataReader, DataReaderInternal},
        qos::DataReaderQos,
        read_condition::{
            impl_dds_read_condition, impl_dds_read_condition_impl, ReadConditionBase,
        },
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::sql::{ast::Expression, parse_expression},
    DdsType,
};

#[derive(Clone)]
pub struct QueryCondition {
    trigger_value: Arc<AtomicBool>,
    view_status_mask: Vec<ViewStateKind>,
    instance_state_mask: Vec<InstanceStateKind>,
    sample_state_mask: Vec<SampleStateKind>,
    query_expression: String,
    query_parameters: Arc<Mutex<Vec<String>>>,
    pub(crate) parsed_expression: Expression,
    datareader: Option<Weak<dyn DataReaderInternal<Qos = DataReaderQos>>>,
    #[allow(clippy::type_complexity)]
    waitset_callback: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl Debug for QueryCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryCondition")
            .field("view_status_mask", &self.view_status_mask)
            .field("instance_state_mask", &self.instance_state_mask)
            .field("sample_state_mask", &self.sample_state_mask)
            .field("query_expression", &self.query_expression)
            .field("query_parameters", &self.query_parameters)
            .field("datareader", &self.datareader.as_ref())
            .field(
                "waitset_callback",
                &self.waitset_callback.lock().unwrap().as_ref().map(|_| "Arc<dyn Fn(bool)>"),
            )
            .finish()
    }
}

impl From<QueryCondition> for Arc<dyn Condition + Send + Sync>
where
    QueryCondition: Condition + Send + Sync + 'static,
{
    fn from(condition: QueryCondition) -> Self {
        Arc::new(condition) as Arc<dyn Condition + Send + Sync>
    }
}

impl From<QueryCondition> for Arc<dyn ReadConditionTrait + Send + Sync>
where
    QueryCondition: ReadConditionTrait + Send + Sync + 'static,
{
    fn from(condition: QueryCondition) -> Self {
        Arc::new(condition) as Arc<dyn ReadConditionTrait + Send + Sync>
    }
}

impl_dds_condition!(QueryCondition);
impl_dds_read_condition!(QueryCondition);
impl Condition for QueryCondition {
    fn get_trigger_value(&self) -> DdsResult<bool> {
        Ok(self.trigger_value.load(Ordering::Acquire))
    }
}

impl QueryCondition {
    pub(crate) fn new(
        view_status_mask: &[ViewStateKind],
        instance_state_mask: &[InstanceStateKind],
        sample_state_mask: &[SampleStateKind],
        query_expression: &str,
        query_parameters: Vec<String>,
        datareader: &Arc<dyn DataReaderInternal<Qos = DataReaderQos>>,
    ) -> DdsResult<Self> {
        let expression = parse_expression(query_expression, true)?;
        expression.validate_expression_parameters(&query_parameters)?;

        Ok(Self {
            trigger_value: Arc::new(AtomicBool::new(false)),
            view_status_mask: view_status_mask.to_vec(),
            instance_state_mask: instance_state_mask.to_vec(),
            sample_state_mask: sample_state_mask.to_vec(),
            query_expression: query_expression.to_string(),
            query_parameters: Arc::new(Mutex::new(query_parameters)),
            parsed_expression: expression,
            datareader: Some(Arc::downgrade(datareader)),
            waitset_callback: Arc::new(Mutex::new(None)),
        })
    }
    pub fn get_query_expression(&self) -> &str {
        &self.query_expression
    }

    pub fn get_query_parameters(&self) -> DdsResult<Vec<String>> {
        match self.query_parameters.lock() {
            Ok(query_parameters) => Ok(query_parameters.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub fn set_query_parameters(&self, query_parameters: Vec<String>) -> DdsResult<()> {
        self.parsed_expression.validate_expression_parameters(&query_parameters)?;

        match self.query_parameters.lock() {
            Ok(mut prev_params) => {
                *prev_params = query_parameters;
                Ok(())
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
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

    pub(crate) fn evaluate_expression<Foo>(&self, data: &Foo) -> DdsResult<bool>
    where
        Foo: DdsType,
    {
        let parameters = self.get_query_parameters()?;
        self.parsed_expression.evaluate(data, &parameters)
    }

    pub(crate) fn get_order_by_fields(&self) -> Option<&Vec<String>> {
        self.parsed_expression.get_order_by_fields()
    }
}

impl From<QueryCondition> for Arc<dyn ReadConditionTrait> {
    fn from(val: QueryCondition) -> Self {
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
        topic::qos::TopicQos,
        DdsType,
    };
    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct HelloWorldType {
        index: u32,
        message: String,
    }

    #[test]
    fn test_waitset_with_querycondition_order_by() {
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
                "order_topic",
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

        // Wait for matching
        let mut condition = reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(10)).unwrap();
        wait_set.detach_condition(condition).unwrap();

        // Create QueryCondition for ORDER BY test: index > 0 ORDER BY message, index
        let query_condition = reader
            .create_querycondition(
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                "index > %0 ORDER BY message, index",
                vec!["0".to_string()], // index > 0 (all samples)
            )
            .unwrap();

        // Send various data in order (for checking sort results)
        writer
            .write(
                &HelloWorldType { index: 300, message: "charlie".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 100, message: "alpha".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(&HelloWorldType { index: 200, message: "beta".to_string() }, InstanceHandle::NIL)
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 150, message: "alpha".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(&HelloWorldType { index: 50, message: "beta".to_string() }, InstanceHandle::NIL)
            .unwrap();

        // Wait for all data to be acknowledged by reader
        writer.wait_for_acknowledgments(Duration::from_seconds(10)).unwrap();
        // Delay to ensure data is in reader cache
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Read samples with QueryCondition
        let samples = reader.read_w_condition(10, query_condition.clone()).unwrap();
        assert_eq!(samples.len(), 5);

        log::info!("=== ORDER BY Test Results ===");
        log::info!("Expected order: ORDER BY message, index");
        log::info!("Should be: alpha(100), alpha(150), beta(50), beta(200), charlie(300)");

        // Check ORDER BY message, index results
        let expected_order = vec![
            (100, "alpha"),   // message: alpha, index: 100
            (150, "alpha"),   // message: alpha, index: 150
            (50, "beta"),     // message: beta, index: 50
            (200, "beta"),    // message: beta, index: 200
            (300, "charlie"), // message: charlie, index: 300
        ];

        for (i, sample) in samples.iter().enumerate() {
            if sample.sample_info().valid_data {
                let data = sample.data().unwrap();
                let (expected_index, expected_message) = expected_order[i];

                log::info!(
                    "Sample {}: index={}, message='{}' (expected: index={}, message='{}')",
                    i + 1,
                    data.index,
                    data.message,
                    expected_index,
                    expected_message
                );

                // Verify ORDER BY sorting is correct
                assert_eq!(data.index, expected_index, "Index mismatch at position {}", i);
                assert_eq!(data.message, expected_message, "Message mismatch at position {}", i);
            }
        }

        assert_eq!(query_condition.get_trigger_value(), Ok(false));
        log::info!("=== ORDER BY Test Completed Successfully ===");
    }

    #[test]
    fn test_waitset_with_querycondition() {
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
                "query_topic",
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

        // Create QueryCondition: samples where index > 100 only
        let query_condition = reader
            .create_querycondition(
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                "index > %0",
                vec!["100".to_string()],
            )
            .unwrap();

        // Connect QueryCondition to WaitSet
        wait_set.attach_condition(query_condition.clone()).unwrap();

        writer
            .write(
                &HelloWorldType { index: 0, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 100, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 200, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();

        // Wait for all data to be acknowledged by reader
        writer.wait_for_acknowledgments(Duration::from_seconds(10)).unwrap();
        // Delay to ensure data is in reader cache
        std::thread::sleep(std::time::Duration::from_secs(1));

        let samples = reader.read_w_condition(10, query_condition.clone()).unwrap();
        assert_eq!(samples.len(), 1); // Only index: 200 matches (index > 100)

        for sample in samples.iter() {
            if sample.sample_info().valid_data {
                log::info!("Received: {:?}", sample.data());
            }
        }

        assert_eq!(query_condition.get_trigger_value(), Ok(false));
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct Inner {
        x: i32,
        y: i32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct Outer {
        inner: Inner,
        label: String,
    }

    #[test]
    fn test_querycondition_nested_struct_field() {
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
            .create_topic::<Outer>(
                "nested_query_topic",
                "Outer",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let rdr_qos = DataReaderQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let reader = subscriber
            .create_datareader::<Outer>(&topic, rdr_qos, None, StatusMask::default())
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let wtr_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<Outer>(&topic, wtr_qos, None, StatusMask::default())
            .unwrap();

        // Wait for matching
        let mut condition = reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(10)).unwrap();
        wait_set.detach_condition(condition).unwrap();

        // Filter on nested field: inner.x > 50
        let qc = reader
            .create_querycondition(
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                "inner.x > %0",
                vec!["50".to_string()],
            )
            .unwrap();

        writer
            .write(
                &Outer { inner: Inner { x: 10, y: 1 }, label: "a".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &Outer { inner: Inner { x: 100, y: 2 }, label: "b".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &Outer { inner: Inner { x: 30, y: 3 }, label: "c".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &Outer { inner: Inner { x: 200, y: 4 }, label: "d".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();

        writer.wait_for_acknowledgments(Duration::from_seconds(10)).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Only inner.x > 50 should match: x=100 ("b") and x=200 ("d")
        let samples = reader.take_w_condition(10, qc).unwrap();
        assert_eq!(samples.len(), 2);

        let data0 = samples[0].data().unwrap();
        let data1 = samples[1].data().unwrap();
        assert_eq!(data0.inner.x, 100);
        assert_eq!(data0.label, "b");
        assert_eq!(data1.inner.x, 200);
        assert_eq!(data1.label, "d");

        // Non-matching samples (x=10, x=30) should still be in cache
        let remaining = reader
            .take(
                10,
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
            )
            .unwrap();
        assert_eq!(remaining.len(), 2);
    }
}
