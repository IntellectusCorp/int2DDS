//! PublisherListener - Callback interface for Publisher status events.
//!
//! The `PublisherListener` trait allows applications to receive asynchronous notifications
//! about status changes related to a `Publisher` and its child DataWriters. Listeners are
//! registered when creating a Publisher or by calling `Publisher::set_listener()`.
//!
//! The publisher listener receives the same callbacks as DataWriterListener for all
//! DataWriters created by this publisher. A DataWriter status is delivered to the writer's,
//! the publisher's, and the participant's listener, each gated by the writer's status mask.

use crate::infrastructure::status::{
    LivelinessLostStatus, OfferedDeadlineMissedStatus, OfferedIncompatibleQosStatus,
    PublicationMatchedStatus,
};

use super::{data_writer::DataWriterBase, qos::DataWriterQos};

pub trait PublisherListener: 'static + Send + Sync {
    fn on_offered_deadline_missed(
        &self,
        _writer: &dyn DataWriterBase<Qos = DataWriterQos>,
        _status: &OfferedDeadlineMissedStatus,
    ) {
    }
    fn on_offered_incompatible_qos(
        &self,
        _writer: &dyn DataWriterBase<Qos = DataWriterQos>,
        _status: &OfferedIncompatibleQosStatus,
    ) {
    }
    fn on_liveliness_lost(
        &self,
        _writer: &dyn DataWriterBase<Qos = DataWriterQos>,
        _status: &LivelinessLostStatus,
    ) {
    }
    fn on_publication_matched(
        &self,
        _writer: &dyn DataWriterBase<Qos = DataWriterQos>,
        _status: &PublicationMatchedStatus,
    ) {
    }
}
