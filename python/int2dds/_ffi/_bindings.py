"""
cffi bindings for int2dds-ffi library.

Uses ABI mode for simplicity and cross-platform compatibility.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

import cffi

ffi = cffi.FFI()

# C definitions for cffi (cleaned version of int2dds-ffi.h)
ffi.cdef("""
    /* QoS constants */
    #define INT2DDS_QOS_RELIABILITY_BEST_EFFORT 0
    #define INT2DDS_QOS_RELIABILITY_RELIABLE 1
    #define INT2DDS_QOS_DURABILITY_VOLATILE 0
    #define INT2DDS_QOS_DURABILITY_TRANSIENT_LOCAL 1
    #define INT2DDS_QOS_DURABILITY_TRANSIENT 2
    #define INT2DDS_QOS_DURABILITY_PERSISTENT 3
    #define INT2DDS_QOS_HISTORY_KEEP_LAST 0
    #define INT2DDS_QOS_HISTORY_KEEP_ALL 1

    /* Status mask bits */
    #define INT2DDS_STATUS_DATA_ON_READERS ...
    #define INT2DDS_STATUS_DATA_AVAILABLE ...
    #define INT2DDS_STATUS_SAMPLE_REJECTED ...
    #define INT2DDS_STATUS_LIVELINESS_CHANGED ...
    #define INT2DDS_STATUS_REQUESTED_DEADLINE_MISSED ...
    #define INT2DDS_STATUS_REQUESTED_INCOMPATIBLE_QOS ...
    #define INT2DDS_STATUS_SAMPLE_LOST ...
    #define INT2DDS_STATUS_SUBSCRIPTION_MATCHED ...
    #define INT2DDS_STATUS_OFFERED_DEADLINE_MISSED ...
    #define INT2DDS_STATUS_OFFERED_INCOMPATIBLE_QOS ...
    #define INT2DDS_STATUS_LIVELINESS_LOST ...
    #define INT2DDS_STATUS_PUBLICATION_MATCHED ...

    /* Return codes */
    #define INT2DDS_RET_OK 0
    #define INT2DDS_RET_ERROR 1
    #define INT2DDS_RET_TIMEOUT 2
    #define INT2DDS_RET_UNSUPPORTED 3
    #define INT2DDS_RET_BAD_ALLOC 10
    #define INT2DDS_RET_INVALID_ARGUMENT 11
    #define INT2DDS_RET_ALREADY_DELETED 20
    #define INT2DDS_RET_NOT_ENABLED 21
    #define INT2DDS_RET_IMMUTABLE_POLICY 22
    #define INT2DDS_RET_INCONSISTENT_POLICY 23
    #define INT2DDS_RET_PRECONDITION_NOT_MET 24
    #define INT2DDS_RET_OUT_OF_RESOURCES 25
    #define INT2DDS_RET_ILLEGAL_OPERATION 26
    #define INT2DDS_RET_NO_DATA 27
    #define INT2DDS_RET_NULL_POINTER 100

    /* Opaque types */
    typedef struct Int2DdsParticipantFactory Int2DdsParticipantFactory;
    typedef struct Int2DdsParticipant Int2DdsParticipant;
    typedef struct Int2DdsPublisher Int2DdsPublisher;
    typedef struct Int2DdsSubscriber Int2DdsSubscriber;
    typedef struct Int2DdsDataWriter Int2DdsDataWriter;
    typedef struct Int2DdsDataReader Int2DdsDataReader;
    typedef struct Int2DdsTopic Int2DdsTopic;
    typedef struct Int2DdsWaitSet Int2DdsWaitSet;
    typedef struct Int2DdsGuardCondition Int2DdsGuardCondition;
    typedef struct Int2DdsStatusCondition Int2DdsStatusCondition;
    typedef struct Int2DdsCondition Int2DdsCondition;
    typedef struct Int2DdsConditionSeq Int2DdsConditionSeq;
    typedef struct Int2DdsDataWriterQos Int2DdsDataWriterQos;
    typedef struct Int2DdsDataReaderQos Int2DdsDataReaderQos;
    typedef struct Int2DdsTopicQos Int2DdsTopicQos;

    typedef int32_t Int2DdsRet;

    /* DomainParticipantFactory */
    Int2DdsRet int2dds_domain_participant_factory_get_instance(
        Int2DdsParticipantFactory **factory_out
    );
    Int2DdsRet int2dds_domain_participant_factory_finalize(
        Int2DdsParticipantFactory *factory
    );

    /* DomainParticipant */
    Int2DdsRet int2dds_create_participant(
        const Int2DdsParticipantFactory *factory,
        const char *name,
        int32_t domain_id,
        Int2DdsParticipant **participant_out
    );
    Int2DdsRet int2dds_delete_participant(Int2DdsParticipant *participant);
    Int2DdsRet int2dds_participant_get_domain_id(
        const Int2DdsParticipant *participant,
        int32_t *domain_id_out
    );
    Int2DdsRet int2dds_participant_assert_liveliness(
        const Int2DdsParticipant *participant
    );
    Int2DdsRet int2dds_participant_delete_contained_entities(
        const Int2DdsParticipant *participant
    );

    /* Publisher */
    Int2DdsRet int2dds_create_publisher(
        const Int2DdsParticipant *participant,
        Int2DdsPublisher **publisher_out
    );
    Int2DdsRet int2dds_delete_publisher(Int2DdsPublisher *publisher);
    Int2DdsRet int2dds_publisher_delete_contained_entities(
        const Int2DdsPublisher *publisher
    );

    /* Subscriber */
    Int2DdsRet int2dds_create_subscriber(
        const Int2DdsParticipant *participant,
        Int2DdsSubscriber **subscriber_out
    );
    Int2DdsRet int2dds_delete_subscriber(Int2DdsSubscriber *subscriber);
    Int2DdsRet int2dds_subscriber_delete_contained_entities(
        const Int2DdsSubscriber *subscriber
    );

    /* Topic */
    Int2DdsRet int2dds_create_topic(
        const Int2DdsParticipant *participant,
        const char *topic_name,
        const char *dds_type_name,
        int32_t extensibility,
        const Int2DdsTopicQos *qos,
        Int2DdsTopic **topic_out
    );
    Int2DdsRet int2dds_create_topic_keyed(
        const Int2DdsParticipant *participant,
        const char *topic_name,
        const char *dds_type_name,
        int32_t extensibility,
        bool has_key,
        const Int2DdsTopicQos *qos,
        Int2DdsTopic **topic_out
    );
    Int2DdsRet int2dds_delete_topic(Int2DdsTopic *topic);
    Int2DdsRet int2dds_topic_get_name(
        const Int2DdsTopic *topic,
        char *name_out,
        size_t name_size
    );
    Int2DdsRet int2dds_topic_get_type_name(
        const Int2DdsTopic *topic,
        char *type_name_out,
        size_t type_name_size
    );

    /* DataWriter */
    Int2DdsRet int2dds_create_datawriter(
        const Int2DdsPublisher *publisher,
        const Int2DdsTopic *topic,
        const Int2DdsDataWriterQos *qos,
        Int2DdsDataWriter **writer_out
    );
    Int2DdsRet int2dds_delete_datawriter(Int2DdsDataWriter *writer);
    Int2DdsRet int2dds_get_publication_matched_status(
        const Int2DdsDataWriter *writer,
        int32_t *total_count_out,
        int32_t *current_count_out
    );
    Int2DdsRet int2dds_write_serialized(
        const Int2DdsDataWriter *writer,
        const uint8_t *data,
        size_t data_len,
        const uint8_t *key,
        size_t key_len
    );
    /* Instance Management */
    Int2DdsRet int2dds_register_instance_serialized(
        const Int2DdsDataWriter *writer,
        const uint8_t *key,
        size_t key_len,
        uint8_t handle_out[16]
    );
    Int2DdsRet int2dds_unregister_instance_serialized(
        const Int2DdsDataWriter *writer,
        const uint8_t *key,
        size_t key_len,
        const uint8_t handle[16]
    );
    Int2DdsRet int2dds_dispose_serialized(
        const Int2DdsDataWriter *writer,
        const uint8_t *key,
        size_t key_len,
        const uint8_t handle[16]
    );
    Int2DdsRet int2dds_lookup_instance_serialized(
        const Int2DdsDataWriter *writer,
        const uint8_t *key,
        size_t key_len,
        uint8_t handle_out[16]
    );
    /* DataReader */
    Int2DdsRet int2dds_create_datareader(
        const Int2DdsSubscriber *subscriber,
        const Int2DdsTopic *topic,
        const Int2DdsDataReaderQos *qos,
        Int2DdsDataReader **reader_out
    );
    Int2DdsRet int2dds_delete_datareader(Int2DdsDataReader *reader);
    Int2DdsRet int2dds_get_subscription_matched_status(
        const Int2DdsDataReader *reader,
        int32_t *total_count_out,
        int32_t *current_count_out
    );
    Int2DdsRet int2dds_take_serialized(
        const Int2DdsDataReader *reader,
        uint8_t *buffer,
        size_t buffer_capacity,
        size_t *actual_size_out,
        bool *valid_data_out
    );
    Int2DdsRet int2dds_read_serialized(
        const Int2DdsDataReader *reader,
        uint8_t *buffer,
        size_t buffer_capacity,
        size_t *actual_size_out,
        bool *valid_data_out
    );

    /* DataWriter QoS */
    Int2DdsRet int2dds_datawriter_qos_create_default(
        Int2DdsDataWriterQos **qos_out
    );
    Int2DdsRet int2dds_datawriter_qos_set_reliability(
        Int2DdsDataWriterQos *qos,
        int32_t kind,
        int64_t max_blocking_time_ns
    );
    Int2DdsRet int2dds_datawriter_qos_set_durability(
        Int2DdsDataWriterQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_datawriter_qos_set_history(
        Int2DdsDataWriterQos *qos,
        int32_t kind,
        int32_t depth
    );
    Int2DdsRet int2dds_datawriter_qos_destroy(Int2DdsDataWriterQos *qos);

    /* DataReader QoS */
    Int2DdsRet int2dds_datareader_qos_create_default(
        Int2DdsDataReaderQos **qos_out
    );
    Int2DdsRet int2dds_datareader_qos_set_reliability(
        Int2DdsDataReaderQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_datareader_qos_set_durability(
        Int2DdsDataReaderQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_datareader_qos_set_history(
        Int2DdsDataReaderQos *qos,
        int32_t kind,
        int32_t depth
    );
    Int2DdsRet int2dds_datareader_qos_destroy(Int2DdsDataReaderQos *qos);

    /* Topic QoS */
    Int2DdsRet int2dds_topic_qos_create_default(Int2DdsTopicQos **qos_out);
    Int2DdsRet int2dds_topic_qos_destroy(Int2DdsTopicQos *qos);

    /* WaitSet */
    Int2DdsRet int2dds_waitset_new(Int2DdsWaitSet **waitset_out);
    Int2DdsRet int2dds_waitset_wait(
        const Int2DdsWaitSet *waitset,
        int64_t timeout_ms
    );
    Int2DdsRet int2dds_waitset_wait_ex(
        const Int2DdsWaitSet *waitset,
        int64_t timeout_ms,
        Int2DdsConditionSeq **conditions_out
    );
    Int2DdsRet int2dds_waitset_delete(Int2DdsWaitSet *waitset);
    Int2DdsRet int2dds_waitset_attach_guard_condition(
        const Int2DdsWaitSet *waitset,
        const Int2DdsGuardCondition *condition
    );
    Int2DdsRet int2dds_waitset_detach_guard_condition(
        const Int2DdsWaitSet *waitset,
        const Int2DdsGuardCondition *condition
    );
    Int2DdsRet int2dds_waitset_attach_condition(
        const Int2DdsWaitSet *waitset,
        const Int2DdsStatusCondition *condition
    );
    Int2DdsRet int2dds_waitset_detach_condition(
        const Int2DdsWaitSet *waitset,
        const Int2DdsStatusCondition *condition
    );
    Int2DdsRet int2dds_waitset_attach_datareader(
        const Int2DdsWaitSet *waitset,
        const Int2DdsDataReader *reader
    );
    Int2DdsRet int2dds_waitset_detach_datareader(
        const Int2DdsWaitSet *waitset,
        const Int2DdsDataReader *reader
    );
    Int2DdsRet int2dds_waitset_attach_datawriter(
        const Int2DdsWaitSet *waitset,
        const Int2DdsDataWriter *writer
    );
    Int2DdsRet int2dds_waitset_detach_datawriter(
        const Int2DdsWaitSet *waitset,
        const Int2DdsDataWriter *writer
    );

    /* GuardCondition */
    Int2DdsRet int2dds_guard_condition_new(
        Int2DdsGuardCondition **condition_out
    );
    Int2DdsRet int2dds_guard_condition_set_trigger_value(
        const Int2DdsGuardCondition *condition,
        bool value
    );
    Int2DdsRet int2dds_guard_condition_get_trigger_value(
        const Int2DdsGuardCondition *condition,
        bool *value_out
    );
    Int2DdsRet int2dds_guard_condition_delete(Int2DdsGuardCondition *condition);

    /* StatusCondition */
    Int2DdsRet int2dds_datareader_get_statuscondition(
        const Int2DdsDataReader *reader,
        Int2DdsStatusCondition **condition_out
    );
    Int2DdsRet int2dds_datawriter_get_statuscondition(
        const Int2DdsDataWriter *writer,
        Int2DdsStatusCondition **condition_out
    );
    Int2DdsRet int2dds_statuscondition_set_enabled_statuses(
        const Int2DdsStatusCondition *condition,
        uint32_t mask
    );
    Int2DdsRet int2dds_statuscondition_get_enabled_statuses(
        const Int2DdsStatusCondition *condition,
        uint32_t *mask_out
    );
    Int2DdsRet int2dds_statuscondition_get_trigger_value(
        const Int2DdsStatusCondition *condition,
        bool *value_out
    );
    Int2DdsRet int2dds_statuscondition_delete(Int2DdsStatusCondition *condition);

    /* ConditionSeq */
    Int2DdsRet int2dds_condition_seq_length(
        const Int2DdsConditionSeq *seq,
        size_t *count_out
    );
    Int2DdsRet int2dds_condition_seq_get(
        const Int2DdsConditionSeq *seq,
        size_t index,
        const Int2DdsCondition **condition_out
    );
    Int2DdsRet int2dds_condition_seq_delete(Int2DdsConditionSeq *seq);
    Int2DdsRet int2dds_condition_get_trigger_value(
        const Int2DdsCondition *condition,
        bool *triggered_out
    );
    Int2DdsRet int2dds_condition_delete(Int2DdsCondition *condition);

    /* Listener Status Structures */
    typedef struct Int2DdsPublicationMatchedStatus {
        int32_t total_count;
        int32_t total_count_change;
        int32_t current_count;
        int32_t current_count_change;
        uint8_t last_subscription_handle[16];
    } Int2DdsPublicationMatchedStatus;

    typedef struct Int2DdsSubscriptionMatchedStatus {
        int32_t total_count;
        int32_t total_count_change;
        int32_t current_count;
        int32_t current_count_change;
        uint8_t last_publication_handle[16];
    } Int2DdsSubscriptionMatchedStatus;

    typedef struct Int2DdsOfferedDeadlineMissedStatus {
        int32_t total_count;
        int32_t total_count_change;
        uint8_t last_instance_handle[16];
    } Int2DdsOfferedDeadlineMissedStatus;

    typedef struct Int2DdsRequestedDeadlineMissedStatus {
        int32_t total_count;
        int32_t total_count_change;
        uint8_t last_instance_handle[16];
    } Int2DdsRequestedDeadlineMissedStatus;

    typedef struct Int2DdsLivelinessLostStatus {
        int32_t total_count;
        int32_t total_count_change;
    } Int2DdsLivelinessLostStatus;

    typedef struct Int2DdsLivelinessChangedStatus {
        int32_t alive_count;
        int32_t not_alive_count;
        int32_t alive_count_change;
        int32_t not_alive_count_change;
        uint8_t last_publication_handle[16];
    } Int2DdsLivelinessChangedStatus;

    typedef struct Int2DdsSampleLostStatus {
        int32_t total_count;
        int32_t total_count_change;
    } Int2DdsSampleLostStatus;


    /* Sample Rejected Status */
    typedef enum {
        INT2DDS_SAMPLE_NOT_REJECTED = 0,
        INT2DDS_SAMPLE_REJECTED_BY_INSTANCES_LIMIT = 1,
        INT2DDS_SAMPLE_REJECTED_BY_SAMPLES_LIMIT = 2,
        INT2DDS_SAMPLE_REJECTED_BY_SAMPLES_PER_INSTANCE_LIMIT = 3
    } Int2DdsSampleRejectedStatusKind;

    typedef struct Int2DdsSampleRejectedStatus {
        int32_t total_count;
        int32_t total_count_change;
        Int2DdsSampleRejectedStatusKind last_reason;
        uint8_t last_instance_handle[16];
    } Int2DdsSampleRejectedStatus;

    /* QoS Policy ID for Incompatible QoS Status */
    typedef enum {
        INT2DDS_QOS_POLICY_INVALID = 0,
        INT2DDS_QOS_POLICY_USERDATA = 1,
        INT2DDS_QOS_POLICY_DURABILITY = 2,
        INT2DDS_QOS_POLICY_PRESENTATION = 3,
        INT2DDS_QOS_POLICY_DEADLINE = 4,
        INT2DDS_QOS_POLICY_LATENCYBUDGET = 5,
        INT2DDS_QOS_POLICY_OWNERSHIP = 6,
        INT2DDS_QOS_POLICY_OWNERSHIPSTRENGTH = 7,
        INT2DDS_QOS_POLICY_LIVELINESS = 8,
        INT2DDS_QOS_POLICY_TIMEBASEDFILTER = 9,
        INT2DDS_QOS_POLICY_PARTITION = 10,
        INT2DDS_QOS_POLICY_RELIABILITY = 11,
        INT2DDS_QOS_POLICY_DESTINATIONORDER = 12,
        INT2DDS_QOS_POLICY_HISTORY = 13,
        INT2DDS_QOS_POLICY_RESOURCELIMITS = 14,
        INT2DDS_QOS_POLICY_ENTITYFACTORY = 15,
        INT2DDS_QOS_POLICY_WRITERDATALIFECYCLE = 16,
        INT2DDS_QOS_POLICY_READERDATALIFECYCLE = 17,
        INT2DDS_QOS_POLICY_TOPICDATA = 18,
        INT2DDS_QOS_POLICY_GROUPDATA = 19,
        INT2DDS_QOS_POLICY_TRANSPORTPRIORITY = 20,
        INT2DDS_QOS_POLICY_LIFESPAN = 21,
        INT2DDS_QOS_POLICY_DURABILITYSERVICE = 22,
        INT2DDS_QOS_POLICY_DATAREPRESENTATION = 23,
        INT2DDS_QOS_POLICY_TYPECONSISTENCYENFORCEMENT = 24
    } Int2DdsQosPolicyId;

    typedef struct Int2DdsRequestedIncompatibleQosStatus {
        int32_t total_count;
        int32_t total_count_change;
        Int2DdsQosPolicyId last_policy_id;
        uint32_t policies_count;
    } Int2DdsRequestedIncompatibleQosStatus;

    typedef struct Int2DdsOfferedIncompatibleQosStatus {
        int32_t total_count;
        int32_t total_count_change;
        Int2DdsQosPolicyId last_policy_id;
        uint32_t policies_count;
    } Int2DdsOfferedIncompatibleQosStatus;

    /* User context for callbacks */
    typedef void *Int2DdsUserContext;

    /* Callback function pointer types */
    typedef void (*Int2DdsOnPublicationMatchedCallback)(
        Int2DdsDataWriter *writer,
        const Int2DdsPublicationMatchedStatus *status,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnOfferedDeadlineMissedCallback)(
        Int2DdsDataWriter *writer,
        const Int2DdsOfferedDeadlineMissedStatus *status,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnLivelinessLostCallback)(
        Int2DdsDataWriter *writer,
        const Int2DdsLivelinessLostStatus *status,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnDataAvailableCallback)(
        Int2DdsDataReader *reader,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnSubscriptionMatchedCallback)(
        Int2DdsDataReader *reader,
        const Int2DdsSubscriptionMatchedStatus *status,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnRequestedDeadlineMissedCallback)(
        Int2DdsDataReader *reader,
        const Int2DdsRequestedDeadlineMissedStatus *status,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnLivelinessChangedCallback)(
        Int2DdsDataReader *reader,
        const Int2DdsLivelinessChangedStatus *status,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnSampleLostCallback)(
        Int2DdsDataReader *reader,
        const Int2DdsSampleLostStatus *status,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnSampleRejectedCallback)(
        Int2DdsDataReader *reader,
        const Int2DdsSampleRejectedStatus *status,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnRequestedIncompatibleQosCallback)(
        Int2DdsDataReader *reader,
        const Int2DdsRequestedIncompatibleQosStatus *status,
        Int2DdsUserContext user_context
    );
    typedef void (*Int2DdsOnOfferedIncompatibleQosCallback)(
        Int2DdsDataWriter *writer,
        const Int2DdsOfferedIncompatibleQosStatus *status,
        Int2DdsUserContext user_context
    );
    /* DataWriter Listener */
    typedef struct Int2DdsDataWriterListener {
        Int2DdsOnPublicationMatchedCallback on_publication_matched;
        Int2DdsOnOfferedDeadlineMissedCallback on_offered_deadline_missed;
        void *on_offered_incompatible_qos;  /* Not fully exposed */
        Int2DdsOnLivelinessLostCallback on_liveliness_lost;
        Int2DdsUserContext user_context;
    } Int2DdsDataWriterListener;

    /* DataReader Listener */
    typedef struct Int2DdsDataReaderListener {
        Int2DdsOnDataAvailableCallback on_data_available;
        Int2DdsOnSubscriptionMatchedCallback on_subscription_matched;
        void *on_sample_rejected;  /* Not fully exposed */
        Int2DdsOnLivelinessChangedCallback on_liveliness_changed;
        Int2DdsOnRequestedDeadlineMissedCallback on_requested_deadline_missed;
        void *on_requested_incompatible_qos;  /* Not fully exposed */
        Int2DdsOnSampleLostCallback on_sample_lost;
        Int2DdsUserContext user_context;
    } Int2DdsDataReaderListener;

    /* DataWriter with Listener */
    Int2DdsRet int2dds_create_datawriter_with_listener(
        const Int2DdsPublisher *publisher,
        const Int2DdsTopic *topic,
        const Int2DdsDataWriterQos *qos,
        const Int2DdsDataWriterListener *listener,
        uint32_t mask,
        Int2DdsDataWriter **writer_out
    );
    Int2DdsRet int2dds_datawriter_set_listener(
        Int2DdsDataWriter *writer,
        const Int2DdsDataWriterListener *listener,
        uint32_t mask
    );

    /* DataReader with Listener */
    Int2DdsRet int2dds_create_datareader_with_listener(
        const Int2DdsSubscriber *subscriber,
        const Int2DdsTopic *topic,
        const Int2DdsDataReaderQos *qos,
        const Int2DdsDataReaderListener *listener,
        uint32_t mask,
        Int2DdsDataReader **reader_out
    );
    Int2DdsRet int2dds_datareader_set_listener(
        Int2DdsDataReader *reader,
        const Int2DdsDataReaderListener *listener,
        uint32_t mask
    );
""")


def _find_library() -> str:
    """Find the int2dds_ffi library path."""
    # Library names by platform
    if sys.platform == "win32":
        lib_name = "int2dds_ffi.dll"
    elif sys.platform == "darwin":
        lib_name = "libint2dds_ffi.dylib"
    else:
        lib_name = "libint2dds_ffi.so"

    # Search paths in order of priority
    search_paths = []

    # 1. Environment variable
    env_path = os.environ.get("INT2DDS_FFI_PATH")
    if env_path:
        search_paths.append(Path(env_path))

    # 2. Relative to this package (for development)
    package_dir = Path(__file__).parent.parent.parent
    search_paths.extend([
        package_dir / lib_name,
        package_dir.parent / "target" / "release" / lib_name,
        package_dir.parent / "target" / "debug" / lib_name,
        package_dir.parent / "ffi" / "target" / "release" / lib_name,
        package_dir.parent / "ffi" / "target" / "debug" / lib_name,
    ])

    # 3. System library paths
    if sys.platform == "win32":
        system_paths = os.environ.get("PATH", "").split(os.pathsep)
    else:
        system_paths = [
            "/usr/local/lib",
            "/usr/lib",
            os.path.expanduser("~/.local/lib"),
        ]
        ld_path = os.environ.get("LD_LIBRARY_PATH", "")
        if ld_path:
            system_paths = ld_path.split(os.pathsep) + system_paths

    for path in system_paths:
        search_paths.append(Path(path) / lib_name)

    # Search for the library
    for path in search_paths:
        if path.exists():
            return str(path)

    # If not found, try loading by name (system library loader)
    return lib_name


# Load the library
_lib_path = _find_library()
try:
    lib = ffi.dlopen(_lib_path)
except OSError as e:
    raise ImportError(
        f"Could not load int2dds_ffi library from '{_lib_path}'. "
        f"Please ensure the library is built and available. "
        f"Set INT2DDS_FFI_PATH environment variable to specify the library location. "
        f"Original error: {e}"
    ) from e
