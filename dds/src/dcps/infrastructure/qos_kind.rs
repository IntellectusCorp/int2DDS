//! Sentinel-aware QoS argument wrapper.
//!
//! `QosKind<T>` is the type used by `create_*` and `set_default_*_qos` entry
//! points to distinguish "use the default" (which is then resolved through
//! the registered-default → profile → spec-default chain) from "use exactly
//! this QoS value". This avoids the prior value-equality based detection,
//! which could not tell `T::default()` apart from a deliberately constructed
//! QoS that happened to equal the spec default.
//!
//! Callers usually do not name this type directly:
//! - Pass `PUBLISHER_QOS_DEFAULT` (and siblings) for the default sentinel.
//! - Pass a concrete `PublisherQos` value (or any other QoS struct); the
//!   blanket `From<T>` impl wraps it as `QosKind::Specific(...)`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QosKind<T> {
    /// Resolve the QoS through the default chain (registered default →
    /// configured default profile → OMG DDS spec default).
    Default,
    /// Use the supplied QoS value verbatim.
    Specific(T),
}

impl<T> From<T> for QosKind<T> {
    fn from(qos: T) -> Self {
        QosKind::Specific(qos)
    }
}
