//! DataSample - Container for received data samples with metadata.
//!
//! A `DataSample<Foo>` represents a single data sample received from a `DataReader`,
//! combining the actual data with associated metadata (`SampleInfo`). DataSamples are
//! returned by `DataReader::read()` and `DataReader::take()` operations.
//!
//! The sample may contain valid data or may be a metadata-only sample (when the instance
//! state is DISPOSED or NO_WRITERS). Use `data()` to access the deserialized data value.

use std::{fmt::Debug, marker::PhantomData, sync::Arc};

use bytes::Bytes;

use crate::{
    core::error::{DdsError, DdsResult},
    topic::type_support::{DdsType, TypeSupport},
};

use super::sample_info::SampleInfo;

pub struct DataSample<Foo> {
    data: Option<Bytes>, // Raw serialized data as received from the RTPS layer
    type_support: Option<Arc<dyn TypeSupport>>,
    pub(crate) sample_info: SampleInfo,
    phantom: PhantomData<fn() -> Foo>,
}

impl<Foo> DataSample<Foo> {
    pub(crate) fn new(
        data: Option<Bytes>,
        sample_info: SampleInfo,
        type_support: Option<Arc<dyn TypeSupport>>,
    ) -> Self {
        Self { data, type_support, sample_info, phantom: PhantomData }
    }
}

impl<Foo> DataSample<Foo>
where
    Foo: DdsType,
{
    pub fn data(&self) -> DdsResult<Foo> {
        match (self.data.as_ref(), self.type_support.as_ref()) {
            // Use the user-provided TypeSupport when available, then downcast.
            (Some(bytes), Some(ts)) => ts
                .deserialize(bytes.as_ref(), None)?
                .downcast::<Foo>()
                .map(|boxed| *boxed)
                .map_err(|_| DdsError::Error("Type downcast failed".to_string())),
            // Fall back to the static DdsType impl when no TypeSupport is attached.
            (Some(bytes), None) => Ok(Foo::deserialize(bytes.as_ref())?),
            (None, _) => Err(DdsError::NoData),
        }
    }

    pub fn sample_info(&self) -> SampleInfo {
        self.sample_info.clone()
    }
}

// Manual impls: Arc<dyn TypeSupport> blocks `derive`.

impl<Foo> Clone for DataSample<Foo> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            type_support: self.type_support.clone(),
            sample_info: self.sample_info.clone(),
            phantom: PhantomData,
        }
    }
}

impl<Foo> Debug for DataSample<Foo> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataSample")
            .field("data", &self.data.as_ref().map(|p| format!("[{} bytes]", p.len())))
            .field("has_type_support", &self.type_support.is_some())
            .field("sample_info", &self.sample_info)
            .finish()
    }
}

impl<Foo> PartialEq for DataSample<Foo> {
    fn eq(&self, other: &Self) -> bool {
        let data_eq = match (self.data.as_ref(), other.data.as_ref()) {
            (Some(a), Some(b)) => a == b,
            (None, None) => true,
            _ => false,
        };
        data_eq && self.sample_info == other.sample_info
    }
}

impl<Foo> Eq for DataSample<Foo> {}
