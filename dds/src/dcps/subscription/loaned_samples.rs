//! Samples loaned from a DataReader's cache (DDS v1.4 2.2.2.5.3.8 rule 3, 2.2.2.5.3.20).

use std::{
    any::Any,
    fmt,
    marker::PhantomData,
    ops::Deref,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use crate::{
    core::error::{DdsError, DdsResult},
    rtps::{common::guid::Guid, entities::history::cache_change::CacheChange},
    subscription::sample_info::SampleInfo,
    topic::type_support::{DdsType, TypeSupport},
};

/// Decodes `change` on first use and returns the reader's cached value afterwards.
pub(crate) fn decode_once<'a, Foo: DdsType>(
    change: &'a CacheChange,
    type_support: &dyn TypeSupport,
) -> DdsResult<&'a Foo> {
    let cell = change.decoded();
    let value = match cell.get() {
        Some(value) => value,
        None => {
            let decoded: Box<Foo> = type_support
                .deserialize(change.data_value(), None)?
                .downcast::<Foo>()
                .map_err(|_| DdsError::Error("Type downcast failed".to_string()))?;
            cell.get_or_set(decoded as Box<dyn Any + Send + Sync>)
        }
    };
    value.downcast_ref::<Foo>().ok_or_else(|| DdsError::Error("Type downcast failed".to_string()))
}

/// Counts one outstanding loan on its reader for as long as it lives.
pub(crate) struct LoanToken {
    reader: Guid,
    outstanding: Arc<AtomicUsize>,
}

impl LoanToken {
    pub(crate) fn new(reader: Guid, outstanding: Arc<AtomicUsize>) -> Self {
        outstanding.fetch_add(1, Ordering::AcqRel);
        Self { reader, outstanding }
    }
}

impl Drop for LoanToken {
    fn drop(&mut self) {
        self.outstanding.fetch_sub(1, Ordering::AcqRel);
    }
}

/// One loaned element: a sample in the reader's cache and its `SampleInfo`.
pub struct LoanedSample<Foo> {
    change: Option<Arc<CacheChange>>,
    type_support: Arc<dyn TypeSupport>,
    info: SampleInfo,
    _foo: PhantomData<fn() -> Foo>,
}

impl<Foo> LoanedSample<Foo> {
    pub(crate) fn new(
        change: Option<Arc<CacheChange>>,
        info: SampleInfo,
        type_support: Arc<dyn TypeSupport>,
    ) -> Self {
        Self { change, type_support, info, _foo: PhantomData }
    }

    pub fn sample_info(&self) -> &SampleInfo {
        &self.info
    }

    /// The sample's serialized bytes, or `None` when the element carries no data.
    pub fn serialized_data(&self) -> Option<&[u8]> {
        match &self.change {
            Some(change) if self.info.valid_data => Some(change.data_value()),
            _ => None,
        }
    }
}

impl<Foo: DdsType> LoanedSample<Foo> {
    /// Borrows the sample from the reader's cache, decoding it on first access.
    pub fn data(&self) -> DdsResult<&Foo> {
        match &self.change {
            Some(change) if self.info.valid_data => decode_once(change, self.type_support.as_ref()),
            _ => Err(DdsError::NoData),
        }
    }
}

impl<Foo> fmt::Debug for LoanedSample<Foo> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoanedSample").field("info", &self.info).finish()
    }
}

/// Samples loaned by a read or take; dropping the value returns the loan.
pub struct LoanedSamples<Foo> {
    samples: Vec<LoanedSample<Foo>>,
    token: LoanToken,
}

impl<Foo> LoanedSamples<Foo> {
    pub(crate) fn new(samples: Vec<LoanedSample<Foo>>, token: LoanToken) -> Self {
        Self { samples, token }
    }

    pub(crate) fn reader(&self) -> Guid {
        self.token.reader
    }
}

impl<Foo> Deref for LoanedSamples<Foo> {
    type Target = [LoanedSample<Foo>];

    fn deref(&self) -> &Self::Target {
        &self.samples
    }
}

