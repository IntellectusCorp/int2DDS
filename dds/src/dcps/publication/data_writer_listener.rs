//! DataWriterListener - Callback interface for DataWriter status events.
//!
//! The `DataWriterListener` trait allows applications to receive asynchronous notifications
//! about status changes related to a `DataWriter`. Listeners are registered when creating
//! a DataWriter or by calling `DataWriter::set_listener()`.
//!
//! # Available Callbacks
//!
//! - `on_offered_deadline_missed`: Called when the writer fails to write data within the deadline period
//! - `on_offered_incompatible_qos`: Called when a matching reader has incompatible QoS
//! - `on_liveliness_lost`: Called when the writer fails to assert its liveliness
//! - `on_publication_matched`: Called when a matching subscription is discovered or lost

use crate::infrastructure::status::{
    LivelinessLostStatus, OfferedDeadlineMissedStatus, OfferedIncompatibleQosStatus,
    PublicationMatchedStatus,
};

use super::data_writer::DataWriter;

pub trait DataWriterListener: 'static + Send + Sync {
    type Foo;
    fn on_offered_deadline_missed(
        &self,
        _writer: &DataWriter<Self::Foo>,
        _status: &OfferedDeadlineMissedStatus,
    ) {
    }
    fn on_offered_incompatible_qos(
        &self,
        _writer: &DataWriter<Self::Foo>,
        _status: &OfferedIncompatibleQosStatus,
    ) {
    }
    fn on_liveliness_lost(&self, _writer: &DataWriter<Self::Foo>, _status: &LivelinessLostStatus) {}
    fn on_publication_matched(
        &self,
        _writer: &DataWriter<Self::Foo>,
        _status: &PublicationMatchedStatus,
    ) {
    }
}
