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
