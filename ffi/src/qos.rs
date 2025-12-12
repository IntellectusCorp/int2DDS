//! # QoS Policy Configuration
//!
//! Functions for creating and configuring Quality of Service policies.
//!
//! ## Supported Policies
//!
//! - **Reliability**: Best-effort or reliable delivery
//! - **Durability**: Volatile, transient-local, transient, or persistent
//! - **History**: Keep-last (with depth) or keep-all
//!
//! ## Usage Pattern
//!
//! ```c
//! Int2DdsDataWriterQos* qos;
//! int2dds_datawriter_qos_create_default(&qos);
//! int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
//! int2dds_datawriter_qos_set_durability(qos, INT2DDS_QOS_DURABILITY_TRANSIENT_LOCAL);
//! // ... use qos when creating DataWriter ...
//! int2dds_datawriter_qos_destroy(qos);
//! ```

use int2dds::{
    core::time::Duration,
    infrastructure::qos_policy::{
        DurabilityQosPolicyKind, HistoryQosPolicyKind, ReliabilityQosPolicyKind,
    },
    publication::qos::DataWriterQos,
    subscription::qos::DataReaderQos,
    topic::qos::TopicQos,
};

use super::error::*;

// QoS kinds (matching DDS spec)
pub const INT2DDS_QOS_RELIABILITY_BEST_EFFORT: i32 = 0;
pub const INT2DDS_QOS_RELIABILITY_RELIABLE: i32 = 1;

pub const INT2DDS_QOS_DURABILITY_VOLATILE: i32 = 0;
pub const INT2DDS_QOS_DURABILITY_TRANSIENT_LOCAL: i32 = 1;
pub const INT2DDS_QOS_DURABILITY_TRANSIENT: i32 = 2;
pub const INT2DDS_QOS_DURABILITY_PERSISTENT: i32 = 3;

pub const INT2DDS_QOS_HISTORY_KEEP_LAST: i32 = 0;
pub const INT2DDS_QOS_HISTORY_KEEP_ALL: i32 = 1;

/// Opaque QoS handle for DataWriter
pub struct Int2DdsDataWriterQos {
    pub(crate) inner: DataWriterQos,
}

/// Opaque QoS handle for DataReader
pub struct Int2DdsDataReaderQos {
    pub(crate) inner: DataReaderQos,
}

/// Opaque QoS handle for Topic
pub struct Int2DdsTopicQos {
    pub(crate) inner: TopicQos,
}

/// Create default DataWriter QoS
///
/// # Safety
/// - `qos_out` must be a valid pointer to a null pointer
/// - The returned QoS must be freed with `int2dds_datawriter_qos_destroy`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_qos_create_default(
    qos_out: *mut *mut Int2DdsDataWriterQos,
) -> Int2DdsRet {
    check_null!(qos_out);

    let qos = Box::new(Int2DdsDataWriterQos { inner: DataWriterQos::default() });

    *qos_out = Box::into_raw(qos);

    INT2DDS_RET_OK
}

