//! SubscriberListener - Callback interface for Subscriber status events.
//!
//! The `SubscriberListener` trait allows applications to receive asynchronous notifications
//! about status changes related to a `Subscriber` and its child DataReaders. Listeners are
//! registered when creating a Subscriber or by calling `Subscriber::set_listener()`.
//!
//! The subscriber listener receives the same callbacks as DataReaderListener, allowing it
//! to handle events from all DataReaders created by this subscriber when they don't have
//! their own listeners registered. It also provides the `on_data_on_readers` callback for
//! coordinated reading across multiple readers.

use crate::infrastructure::status::{
    LivelinessChangedStatus, RequestedDeadlineMissedStatus, RequestedIncompatibleQosStatus,
    SampleLostStatus, SampleRejectedStatus, SubscriptionMatchedStatus,
};

use super::{data_reader::DataReaderBase, qos::DataReaderQos, subscriber::Subscriber};

pub trait SubscriberListener: 'static + Send + Sync {
    fn on_sample_rejected(
        &self,
        _reader: &dyn DataReaderBase<Qos = DataReaderQos>,
        _status: &SampleRejectedStatus,
    ) {
    }
    fn on_liveliness_changed(
        &self,
        _reader: &dyn DataReaderBase<Qos = DataReaderQos>,
        _status: &LivelinessChangedStatus,
    ) {
    }
    fn on_requested_deadline_missed(
        &self,
        _reader: &dyn DataReaderBase<Qos = DataReaderQos>,
        _status: &RequestedDeadlineMissedStatus,
    ) {
    }
    fn on_requested_incompatible_qos(
        &self,
        _reader: &dyn DataReaderBase<Qos = DataReaderQos>,
        _status: &RequestedIncompatibleQosStatus,
    ) {
    }
    fn on_data_available(&self, _reader: &dyn DataReaderBase<Qos = DataReaderQos>) {}
    fn on_subscription_matched(
        &self,
        _reader: &dyn DataReaderBase<Qos = DataReaderQos>,
        _status: &SubscriptionMatchedStatus,
    ) {
    }
    fn on_sample_lost(
        &self,
        _reader: &dyn DataReaderBase<Qos = DataReaderQos>,
        _status: &SampleLostStatus,
    ) {
    }
    fn on_data_on_readers(&self, _subscriber: &Subscriber) {}
}
