//! DataReaderListener - Callback interface for DataReader status events.
//!
//! The `DataReaderListener` trait allows applications to receive asynchronous notifications
//! about status changes and data availability related to a `DataReader`. Listeners are
//! registered when creating a DataReader or by calling `DataReader::set_listener()`.
//!
//! # Available Callbacks
//!
//! - `on_data_available`: Called when new data samples are available to read
//! - `on_sample_rejected`: Called when a sample is rejected (due to resource limits, etc.)
//! - `on_liveliness_changed`: Called when a matched writer's liveliness changes
//! - `on_requested_deadline_missed`: Called when data is not received within the deadline
//! - `on_requested_incompatible_qos`: Called when a matching writer's QoS is incompatible
//! - `on_sample_lost`: Called when samples are lost and cannot be recovered (reliability protocol)
//! - `on_subscription_matched`: Called when a matching publication is discovered or lost

use crate::infrastructure::status::{
    LivelinessChangedStatus, RequestedDeadlineMissedStatus, RequestedIncompatibleQosStatus,
    SampleLostStatus, SampleRejectedStatus, SubscriptionMatchedStatus,
};

use super::data_reader::DataReader;

pub trait DataReaderListener: 'static + Send + Sync {
    type Foo;

    fn on_sample_rejected(&self, _reader: &DataReader<Self::Foo>, _status: &SampleRejectedStatus) {}
    fn on_liveliness_changed(
        &self,
        _reader: &DataReader<Self::Foo>,
        _status: &LivelinessChangedStatus,
    ) {
    }
    fn on_requested_deadline_missed(
        &self,
        _reader: &DataReader<Self::Foo>,
        _status: &RequestedDeadlineMissedStatus,
    ) {
    }
    fn on_requested_incompatible_qos(
        &self,
        _reader: &DataReader<Self::Foo>,
        _status: &RequestedIncompatibleQosStatus,
    ) {
    }
    fn on_data_available(&self, _reader: &DataReader<Self::Foo>) {}
    fn on_subscription_matched(
        &self,
        _reader: &DataReader<Self::Foo>,
        _status: &SubscriptionMatchedStatus,
    ) {
    }
    fn on_sample_lost(&self, _reader: &DataReader<Self::Foo>, _status: &SampleLostStatus) {}
}
