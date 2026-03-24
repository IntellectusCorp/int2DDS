//! RPC sample types.
//!
//! Wraps DDS data samples with RPC-specific metadata (e.g. request identity).

use int2dds::dcps::subscription::data_sample::DataSample;

/// Immutable value type pairing received data with SampleInfo.
pub type Sample<T> = DataSample<T>;
