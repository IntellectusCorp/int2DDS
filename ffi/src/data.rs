//! # FFI Data Type
//!
//! Minimal type placeholder for DDS generic parameters.
//!
//! `Int2DdsData` is used as the generic type parameter for `DataWriter<Int2DdsData>`
//! and `DataReader<Int2DdsData>` in the FFI layer. Actual serialization/deserialization
//! is handled by the registered `RawTypeSupport`, not by this type.

use std::any::{Any, TypeId};
use std::sync::Arc;

use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::core::error::{DdsError, DdsResult},
    rtps::common::types::SerializedData,
    serialize::cdr::ExtensibilityKind,
    topic::{
        sql::ast::Parameter,
        type_support::{DdsType, FieldAccessor, SerializationFormat, TypeSupport},
    },
    xtypes::TypePlans,
};

/// Minimal data type for FFI generic parameters.
///
/// This type exists solely to satisfy the `DdsType` trait bound on
/// `DataWriter<T>` and `DataReader<T>`. All actual data flows through
/// raw serialized bytes via `int2dds_datawriter_write_serialized` /
/// `int2dds_datareader_take_serialized`.
#[derive(Debug, Clone, Default)]
pub struct Int2DdsData {
    /// Raw CDR bytes stored during deserialize() for key and filter access
    pub(crate) cdr_bytes: Option<Vec<u8>>,
    /// Compiled layout of the topic type. ContentFilteredTopic / QueryCondition
    /// field access reads the sample through it; absent on a topic created
    /// without type information.
    pub(crate) plans: Option<Arc<TypePlans>>,
}

unsafe impl Send for Int2DdsData {}
unsafe impl Sync for Int2DdsData {}

/// Placeholder TypeSupport for Int2DdsData.
///
/// Returns errors for all operations. The actual TypeSupport used at runtime
/// is `RawTypeSupport`, which is registered with the DomainParticipant
/// via `int2dds_create_topic_raw`.
#[derive(Debug, Clone, Default)]
pub struct Int2DdsDataTypeSupport;

impl FieldAccessor for Int2DdsDataTypeSupport {
    fn get_field_value(&self, _data: &dyn Any, _field_path: &str) -> DdsResult<Parameter> {
        Err(DdsError::Error("Int2DdsDataTypeSupport: Use registered TypeSupport".to_string()))
    }

    fn has_field(&self, _field_path: &str) -> bool {
        false
    }
}

impl TypeSupport for Int2DdsDataTypeSupport {
    fn type_id(&self) -> TypeId {
        TypeId::of::<Int2DdsData>()
    }

    fn get_type_name(&self) -> &str {
        "Int2DdsData"
    }

    fn serialize(
        &self,
        _data: &dyn Any,
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<SerializedData> {
        Err(DdsError::Error("Int2DdsDataTypeSupport: Use registered TypeSupport".to_string()))
    }

    fn deserialize(
        &self,
        _data: &[u8],
        _format: Option<&SerializationFormat>,
    ) -> DdsResult<Box<dyn Any>> {
        Err(DdsError::Error("Int2DdsDataTypeSupport: Use registered TypeSupport".to_string()))
    }

    fn serialize_key(&self, _data: &dyn Any) -> DdsResult<SerializedData> {
        Ok(Arc::from(Vec::new()))
    }

    fn deserialize_key(&self, _serialized_key: &[u8]) -> DdsResult<Box<dyn Any + Send + Sync>> {
        Err(DdsError::Error("Int2DdsDataTypeSupport: Use registered TypeSupport".to_string()))
    }

    fn compute_key(&self, _data: &dyn Any) -> InstanceHandle {
        InstanceHandle::NIL
    }

    fn is_compute_key_provided(&self) -> bool {
        false
    }

    fn get_extensibility_kind(&self) -> ExtensibilityKind {
        ExtensibilityKind::Appendable
    }
}

impl DdsType for Int2DdsData {
    type TypeSupport = Int2DdsDataTypeSupport;
    type FieldAccessor = Int2DdsDataTypeSupport;

    fn has_field(&self, field_path: &str) -> DdsResult<bool> {
        Ok(self.plans.as_ref().is_some_and(|plans| plans.has_field(field_path)))
    }

    fn get_field_value(&self, field_path: &str) -> DdsResult<Parameter> {
        let cdr_bytes = self
            .cdr_bytes
            .as_ref()
            .ok_or_else(|| DdsError::Error("No CDR bytes available".to_string()))?;
        let plans = self
            .plans
            .as_ref()
            .ok_or_else(|| DdsError::Error("No type information available".to_string()))?;

        plans.field_value(cdr_bytes, field_path).ok_or_else(|| {
            DdsError::Error(format!("Type layout does not support reading field '{}'", field_path))
        })?
    }
}
