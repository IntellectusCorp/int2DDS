//! DataSample - Container for received data samples with metadata.
//!
//! A `DataSample<Foo>` represents a single data sample received from a `DataReader`,
//! combining the actual data with associated metadata (`SampleInfo`). DataSamples are
//! returned by `DataReader::read()` and `DataReader::take()` operations.
//!
//! The sample may contain valid data or may be a metadata-only sample (when the instance
//! state is DISPOSED or NO_WRITERS). Use `data()` to access the deserialized data value.

use std::{
    fmt::Debug,
    marker::PhantomData,
    sync::{Arc, OnceLock},
};

use bytes::Bytes;
use smallvec::SmallVec;

use crate::{
    core::error::{DdsError, DdsResult},
    topic::type_support::{DdsType, TypeSupport},
};

use super::sample_info::SampleInfo;

// Raw serialized payload of a received sample. Contiguous is the common path;
// Chained holds fragment chunks (scatter-gather) so `data()` deserializes across
// them without a contiguous reassembly. `cached` is shared with the source
// CacheChange and other readers, so any contiguous fallback runs once per sample.
pub(crate) enum SamplePayload {
    Contiguous(Bytes),
    Chained { chunks: SmallVec<[Bytes; 16]>, cached: Arc<OnceLock<Bytes>> },
}

impl SamplePayload {
    fn len(&self) -> usize {
        match self {
            SamplePayload::Contiguous(b) => b.len(),
            SamplePayload::Chained { chunks, .. } => chunks.iter().map(|c| c.len()).sum(),
        }
    }

    // Contiguous view, materializing chained chunks once into the shared cache.
    fn materialized(&self) -> Bytes {
        match self {
            SamplePayload::Contiguous(b) => b.clone(),
            SamplePayload::Chained { chunks, cached } => cached
                .get_or_init(|| {
                    let total: usize = chunks.iter().map(|c| c.len()).sum();
                    let mut buf = Vec::with_capacity(total);
                    for c in chunks {
                        buf.extend_from_slice(c);
                    }
                    Bytes::from(buf)
                })
                .clone(),
        }
    }
}

impl Clone for SamplePayload {
    fn clone(&self) -> Self {
        match self {
            SamplePayload::Contiguous(b) => SamplePayload::Contiguous(b.clone()),
            SamplePayload::Chained { chunks, cached } => {
                SamplePayload::Chained { chunks: chunks.clone(), cached: cached.clone() }
            }
        }
    }
}

pub struct DataSample<Foo> {
    data: Option<SamplePayload>, // Raw serialized data as received from the RTPS layer
    type_support: Option<Arc<dyn TypeSupport>>,
    pub(crate) sample_info: SampleInfo,
    phantom: PhantomData<fn() -> Foo>,
}

impl<Foo> DataSample<Foo> {
    pub(crate) fn new(
        data: Option<SamplePayload>,
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
            (Some(payload), Some(ts)) => {
                let any_box = match payload {
                    SamplePayload::Contiguous(bytes) => ts.deserialize(bytes.as_ref(), None)?,
                    // Reuse an already-materialized buffer if some other path made
                    // one; otherwise deserialize directly across the chunks.
                    SamplePayload::Chained { chunks, cached } => match cached.get() {
                        Some(b) => ts.deserialize(b.as_ref(), None)?,
                        None => ts.deserialize_chained(chunks, None)?,
                    },
                };
                any_box
                    .downcast::<Foo>()
                    .map(|boxed| *boxed)
                    .map_err(|_| DdsError::Error("Type downcast failed".to_string()))
            }
            // Fall back to the static DdsType impl when no TypeSupport is attached.
            // Contiguous borrows directly (no clone); only chained materializes.
            (Some(SamplePayload::Contiguous(bytes)), None) => Ok(Foo::deserialize(bytes.as_ref())?),
            (Some(payload @ SamplePayload::Chained { .. }), None) => {
                Ok(Foo::deserialize(payload.materialized().as_ref())?)
            }
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
            // Contiguous compares bytes directly (no clone); materialize only if chained.
            (Some(SamplePayload::Contiguous(a)), Some(SamplePayload::Contiguous(b))) => a == b,
            (Some(a), Some(b)) => a.materialized() == b.materialized(),
            (None, None) => true,
            _ => false,
        };
        data_eq && self.sample_info == other.sample_info
    }
}

impl<Foo> Eq for DataSample<Foo> {}
