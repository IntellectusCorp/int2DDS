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

use std::ffi::CStr;
use std::os::raw::c_char;

use int2dds::{
    core::time::Duration,
    infrastructure::qos_policy::{
        DataRepresentationId, DataRepresentationQosPolicy, DurabilityQosPolicyKind,
        HistoryQosPolicyKind, LivelinessQosPolicyKind, ReliabilityQosPolicyKind,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::qos::{DataReaderQos, SubscriberQos},
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

pub const INT2DDS_QOS_DATA_REPR_XCDR1: i32 = 0;
pub const INT2DDS_QOS_DATA_REPR_XCDR2: i32 = 2;

// Liveliness kinds
pub const INT2DDS_QOS_LIVELINESS_AUTOMATIC: i32 = 0;
pub const INT2DDS_QOS_LIVELINESS_MANUAL_BY_PARTICIPANT: i32 = 1;
pub const INT2DDS_QOS_LIVELINESS_MANUAL_BY_TOPIC: i32 = 2;

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

/// Set data representation QoS for DataWriter
///
/// Controls which encoding is advertised in DDS discovery.
/// - `INT2DDS_QOS_DATA_REPR_XCDR1` (0): XCDR1 — for FINAL extensibility
/// - `INT2DDS_QOS_DATA_REPR_XCDR2` (2): XCDR2 — for APPENDABLE/MUTABLE extensibility
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid data representation kind
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_qos_set_data_representation(
    qos: *mut Int2DdsDataWriterQos,
    kind: i32,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let repr_id = match kind {
        INT2DDS_QOS_DATA_REPR_XCDR1 => DataRepresentationId::XcdrDataRepresentation,
        INT2DDS_QOS_DATA_REPR_XCDR2 => DataRepresentationId::Xcdr2DataRepresentation,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.data_representation = DataRepresentationQosPolicy { value: vec![repr_id] };

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

/// Set data representation QoS for DataReader
///
/// Controls which encoding is advertised in DDS discovery.
/// - `INT2DDS_QOS_DATA_REPR_XCDR1` (0): XCDR1 — for FINAL extensibility
/// - `INT2DDS_QOS_DATA_REPR_XCDR2` (2): XCDR2 — for APPENDABLE/MUTABLE extensibility
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid data representation kind
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_qos_set_data_representation(
    qos: *mut Int2DdsDataReaderQos,
    kind: i32,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let repr_id = match kind {
        INT2DDS_QOS_DATA_REPR_XCDR1 => DataRepresentationId::XcdrDataRepresentation,
        INT2DDS_QOS_DATA_REPR_XCDR2 => DataRepresentationId::Xcdr2DataRepresentation,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.data_representation = DataRepresentationQosPolicy { value: vec![repr_id] };

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

// ============================================================================
// Deadline QoS (Feature 4)
// ============================================================================

/// Set deadline QoS for DataWriter
///
/// # Safety
/// - `qos` must be a valid QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_qos_set_deadline(
    qos: *mut Int2DdsDataWriterQos,
    period_ns: i64,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;
    qos_ref.inner.deadline.period = Duration {
        sec: (period_ns / 1_000_000_000) as i32,
        nanosec: (period_ns % 1_000_000_000) as u32,
    };

    INT2DDS_RET_OK
}

/// Set deadline QoS for DataReader
///
/// # Safety
/// - `qos` must be a valid QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_qos_set_deadline(
    qos: *mut Int2DdsDataReaderQos,
    period_ns: i64,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;
    qos_ref.inner.deadline.period = Duration {
        sec: (period_ns / 1_000_000_000) as i32,
        nanosec: (period_ns % 1_000_000_000) as u32,
    };

    INT2DDS_RET_OK
}

// ============================================================================
// Liveliness QoS (Feature 5)
// ============================================================================

/// Set liveliness QoS for DataWriter
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid liveliness kind (0=Automatic, 1=ManualByParticipant, 2=ManualByTopic)
#[no_mangle]
pub unsafe extern "C" fn int2dds_datawriter_qos_set_liveliness(
    qos: *mut Int2DdsDataWriterQos,
    kind: i32,
    lease_duration_ns: i64,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let liveliness_kind = match kind {
        INT2DDS_QOS_LIVELINESS_AUTOMATIC => LivelinessQosPolicyKind::Automatic,
        INT2DDS_QOS_LIVELINESS_MANUAL_BY_PARTICIPANT => {
            LivelinessQosPolicyKind::ManualByParticipant
        }
        INT2DDS_QOS_LIVELINESS_MANUAL_BY_TOPIC => LivelinessQosPolicyKind::ManualByTopic,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.liveliness.kind = liveliness_kind;
    qos_ref.inner.liveliness.lease_duration = Duration {
        sec: (lease_duration_ns / 1_000_000_000) as i32,
        nanosec: (lease_duration_ns % 1_000_000_000) as u32,
    };

    INT2DDS_RET_OK
}

/// Set liveliness QoS for DataReader
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `kind` must be a valid liveliness kind (0=Automatic, 1=ManualByParticipant, 2=ManualByTopic)
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_qos_set_liveliness(
    qos: *mut Int2DdsDataReaderQos,
    kind: i32,
    lease_duration_ns: i64,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    let liveliness_kind = match kind {
        INT2DDS_QOS_LIVELINESS_AUTOMATIC => LivelinessQosPolicyKind::Automatic,
        INT2DDS_QOS_LIVELINESS_MANUAL_BY_PARTICIPANT => {
            LivelinessQosPolicyKind::ManualByParticipant
        }
        INT2DDS_QOS_LIVELINESS_MANUAL_BY_TOPIC => LivelinessQosPolicyKind::ManualByTopic,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    qos_ref.inner.liveliness.kind = liveliness_kind;
    qos_ref.inner.liveliness.lease_duration = Duration {
        sec: (lease_duration_ns / 1_000_000_000) as i32,
        nanosec: (lease_duration_ns % 1_000_000_000) as u32,
    };

    INT2DDS_RET_OK
}

// ============================================================================
// Publisher/Subscriber QoS (Feature 6)
// ============================================================================

/// Opaque QoS handle for Publisher
pub struct Int2DdsPublisherQos {
    pub(crate) inner: PublisherQos,
}

/// Opaque QoS handle for Subscriber
pub struct Int2DdsSubscriberQos {
    pub(crate) inner: SubscriberQos,
}

/// Create default Publisher QoS
///
/// # Safety
/// - `qos_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_qos_create_default(
    qos_out: *mut *mut Int2DdsPublisherQos,
) -> Int2DdsRet {
    check_null!(qos_out);

    let qos = Box::new(Int2DdsPublisherQos { inner: PublisherQos::default() });
    *qos_out = Box::into_raw(qos);

    INT2DDS_RET_OK
}

/// Set partition QoS for Publisher
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `partitions` must point to `partition_count` valid C strings
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_qos_set_partition(
    qos: *mut Int2DdsPublisherQos,
    partitions: *const *const c_char,
    partition_count: usize,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    if partition_count == 0 {
        qos_ref.inner.partition.name = Vec::new();
        return INT2DDS_RET_OK;
    }

    check_null!(partitions);

    let partition_ptrs = std::slice::from_raw_parts(partitions, partition_count);
    let mut names = Vec::with_capacity(partition_count);
    for &ptr in partition_ptrs {
        if ptr.is_null() {
            return INT2DDS_RET_NULL_POINTER;
        }
        let c_str = CStr::from_ptr(ptr);
        match c_str.to_str() {
            Ok(s) => names.push(s.to_owned()),
            Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
        }
    }

    qos_ref.inner.partition.name = names;

    INT2DDS_RET_OK
}

/// Destroy Publisher QoS
///
/// # Safety
/// - `qos` must be a valid QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_publisher_qos_destroy(
    qos: *mut Int2DdsPublisherQos,
) -> Int2DdsRet {
    if qos.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }
    let _ = Box::from_raw(qos);
    INT2DDS_RET_OK
}

/// Create default Subscriber QoS
///
/// # Safety
/// - `qos_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscriber_qos_create_default(
    qos_out: *mut *mut Int2DdsSubscriberQos,
) -> Int2DdsRet {
    check_null!(qos_out);

    let qos = Box::new(Int2DdsSubscriberQos { inner: SubscriberQos::default() });
    *qos_out = Box::into_raw(qos);

    INT2DDS_RET_OK
}

/// Set partition QoS for Subscriber
///
/// # Safety
/// - `qos` must be a valid QoS handle
/// - `partitions` must point to `partition_count` valid C strings
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscriber_qos_set_partition(
    qos: *mut Int2DdsSubscriberQos,
    partitions: *const *const c_char,
    partition_count: usize,
) -> Int2DdsRet {
    check_null!(qos);

    let qos_ref = &mut *qos;

    if partition_count == 0 {
        qos_ref.inner.partition.name = Vec::new();
        return INT2DDS_RET_OK;
    }

    check_null!(partitions);

    let partition_ptrs = std::slice::from_raw_parts(partitions, partition_count);
    let mut names = Vec::with_capacity(partition_count);
    for &ptr in partition_ptrs {
        if ptr.is_null() {
            return INT2DDS_RET_NULL_POINTER;
        }
        let c_str = CStr::from_ptr(ptr);
        match c_str.to_str() {
            Ok(s) => names.push(s.to_owned()),
            Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
        }
    }

    qos_ref.inner.partition.name = names;

    INT2DDS_RET_OK
}

/// Destroy Subscriber QoS
///
/// # Safety
/// - `qos` must be a valid QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_subscriber_qos_destroy(
    qos: *mut Int2DdsSubscriberQos,
) -> Int2DdsRet {
    if qos.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }
    let _ = Box::from_raw(qos);
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

    #[test]
    fn test_deadline_qos() {
        unsafe {
            let mut qos: *mut Int2DdsDataWriterQos = ptr::null_mut();
            let ret = int2dds_datawriter_qos_create_default(&mut qos as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);

            let ret = int2dds_datawriter_qos_set_deadline(qos, 1_000_000_000); // 1 second
            assert_eq!(ret, INT2DDS_RET_OK);

            let ret = int2dds_datawriter_qos_destroy(qos);
            assert_eq!(ret, INT2DDS_RET_OK);

            // DataReader
            let mut rqos: *mut Int2DdsDataReaderQos = ptr::null_mut();
            let ret = int2dds_datareader_qos_create_default(&mut rqos as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);

            let ret = int2dds_datareader_qos_set_deadline(rqos, 500_000_000); // 500ms
            assert_eq!(ret, INT2DDS_RET_OK);

            let ret = int2dds_datareader_qos_destroy(rqos);
            assert_eq!(ret, INT2DDS_RET_OK);
        }
    }

    #[test]
    fn test_liveliness_qos() {
        unsafe {
            let mut qos: *mut Int2DdsDataWriterQos = ptr::null_mut();
            let ret = int2dds_datawriter_qos_create_default(&mut qos as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);

            let ret = int2dds_datawriter_qos_set_liveliness(
                qos,
                INT2DDS_QOS_LIVELINESS_MANUAL_BY_TOPIC,
                2_000_000_000, // 2 seconds
            );
            assert_eq!(ret, INT2DDS_RET_OK);

            // Invalid kind
            let ret = int2dds_datawriter_qos_set_liveliness(qos, 99, 1_000_000_000);
            assert_eq!(ret, INT2DDS_RET_INVALID_ARGUMENT);

            let ret = int2dds_datawriter_qos_destroy(qos);
            assert_eq!(ret, INT2DDS_RET_OK);
        }
    }

    #[test]
    fn test_publisher_qos_create_destroy() {
        unsafe {
            let mut qos: *mut Int2DdsPublisherQos = ptr::null_mut();
            let ret = int2dds_publisher_qos_create_default(&mut qos as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!qos.is_null());

            let ret = int2dds_publisher_qos_destroy(qos);
            assert_eq!(ret, INT2DDS_RET_OK);
        }
    }

    #[test]
    fn test_subscriber_qos_create_destroy() {
        unsafe {
            let mut qos: *mut Int2DdsSubscriberQos = ptr::null_mut();
            let ret = int2dds_subscriber_qos_create_default(&mut qos as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!qos.is_null());

            let ret = int2dds_subscriber_qos_destroy(qos);
            assert_eq!(ret, INT2DDS_RET_OK);
        }
    }

    #[test]
    fn test_publisher_qos_partition() {
        unsafe {
            let mut qos: *mut Int2DdsPublisherQos = ptr::null_mut();
            let ret = int2dds_publisher_qos_create_default(&mut qos as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);

            let p1 = b"partition_a\0".as_ptr() as *const c_char;
            let p2 = b"partition_b\0".as_ptr() as *const c_char;
            let partitions = [p1, p2];
            let ret = int2dds_publisher_qos_set_partition(qos, partitions.as_ptr(), 2);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert_eq!((*qos).inner.partition.name, vec!["partition_a", "partition_b"]);

            let ret = int2dds_publisher_qos_destroy(qos);
            assert_eq!(ret, INT2DDS_RET_OK);
        }
    }
}