impl<'a, Foo> IntoIterator for &'a LoanedSamples<Foo> {
    type Item = &'a LoanedSample<Foo>;
    type IntoIter = std::slice::Iter<'a, LoanedSample<Foo>>;

    fn into_iter(self) -> Self::IntoIter {
        self.samples.iter()
    }
}

impl<Foo> fmt::Debug for LoanedSamples<Foo> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.samples).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::instance_handle::InstanceHandle,
        core::time::Time,
        dcps::topic::type_support::DdsType,
        subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    };
    use bytes::Bytes;

    #[derive(DdsType)]
    struct Shape {
        #[dds(key)]
        id: u32,
        size: u32,
    }

    fn info(valid_data: bool) -> SampleInfo {
        SampleInfo {
            source_timestamp: Time::default(),
            sample_state: SampleStateKind::NOT_READ_SAMPLE_STATE,
            view_state: ViewStateKind::NEW_VIEW_STATE,
            instance_handle: InstanceHandle::NIL,
            instance_state: InstanceStateKind::ALIVE_INSTANCE_STATE,
            disposed_generation_count: 0,
            no_writers_generation_count: 0,
            absolute_generation_rank: 0,
            sample_rank: 0,
            generation_rank: 0,
            publication_handle: InstanceHandle::NIL,
            valid_data,
        }
    }

    fn change_with(payload: &[u8]) -> Arc<CacheChange> {
        let mut change = CacheChange::empty();
        change.set_shared_payload(Bytes::copy_from_slice(payload));
        Arc::new(change)
    }

    fn loaned(change: Option<Arc<CacheChange>>, valid_data: bool) -> LoanedSample<Shape> {
        LoanedSample::new(change, info(valid_data), Shape::get_type_support())
    }

    fn token(count: &Arc<AtomicUsize>) -> LoanToken {
        LoanToken::new(Guid::UNKNOWN, Arc::clone(count))
    }

    #[test]
    fn concurrent_data_calls_share_one_value() {
        let change = change_with(&Shape { id: 1, size: 2 }.serialize().unwrap());
        let sample = loaned(Some(change), true);
        let (x, y) = std::thread::scope(|s| {
            let x = s.spawn(|| sample.data().unwrap() as *const Shape as usize);
            let y = s.spawn(|| sample.data().unwrap() as *const Shape as usize);
            (x.join().unwrap(), y.join().unwrap())
        });
        assert_eq!(x, y);
    }

    #[test]
    fn notification_element_has_no_data() {
        let notification = loaned(None, false);
        assert!(matches!(notification.data(), Err(DdsError::NoData)));
        assert!(notification.serialized_data().is_none());
    }

    #[test]
    fn undecodable_payload_is_an_error_and_leaves_the_cell_empty() {
        let change = change_with(&[0xff]);
        let sample = loaned(Some(Arc::clone(&change)), true);
        assert!(sample.data().is_err());
        assert!(sample.data().is_err());
        assert!(change.decoded().get().is_none());
    }

    #[test]
    fn serialized_data_lends_the_payload() {
        let bytes = Shape { id: 5, size: 6 }.serialize().unwrap();
        let change = change_with(&bytes);
        let sample = loaned(Some(Arc::clone(&change)), true);
        let lent = sample.serialized_data().unwrap();
        assert_eq!(lent, &bytes[..]);
        assert!(std::ptr::eq(lent, change.data_value()));
    }

    #[test]
    fn token_counts_the_loan_until_dropped_on_any_thread() {
        let count = Arc::new(AtomicUsize::new(0));
        let loan = LoanedSamples::<Shape>::new(vec![loaned(None, false)], token(&count));
        assert_eq!(count.load(Ordering::Acquire), 1);
        assert_eq!(loan.len(), 1);
        std::thread::spawn(move || drop(loan)).join().unwrap();
        assert_eq!(count.load(Ordering::Acquire), 0);
    }

    #[test]
    fn loaned_samples_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LoanedSamples<Shape>>();
        assert_send_sync::<LoanedSample<Shape>>();
    }
}