/// Set reliability QoS for DataWriter
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid reliability kind
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_qos_set_reliability(
    qos: *mut Int2DdsDataWriterQos,
    kind: i32,
    max_blocking_time_ns: i64,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let reliability_kind = match kind {
        INT2DDS_QOS_RELIABILITY_BEST_EFFORT => ReliabilityQosPolicyKind::BestEffort,
        INT2DDS_QOS_RELIABILITY_RELIABLE => ReliabilityQosPolicyKind::Reliable,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.reliability.kind = reliability_kind;
    qos_ref.inner.reliability.max_blocking_time = Duration {
        sec: (max_blocking_time_ns / 1_000_000_000) as i32,
        nanosec: (max_blocking_time_ns % 1_000_000_000) as u32,
    };

    INT2DDS_RET_OK
}

/// Set durability QoS for DataWriter
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid durability kind
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_qos_set_durability(
    qos: *mut Int2DdsDataWriterQos,
    kind: i32,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let durability_kind = match kind {
        INT2DDS_QOS_DURABILITY_VOLATILE => DurabilityQosPolicyKind::Volatile,
        INT2DDS_QOS_DURABILITY_TRANSIENT_LOCAL => DurabilityQosPolicyKind::TransientLocal,
        INT2DDS_QOS_DURABILITY_TRANSIENT => DurabilityQosPolicyKind::Transient,
        INT2DDS_QOS_DURABILITY_PERSISTENT => DurabilityQosPolicyKind::Persistent,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.durability.kind = durability_kind;

    INT2DDS_RET_OK
}

/// Set history QoS for DataWriter
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid history kind
/// - For KEEP_LAST, `depth` must be > 0
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_qos_set_history(
    qos: *mut Int2DdsDataWriterQos,
    kind: i32,
    depth: i32,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let history_kind = match kind {
        INT2DDS_QOS_HISTORY_KEEP_LAST => HistoryQosPolicyKind::KeepLast(depth),
        INT2DDS_QOS_HISTORY_KEEP_ALL => HistoryQosPolicyKind::KeepAll,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.history.kind = history_kind;

    INT2DDS_RET_OK
}

/// Destroy DataWriter QoS
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `qos` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_qos_destroy(
    qos: *mut Int2DdsDataWriterQos,
) -> Int2DdsRet {
    if qos.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let _qos = Box::from_raw(qos);

    INT2DDS_RET_OK
}

/// Create default DataReader QoS
///
/// # Safety
/// - `qos_out` must be a valid pointer to a null pointer
/// - The returned QoS must be freed with `int2dds_datareader_qos_destroy`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_qos_create_default(
    qos_out: *mut *mut Int2DdsDataReaderQos,
) -> Int2DdsRet {
    check_null!(qos_out);

    let qos = Box::new(Int2DdsDataReaderQos { inner: DataReaderQos::default() });

    *qos_out = Box::into_raw(qos);

    INT2DDS_RET_OK
}

/// Set reliability QoS for DataReader
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid reliability kind
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_qos_set_reliability(
    qos: *mut Int2DdsDataReaderQos,
    kind: i32,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let reliability_kind = match kind {
        INT2DDS_QOS_RELIABILITY_BEST_EFFORT => ReliabilityQosPolicyKind::BestEffort,
        INT2DDS_QOS_RELIABILITY_RELIABLE => ReliabilityQosPolicyKind::Reliable,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.reliability.kind = reliability_kind;

    INT2DDS_RET_OK
}

/// Set durability QoS for DataReader
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid durability kind
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_qos_set_durability(
    qos: *mut Int2DdsDataReaderQos,
    kind: i32,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let durability_kind = match kind {
        INT2DDS_QOS_DURABILITY_VOLATILE => DurabilityQosPolicyKind::Volatile,
        INT2DDS_QOS_DURABILITY_TRANSIENT_LOCAL => DurabilityQosPolicyKind::TransientLocal,
        INT2DDS_QOS_DURABILITY_TRANSIENT => DurabilityQosPolicyKind::Transient,
        INT2DDS_QOS_DURABILITY_PERSISTENT => DurabilityQosPolicyKind::Persistent,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.durability.kind = durability_kind;

    INT2DDS_RET_OK
}

/// Set history QoS for DataReader
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid history kind
/// - For KEEP_LAST, `depth` must be > 0
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_qos_set_history(
    qos: *mut Int2DdsDataReaderQos,
    kind: i32,
    depth: i32,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let history_kind = match kind {
        INT2DDS_QOS_HISTORY_KEEP_LAST => HistoryQosPolicyKind::KeepLast(depth),
        INT2DDS_QOS_HISTORY_KEEP_ALL => HistoryQosPolicyKind::KeepAll,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.history.kind = history_kind;

    INT2DDS_RET_OK
}

/// Destroy DataReader QoS
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `qos` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_qos_destroy(
    qos: *mut Int2DdsDataReaderQos,
) -> Int2DdsRet {
    if qos.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let _qos = Box::from_raw(qos);

    INT2DDS_RET_OK
}

/// Create default Topic QoS
///
/// # Safety
/// - `qos_out` must be a valid pointer to a null pointer
/// - The returned QoS must be freed with `int2dds_topic_qos_destroy`
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_qos_create_default(
    qos_out: *mut *mut Int2DdsTopicQos,
) -> Int2DdsRet {
    check_null!(qos_out);

    let qos = Box::new(Int2DdsTopicQos { inner: TopicQos::default() });

    *qos_out = Box::into_raw(qos);

    INT2DDS_RET_OK
}

/// Destroy Topic QoS
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `qos` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_qos_destroy(qos: *mut Int2DdsTopicQos) -> Int2DdsRet {
    if qos.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let _qos = Box::from_raw(qos);

    INT2DDS_RET_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    #[test]
    fn test_qos_create_destroy() {
        unsafe {
            let mut qos: *mut Int2DdsDataWriterQos = ptr::null_mut();

            // Create default QoS
            let ret = int2dds_datawriter_qos_create_default(&mut qos as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!qos.is_null());

            // Set reliability
            let ret = int2dds_datawriter_qos_set_reliability(
                qos,
                INT2DDS_QOS_RELIABILITY_RELIABLE,
                100_000_000, // 100ms
            );
            assert_eq!(ret, INT2DDS_RET_OK);

            // Set durability
            let ret =
                int2dds_datawriter_qos_set_durability(qos, INT2DDS_QOS_DURABILITY_TRANSIENT_LOCAL);
            assert_eq!(ret, INT2DDS_RET_OK);

            // Set history
            let ret = int2dds_datawriter_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 10);
            assert_eq!(ret, INT2DDS_RET_OK);

            // Destroy QoS
            let ret = int2dds_datawriter_qos_destroy(qos);
            assert_eq!(ret, INT2DDS_RET_OK);
        }
    }
}
