//! DataSample - Container for received data samples with metadata.
//!
//! A `DataSample<Foo>` represents a single data sample received from a `DataReader`,
//! combining the actual data with associated metadata (`SampleInfo`). DataSamples are
//! returned by `DataReader::read()` and `DataReader::take()` operations.
//!
//! The sample may contain valid data or may be a metadata-only sample (when the instance
//! state is DISPOSED or NO_WRITERS). Use `data()` to access the deserialized data value.

use std::{marker::PhantomData, sync::Arc};

use crate::{
    core::error::{DdsError, DdsResult},
    topic::type_support::DdsType,
};

use super::sample_info::SampleInfo;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DataSample<Foo> {
    data: Option<Arc<[u8]>>,
    pub(crate) sample_info: SampleInfo,
    phantom: PhantomData<fn() -> Foo>,
}

impl<Foo> DataSample<Foo> {
    pub(crate) fn new(data: Option<Arc<[u8]>>, sample_info: SampleInfo) -> Self {
        Self { data, sample_info, phantom: PhantomData }
    }

    /// Get raw bytes directly without deserialization (zero-copy).
    /// Returns a slice reference to the underlying data, bypassing all
    /// serialization overhead. Use this for FFI where raw bytes are needed.
    #[inline]
    pub fn raw_bytes(&self) -> Option<&[u8]> {
        self.data.as_deref()
    }
}

impl<Foo> DataSample<Foo>
where
    Foo: DdsType,
{
    pub fn data(&self) -> DdsResult<Foo> {
        match self.data.as_ref() {
            Some(data) => Ok(Foo::deserialize(data.as_ref())?),
            None => Err(DdsError::NoData),
        }
    }

    pub fn sample_info(&self) -> SampleInfo {
        self.sample_info.clone()
    }
}
