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
    topic::type_support::{DdsType, TypeSupport},
};

use super::sample_info::SampleInfo;

/// A data sample received from a DataReader.
///
/// Contains the serialized data bytes, optional type support for deserialization,
/// and associated sample metadata (SampleInfo).
pub struct DataSample<Foo> {
    data: Option<Arc<[u8]>>,
    type_support: Option<Arc<dyn TypeSupport>>,
    pub(crate) sample_info: SampleInfo,
    phantom: PhantomData<fn() -> Foo>,
}

impl<Foo> DataSample<Foo> {
    /// Create a new DataSample without type support (uses default TypeSupport for deserialization)
    pub(crate) fn new(
        data: Option<Arc<[u8]>>,
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
    /// Deserialize and return the data value.
    ///
    /// If a type support was provided during construction, it will be used for
    /// deserialization. Otherwise, falls back to the default TypeSupport.
    pub fn data(&self) -> DdsResult<Foo> {
        match (self.data.as_ref(), self.type_support.as_ref()) {
            (Some(data), Some(ts)) => {
                // Use the registered TypeSupport for deserialization
                let any_box = ts.deserialize(data.as_ref(), None)?;
                any_box
                    .downcast::<Foo>()
                    .map(|boxed| *boxed)
                    .map_err(|_| DdsError::Error("Type downcast failed".to_string()))
            }
            (Some(data), None) => {
                // Fallback: use default TypeSupport (backward compatibility)
                Ok(Foo::deserialize(data.as_ref())?)
            }
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
            .field("data", &self.data.as_ref().map(|d| format!("[{} bytes]", d.len())))
            .field("has_type_support", &self.type_support.is_some())
            .field("sample_info", &self.sample_info)
            .finish()
    }
}

impl<Foo> PartialEq for DataSample<Foo> {
    fn eq(&self, other: &Self) -> bool {
        // Compare data and sample_info, ignore type_support (it's just a helper)
        self.data == other.data && self.sample_info == other.sample_info
    }
}

impl<Foo> Eq for DataSample<Foo> {}
