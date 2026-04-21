//! DataSample - Container for received data samples with metadata.
//!
//! A `DataSample<Foo>` represents a single data sample received from a `DataReader`,
//! combining the actual data with associated metadata (`SampleInfo`). DataSamples are
//! returned by `DataReader::read()` and `DataReader::take()` operations.
//!
//! The sample may contain valid data or may be a metadata-only sample (when the instance
//! state is DISPOSED or NO_WRITERS). Use `data()` to access the deserialized data value.

use std::{fmt::Debug, marker::PhantomData, sync::Arc};

use crate::{
    core::error::{DdsError, DdsResult},
    rtps::entities::history::cache_change::CacheChange,
    topic::type_support::{DdsType, TypeSupport},
};

use super::sample_info::SampleInfo;

/// A data sample received from a DataReader.
///
/// Holds the original `Arc<CacheChange>` from the reader history so the serialized
/// payload is shared without copying.  Deserialization happens lazily when the
/// user calls `data()`.
pub struct DataSample<Foo> {
    change: Option<Arc<CacheChange>>,
    type_support: Option<Arc<dyn TypeSupport>>,
    pub(crate) sample_info: SampleInfo,
    phantom: PhantomData<fn() -> Foo>,
}

impl<Foo> DataSample<Foo> {
    pub(crate) fn new(
        change: Option<Arc<CacheChange>>,
        sample_info: SampleInfo,
        type_support: Option<Arc<dyn TypeSupport>>,
    ) -> Self {
        Self { change, type_support, sample_info, phantom: PhantomData }
    }
}

impl<Foo> DataSample<Foo>
where
    Foo: DdsType,
{
    /// Deserialize and return the data value.
    ///
    /// If a type support was provided during construction, it will be used for
    /// deserialization. Otherwise, falls back to the default TypeSupport.
    pub fn data(&self) -> DdsResult<Foo> {
        match (self.change.as_ref(), self.type_support.as_ref()) {
            (Some(change), Some(ts)) => {
                let any_box = ts.deserialize(change.data_value(), None)?;
                any_box
                    .downcast::<Foo>()
                    .map(|boxed| *boxed)
                    .map_err(|_| DdsError::Error("Type downcast failed".to_string()))
            }
            (Some(change), None) => Ok(Foo::deserialize(change.data_value())?),
            (None, _) => Err(DdsError::NoData),
        }
    }

    pub fn sample_info(&self) -> SampleInfo {
        self.sample_info.clone()
    }
}

// Manual implementations for traits that can't be derived due to Arc<dyn TypeSupport>

impl<Foo> Clone for DataSample<Foo> {
    fn clone(&self) -> Self {
        Self {
            change: self.change.clone(),
            type_support: self.type_support.clone(),
            sample_info: self.sample_info.clone(),
            phantom: PhantomData,
        }
    }
}

impl<Foo> Debug for DataSample<Foo> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataSample")
            .field(
                "data",
                &self.change.as_ref().map(|c| format!("[{} bytes]", c.data_value().len())),
            )
            .field("has_type_support", &self.type_support.is_some())
            .field("sample_info", &self.sample_info)
            .finish()
    }
}

impl<Foo> PartialEq for DataSample<Foo> {
    fn eq(&self, other: &Self) -> bool {
        let data_eq = match (self.change.as_ref(), other.change.as_ref()) {
            (Some(a), Some(b)) => a.data_value() == b.data_value(),
            (None, None) => true,
            _ => false,
        };
        data_eq && self.sample_info == other.sample_info
    }
}

impl<Foo> Eq for DataSample<Foo> {}
