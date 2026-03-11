//! RPC sample types.
//!
//! C++ spec defines LoanedSamples, SharedSamples, SampleRef, WriteSampleRef,
//! and SampleIterator for C++ memory management. These are unnecessary in Rust:
//! `Vec<Sample<T>>` replaces LoanedSamples/SharedSamples, `&T` replaces
//! SampleRef/WriteSampleRef, and `IntoIterator` replaces SampleIterator.
//! The Java binding omits them as well.

use int2dds::dcps::subscription::data_sample::DataSample;

use crate::types::SampleIdentity;

/// Immutable value type pairing received data with SampleInfo.
pub type Sample<T> = DataSample<T>;

/// Value type pairing outgoing data with a SampleIdentity.
/// After `send_request()`, the middleware populates the identity so the
/// caller can use it to match the corresponding reply.
pub struct WriteSample<T> {
    data: T,
    identity: Option<SampleIdentity>,
}

impl<T> WriteSample<T> {
    pub fn new(data: T) -> Self {
        Self { data, identity: None }
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut T {
        &mut self.data
    }

    pub fn set_data(&mut self, data: T) {
        self.data = data;
    }

    /// Returns the identity assigned by the middleware after sending.
    /// `None` if the sample has not been sent yet.
    pub fn identity(&self) -> Option<&SampleIdentity> {
        self.identity.as_ref()
    }

    /// Called by the middleware to fill in the identity after sending.
    pub fn set_identity(&mut self, id: SampleIdentity) {
        self.identity = Some(id);
    }
}
