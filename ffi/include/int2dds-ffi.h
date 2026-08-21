#ifndef INT2DDS_FFI_H
#define INT2DDS_FFI_H

#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#if defined(__GNUC__) || defined(__clang__)
#define INT2DDS_DEPRECATED(msg) __attribute__((deprecated(msg)))
#elif defined(_MSC_VER)
#define INT2DDS_DEPRECATED(msg) __declspec(deprecated(msg))
#else
#define INT2DDS_DEPRECATED(msg)
#endif

/**
 * Value-kind discriminators returned by `int2dds_dynamic_value_kind`.
 */
#define INT2DDS_VALUE_KIND_BOOLEAN 0

#define INT2DDS_VALUE_KIND_INT8 1

#define INT2DDS_VALUE_KIND_INT16 2

#define INT2DDS_VALUE_KIND_INT32 3

#define INT2DDS_VALUE_KIND_INT64 4

#define INT2DDS_VALUE_KIND_UINT8 5

#define INT2DDS_VALUE_KIND_UINT16 6

#define INT2DDS_VALUE_KIND_UINT32 7

#define INT2DDS_VALUE_KIND_UINT64 8

#define INT2DDS_VALUE_KIND_FLOAT32 9

#define INT2DDS_VALUE_KIND_FLOAT64 10

#define INT2DDS_VALUE_KIND_CHAR8 11

#define INT2DDS_VALUE_KIND_BYTE 12

#define INT2DDS_VALUE_KIND_STRING 13

#define INT2DDS_VALUE_KIND_WSTRING 14

#define INT2DDS_VALUE_KIND_ENUM 15

#define INT2DDS_VALUE_KIND_UNION 16

#define INT2DDS_VALUE_KIND_BITMASK 17

#define INT2DDS_VALUE_KIND_BITSET 18

#define INT2DDS_VALUE_KIND_STRUCT 19

#define INT2DDS_VALUE_KIND_SEQUENCE 20

#define INT2DDS_VALUE_KIND_ARRAY 21

#define INT2DDS_VALUE_KIND_MAP 22

#define INT2DDS_VALUE_KIND_OPTIONAL 23

#define INT2DDS_VALUE_KIND_NULL 24

#define INT2DDS_QOS_RELIABILITY_BEST_EFFORT 0

#define INT2DDS_QOS_RELIABILITY_RELIABLE 1

#define INT2DDS_QOS_DURABILITY_VOLATILE 0

#define INT2DDS_QOS_DURABILITY_TRANSIENT_LOCAL 1

#define INT2DDS_QOS_DURABILITY_TRANSIENT 2

#define INT2DDS_QOS_DURABILITY_PERSISTENT 3

#define INT2DDS_QOS_HISTORY_KEEP_LAST 0

#define INT2DDS_QOS_HISTORY_KEEP_ALL 1

#define INT2DDS_QOS_DATA_REPR_XCDR1 0

#define INT2DDS_QOS_DATA_REPR_XCDR2 2

#define INT2DDS_QOS_LIVELINESS_AUTOMATIC 0

#define INT2DDS_QOS_LIVELINESS_MANUAL_BY_PARTICIPANT 1

#define INT2DDS_QOS_LIVELINESS_MANUAL_BY_TOPIC 2

#define INT2DDS_QOS_OWNERSHIP_SHARED 0

#define INT2DDS_QOS_OWNERSHIP_EXCLUSIVE 1

#define INT2DDS_QOS_DEST_ORDER_BY_RECEPTION 0

#define INT2DDS_QOS_DEST_ORDER_BY_SOURCE 1

#define INT2DDS_QOS_LIFESPAN_REF_BY_SOURCE 0

#define INT2DDS_QOS_LIFESPAN_REF_BY_RECEPTION 1

#define INT2DDS_STATUS_INCONSISTENT_TOPIC (1 << 0)

#define INT2DDS_STATUS_OFFERED_DEADLINE_MISSED (1 << 1)

#define INT2DDS_STATUS_REQUESTED_DEADLINE_MISSED (1 << 2)

#define INT2DDS_STATUS_OFFERED_INCOMPATIBLE_TYPE (1 << 3)

#define INT2DDS_STATUS_REQUESTED_INCOMPATIBLE_TYPE (1 << 4)

#define INT2DDS_STATUS_OFFERED_INCOMPATIBLE_QOS (1 << 5)

#define INT2DDS_STATUS_REQUESTED_INCOMPATIBLE_QOS (1 << 6)

#define INT2DDS_STATUS_SAMPLE_LOST (1 << 7)

#define INT2DDS_STATUS_SAMPLE_REJECTED (1 << 8)

#define INT2DDS_STATUS_DATA_ON_READERS (1 << 9)

#define INT2DDS_STATUS_DATA_AVAILABLE (1 << 10)

#define INT2DDS_STATUS_LIVELINESS_LOST (1 << 11)

#define INT2DDS_STATUS_LIVELINESS_CHANGED (1 << 12)

#define INT2DDS_STATUS_PUBLICATION_MATCHED (1 << 13)

#define INT2DDS_STATUS_SUBSCRIPTION_MATCHED (1 << 14)

#define INT2DDS_SAMPLE_STATE_READ 1

#define INT2DDS_SAMPLE_STATE_NOT_READ 2

#define INT2DDS_SAMPLE_STATE_ANY 65535

#define INT2DDS_VIEW_STATE_NEW 1

#define INT2DDS_VIEW_STATE_NOT_NEW 2

#define INT2DDS_VIEW_STATE_ANY 65535

#define INT2DDS_INSTANCE_STATE_ALIVE 1

#define INT2DDS_INSTANCE_STATE_NOT_ALIVE_DISPOSED 2

#define INT2DDS_INSTANCE_STATE_NOT_ALIVE_NO_WRITERS 4

#define INT2DDS_INSTANCE_STATE_ANY 65535

/**
 * Field type constants for C FFI.
 */
#define INT2DDS_FIELD_BOOL 0

#define INT2DDS_FIELD_BYTE 1

#define INT2DDS_FIELD_CHAR8 2

#define INT2DDS_FIELD_INT8 3

#define INT2DDS_FIELD_INT16 4

#define INT2DDS_FIELD_INT32 5

#define INT2DDS_FIELD_INT64 6

#define INT2DDS_FIELD_UINT8 7

#define INT2DDS_FIELD_UINT16 8

#define INT2DDS_FIELD_UINT32 9

#define INT2DDS_FIELD_UINT64 10

#define INT2DDS_FIELD_FLOAT32 11

#define INT2DDS_FIELD_FLOAT64 12

#define INT2DDS_FIELD_STRING 13

#define INT2DDS_FIELD_CHAR16 14

#define INT2DDS_FIELD_WSTRING 15

#define INT2DDS_FIELD_NESTED 16

#define INT2DDS_FIELD_SEQUENCE 17

#define INT2DDS_FIELD_ARRAY 18

#define INT2DDS_FIELD_MAP 19

#define INT2DDS_MEMBER_KEY (1 << 0)

#define INT2DDS_MEMBER_OPTIONAL (1 << 1)

#define INT2DDS_MEMBER_MUST_UNDERSTAND (1 << 2)

#define INT2DDS_MEMBER_EXTERNAL (1 << 3)

/**
 * C-compatible QoS policy ID enum
 */
typedef enum Int2DdsQosPolicyId {
  Invalid = 0,
  UserData = 1,
  Durability = 2,
  Presentation = 3,
  Deadline = 4,
  LatencyBudget = 5,
  Ownership = 6,
  OwnershipStrength = 7,
  Liveliness = 8,
  TimeBasedFilter = 9,
  Partition = 10,
  Reliability = 11,
  DestinationOrder = 12,
  History = 13,
  ResourceLimits = 14,
  EntityFactory = 15,
  WriterDataLifecycle = 16,
  ReaderDataLifecycle = 17,
  TopicData = 18,
  GroupData = 19,
  TransportPriority = 20,
  Lifespan = 21,
  DurabilityService = 22,
  DataRepresentation = 23,
  TypeConsistencyEnforcement = 24,
  Property = 25,
} Int2DdsQosPolicyId;

/**
 * C-compatible sample rejected status kind
 */
typedef enum Int2DdsSampleRejectedStatusKind {
  NotRejected = 0,
  RejectedByInstancesLimit = 1,
  RejectedBySamplesLimit = 2,
  RejectedBySamplesPerInstanceLimit = 3,
} Int2DdsSampleRejectedStatusKind;

/**
 * Generic condition handle for use with WaitSet
 */
typedef struct Int2DdsCondition Int2DdsCondition;

/**
 * Sequence of conditions returned from WaitSet::wait
 */
typedef struct Int2DdsConditionSeq Int2DdsConditionSeq;

/**
 * Opaque handle wrapping a [`ConfiguredParticipant`] — the whole tree built by
 * [`int2dds_create_participant_from_config`]. Destroy with
 * [`int2dds_configured_participant_destroy`].
 */
typedef struct Int2DdsConfiguredParticipant Int2DdsConfiguredParticipant;

/**
 * Opaque handle to a ContentFilteredTopic
 */
typedef struct Int2DdsContentFilteredTopic Int2DdsContentFilteredTopic;

/**
 * Opaque handle to a DataReader
 */
typedef struct Int2DdsDataReader Int2DdsDataReader;

/**
 * Opaque QoS handle for DataReader
 */
typedef struct Int2DdsDataReaderQos Int2DdsDataReaderQos;

/**
 * Opaque handle to a DataWriter
 */
typedef struct Int2DdsDataWriter Int2DdsDataWriter;

/**
 * Opaque QoS handle for DataWriter
 */
typedef struct Int2DdsDataWriterQos Int2DdsDataWriterQos;

typedef struct Int2DdsDynamicData Int2DdsDynamicData;

/**
 * Opaque handle to a dynamic DataReader (`DataReader<DynamicData>`).
 */
typedef struct Int2DdsDynamicDataReader Int2DdsDynamicDataReader;

/**
 * Opaque handle to a dynamic DataWriter (`DataWriter<DynamicData>`).
 */
typedef struct Int2DdsDynamicDataWriter Int2DdsDynamicDataWriter;

/**
 * Opaque handle wrapping an `Arc<DynamicTypeSupport>` (full dependency closure
 * resolved). Obtain one from an XML registry or a discovered TypeObject.
 */
typedef struct Int2DdsDynamicTypeSupport Int2DdsDynamicTypeSupport;

/**
 * Opaque, owning handle wrapping a [`DynamicValue`].
 */
typedef struct Int2DdsDynamicValue Int2DdsDynamicValue;

/**
 * Opaque handle to a GuardCondition
 */
typedef struct Int2DdsGuardCondition Int2DdsGuardCondition;

/**
 * Opaque handle to a DomainParticipant
 */
typedef struct Int2DdsParticipant Int2DdsParticipant;

typedef struct Int2DdsParticipantBuiltinTopicData Int2DdsParticipantBuiltinTopicData;

/**
 * Opaque handle to a DomainParticipantFactory
 */
typedef struct Int2DdsParticipantFactory Int2DdsParticipantFactory;

/**
 * Opaque QoS handle for DomainParticipant
 */
typedef struct Int2DdsParticipantQos Int2DdsParticipantQos;

typedef struct Int2DdsPublicationBuiltinTopicData Int2DdsPublicationBuiltinTopicData;

typedef struct Int2DdsPublicationBuiltinTopicDataSeq Int2DdsPublicationBuiltinTopicDataSeq;

/**
 * Opaque handle to a Publisher
 */
typedef struct Int2DdsPublisher Int2DdsPublisher;

/**
 * Opaque QoS handle for Publisher
 */
typedef struct Int2DdsPublisherQos Int2DdsPublisherQos;

/**
 * Opaque handle to a ReadCondition or QueryCondition.
 * `inner` is used by WaitSet (trait object); `kind` provides concrete access
 * for the filtered read/take path and query-parameter mutation.
 */
typedef struct Int2DdsReadCondition Int2DdsReadCondition;

/**
 * Opaque sequence of (serialized data, SampleInfo) pairs for batch read/take
 */
typedef struct Int2DdsSampleSeq Int2DdsSampleSeq;

typedef struct Int2DdsSerializedLoan Int2DdsSerializedLoan;

typedef struct Int2DdsSerializedWriteLoan Int2DdsSerializedWriteLoan;

/**
 * Opaque handle to a StatusCondition
 * `inner` is used by WaitSet (trait object), `kind` provides concrete access.
 */
typedef struct Int2DdsStatusCondition Int2DdsStatusCondition;

/**
 * Opaque handle to a Subscriber
 */
typedef struct Int2DdsSubscriber Int2DdsSubscriber;

/**
 * Opaque QoS handle for Subscriber
 */
typedef struct Int2DdsSubscriberQos Int2DdsSubscriberQos;

typedef struct Int2DdsSubscriptionBuiltinTopicData Int2DdsSubscriptionBuiltinTopicData;

typedef struct Int2DdsSubscriptionBuiltinTopicDataSeq Int2DdsSubscriptionBuiltinTopicDataSeq;

/**
 * Opaque handle to a Topic
 */
typedef struct Int2DdsTopic Int2DdsTopic;

/**
 * Opaque QoS handle for Topic
 */
typedef struct Int2DdsTopicQos Int2DdsTopicQos;

/**
 * Opaque type info builder for the FFI layer.
 *
 * Collects field descriptions and builds TypeIdentifier/TypeObject
 * for DDS-XTypes discovery.
 */
typedef struct Int2DdsTypeInfo Int2DdsTypeInfo;

/**
 * Opaque handle wrapping a discovered TypeObject.
 */
typedef struct Int2DdsTypeObject Int2DdsTypeObject;

/**
 * Opaque handle to a WaitSet
 */
typedef struct Int2DdsWaitSet Int2DdsWaitSet;

/**
 * Opaque handle wrapping an [`XmlTypeRegistry`].
 */
typedef struct Int2DdsXmlTypeRegistry Int2DdsXmlTypeRegistry;

/**
 * FFI return codes
 */
typedef int32_t Int2DdsRet;

/**
 * C callback invoked on every remote endpoint discovery/dispose.
 * `is_writer`: 1 = publication/writer, 0 = subscription/reader.
 * `is_alive`:  1 = alive (`pub_data` or `sub_data` valid), 0 = disposed (data null).
 * `guid` always points to the 16-byte endpoint GUID (valid only during the call).
 * All pointers are borrowed and must be copied out before returning.
 */
typedef void (*Int2DdsEndpointDiscoveryCallback)(void *ctx,
                                                 int32_t is_writer,
                                                 int32_t is_alive,
                                                 const struct Int2DdsPublicationBuiltinTopicData *pub_data,
                                                 const struct Int2DdsSubscriptionBuiltinTopicData *sub_data,
                                                 const uint8_t (*guid)[16]);

/**
 * C-visible per-member information returned by `int2dds_type_object_member_info`.
 */
typedef struct Int2DdsMemberInfo {
  uint32_t member_id;
  int32_t kind;
  int32_t flags;
} Int2DdsMemberInfo;

/**
 * FFI-safe SampleInfo returned to C callers
 */
typedef struct Int2DdsSampleInfo {
  int32_t source_timestamp_sec;
  uint32_t source_timestamp_nanosec;
  uint32_t sample_state;
  uint32_t view_state;
  uint32_t instance_state;
  uint8_t instance_handle[16];
  uint8_t publication_handle[16];
  int32_t disposed_generation_count;
  int32_t no_writers_generation_count;
  int32_t sample_rank;
  int32_t generation_rank;
  int32_t absolute_generation_rank;
  bool valid_data;
} Int2DdsSampleInfo;

/**
 * C-compatible publication matched status
 */
typedef struct Int2DdsPublicationMatchedStatus {
  /**
   * Total cumulative count of DataReaders that matched
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
  /**
   * Current number of matched DataReaders
   */
  int32_t current_count;
  /**
   * Change in current_count since last access
   */
  int32_t current_count_change;
  /**
   * Handle of the last matched DataReader
   */
  uint8_t last_subscription_handle[16];
} Int2DdsPublicationMatchedStatus;

/**
 * User context passed to all callbacks
 */
typedef void *Int2DdsUserContext;

typedef void (*Int2DdsOnPublicationMatchedCallback)(struct Int2DdsDataWriter *writer,
                                                    const struct Int2DdsPublicationMatchedStatus *status,
                                                    Int2DdsUserContext user_context);

/**
 * C-compatible offered deadline missed status
 */
typedef struct Int2DdsOfferedDeadlineMissedStatus {
  /**
   * Total cumulative count of missed deadlines
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
  /**
   * Handle of the last instance for which deadline was missed
   */
  uint8_t last_instance_handle[16];
} Int2DdsOfferedDeadlineMissedStatus;

typedef void (*Int2DdsOnOfferedDeadlineMissedCallback)(struct Int2DdsDataWriter *writer,
                                                       const struct Int2DdsOfferedDeadlineMissedStatus *status,
                                                       Int2DdsUserContext user_context);

/**
 * C-compatible offered incompatible QoS status
 */
typedef struct Int2DdsOfferedIncompatibleQosStatus {
  /**
   * Total cumulative count of incompatible QoS
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
  /**
   * ID of the last incompatible policy
   */
  enum Int2DdsQosPolicyId last_policy_id;
  /**
   * Count of policies (always 0 for now, policies list not exposed)
   */
  uint32_t policies_count;
} Int2DdsOfferedIncompatibleQosStatus;

typedef void (*Int2DdsOnOfferedIncompatibleQosCallback)(struct Int2DdsDataWriter *writer,
                                                        const struct Int2DdsOfferedIncompatibleQosStatus *status,
                                                        Int2DdsUserContext user_context);

/**
 * C-compatible liveliness lost status
 */
typedef struct Int2DdsLivelinessLostStatus {
  /**
   * Total cumulative count of times liveliness was lost
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
} Int2DdsLivelinessLostStatus;

typedef void (*Int2DdsOnLivelinessLostCallback)(struct Int2DdsDataWriter *writer,
                                                const struct Int2DdsLivelinessLostStatus *status,
                                                Int2DdsUserContext user_context);

/**
 * C-compatible DataWriter listener callbacks
 */
typedef struct Int2DdsDataWriterListener {
  Int2DdsOnPublicationMatchedCallback on_publication_matched;
  Int2DdsOnOfferedDeadlineMissedCallback on_offered_deadline_missed;
  Int2DdsOnOfferedIncompatibleQosCallback on_offered_incompatible_qos;
  Int2DdsOnLivelinessLostCallback on_liveliness_lost;
  Int2DdsUserContext user_context;
} Int2DdsDataWriterListener;

/**
 * C-compatible offered incompatible type status
 */
typedef struct Int2DdsOfferedIncompatibleTypeStatus {
  /**
   * Total cumulative count of incompatible type
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
} Int2DdsOfferedIncompatibleTypeStatus;

typedef void (*Int2DdsOnDataAvailableCallback)(struct Int2DdsDataReader *reader,
                                               Int2DdsUserContext user_context);

/**
 * C-compatible subscription matched status
 */
typedef struct Int2DdsSubscriptionMatchedStatus {
  /**
   * Total cumulative count of DataWriters that matched
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
  /**
   * Current number of matched DataWriters
   */
  int32_t current_count;
  /**
   * Change in current_count since last access
   */
  int32_t current_count_change;
  /**
   * Handle of the last matched DataWriter
   */
  uint8_t last_publication_handle[16];
} Int2DdsSubscriptionMatchedStatus;

typedef void (*Int2DdsOnSubscriptionMatchedCallback)(struct Int2DdsDataReader *reader,
                                                     const struct Int2DdsSubscriptionMatchedStatus *status,
                                                     Int2DdsUserContext user_context);

/**
 * C-compatible sample rejected status
 */
typedef struct Int2DdsSampleRejectedStatus {
  /**
   * Total cumulative count of samples rejected
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
  /**
   * Reason for last sample rejection
   */
  enum Int2DdsSampleRejectedStatusKind last_reason;
  /**
   * Handle of the instance for the last rejected sample
   */
  uint8_t last_instance_handle[16];
} Int2DdsSampleRejectedStatus;

typedef void (*Int2DdsOnSampleRejectedCallback)(struct Int2DdsDataReader *reader,
                                                const struct Int2DdsSampleRejectedStatus *status,
                                                Int2DdsUserContext user_context);

/**
 * C-compatible liveliness changed status
 */
typedef struct Int2DdsLivelinessChangedStatus {
  /**
   * Current count of alive DataWriters
   */
  int32_t alive_count;
  /**
   * Current count of not-alive DataWriters
   */
  int32_t not_alive_count;
  /**
   * Change in alive_count since last access
   */
  int32_t alive_count_change;
  /**
   * Change in not_alive_count since last access
   */
  int32_t not_alive_count_change;
  /**
   * Handle of the last DataWriter whose liveliness changed
   */
  uint8_t last_publication_handle[16];
} Int2DdsLivelinessChangedStatus;

typedef void (*Int2DdsOnLivelinessChangedCallback)(struct Int2DdsDataReader *reader,
                                                   const struct Int2DdsLivelinessChangedStatus *status,
                                                   Int2DdsUserContext user_context);

/**
 * C-compatible requested deadline missed status
 */
typedef struct Int2DdsRequestedDeadlineMissedStatus {
  /**
   * Total cumulative count of missed deadlines
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
  /**
   * Handle of the last instance for which deadline was missed
   */
  uint8_t last_instance_handle[16];
} Int2DdsRequestedDeadlineMissedStatus;

typedef void (*Int2DdsOnRequestedDeadlineMissedCallback)(struct Int2DdsDataReader *reader,
                                                         const struct Int2DdsRequestedDeadlineMissedStatus *status,
                                                         Int2DdsUserContext user_context);

/**
 * C-compatible requested incompatible QoS status
 */
typedef struct Int2DdsRequestedIncompatibleQosStatus {
  /**
   * Total cumulative count of incompatible QoS
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
  /**
   * ID of the last incompatible policy
   */
  enum Int2DdsQosPolicyId last_policy_id;
  /**
   * Count of policies (always 0 for now, policies list not exposed)
   */
  uint32_t policies_count;
} Int2DdsRequestedIncompatibleQosStatus;

typedef void (*Int2DdsOnRequestedIncompatibleQosCallback)(struct Int2DdsDataReader *reader,
                                                          const struct Int2DdsRequestedIncompatibleQosStatus *status,
                                                          Int2DdsUserContext user_context);

/**
 * C-compatible sample lost status
 */
typedef struct Int2DdsSampleLostStatus {
  /**
   * Total cumulative count of samples lost
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
} Int2DdsSampleLostStatus;

typedef void (*Int2DdsOnSampleLostCallback)(struct Int2DdsDataReader *reader,
                                            const struct Int2DdsSampleLostStatus *status,
                                            Int2DdsUserContext user_context);

/**
 * C-compatible DataReader listener callbacks
 */
typedef struct Int2DdsDataReaderListener {
  Int2DdsOnDataAvailableCallback on_data_available;
  Int2DdsOnSubscriptionMatchedCallback on_subscription_matched;
  Int2DdsOnSampleRejectedCallback on_sample_rejected;
  Int2DdsOnLivelinessChangedCallback on_liveliness_changed;
  Int2DdsOnRequestedDeadlineMissedCallback on_requested_deadline_missed;
  Int2DdsOnRequestedIncompatibleQosCallback on_requested_incompatible_qos;
  Int2DdsOnSampleLostCallback on_sample_lost;
  Int2DdsUserContext user_context;
} Int2DdsDataReaderListener;

/**
 * C-compatible requested incompatible type status
 */
typedef struct Int2DdsRequestedIncompatibleTypeStatus {
  /**
   * Total cumulative count of incompatible type
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
} Int2DdsRequestedIncompatibleTypeStatus;

/**
 * C-compatible inconsistent topic status
 */
typedef struct Int2DdsInconsistentTopicStatus {
  /**
   * Total cumulative count of inconsistent topics detected
   */
  int32_t total_count;
  /**
   * Change in total_count since last access
   */
  int32_t total_count_change;
} Int2DdsInconsistentTopicStatus;

#define INT2DDS_RET_OK 0

#define INT2DDS_RET_ERROR 1

#define INT2DDS_RET_TIMEOUT 2

#define INT2DDS_RET_UNSUPPORTED 3

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

#define INT2DDS_RET_BUFFER_TOO_SMALL 101

#define INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND 200

#define INT2DDS_RET_DYNAMIC_TYPE_MISMATCH 201

#define INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE 202

#define INT2DDS_RET_DYNAMIC_TIMEOUT 203

#define INT2DDS_RET_DYNAMIC_DECODE_ERROR 204

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Create a new GuardCondition
 *
 * # Safety
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_guardcondition_delete`
 */
Int2DdsRet int2dds_guardcondition_new(struct Int2DdsGuardCondition **condition_out);

/**
 * Set the trigger value of a GuardCondition
 *
 * # Safety
 * - `condition` must be a valid guard condition
 * - `value` is the new trigger value (true to trigger, false to reset)
 */
Int2DdsRet int2dds_guardcondition_set_trigger_value(const struct Int2DdsGuardCondition *condition,
                                                    bool value);

/**
 * Get the trigger value of a GuardCondition
 *
 * # Safety
 * - `condition` must be a valid guard condition
 * - `value_out` must be a valid pointer
 */
Int2DdsRet int2dds_guardcondition_get_trigger_value(const struct Int2DdsGuardCondition *condition,
                                                    bool *value_out);

/**
 * Delete a GuardCondition
 *
 * # Safety
 * - `condition` must be a valid guard condition
 * - `condition` must not be used after this call
 * - The condition should be detached from any WaitSets first
 */
Int2DdsRet int2dds_guardcondition_delete(struct Int2DdsGuardCondition *condition);

/**
 * Load QoS profiles (and, for XML files, the `<types>` section) from one or
 * more files into the factory singleton. Profiles loaded here can then be used
 * with the `*_with_profile` creators and with
 * [`int2dds_create_participant_from_config`]; types can be fetched with
 * [`int2dds_get_dynamic_type_support`].
 *
 * # Safety
 * - `paths` must point to `count` valid, null-terminated UTF-8 C strings
 * - each element of `paths` must be non-null
 */
Int2DdsRet int2dds_load_profiles(const char *const *paths, uintptr_t count);

/**
 * Build a dynamic type support for a type declared in a `<types>` section that
 * was loaded via [`int2dds_load_profiles`]. Destroy the result with
 * `int2dds_dynamic_type_support_destroy`.
 *
 * # Safety
 * - `type_name` must be a valid, null-terminated UTF-8 C string
 * - `out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_get_dynamic_type_support(const char *type_name,
                                            struct Int2DdsDynamicTypeSupport **out);

/**
 * Build an entire participant tree from a `<domain_participant_library>`
 * declaration at `path` (`ParticipantLibrary::Participant`, e.g.
 * `"PL::PubApp"`). The XML must have been loaded via
 * [`int2dds_load_profiles`]. Endpoints carry `DynamicData` and are fetched by
 * their XML name with the accessors below. Destroy with
 * [`int2dds_configured_participant_destroy`].
 *
 * # Safety
 * - `path` must be a valid, null-terminated UTF-8 C string
 * - `out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_create_participant_from_config(const struct Int2DdsParticipantFactory *_factory,
                                                  const char *path,
                                                  struct Int2DdsConfiguredParticipant **out);

/**
 * Get the datawriter declared as `"<publisher>::<writer>"` from a configured
 * tree. Returns `INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND` when no such writer
 * exists. The returned handle must be freed with `int2dds_dynamic_writer_destroy`.
 *
 * # Safety
 * - `configured` must be a handle from `int2dds_create_participant_from_config`
 * - `name` must be a valid, null-terminated UTF-8 C string
 * - `out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_configured_participant_get_datawriter(const struct Int2DdsConfiguredParticipant *configured,
                                                         const char *name,
                                                         struct Int2DdsDynamicDataWriter **out);

/**
 * Get the datareader declared as `"<subscriber>::<reader>"` from a configured
 * tree. Returns `INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND` when no such reader
 * exists. The returned handle must be freed with `int2dds_dynamic_reader_destroy`.
 *
 * # Safety
 * - `configured` must be a handle from `int2dds_create_participant_from_config`
 * - `name` must be a valid, null-terminated UTF-8 C string
 * - `out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_configured_participant_get_datareader(const struct Int2DdsConfiguredParticipant *configured,
                                                         const char *name,
                                                         struct Int2DdsDynamicDataReader **out);

/**
 * Destroy a configured-participant handle and fully tear down its tree. Safe to
 * call with null. This drops the tree's owned publishers/subscribers/topics,
 * then deletes the participant's contained entities and removes the participant
 * from the factory (equivalent to `delete_contained_entities` +
 * `delete_participant`). Destroy any datawriter/datareader handles obtained via
 * the accessors above BEFORE calling this.
 *
 * # Safety
 * - `configured` must be null or a handle from
 *   `int2dds_create_participant_from_config`, not used after this call
 */
void int2dds_configured_participant_destroy(struct Int2DdsConfiguredParticipant *configured);

/**
 * Get the DomainParticipantFactory singleton instance
 *
 * # Safety
 * - `factory_out` must be a valid pointer to a null pointer
 * - The returned factory must be freed with `int2dds_domain_participant_factory_finalize`
 */
Int2DdsRet int2dds_domain_participant_factory_get_instance(struct Int2DdsParticipantFactory **factory_out);

/**
 * Finalize the DomainParticipantFactory and free resources
 *
 * # Safety
 * - `factory` must be a valid factory created by `int2dds_domain_participant_factory_get_instance`
 * - `factory` must not be used after this call
 */
Int2DdsRet int2dds_domain_participant_factory_finalize(struct Int2DdsParticipantFactory *factory);

/**
 * Look up an existing participant on `domain_id`.
 *
 * On success `*participant_out` receives a NEW handle aliasing the existing
 * core participant (freed independently with `int2dds_delete_participant`). If
 * no participant exists on that domain, returns `INT2DDS_RET_OK` with
 * `*participant_out` set to null.
 *
 * # Safety
 * - `participant_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_domain_participant_factory_lookup_participant(const struct Int2DdsParticipantFactory *_factory,
                                                                 int32_t domain_id,
                                                                 struct Int2DdsParticipant **participant_out);

/**
 * Set the factory's `autoenable_created_entities` policy (the only member of
 * DomainParticipantFactoryQos).
 *
 * # Safety
 * - none beyond a valid process; the factory handle is ignored (singleton).
 */
Int2DdsRet int2dds_domain_participant_factory_set_qos(const struct Int2DdsParticipantFactory *_factory,
                                                      bool autoenable_created_entities);

/**
 * Get the factory's `autoenable_created_entities` policy.
 *
 * # Safety
 * - `autoenable_out` must be a valid pointer
 */
Int2DdsRet int2dds_domain_participant_factory_get_qos(const struct Int2DdsParticipantFactory *_factory,
                                                      bool *autoenable_out);

/**
 * Set the factory's default participant QoS (used when a participant is created
 * with default QoS). Pass a QoS handle, or null to reset to the built-in
 * default.
 *
 * # Safety
 * - `qos`, if non-null, must be a valid participant QoS handle
 */
Int2DdsRet int2dds_domain_participant_factory_set_default_participant_qos(const struct Int2DdsParticipantFactory *_factory,
                                                                          const struct Int2DdsParticipantQos *qos);

/**
 * Get the factory's default participant QoS. On success `*qos_out` receives a
 * new handle the caller frees with `int2dds_participant_qos_destroy`.
 *
 * # Safety
 * - `qos_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_domain_participant_factory_get_default_participant_qos(const struct Int2DdsParticipantFactory *_factory,
                                                                          struct Int2DdsParticipantQos **qos_out);

/**
 * Get discovered participant instance handles.
 *
 * Writes up to `capacity` handles into `handles_out`.
 * `count_out` receives the total number of discovered participants
 * (may be greater than `capacity`).
 */
Int2DdsRet int2dds_participant_get_discovered_participants(const struct Int2DdsParticipant *participant,
                                                           uint8_t (*handles_out)[16],
                                                           uintptr_t capacity,
                                                           uintptr_t *count_out);

/**
 * Get matched subscription instance handles for a DataWriter.
 */
Int2DdsRet int2dds_datawriter_get_matched_subscriptions(const struct Int2DdsDataWriter *writer,
                                                        uint8_t (*handles_out)[16],
                                                        uintptr_t capacity,
                                                        uintptr_t *count_out);

/**
 * Get matched publication instance handles for a DataReader.
 */
Int2DdsRet int2dds_datareader_get_matched_publications(const struct Int2DdsDataReader *reader,
                                                       uint8_t (*handles_out)[16],
                                                       uintptr_t capacity,
                                                       uintptr_t *count_out);

/**
 * Collect a snapshot of discovered publications via the builtin DCPSPublication reader.
 */
Int2DdsRet int2dds_participant_take_discovered_publications_snapshot(const struct Int2DdsParticipant *participant,
                                                                     int32_t timeout_ms,
                                                                     struct Int2DdsPublicationBuiltinTopicDataSeq **seq_out);

/**
 * Collect a snapshot of discovered publications restricted to the given instance states.
 * `instance_state_mask` takes `INT2DDS_INSTANCE_STATE_*` values combined with a bitwise or.
 */
Int2DdsRet int2dds_participant_take_discovered_publications_snapshot_filtered(const struct Int2DdsParticipant *participant,
                                                                              int32_t timeout_ms,
                                                                              uint32_t instance_state_mask,
                                                                              struct Int2DdsPublicationBuiltinTopicDataSeq **seq_out);

/**
 * Instance state of the entry at `index`, as an `INT2DDS_INSTANCE_STATE_*` value.
 */
Int2DdsRet int2dds_publication_builtin_topic_data_seq_get_instance_state(const struct Int2DdsPublicationBuiltinTopicDataSeq *seq,
                                                                         uintptr_t index,
                                                                         uint32_t *instance_state_out);

/**
 * Instance handle of the entry at `index`, which is the endpoint GUID.
 * Present even for an entry with no announcement to read a GUID out of.
 */
Int2DdsRet int2dds_publication_builtin_topic_data_seq_get_instance_handle(const struct Int2DdsPublicationBuiltinTopicDataSeq *seq,
                                                                          uintptr_t index,
                                                                          uint8_t (*handle_out)[16]);

Int2DdsRet int2dds_publication_builtin_topic_data_seq_length(const struct Int2DdsPublicationBuiltinTopicDataSeq *seq,
                                                             uintptr_t *count_out);

Int2DdsRet int2dds_publication_builtin_topic_data_seq_get(const struct Int2DdsPublicationBuiltinTopicDataSeq *seq,
                                                          uintptr_t index,
                                                          struct Int2DdsPublicationBuiltinTopicData **data_out);

Int2DdsRet int2dds_publication_builtin_topic_data_seq_delete(struct Int2DdsPublicationBuiltinTopicDataSeq *seq);

/**
 * Collect a snapshot of discovered subscriptions via the builtin DCPSSubscription reader.
 */
Int2DdsRet int2dds_participant_take_discovered_subscriptions_snapshot(const struct Int2DdsParticipant *participant,
                                                                      int32_t timeout_ms,
                                                                      struct Int2DdsSubscriptionBuiltinTopicDataSeq **seq_out);

/**
 * Collect a snapshot of discovered subscriptions restricted to the given instance states.
 * See `int2dds_participant_take_discovered_publications_snapshot_filtered` for the mask.
 */
Int2DdsRet int2dds_participant_take_discovered_subscriptions_snapshot_filtered(const struct Int2DdsParticipant *participant,
                                                                               int32_t timeout_ms,
                                                                               uint32_t instance_state_mask,
                                                                               struct Int2DdsSubscriptionBuiltinTopicDataSeq **seq_out);

/**
 * Instance state of the entry at `index`, as an `INT2DDS_INSTANCE_STATE_*` value.
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_seq_get_instance_state(const struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq,
                                                                          uintptr_t index,
                                                                          uint32_t *instance_state_out);

/**
 * Instance handle of the entry at `index`, which is the endpoint GUID.
 * See `int2dds_publication_builtin_topic_data_seq_get_instance_handle`.
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_seq_get_instance_handle(const struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq,
                                                                           uintptr_t index,
                                                                           uint8_t (*handle_out)[16]);

Int2DdsRet int2dds_subscription_builtin_topic_data_seq_length(const struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq,
                                                              uintptr_t *count_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_seq_get(const struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq,
                                                           uintptr_t index,
                                                           struct Int2DdsSubscriptionBuiltinTopicData **data_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_seq_delete(struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq);

/**
 * Get discovered participant data for a given handle.
 * On success, `*data_out` receives a heap-allocated opaque pointer.
 * The caller must free it with `int2dds_participant_builtin_topic_data_destroy`.
 */
Int2DdsRet int2dds_participant_get_discovered_participant_data(const struct Int2DdsParticipant *participant,
                                                               const uint8_t (*handle)[16],
                                                               struct Int2DdsParticipantBuiltinTopicData **data_out);

/**
 * Get matched subscription data for a given handle.
 * On success, `*data_out` receives a heap-allocated opaque pointer.
 * The caller must free it with `int2dds_subscription_builtin_topic_data_destroy`.
 */
Int2DdsRet int2dds_datawriter_get_matched_subscription_data(const struct Int2DdsDataWriter *writer,
                                                            const uint8_t (*handle)[16],
                                                            struct Int2DdsSubscriptionBuiltinTopicData **data_out);

/**
 * Get matched publication data for a given handle.
 * On success, `*data_out` receives a heap-allocated opaque pointer.
 * The caller must free it with `int2dds_publication_builtin_topic_data_destroy`.
 */
Int2DdsRet int2dds_datareader_get_matched_publication_data(const struct Int2DdsDataReader *reader,
                                                           const uint8_t (*handle)[16],
                                                           struct Int2DdsPublicationBuiltinTopicData **data_out);

/**
 * Get the key from a ParticipantBuiltinTopicData.
 * `key_out` must point to a 12-byte buffer.
 */
Int2DdsRet int2dds_participant_builtin_topic_data_get_key(const struct Int2DdsParticipantBuiltinTopicData *data,
                                                          uint8_t (*key_out)[12]);

/**
 * Get the user_data from a ParticipantBuiltinTopicData.
 * Copies up to `capacity` bytes into `buf`. `size_out` receives the actual size.
 */
Int2DdsRet int2dds_participant_builtin_topic_data_get_user_data(const struct Int2DdsParticipantBuiltinTopicData *data,
                                                                uint8_t *buf,
                                                                uintptr_t capacity,
                                                                uintptr_t *size_out);

/**
 * Free a ParticipantBuiltinTopicData obtained from discovery.
 */
Int2DdsRet int2dds_participant_builtin_topic_data_destroy(struct Int2DdsParticipantBuiltinTopicData *data);

/**
 * Get the key from a PublicationBuiltinTopicData.
 * `key_out` must point to a 12-byte buffer.
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_key(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                          uint8_t (*key_out)[12]);

/**
 * Get the endpoint GUID from a PublicationBuiltinTopicData.
 * `guid_out` must point to a 16-byte buffer.
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_endpoint_guid(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                    uint8_t (*guid_out)[16]);

/**
 * Get the participant key from a PublicationBuiltinTopicData.
 * `key_out` must point to a 12-byte buffer.
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_participant_key(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                      uint8_t (*key_out)[12]);

/**
 * Get the topic name from a PublicationBuiltinTopicData.
 * Copies a null-terminated UTF-8 string into `buf`.
 * `size_out` receives the required size (including null terminator).
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_topic_name(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                 uint8_t *buf,
                                                                 uintptr_t capacity,
                                                                 uintptr_t *size_out);

/**
 * Get the type name from a PublicationBuiltinTopicData.
 * Copies a null-terminated UTF-8 string into `buf`.
 * `size_out` receives the required size (including null terminator).
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_type_name(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                uint8_t *buf,
                                                                uintptr_t capacity,
                                                                uintptr_t *size_out);

/**
 * Take a clone of the TypeObject embedded in a PublicationBuiltinTopicData.
 * Caller owns the returned handle and must destroy it via `int2dds_type_object_destroy`.
 * Returns DYNAMIC_FIELD_NOT_FOUND if the publication did not carry a TypeObject.
 */
Int2DdsRet int2dds_publication_builtin_topic_data_take_type_object(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                   struct Int2DdsTypeObject **out);

/**
 * Get the reliability kind from a PublicationBuiltinTopicData.
 * `kind_out`: 0 = BEST_EFFORT, 1 = RELIABLE (matches INT2DDS_QOS_RELIABILITY_*).
 *
 * # Safety
 * - `data` must be a valid PublicationBuiltinTopicData
 * - `kind_out` must be a valid pointer
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_reliability_kind(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                       int32_t *kind_out);

/**
 * Get the durability kind from a PublicationBuiltinTopicData.
 * `kind_out`: 0 = VOLATILE, 1 = TRANSIENT_LOCAL, 2 = TRANSIENT, 3 = PERSISTENT.
 *
 * # Safety
 * - `data` must be a valid PublicationBuiltinTopicData
 * - `kind_out` must be a valid pointer
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_durability_kind(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                      int32_t *kind_out);

/**
 * Get the liveliness kind from a PublicationBuiltinTopicData.
 * `kind_out`: 0 = AUTOMATIC, 1 = MANUAL_BY_PARTICIPANT, 2 = MANUAL_BY_TOPIC.
 *
 * # Safety
 * - `data` must be a valid PublicationBuiltinTopicData
 * - `kind_out` must be a valid pointer
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_liveliness_kind(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                      int32_t *kind_out);

/**
 * Get the liveliness lease duration from a PublicationBuiltinTopicData.
 * An infinite duration is reported as (0x7fffffff, 0x7fffffff).
 *
 * # Safety
 * - `data` must be a valid PublicationBuiltinTopicData
 * - `sec_out` and `nanosec_out` must be valid pointers
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_liveliness_lease_duration(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                                int32_t *sec_out,
                                                                                uint32_t *nanosec_out);

/**
 * Get the deadline period from a PublicationBuiltinTopicData.
 * An infinite duration is reported as (0x7fffffff, 0x7fffffff).
 *
 * # Safety
 * - `data` must be a valid PublicationBuiltinTopicData
 * - `sec_out` and `nanosec_out` must be valid pointers
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_deadline(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                               int32_t *sec_out,
                                                               uint32_t *nanosec_out);

/**
 * Get the lifespan duration from a PublicationBuiltinTopicData.
 * An infinite duration is reported as (0x7fffffff, 0x7fffffff).
 *
 * # Safety
 * - `data` must be a valid PublicationBuiltinTopicData
 * - `sec_out` and `nanosec_out` must be valid pointers
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_lifespan(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                               int32_t *sec_out,
                                                               uint32_t *nanosec_out);

/**
 * Get the user_data from a PublicationBuiltinTopicData.
 * Copies up to `capacity` bytes into `buf`. `size_out` receives the actual size.
 *
 * # Safety
 * - `data` must be a valid PublicationBuiltinTopicData
 * - `buf` must be valid for `capacity` bytes, or null to query the size only
 * - `size_out` must be a valid pointer
 */
Int2DdsRet int2dds_publication_builtin_topic_data_get_user_data(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                uint8_t *buf,
                                                                uintptr_t capacity,
                                                                uintptr_t *size_out);

/**
 * Free a PublicationBuiltinTopicData obtained from discovery.
 */
Int2DdsRet int2dds_publication_builtin_topic_data_destroy(struct Int2DdsPublicationBuiltinTopicData *data);

/**
 * Get the key from a SubscriptionBuiltinTopicData.
 * `key_out` must point to a 12-byte buffer.
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_key(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                           uint8_t (*key_out)[12]);

/**
 * Get the endpoint GUID from a SubscriptionBuiltinTopicData.
 * `guid_out` must point to a 16-byte buffer.
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_endpoint_guid(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                     uint8_t (*guid_out)[16]);

/**
 * Get the participant key from a SubscriptionBuiltinTopicData.
 * `key_out` must point to a 12-byte buffer.
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_participant_key(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                       uint8_t (*key_out)[12]);

/**
 * Get the topic name from a SubscriptionBuiltinTopicData.
 * Copies a null-terminated UTF-8 string into `buf`.
 * `size_out` receives the required size (including null terminator).
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_topic_name(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                  uint8_t *buf,
                                                                  uintptr_t capacity,
                                                                  uintptr_t *size_out);

/**
 * Get the type name from a SubscriptionBuiltinTopicData.
 * Copies a null-terminated UTF-8 string into `buf`.
 * `size_out` receives the required size (including null terminator).
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_type_name(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                 uint8_t *buf,
                                                                 uintptr_t capacity,
                                                                 uintptr_t *size_out);

/**
 * Get the reliability kind from a SubscriptionBuiltinTopicData.
 * `kind_out`: 0 = BEST_EFFORT, 1 = RELIABLE (matches INT2DDS_QOS_RELIABILITY_*).
 *
 * # Safety
 * - `data` must be a valid SubscriptionBuiltinTopicData
 * - `kind_out` must be a valid pointer
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_reliability_kind(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                        int32_t *kind_out);

/**
 * Get the durability kind from a SubscriptionBuiltinTopicData.
 * `kind_out`: 0 = VOLATILE, 1 = TRANSIENT_LOCAL, 2 = TRANSIENT, 3 = PERSISTENT.
 *
 * # Safety
 * - `data` must be a valid SubscriptionBuiltinTopicData
 * - `kind_out` must be a valid pointer
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_durability_kind(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                       int32_t *kind_out);

/**
 * Get the liveliness kind from a SubscriptionBuiltinTopicData.
 * `kind_out`: 0 = AUTOMATIC, 1 = MANUAL_BY_PARTICIPANT, 2 = MANUAL_BY_TOPIC.
 *
 * # Safety
 * - `data` must be a valid SubscriptionBuiltinTopicData
 * - `kind_out` must be a valid pointer
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_liveliness_kind(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                       int32_t *kind_out);

/**
 * Get the liveliness lease duration from a SubscriptionBuiltinTopicData.
 * An infinite duration is reported as (0x7fffffff, 0x7fffffff).
 *
 * # Safety
 * - `data` must be a valid SubscriptionBuiltinTopicData
 * - `sec_out` and `nanosec_out` must be valid pointers
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_liveliness_lease_duration(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                                 int32_t *sec_out,
                                                                                 uint32_t *nanosec_out);

/**
 * Get the deadline period from a SubscriptionBuiltinTopicData.
 * An infinite duration is reported as (0x7fffffff, 0x7fffffff).
 *
 * # Safety
 * - `data` must be a valid SubscriptionBuiltinTopicData
 * - `sec_out` and `nanosec_out` must be valid pointers
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_deadline(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                int32_t *sec_out,
                                                                uint32_t *nanosec_out);

/**
 * Get the user_data from a SubscriptionBuiltinTopicData.
 * Copies up to `capacity` bytes into `buf`. `size_out` receives the actual size.
 *
 * # Safety
 * - `data` must be a valid SubscriptionBuiltinTopicData
 * - `buf` must be valid for `capacity` bytes, or null to query the size only
 * - `size_out` must be a valid pointer
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_get_user_data(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                 uint8_t *buf,
                                                                 uintptr_t capacity,
                                                                 uintptr_t *size_out);

/**
 * Free a SubscriptionBuiltinTopicData obtained from discovery.
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_destroy(struct Int2DdsSubscriptionBuiltinTopicData *data);

/**
 * Register a callback for remote endpoint discovery events. `callback` must be
 * non-null; to disable, register a no-op callback (or destroy the participant)
 * before freeing `ctx`.
 */
Int2DdsRet int2dds_participant_set_endpoint_discovery_callback(const struct Int2DdsParticipant *participant,
                                                               Int2DdsEndpointDiscoveryCallback callback,
                                                               void *ctx);

/**
 * Get the builtin subscriber for discovery topics.
 */
Int2DdsRet int2dds_participant_get_builtin_subscriber(const struct Int2DdsParticipant *participant,
                                                      struct Int2DdsSubscriber **out);

/**
 * Take one DCPSPublication discovery sample, optionally filtered by topic
 * name. Blocks up to `timeout_ms` milliseconds (negative = infinite). Returns
 * DYNAMIC_TIMEOUT on no match. Destroy the result via
 * `int2dds_publication_builtin_topic_data_destroy`.
 */
Int2DdsRet int2dds_subscriber_take_publication_data(const struct Int2DdsSubscriber *builtin_sub,
                                                    const char *topic_name_filter,
                                                    int32_t timeout_ms,
                                                    struct Int2DdsPublicationBuiltinTopicData **out);

/**
 * High-level helper: wait until a publication for `topic_name` is discovered
 * AND its TypeObject is present, then return the TypeObject and type name.
 */
Int2DdsRet int2dds_participant_wait_for_type_object(const struct Int2DdsParticipant *participant,
                                                    const char *topic_name,
                                                    int32_t timeout_ms,
                                                    struct Int2DdsTypeObject **type_obj_out,
                                                    char *type_name_buf,
                                                    uintptr_t type_name_buf_len,
                                                    uintptr_t *out_len);

/**
 * Destroy a TypeObject handle. Safe to call with null.
 */
void int2dds_type_object_destroy(struct Int2DdsTypeObject *t);

/**
 * Return extensibility: 0 = Final, 1 = Appendable, 2 = Mutable.
 */
Int2DdsRet int2dds_type_object_extensibility(const struct Int2DdsTypeObject *t, int32_t *out);

/**
 * Return the number of struct members.
 */
Int2DdsRet int2dds_type_object_member_count(const struct Int2DdsTypeObject *t, uint32_t *out);

/**
 * Fill `out` with member info at `index`.
 */
Int2DdsRet int2dds_type_object_member_info(const struct Int2DdsTypeObject *t,
                                           uint32_t index,
                                           struct Int2DdsMemberInfo *out);

/**
 * Copy member name at `index` into `buf`.
 */
Int2DdsRet int2dds_type_object_member_name(const struct Int2DdsTypeObject *t,
                                           uint32_t index,
                                           char *buf,
                                           uintptr_t buf_len,
                                           uintptr_t *out_len);

/**
 * Find a member index by name.
 */
Int2DdsRet int2dds_type_object_find_member(const struct Int2DdsTypeObject *t,
                                           const char *name,
                                           uint32_t *index_out);

/**
 * Register a topic backed by a discovered TypeObject. The TypeObject is cloned
 * internally; the caller still owns and must destroy `type_obj`.
 */
Int2DdsRet int2dds_create_topic_with_type_object(const struct Int2DdsParticipant *participant,
                                                 const char *topic_name,
                                                 const char *type_name,
                                                 const struct Int2DdsTypeObject *type_obj,
                                                 const struct Int2DdsTopicQos *qos,
                                                 struct Int2DdsTopic **out);

/**
 * Read a char8 field (returned as its byte value) from a flat sample.
 */
Int2DdsRet int2dds_dynamic_sample_get_char8(const uint8_t *bytes,
                                            uintptr_t len,
                                            const struct Int2DdsTypeObject *type_obj,
                                            const char *field_name,
                                            uint8_t *out);

/**
 * Read a string field by name from a flat sample.
 */
Int2DdsRet int2dds_dynamic_sample_get_string(const uint8_t *bytes,
                                             uintptr_t len,
                                             const struct Int2DdsTypeObject *type_obj,
                                             const char *field_name,
                                             char *out_buf,
                                             uintptr_t buf_cap,
                                             uintptr_t *out_len);

Int2DdsRet int2dds_dynamic_data_from_sample(const struct Int2DdsParticipant *participant,
                                            const uint8_t *bytes,
                                            uintptr_t len,
                                            const struct Int2DdsTypeObject *type_obj,
                                            struct Int2DdsDynamicData **out);

/**
 * Destroy a DynamicData handle. Safe to call with null.
 */
void int2dds_dynamic_data_destroy(struct Int2DdsDynamicData *d);

/**
 * Read a char8 field (as its byte value) at `field_path`.
 */
Int2DdsRet int2dds_dynamic_data_get_char8(const struct Int2DdsDynamicData *data,
                                          const char *field_path,
                                          uint8_t *out);

/**
 * Read a string field at `field_path` into a caller-supplied buffer.
 */
Int2DdsRet int2dds_dynamic_data_get_string(const struct Int2DdsDynamicData *data,
                                           const char *field_path,
                                           char *out_buf,
                                           uintptr_t buf_cap,
                                           uintptr_t *out_len);

/**
 * Get the element count of a sequence/array field at `field_path`.
 */
Int2DdsRet int2dds_dynamic_data_get_len(const struct Int2DdsDynamicData *data,
                                        const char *field_path,
                                        uintptr_t *out);

/**
 * Extract a nested struct value at `field_path` as a new DynamicData handle.
 * Destroy it with `int2dds_dynamic_data_destroy`.
 */
Int2DdsRet int2dds_dynamic_data_get_member(const struct Int2DdsDynamicData *data,
                                           const char *field_path,
                                           struct Int2DdsDynamicData **out);

/**
 * Destroy a dynamic type support handle. Safe to call with null.
 */
void int2dds_dynamic_type_support_destroy(struct Int2DdsDynamicTypeSupport *s);

/**
 * Register a topic backed by a dynamic type support. The support's full type
 * closure is advertised during discovery.
 */
Int2DdsRet int2dds_create_topic_dynamic(const struct Int2DdsParticipant *participant,
                                        const char *topic_name,
                                        const struct Int2DdsDynamicTypeSupport *type_support,
                                        const struct Int2DdsTopicQos *qos,
                                        struct Int2DdsTopic **out);

/**
 * Create a dynamic DataWriter. Pass null `qos` to use the default.
 */
Int2DdsRet int2dds_create_datawriter_dynamic(const struct Int2DdsPublisher *publisher,
                                             const struct Int2DdsTopic *topic,
                                             const struct Int2DdsDynamicTypeSupport *type_support,
                                             const struct Int2DdsDataWriterQos *qos,
                                             struct Int2DdsDynamicDataWriter **out);

/**
 * Create a dynamic DataReader. Pass null `qos` to use the default.
 */
Int2DdsRet int2dds_create_datareader_dynamic(const struct Int2DdsSubscriber *subscriber,
                                             const struct Int2DdsTopic *topic,
                                             const struct Int2DdsDynamicTypeSupport *type_support,
                                             const struct Int2DdsDataReaderQos *qos,
                                             struct Int2DdsDynamicDataReader **out);

/**
 * Destroy a dynamic DataWriter handle. Safe to call with null.
 *
 * Mirrors `int2dds_delete_datawriter`: unregisters the writer from its
 * parent publisher so the publisher/participant can be deleted afterward.
 * Best-effort, since a void destructor cannot report a failure code; the
 * FFI wrapper is freed either way.
 */
void int2dds_dynamic_writer_destroy(struct Int2DdsDynamicDataWriter *w);

/**
 * Destroy a dynamic DataReader handle. Safe to call with null.
 *
 * Mirrors `int2dds_delete_datareader`: unregisters the reader from its
 * parent subscriber so the subscriber/participant can be deleted afterward.
 * Best-effort, since a void destructor cannot report a failure code; the
 * FFI wrapper is freed either way.
 */
void int2dds_dynamic_reader_destroy(struct Int2DdsDynamicDataReader *r);

/**
 * Get the effective QoS of a dynamic DataWriter.
 *
 * A dynamic writer handle is `Int2DdsDynamicDataWriter`, a different type from the
 * typed `Int2DdsDataWriter`, so `int2dds_datawriter_get_qos` cannot be used on it.
 * The QoS handle written to `qos_out` is the *same* `Int2DdsDataWriterQos` type the
 * typed path returns, so every `int2dds_datawriter_qos_get_*` accessor applies.
 * The caller owns it and must release it with `int2dds_datawriter_qos_destroy`.
 *
 * # Safety
 *
 * `writer` must be a valid handle from `int2dds_create_datawriter_dynamic` and
 * `qos_out` must point to writable storage for one pointer.
 */
Int2DdsRet int2dds_dynamic_writer_get_qos(const struct Int2DdsDynamicDataWriter *writer,
                                          struct Int2DdsDataWriterQos **qos_out);

/**
 * Get the effective QoS of a dynamic DataReader.
 *
 * Counterpart to `int2dds_dynamic_writer_get_qos`. The handle written to `qos_out`
 * is the same `Int2DdsDataReaderQos` type the typed path returns, so every
 * `int2dds_datareader_qos_get_*` accessor applies. The caller owns it and must
 * release it with `int2dds_datareader_qos_destroy`.
 *
 * # Safety
 *
 * `reader` must be a valid handle from `int2dds_create_datareader_dynamic` and
 * `qos_out` must point to writable storage for one pointer.
 */
Int2DdsRet int2dds_dynamic_reader_get_qos(const struct Int2DdsDynamicDataReader *reader,
                                          struct Int2DdsDataReaderQos **qos_out);

/**
 * Current number of DataReaders matched to this dynamic writer.
 */
Int2DdsRet int2dds_dynamic_writer_publication_matched_count(const struct Int2DdsDynamicDataWriter *writer,
                                                            int32_t *out);

/**
 * Current number of DataWriters matched to this dynamic reader.
 */
Int2DdsRet int2dds_dynamic_reader_subscription_matched_count(const struct Int2DdsDynamicDataReader *reader,
                                                             int32_t *out);

/**
 * Create an empty, writable DynamicData for the given type support.
 * Populate it with the `int2dds_dynamic_data_set_*` setters, then publish via
 * `int2dds_dynamic_writer_write`. Destroy with `int2dds_dynamic_data_destroy`.
 */
Int2DdsRet int2dds_dynamic_data_create(const struct Int2DdsDynamicTypeSupport *type_support,
                                       struct Int2DdsDynamicData **out);

/**
 * Set a char8 field (given as its byte value) on a top-level field.
 */
Int2DdsRet int2dds_dynamic_data_set_char8(struct Int2DdsDynamicData *data,
                                          const char *field,
                                          uint8_t value);

/**
 * Set a string field on a top-level field. `value` must be null-terminated UTF-8.
 */
Int2DdsRet int2dds_dynamic_data_set_string(struct Int2DdsDynamicData *data,
                                           const char *field,
                                           const char *value);

/**
 * Publish a populated DynamicData sample.
 */
Int2DdsRet int2dds_dynamic_writer_write(const struct Int2DdsDynamicDataWriter *writer,
                                        const struct Int2DdsDynamicData *data);

/**
 * Take the next available DynamicData sample. On success `out_data` receives a
 * new DynamicData handle (destroy with `int2dds_dynamic_data_destroy`) and, if
 * non-null, `out_info` receives the sample info. Returns INT2DDS_RET_NO_DATA
 * when no valid sample is available.
 */
Int2DdsRet int2dds_dynamic_reader_take(const struct Int2DdsDynamicDataReader *reader,
                                       struct Int2DdsDynamicData **out_data,
                                       struct Int2DdsSampleInfo *out_info);

/**
 * Construct a char8 value from its byte value.
 */
Int2DdsRet int2dds_dynamic_value_char8(uint8_t value, struct Int2DdsDynamicValue **out);

/**
 * Construct a UTF-8 string value. `value` must be null-terminated UTF-8.
 */
Int2DdsRet int2dds_dynamic_value_string(const char *value, struct Int2DdsDynamicValue **out);

/**
 * Construct a wide-string value. `value` must be null-terminated UTF-8.
 */
Int2DdsRet int2dds_dynamic_value_wstring(const char *value, struct Int2DdsDynamicValue **out);

/**
 * Construct an enum value from its literal name and numeric value. The name may
 * be empty when only the numeric value is known.
 */
Int2DdsRet int2dds_dynamic_value_enum(const char *name,
                                      int32_t value,
                                      struct Int2DdsDynamicValue **out);

/**
 * Construct a nested struct value by cloning a DynamicData instance.
 */
Int2DdsRet int2dds_dynamic_value_struct(const struct Int2DdsDynamicData *data,
                                        struct Int2DdsDynamicValue **out);

/**
 * Construct an empty sequence value. Append elements with
 * `int2dds_dynamic_value_push`.
 */
Int2DdsRet int2dds_dynamic_value_sequence(struct Int2DdsDynamicValue **out);

/**
 * Construct an empty array value. Append elements with
 * `int2dds_dynamic_value_push`.
 */
Int2DdsRet int2dds_dynamic_value_array(struct Int2DdsDynamicValue **out);

/**
 * Construct an empty map value. Add entries with
 * `int2dds_dynamic_value_map_insert`.
 */
Int2DdsRet int2dds_dynamic_value_map(struct Int2DdsDynamicValue **out);

/**
 * Construct a union value from a discriminator and the selected branch value.
 * Both inputs are consumed on success.
 */
Int2DdsRet int2dds_dynamic_value_union(struct Int2DdsDynamicValue *discriminator,
                                       struct Int2DdsDynamicValue *value,
                                       struct Int2DdsDynamicValue **out);

/**
 * Append `element` to a sequence/array value. Consumes `element` on success;
 * on error `element` is left owned by the caller.
 */
Int2DdsRet int2dds_dynamic_value_push(struct Int2DdsDynamicValue *collection,
                                      struct Int2DdsDynamicValue *element);

/**
 * Insert a key/value pair into a map value. Consumes `key` and `value` on
 * success; on error both are left owned by the caller.
 */
Int2DdsRet int2dds_dynamic_value_map_insert(struct Int2DdsDynamicValue *map,
                                            struct Int2DdsDynamicValue *key,
                                            struct Int2DdsDynamicValue *value);

/**
 * Destroy a value handle. Safe to call with null.
 */
void int2dds_dynamic_value_destroy(struct Int2DdsDynamicValue *value);

/**
 * Set a top-level field to `value`. Consumes `value` on a successful parse of
 * `field` (regardless of whether the field exists); returns the field handle to
 * the caller only when `field` is not valid UTF-8.
 */
Int2DdsRet int2dds_dynamic_data_set_value(struct Int2DdsDynamicData *data,
                                          const char *field,
                                          struct Int2DdsDynamicValue *value);

/**
 * Clone the value at a dotted/indexed `path` into a new value handle. Destroy
 * it with `int2dds_dynamic_value_destroy`.
 */
Int2DdsRet int2dds_dynamic_data_get_value(const struct Int2DdsDynamicData *data,
                                          const char *path,
                                          struct Int2DdsDynamicValue **out);

/**
 * Report the kind of a value (one of the `INT2DDS_VALUE_KIND_*` constants).
 */
Int2DdsRet int2dds_dynamic_value_kind(const struct Int2DdsDynamicValue *value, int32_t *out);

/**
 * Read a char8 value as its byte value.
 */
Int2DdsRet int2dds_dynamic_value_as_char8(const struct Int2DdsDynamicValue *value, uint8_t *out);

/**
 * Read a string or wide-string value into `buf`.
 */
Int2DdsRet int2dds_dynamic_value_as_string(const struct Int2DdsDynamicValue *value,
                                           char *buf,
                                           uintptr_t buf_len,
                                           uintptr_t *out_len);

/**
 * Format any value as a human-readable string into `buf`, regardless of kind
 * (mirrors the core `Display`).
 */
Int2DdsRet int2dds_dynamic_value_to_string(const struct Int2DdsDynamicValue *value,
                                           char *buf,
                                           uintptr_t buf_len,
                                           uintptr_t *out_len);

/**
 * Read an enum value's literal name into `buf` and its numeric value into
 * `out_value`.
 */
Int2DdsRet int2dds_dynamic_value_as_enum(const struct Int2DdsDynamicValue *value,
                                         char *buf,
                                         uintptr_t buf_len,
                                         uintptr_t *out_len,
                                         int32_t *out_value);

/**
 * Read a bitmask value's packed bits.
 */
Int2DdsRet int2dds_dynamic_value_as_bitmask(const struct Int2DdsDynamicValue *value, uint64_t *out);

/**
 * Read a bitset value's packed bitfields.
 */
Int2DdsRet int2dds_dynamic_value_as_bitset(const struct Int2DdsDynamicValue *value, uint64_t *out);

/**
 * Element count of a sequence/array/map value.
 */
Int2DdsRet int2dds_dynamic_value_len(const struct Int2DdsDynamicValue *value, uintptr_t *out);

/**
 * Clone the element at `index` of a sequence/array value.
 */
Int2DdsRet int2dds_dynamic_value_element(const struct Int2DdsDynamicValue *value,
                                         uintptr_t index,
                                         struct Int2DdsDynamicValue **out);

/**
 * Clone the key of the map entry at `index`.
 */
Int2DdsRet int2dds_dynamic_value_map_key(const struct Int2DdsDynamicValue *value,
                                         uintptr_t index,
                                         struct Int2DdsDynamicValue **out);

/**
 * Clone the value of the map entry at `index`.
 */
Int2DdsRet int2dds_dynamic_value_map_value(const struct Int2DdsDynamicValue *value,
                                           uintptr_t index,
                                           struct Int2DdsDynamicValue **out);

/**
 * Clone a nested struct value into a new DynamicData handle. Destroy it with
 * `int2dds_dynamic_data_destroy`.
 */
Int2DdsRet int2dds_dynamic_value_as_struct(const struct Int2DdsDynamicValue *value,
                                           struct Int2DdsDynamicData **out);

/**
 * Clone a union value's discriminator into a new value handle.
 */
Int2DdsRet int2dds_dynamic_value_union_discriminator(const struct Int2DdsDynamicValue *value,
                                                     struct Int2DdsDynamicValue **out);

/**
 * Clone a union value's selected branch value into a new value handle.
 */
Int2DdsRet int2dds_dynamic_value_union_value(const struct Int2DdsDynamicValue *value,
                                             struct Int2DdsDynamicValue **out);

/**
 * Sets the IPv4 multicast TTL fallback via the `INT2DDS_MULTICAST_TTL`
 * environment variable.
 *
 * Used by `TransportConfig` only when a `DomainParticipantQos` does not carry an
 * explicit `int2dds.transport.UDPv4.multicast_ttl` property entry, so explicit
 * QoS settings always win.
 *
 * # Safety
 * Call before the first `DomainParticipant` is created. Mutating process
 * environment from threads other than the main one is undefined behavior on
 * some platforms.
 */
Int2DdsRet int2dds_env_set_multicast_ttl(uint8_t ttl);

/**
 * Reads the current `INT2DDS_MULTICAST_TTL` env override.
 *
 * On success writes the parsed TTL into `*ttl_out` and sets `*has_value_out`
 * to `true`. When the variable is unset, empty, or invalid (non-`u8`),
 * `*has_value_out` is set to `false` and `*ttl_out` is left untouched.
 *
 * # Safety
 * `ttl_out` and `has_value_out` must be valid, writable pointers.
 */
Int2DdsRet int2dds_env_get_multicast_ttl(uint8_t *ttl_out, bool *has_value_out);

/**
 * Sets the QoS profile file path(s) to auto-load, via the `DDS_QOS_PROFILE`
 * environment variable. The `DomainParticipantFactory` singleton auto-loads
 * these when it first initializes (on the first participant creation); the
 * `*_QOS_DEFAULT` resolution then draws QoS from the selected default profile.
 *
 * Multiple paths may be joined with `,` (also `;` on Windows / `:` on Unix).
 *
 * # Safety
 * - `path` must be a valid, null-terminated UTF-8 C string.
 * - Call before the first `DomainParticipant` is created so the factory
 *   singleton picks it up when it initializes.
 */
Int2DdsRet int2dds_env_set_qos_profile(const char *path);

/**
 * Selects the default QoS profile (`"Library::Profile"`) via the
 * `DDS_DEFAULT_QOS_PROFILE` environment variable. The `*_QOS_DEFAULT`
 * resolution reads this at entity-creation time, so `NULL`-QoS creators
 * (participant/publisher/writer with default QoS) draw from this profile.
 *
 * # Safety
 * - `profile` must be a valid, null-terminated UTF-8 C string
 *   (e.g. `"HelloWorldDataFrag::Reliable"`).
 */
Int2DdsRet int2dds_env_set_default_qos_profile(const char *profile);

/**
 * Copy the calling thread's last error message (UTF-8, NUL-terminated) into `buf`.
 * Returns the full message byte length, excluding the NUL.
 *
 * `buf` null or `buf_len <= 0`: query mode, writes nothing, returns the length.
 * Message longer than `buf_len - 1`: truncated at a UTF-8 boundary, still
 * returns the full (pre-truncation) length. No message: writes "" and returns 0.
 *
 * # Safety
 * `buf` must be null or point to at least `buf_len` writable bytes.
 */
int32_t int2dds_last_error_message(char *buf, int32_t buf_len);

/**
 * Create a DomainParticipant
 *
 * # Safety
 * - `qos` can be null for the default QoS (engages the core resolution chain:
 *   registered default → configured default profile → spec default); a non-null
 *   handle is cloned internally and remains owned by the caller
 * - `participant_out` must be a valid pointer to a null pointer
 * - The returned participant must be freed with `int2dds_delete_participant`
 */
Int2DdsRet int2dds_create_participant(const struct Int2DdsParticipantFactory *_factory,
                                      int32_t domain_id,
                                      const struct Int2DdsParticipantQos *qos,
                                      struct Int2DdsParticipant **participant_out);

/**
 * Create a DomainParticipant using a QoS profile path
 *
 * # Safety
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `participant_out` must be a valid pointer to a null pointer
 * - The returned participant must be freed with `int2dds_delete_participant`
 */
Int2DdsRet int2dds_create_participant_with_profile(const struct Int2DdsParticipantFactory *_factory,
                                                   int32_t domain_id,
                                                   const char *qos_path,
                                                   struct Int2DdsParticipant **participant_out);

/**
 * Delete a DomainParticipant
 *
 * # Safety
 * - `participant` must be a valid participant created by `int2dds_create_participant`
 * - `participant` must not be used after this call
 * - All entities created by this participant must be deleted first
 */
Int2DdsRet int2dds_delete_participant(struct Int2DdsParticipant *participant);

/**
 * Assert liveliness for a participant (for MANUAL_BY_PARTICIPANT liveliness)
 *
 * # Safety
 * - `participant` must be a valid participant
 */
Int2DdsRet int2dds_participant_assert_liveliness(const struct Int2DdsParticipant *participant);

/**
 * Get the domain ID of a participant
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `domain_id_out` must be a valid pointer
 */
Int2DdsRet int2dds_participant_get_domain_id(const struct Int2DdsParticipant *participant,
                                             int32_t *domain_id_out);

/**
 * Set the QoS of a participant.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `qos` must be a valid QoS created by `int2dds_participant_qos_create_default`
 */
Int2DdsRet int2dds_participant_set_qos(const struct Int2DdsParticipant *participant,
                                       const struct Int2DdsParticipantQos *qos);

/**
 * Get the QoS of a participant.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `qos_out` must be a valid pointer to a null pointer
 * - The returned QoS must be freed with `int2dds_participant_qos_destroy`
 */
Int2DdsRet int2dds_participant_get_qos(const struct Int2DdsParticipant *participant,
                                       struct Int2DdsParticipantQos **qos_out);

/**
 * Delete all entities contained by a participant
 *
 * This operation deletes all Publisher, Subscriber, Topic, ContentFilteredTopic
 * and MultiTopic objects created through this participant. It recursively calls
 * delete_contained_entities on each contained entity.
 *
 * # Safety
 * - `participant` must be a valid participant
 */
Int2DdsRet int2dds_participant_delete_contained_entities(const struct Int2DdsParticipant *participant);

/**
 * Get the participant's current wall-clock time.
 *
 * `sec` is a Unix timestamp (seconds since the epoch); `nanosec` is the
 * sub-second remainder.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `sec_out` and `nanosec_out` must be valid pointers
 */
Int2DdsRet int2dds_participant_get_current_time(const struct Int2DdsParticipant *participant,
                                                int32_t *sec_out,
                                                uint32_t *nanosec_out);

/**
 * Test whether an entity (Publisher/Subscriber/Topic and their children) with
 * the given 16-byte instance handle belongs to this participant.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `handle` must point to a 16-byte instance handle
 * - `result_out` must be a valid pointer
 */
Int2DdsRet int2dds_participant_contains_entity(const struct Int2DdsParticipant *participant,
                                               const uint8_t (*handle)[16],
                                               bool *result_out);

/**
 * Find an existing local Topic by name, blocking up to `timeout_ms` for it to
 * appear (a negative value blocks indefinitely — avoid it if the topic may
 * never exist). `dds_type_name` labels the returned handle for the raw path and
 * must match the caller's expected type.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `topic_name` and `dds_type_name` must be valid null-terminated C strings
 * - `topic_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_participant_find_topic(const struct Int2DdsParticipant *participant,
                                          const char *topic_name,
                                          const char *dds_type_name,
                                          int32_t timeout_ms,
                                          struct Int2DdsTopic **topic_out);

/**
 * Create a Publisher
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `qos` can be null for the default QoS (engages the core resolution chain:
 *   registered default → configured default profile → spec default)
 * - `publisher_out` must be a valid pointer to a null pointer
 * - The returned publisher must be freed with `int2dds_delete_publisher`
 */
Int2DdsRet int2dds_create_publisher(const struct Int2DdsParticipant *participant,
                                    const struct Int2DdsPublisherQos *qos,
                                    struct Int2DdsPublisher **publisher_out);

/**
 * Create a Publisher using a QoS profile path
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `publisher_out` must be a valid pointer to a null pointer
 * - The returned publisher must be freed with `int2dds_delete_publisher`
 */
Int2DdsRet int2dds_create_publisher_with_profile(const struct Int2DdsParticipant *participant,
                                                 const char *qos_path,
                                                 struct Int2DdsPublisher **publisher_out);

/**
 * Set QoS on a Publisher
 *
 * Applies new QoS policies to an existing Publisher. Some policies can only
 * be changed before the entity is enabled; attempting to change immutable
 * policies on an enabled entity returns IMMUTABLE_POLICY.
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `qos` must be a valid publisher QoS handle
 */
Int2DdsRet int2dds_publisher_set_qos(const struct Int2DdsPublisher *publisher,
                                     const struct Int2DdsPublisherQos *qos);

/**
 * Get QoS from a Publisher
 *
 * Returns a new QoS handle containing the current QoS policies of the Publisher.
 * The returned handle must be freed with `int2dds_publisher_qos_destroy`.
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `qos_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_publisher_get_qos(const struct Int2DdsPublisher *publisher,
                                     struct Int2DdsPublisherQos **qos_out);

/**
 * Get the 16-byte instance handle of a Publisher.
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `handle_out` must point to a 16-byte buffer
 */
Int2DdsRet int2dds_publisher_get_instance_handle(const struct Int2DdsPublisher *publisher,
                                                 uint8_t (*handle_out)[16]);

/**
 * Set QoS on a DataWriter
 *
 * Applies new QoS policies to an existing DataWriter. Some policies can only
 * be changed before the entity is enabled; attempting to change immutable
 * policies on an enabled entity returns IMMUTABLE_POLICY.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `qos` must be a valid datawriter QoS handle
 */
Int2DdsRet int2dds_datawriter_set_qos(const struct Int2DdsDataWriter *writer,
                                      const struct Int2DdsDataWriterQos *qos);

/**
 * Get QoS from a DataWriter
 *
 * Returns a new QoS handle containing the current QoS policies of the DataWriter.
 * The returned handle must be freed with `int2dds_datawriter_qos_destroy`.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `qos_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_datawriter_get_qos(const struct Int2DdsDataWriter *writer,
                                      struct Int2DdsDataWriterQos **qos_out);

/**
 * Effective data representation of a DataWriter (`INT2DDS_QOS_DATA_REPR_*`).
 *
 * Resolves an unset (empty) DataRepresentation QoS to the library default, so
 * generated serializers can encode exactly what the writer advertises over
 * discovery. Falls back to the library default if the writer is null or its
 * QoS cannot be read.
 *
 * # Safety
 * - `writer` must be null or a valid datawriter
 */
int32_t int2dds_datawriter_data_representation(const struct Int2DdsDataWriter *writer);

/**
 * Get the 16-byte RTPS GUID of a DataWriter.
 *
 * Writes the writer's endpoint GUID (the same value advertised over SEDP
 * discovery as `endpoint_guid`) into `guid_out`. Read-only.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `guid_out` must be a valid pointer to a 16-byte buffer
 */
Int2DdsRet int2dds_datawriter_get_guid(const struct Int2DdsDataWriter *writer,
                                       uint8_t (*guid_out)[16]);

/**
 * Delete a Publisher
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `publisher` must not be used after this call
 * - All DataWriters created by this publisher must be deleted first
 */
Int2DdsRet int2dds_delete_publisher(struct Int2DdsPublisher *publisher);

/**
 * Create a DataWriter
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `topic` must be a valid topic
 * - `qos` can be null for default QoS
 * - `listener` can be null for no listener; `mask` specifies which status
 *   changes trigger callbacks (pass 0 with a null listener)
 * - `writer_out` must be a valid pointer to a null pointer
 * - The returned writer must be freed with `int2dds_delete_datawriter`
 * - Listener callbacks must be thread-safe and remain valid until writer is deleted
 */
Int2DdsRet int2dds_create_datawriter(const struct Int2DdsPublisher *publisher,
                                     const struct Int2DdsTopic *topic,
                                     const struct Int2DdsDataWriterQos *qos,
                                     const struct Int2DdsDataWriterListener *listener,
                                     uint32_t mask,
                                     struct Int2DdsDataWriter **writer_out);

/**
 * Create a DataWriter using a QoS profile path
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `topic` must be a valid topic
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `listener` can be null for no listener; `mask` specifies which status
 *   changes trigger callbacks (pass 0 with a null listener)
 * - `writer_out` must be a valid pointer to a null pointer
 * - The returned writer must be freed with `int2dds_delete_datawriter`
 * - Listener callbacks must be thread-safe and remain valid until writer is deleted
 */
Int2DdsRet int2dds_create_datawriter_with_profile(const struct Int2DdsPublisher *publisher,
                                                  const struct Int2DdsTopic *topic,
                                                  const char *qos_path,
                                                  const struct Int2DdsDataWriterListener *listener,
                                                  uint32_t mask,
                                                  struct Int2DdsDataWriter **writer_out);

/**
 * Set or update the listener for a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `listener` can be null to remove the listener
 * - `mask` specifies which status changes trigger callbacks
 * - Listener callbacks must be thread-safe
 */
Int2DdsRet int2dds_datawriter_set_listener(struct Int2DdsDataWriter *writer,
                                           const struct Int2DdsDataWriterListener *listener,
                                           uint32_t mask);

/**
 * Get the current listener from a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `listener_out` must be a valid pointer to Int2DdsDataWriterListener
 * - Returns a copy of the listener callbacks and user context
 */
Int2DdsRet int2dds_datawriter_get_listener(const struct Int2DdsDataWriter *writer,
                                           struct Int2DdsDataWriterListener *listener_out);

/**
 * Delete a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `writer` must not be used after this call
 */
Int2DdsRet int2dds_delete_datawriter(struct Int2DdsDataWriter *writer);

/**
 * Get publication matched status for a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datawriter_get_publication_matched_status(const struct Int2DdsDataWriter *writer,
                                                             struct Int2DdsPublicationMatchedStatus *status_out);

/**
 * Get liveliness lost status for a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datawriter_get_liveliness_lost_status(const struct Int2DdsDataWriter *writer,
                                                         struct Int2DdsLivelinessLostStatus *status_out);

/**
 * Get offered deadline missed status for a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datawriter_get_offered_deadline_missed_status(const struct Int2DdsDataWriter *writer,
                                                                 struct Int2DdsOfferedDeadlineMissedStatus *status_out);

/**
 * Get offered incompatible QoS status for a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datawriter_get_offered_incompatible_qos_status(const struct Int2DdsDataWriter *writer,
                                                                  struct Int2DdsOfferedIncompatibleQosStatus *status_out);

/**
 * Get offered incompatible type status for a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datawriter_get_offered_incompatible_type_status(const struct Int2DdsDataWriter *writer,
                                                                   struct Int2DdsOfferedIncompatibleTypeStatus *status_out);

/**
 * Delete all entities contained by a publisher
 *
 * This operation deletes all DataWriter objects contained by this Publisher.
 *
 * # Safety
 * - `publisher` must be a valid publisher
 */
Int2DdsRet int2dds_publisher_delete_contained_entities(const struct Int2DdsPublisher *publisher);

/**
 * Write pre-serialized data to a DataWriter, bypassing TypeSupport serialization.
 *
 * The caller is responsible for CDR-serializing the data (including the
 * encapsulation header) before calling this function.
 *
 * # Parameters
 * - `writer`: A valid datawriter
 * - `data`: Pointer to the CDR-serialized byte buffer
 * - `data_len`: Length of the serialized data in bytes
 *
 * The instance key and KeyHash are derived canonically from `data`
 * (the full serialized sample).
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `data` must point to at least `data_len` readable bytes
 */
Int2DdsRet int2dds_datawriter_write_serialized(const struct Int2DdsDataWriter *writer,
                                               const uint8_t *data,
                                               uintptr_t data_len);

Int2DdsRet int2dds_datawriter_prepare_serialized_write(const struct Int2DdsDataWriter *writer,
                                                       uintptr_t capacity,
                                                       uint8_t **data_out,
                                                       uintptr_t *capacity_out,
                                                       struct Int2DdsSerializedWriteLoan **loan_out);

/**
 * Commit a staged serialized write.
 *
 * # Ownership
 * On success the loan is consumed and `loan` is freed — do not use or abort it.
 * On failure the loan handle is left valid: the caller must release it with
 * `int2dds_datawriter_abort_serialized_write` (or discard it after a subsequent successful
 * path). This makes the common binding idiom (`commit`; on error `abort`)
 * memory-safe rather than a double-free.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `loan` must be a valid loan from `int2dds_datawriter_prepare_serialized_write` that has
 *   not already been committed or aborted
 */
Int2DdsRet int2dds_datawriter_commit_serialized_write(const struct Int2DdsDataWriter *writer,
                                                      struct Int2DdsSerializedWriteLoan *loan,
                                                      uintptr_t actual_size);

Int2DdsRet int2dds_datawriter_abort_serialized_write(struct Int2DdsSerializedWriteLoan *loan);

/**
 * Write pre-serialized data with an explicit source timestamp.
 *
 * Same as `int2dds_datawriter_write_serialized`, but allows the caller to specify a
 * source timestamp instead of using the current time.
 *
 * # Parameters
 * - `writer`: A valid datawriter
 * - `data`: Pointer to the CDR-serialized byte buffer
 * - `data_len`: Length of the serialized data in bytes
 * - `timestamp_sec`: Seconds component of the source timestamp
 * - `timestamp_nanosec`: Nanoseconds component of the source timestamp
 *
 * The instance key and KeyHash are derived canonically from `data`
 * (the full serialized sample).
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `data` must point to at least `data_len` readable bytes
 */
Int2DdsRet int2dds_datawriter_write_serialized_w_timestamp(const struct Int2DdsDataWriter *writer,
                                                           const uint8_t *data,
                                                           uintptr_t data_len,
                                                           int32_t timestamp_sec,
                                                           uint32_t timestamp_nanosec);

/**
 * Block until all reliable DataReader entities have acknowledged all written data,
 * or until the timeout expires.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 */
Int2DdsRet int2dds_datawriter_wait_for_acknowledgments(const struct Int2DdsDataWriter *writer,
                                                       int64_t timeout_ms);

/**
 * Block until all reliable DataWriter entities owned by this Publisher have been
 * acknowledged by all matched reliable DataReader entities, or until the timeout expires.
 *
 * # Safety
 * - `publisher` must be a valid publisher
 */
Int2DdsRet int2dds_publisher_wait_for_acknowledgments(const struct Int2DdsPublisher *publisher,
                                                      int64_t timeout_ms);

/**
 * Register an instance from the full serialized sample. The canonical KeyHash is
 * derived from the sample by the core (matching the wire); `key`/`key_len` carry
 * the serialized sample bytes.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `key` must point to at least `key_len` readable bytes
 * - `handle_out` must be a valid pointer to a 16-byte array
 */
Int2DdsRet int2dds_datawriter_register_instance(const struct Int2DdsDataWriter *writer,
                                                const uint8_t *key,
                                                uintptr_t key_len,
                                                uint8_t (*handle_out)[16]);

/**
 * Dispose an instance from the full serialized sample. `key`/`key_len` carry the
 * serialized sample bytes (not a pre-serialized key); the core derives the canonical
 * KeyHash from them. A keyed topic must have been created with a full TypeObject
 * (int2dds_create_topic_with_type_info / _with_field_descriptors), otherwise this
 * returns INT2DDS_RET_PRECONDITION_NOT_MET.
 *
 * `key` may be null when `handle` carries a valid instance handle (e.g. from
 * `int2dds_datawriter_register_instance`); the serialized key is then looked up
 * from the writer's instance registry. A null `key` with a nil/unknown handle
 * returns INT2DDS_RET_BAD_PARAMETER.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - If `key` is not null, it must point to at least `key_len` readable bytes
 * - `handle` must be a valid pointer to a 16-byte instance handle (or null for NIL)
 */
Int2DdsRet int2dds_datawriter_dispose(const struct Int2DdsDataWriter *writer,
                                      const uint8_t *key,
                                      uintptr_t key_len,
                                      const uint8_t (*handle)[16]);

/**
 * Unregister an instance from the full serialized sample. `key`/`key_len` carry the
 * serialized sample bytes (not a pre-serialized key); the core derives the canonical
 * KeyHash from them. A keyed topic must have been created with a full TypeObject
 * (int2dds_create_topic_with_type_info / _with_field_descriptors), otherwise this
 * returns INT2DDS_RET_PRECONDITION_NOT_MET.
 *
 * `key` may be null when `handle` carries a valid instance handle (e.g. from
 * `int2dds_datawriter_register_instance`); the serialized key is then looked up
 * from the writer's instance registry. A null `key` with a nil/unknown handle
 * returns INT2DDS_RET_BAD_PARAMETER.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - If `key` is not null, it must point to at least `key_len` readable bytes
 * - `handle` must be a valid pointer to a 16-byte instance handle (or null for NIL)
 */
Int2DdsRet int2dds_datawriter_unregister_instance(const struct Int2DdsDataWriter *writer,
                                                  const uint8_t *key,
                                                  uintptr_t key_len,
                                                  const uint8_t (*handle)[16]);

/**
 * Lookup an instance handle from the full serialized sample. `key`/`key_len` carry the
 * serialized sample bytes (not a pre-serialized key); the core derives the canonical
 * KeyHash from them. A keyed topic must have been created with a full TypeObject
 * (int2dds_create_topic_with_type_info / _with_field_descriptors), otherwise this
 * returns INT2DDS_RET_PRECONDITION_NOT_MET.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `key` must point to at least `key_len` readable bytes
 * - `handle_out` must be a valid pointer to a 16-byte array
 */
Int2DdsRet int2dds_datawriter_lookup_instance(const struct Int2DdsDataWriter *writer,
                                              const uint8_t *key,
                                              uintptr_t key_len,
                                              uint8_t (*handle_out)[16]);

/**
 * Get key value for an instance handle
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `handle` must be a valid pointer to a 16-byte instance handle
 * - `key_buf` must point to at least `key_capacity` writable bytes
 * - `key_size_out` must be a valid pointer
 */
Int2DdsRet int2dds_datawriter_get_key_value(const struct Int2DdsDataWriter *writer,
                                            const uint8_t (*handle)[16],
                                            uint8_t *key_buf,
                                            uintptr_t key_capacity,
                                            uintptr_t *key_size_out);

/**
 * Assert liveliness for a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 */
Int2DdsRet int2dds_datawriter_assert_liveliness(const struct Int2DdsDataWriter *writer);

/**
 * Create default DataWriter QoS
 *
 * # Safety
 * - `qos_out` must be a valid pointer to a null pointer
 * - The returned QoS must be freed with `int2dds_datawriter_qos_destroy`
 */
Int2DdsRet int2dds_datawriter_qos_create_default(struct Int2DdsDataWriterQos **qos_out);

/**
 * Set reliability QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid reliability kind
 */
Int2DdsRet int2dds_datawriter_qos_set_reliability(struct Int2DdsDataWriterQos *qos,
                                                  int32_t kind,
                                                  int64_t max_blocking_time_ns);

/**
 * Set durability QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid durability kind
 */
Int2DdsRet int2dds_datawriter_qos_set_durability(struct Int2DdsDataWriterQos *qos, int32_t kind);

/**
 * Set history QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid history kind
 * - For KEEP_LAST, `depth` must be > 0
 */
Int2DdsRet int2dds_datawriter_qos_set_history(struct Int2DdsDataWriterQos *qos,
                                              int32_t kind,
                                              int32_t depth);

/**
 * Set data representation QoS for DataWriter
 *
 * Controls which encoding is advertised in DDS discovery.
 * - `INT2DDS_QOS_DATA_REPR_XCDR1` (0): XCDR1 — for FINAL extensibility
 * - `INT2DDS_QOS_DATA_REPR_XCDR2` (2): XCDR2 — for APPENDABLE/MUTABLE extensibility
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid data representation kind
 */
Int2DdsRet int2dds_datawriter_qos_set_data_representation(struct Int2DdsDataWriterQos *qos,
                                                          int32_t kind);

/**
 * Set ownership QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind`: 0=Shared, 1=Exclusive
 */
Int2DdsRet int2dds_datawriter_qos_set_ownership(struct Int2DdsDataWriterQos *qos, int32_t kind);

/**
 * Set ownership strength QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_set_ownership_strength(struct Int2DdsDataWriterQos *qos,
                                                         int32_t value);

/**
 * Set the DataFrag QoS (per-writer RTPS DATA_FRAG fragment size, in bytes) for
 * DataWriter. Resolved when the writer is created: values `> 65000` are clamped
 * to 65000, and `<= 0` means unspecified, falling back to the
 * `INT2DDS_DATA_FRAG_SIZE` environment variable and then to 65000.
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_set_data_frag(struct Int2DdsDataWriterQos *qos, int32_t value);

/**
 * Set resource limits QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_set_resource_limits(struct Int2DdsDataWriterQos *qos,
                                                      int32_t max_samples,
                                                      int32_t max_instances,
                                                      int32_t max_per_instance);

/**
 * Set lifespan QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_set_lifespan(struct Int2DdsDataWriterQos *qos,
                                               int64_t duration_ns);

/**
 * Set destination order QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind`: 0=ByReceptionTimestamp, 1=BySourceTimestamp
 */
Int2DdsRet int2dds_datawriter_qos_set_destination_order(struct Int2DdsDataWriterQos *qos,
                                                        int32_t kind);

/**
 * Set latency budget QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_set_latency_budget(struct Int2DdsDataWriterQos *qos,
                                                     int64_t duration_ns);

/**
 * Set transport priority QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_set_transport_priority(struct Int2DdsDataWriterQos *qos,
                                                         int32_t priority);

/**
 * Set user data QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `data` must point to `data_len` bytes, or be null if `data_len` is 0
 */
Int2DdsRet int2dds_datawriter_qos_set_user_data(struct Int2DdsDataWriterQos *qos,
                                                const uint8_t *data,
                                                uintptr_t data_len);

/**
 * Set writer data lifecycle QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_set_writer_data_lifecycle(struct Int2DdsDataWriterQos *qos,
                                                            bool autodispose);

/**
 * Get reliability QoS from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_reliability(const struct Int2DdsDataWriterQos *qos,
                                                  int32_t *kind_out,
                                                  int64_t *max_blocking_time_ns_out);

/**
 * Get durability QoS from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_durability(const struct Int2DdsDataWriterQos *qos,
                                                 int32_t *kind_out);

/**
 * Get history QoS from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_history(const struct Int2DdsDataWriterQos *qos,
                                              int32_t *kind_out,
                                              int32_t *depth_out);

/**
 * Get ownership QoS from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_ownership(const struct Int2DdsDataWriterQos *qos,
                                                int32_t *kind_out);

/**
 * Get ownership strength from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_ownership_strength(const struct Int2DdsDataWriterQos *qos,
                                                         int32_t *value_out);

/**
 * Get the DataFrag QoS (fragment size, in bytes) from a DataWriter QoS handle.
 * Returns the value as set, not the resolved size: `0` means unspecified, so the
 * writer will use `INT2DDS_DATA_FRAG_SIZE` if set and 65000 otherwise.
 */
Int2DdsRet int2dds_datawriter_qos_get_data_frag(const struct Int2DdsDataWriterQos *qos,
                                                int32_t *value_out);

/**
 * Get resource limits from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_resource_limits(const struct Int2DdsDataWriterQos *qos,
                                                      int32_t *max_samples_out,
                                                      int32_t *max_instances_out,
                                                      int32_t *max_per_instance_out);

/**
 * Get lifespan from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_lifespan(const struct Int2DdsDataWriterQos *qos,
                                               int64_t *duration_ns_out);

/**
 * Get destination order from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_destination_order(const struct Int2DdsDataWriterQos *qos,
                                                        int32_t *kind_out);

/**
 * Get deadline from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_deadline(const struct Int2DdsDataWriterQos *qos,
                                               int64_t *period_ns_out);

/**
 * Get liveliness from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_liveliness(const struct Int2DdsDataWriterQos *qos,
                                                 int32_t *kind_out,
                                                 int64_t *lease_duration_ns_out);

/**
 * Get data representation from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_data_representation(const struct Int2DdsDataWriterQos *qos,
                                                          int32_t *kind_out);

/**
 * Returns the library's default data representation (`INT2DDS_QOS_DATA_REPR_*`)
 * used when an application creates an endpoint without setting one explicitly.
 *
 * Single source of truth for language bindings: instead of hardcoding XCDR1,
 * bindings should query this so a change to the Rust core default propagates
 * automatically to what they serialize and advertise.
 */
int32_t int2dds_default_data_representation(void);

/**
 * Returns the library's default type extensibility (`0` = Final, `1` = Appendable,
 * `2` = Mutable) applied when a type does not declare one explicitly.
 *
 * Single source of truth for language bindings: instead of hardcoding Appendable,
 * bindings should query this so a change to the Rust core default (the
 * `ExtensibilityKind` enum default) propagates automatically to how they frame
 * serialized samples (DHEADER presence) and advertise types.
 */
int32_t int2dds_default_extensibility(void);

/**
 * Get transport priority from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_transport_priority(const struct Int2DdsDataWriterQos *qos,
                                                         int32_t *value_out);

/**
 * Get latency budget from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_latency_budget(const struct Int2DdsDataWriterQos *qos,
                                                     int64_t *duration_ns_out);

/**
 * Get writer data lifecycle from DataWriter QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_get_writer_data_lifecycle(const struct Int2DdsDataWriterQos *qos,
                                                            bool *autodispose_out);

/**
 * Destroy DataWriter QoS
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `qos` must not be used after this call
 */
Int2DdsRet int2dds_datawriter_qos_destroy(struct Int2DdsDataWriterQos *qos);

/**
 * Create default DataReader QoS
 *
 * # Safety
 * - `qos_out` must be a valid pointer to a null pointer
 * - The returned QoS must be freed with `int2dds_datareader_qos_destroy`
 */
Int2DdsRet int2dds_datareader_qos_create_default(struct Int2DdsDataReaderQos **qos_out);

/**
 * Set reliability QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid reliability kind
 */
Int2DdsRet int2dds_datareader_qos_set_reliability(struct Int2DdsDataReaderQos *qos,
                                                  int32_t kind,
                                                  int64_t max_blocking_time_ns);

/**
 * Set durability QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid durability kind
 */
Int2DdsRet int2dds_datareader_qos_set_durability(struct Int2DdsDataReaderQos *qos, int32_t kind);

/**
 * Set history QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid history kind
 * - For KEEP_LAST, `depth` must be > 0
 */
Int2DdsRet int2dds_datareader_qos_set_history(struct Int2DdsDataReaderQos *qos,
                                              int32_t kind,
                                              int32_t depth);

/**
 * Set data representation QoS for DataReader
 *
 * Controls which encoding is advertised in DDS discovery.
 * - `INT2DDS_QOS_DATA_REPR_XCDR1` (0): XCDR1 — for FINAL extensibility
 * - `INT2DDS_QOS_DATA_REPR_XCDR2` (2): XCDR2 — for APPENDABLE/MUTABLE extensibility
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid data representation kind
 */
Int2DdsRet int2dds_datareader_qos_set_data_representation(struct Int2DdsDataReaderQos *qos,
                                                          int32_t kind);

/**
 * Set ownership QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind`: 0=Shared, 1=Exclusive
 */
Int2DdsRet int2dds_datareader_qos_set_ownership(struct Int2DdsDataReaderQos *qos, int32_t kind);

/**
 * Set resource limits QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datareader_qos_set_resource_limits(struct Int2DdsDataReaderQos *qos,
                                                      int32_t max_samples,
                                                      int32_t max_instances,
                                                      int32_t max_per_instance);

/**
 * Set destination order QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind`: 0=ByReceptionTimestamp, 1=BySourceTimestamp
 */
Int2DdsRet int2dds_datareader_qos_set_destination_order(struct Int2DdsDataReaderQos *qos,
                                                        int32_t kind);

/**
 * Set time-based filter QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datareader_qos_set_time_based_filter(struct Int2DdsDataReaderQos *qos,
                                                        int64_t minimum_separation_ns);

/**
 * Set latency budget QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datareader_qos_set_latency_budget(struct Int2DdsDataReaderQos *qos,
                                                     int64_t duration_ns);

/**
 * Set user data QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `data` must point to `data_len` bytes, or be null if `data_len` is 0
 */
Int2DdsRet int2dds_datareader_qos_set_user_data(struct Int2DdsDataReaderQos *qos,
                                                const uint8_t *data,
                                                uintptr_t data_len);

/**
 * Set reader data lifecycle QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datareader_qos_set_reader_data_lifecycle(struct Int2DdsDataReaderQos *qos,
                                                            int64_t autopurge_nowriter_ns,
                                                            int64_t autopurge_disposed_ns);

Int2DdsRet int2dds_datareader_qos_get_reliability(const struct Int2DdsDataReaderQos *qos,
                                                  int32_t *kind_out,
                                                  int64_t *max_blocking_time_ns_out);

Int2DdsRet int2dds_datareader_qos_get_durability(const struct Int2DdsDataReaderQos *qos,
                                                 int32_t *kind_out);

Int2DdsRet int2dds_datareader_qos_get_history(const struct Int2DdsDataReaderQos *qos,
                                              int32_t *kind_out,
                                              int32_t *depth_out);

Int2DdsRet int2dds_datareader_qos_get_ownership(const struct Int2DdsDataReaderQos *qos,
                                                int32_t *kind_out);

Int2DdsRet int2dds_datareader_qos_get_resource_limits(const struct Int2DdsDataReaderQos *qos,
                                                      int32_t *max_samples_out,
                                                      int32_t *max_instances_out,
                                                      int32_t *max_per_instance_out);

Int2DdsRet int2dds_datareader_qos_get_destination_order(const struct Int2DdsDataReaderQos *qos,
                                                        int32_t *kind_out);

Int2DdsRet int2dds_datareader_qos_set_lifespan_reference(struct Int2DdsDataReaderQos *qos,
                                                         int32_t kind);

Int2DdsRet int2dds_datareader_qos_get_lifespan_reference(const struct Int2DdsDataReaderQos *qos,
                                                         int32_t *kind_out);

Int2DdsRet int2dds_datareader_qos_get_deadline(const struct Int2DdsDataReaderQos *qos,
                                               int64_t *period_ns_out);

Int2DdsRet int2dds_datareader_qos_get_liveliness(const struct Int2DdsDataReaderQos *qos,
                                                 int32_t *kind_out,
                                                 int64_t *lease_duration_ns_out);

Int2DdsRet int2dds_datareader_qos_get_data_representation(const struct Int2DdsDataReaderQos *qos,
                                                          int32_t *kind_out);

Int2DdsRet int2dds_datareader_qos_get_latency_budget(const struct Int2DdsDataReaderQos *qos,
                                                     int64_t *duration_ns_out);

Int2DdsRet int2dds_datareader_qos_get_time_based_filter(const struct Int2DdsDataReaderQos *qos,
                                                        int64_t *min_separation_ns_out);

Int2DdsRet int2dds_datareader_qos_get_reader_data_lifecycle(const struct Int2DdsDataReaderQos *qos,
                                                            int64_t *autopurge_nowriter_ns_out,
                                                            int64_t *autopurge_disposed_ns_out);

/**
 * Destroy DataReader QoS
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `qos` must not be used after this call
 */
Int2DdsRet int2dds_datareader_qos_destroy(struct Int2DdsDataReaderQos *qos);

/**
 * Create default Topic QoS
 *
 * # Safety
 * - `qos_out` must be a valid pointer to a null pointer
 * - The returned QoS must be freed with `int2dds_topic_qos_destroy`
 */
Int2DdsRet int2dds_topic_qos_create_default(struct Int2DdsTopicQos **qos_out);

/**
 * Set reliability QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid reliability kind
 */
Int2DdsRet int2dds_topic_qos_set_reliability(struct Int2DdsTopicQos *qos,
                                             int32_t kind,
                                             int64_t max_blocking_time_ns);

/**
 * Set durability QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid durability kind
 */
Int2DdsRet int2dds_topic_qos_set_durability(struct Int2DdsTopicQos *qos, int32_t kind);

/**
 * Set history QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid history kind
 * - For KEEP_LAST, `depth` must be > 0
 */
Int2DdsRet int2dds_topic_qos_set_history(struct Int2DdsTopicQos *qos, int32_t kind, int32_t depth);

/**
 * Set deadline QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_topic_qos_set_deadline(struct Int2DdsTopicQos *qos, int64_t period_ns);

/**
 * Set liveliness QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid liveliness kind (0=Automatic, 1=ManualByParticipant, 2=ManualByTopic)
 */
Int2DdsRet int2dds_topic_qos_set_liveliness(struct Int2DdsTopicQos *qos,
                                            int32_t kind,
                                            int64_t lease_duration_ns);

/**
 * Set destination order QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind`: 0=ByReceptionTimestamp, 1=BySourceTimestamp
 */
Int2DdsRet int2dds_topic_qos_set_destination_order(struct Int2DdsTopicQos *qos, int32_t kind);

/**
 * Set resource limits QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_topic_qos_set_resource_limits(struct Int2DdsTopicQos *qos,
                                                 int32_t max_samples,
                                                 int32_t max_instances,
                                                 int32_t max_per_instance);

/**
 * Set transport priority QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_topic_qos_set_transport_priority(struct Int2DdsTopicQos *qos, int32_t priority);

/**
 * Set lifespan QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_topic_qos_set_lifespan(struct Int2DdsTopicQos *qos, int64_t duration_ns);

/**
 * Set ownership QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind`: 0=Shared, 1=Exclusive
 */
Int2DdsRet int2dds_topic_qos_set_ownership(struct Int2DdsTopicQos *qos, int32_t kind);

/**
 * Set data representation QoS for Topic
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid data representation kind
 */
Int2DdsRet int2dds_topic_qos_set_data_representation(struct Int2DdsTopicQos *qos, int32_t kind);

/**
 * Destroy Topic QoS
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `qos` must not be used after this call
 */
Int2DdsRet int2dds_topic_qos_destroy(struct Int2DdsTopicQos *qos);

/**
 * Create default DomainParticipant QoS
 *
 * # Safety
 * - `qos_out` must be a valid pointer to a null pointer
 * - The returned QoS must be freed with `int2dds_participant_qos_destroy`
 */
Int2DdsRet int2dds_participant_qos_create_default(struct Int2DdsParticipantQos **qos_out);

/**
 * Set user data QoS for DomainParticipant
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `data` must point to `data_len` bytes, or be null if `data_len` is 0
 */
Int2DdsRet int2dds_participant_qos_set_user_data(struct Int2DdsParticipantQos *qos,
                                                 const uint8_t *data,
                                                 uintptr_t data_len);

/**
 * Add or overwrite a text property by name (PropertyQosPolicy).
 *
 * # Safety
 * `qos`, `name`, `value` must be valid non-null C strings.
 */
Int2DdsRet int2dds_participant_qos_add_property(struct Int2DdsParticipantQos *qos,
                                                const char *name,
                                                const char *value,
                                                bool propagate);

/**
 * Add or overwrite a binary property by name (PropertyQosPolicy).
 *
 * # Safety
 * `qos`, `name` must be valid non-null. `data` may be null when `data_len == 0`.
 */
Int2DdsRet int2dds_participant_qos_add_binary_property(struct Int2DdsParticipantQos *qos,
                                                       const char *name,
                                                       const uint8_t *data,
                                                       uintptr_t data_len,
                                                       bool propagate);

/**
 * Lookup a text property by name. The caller provides a `out_buf` of `out_cap`
 * bytes; the value is written without a NUL terminator and `*out_len` is set to
 * the number of bytes the value occupies. If the buffer is too small,
 * `INT2DDS_RET_BUFFER_TOO_SMALL` is returned with `*out_len` populated so the
 * caller can resize and retry.
 *
 * # Safety
 * `qos`, `name`, `out_len` must be non-null. `out_buf` may be null only when
 * `out_cap == 0` (size-probe call).
 */
Int2DdsRet int2dds_participant_qos_find_property(const struct Int2DdsParticipantQos *qos,
                                                 const char *name,
                                                 char *out_buf,
                                                 uintptr_t out_cap,
                                                 uintptr_t *out_len);

/**
 * Remove a text property by name.
 *
 * Returns `INT2DDS_RET_NO_DATA` when the name is not present.
 *
 * # Safety
 * `qos`, `name` must be valid non-null.
 */
Int2DdsRet int2dds_participant_qos_remove_property(struct Int2DdsParticipantQos *qos,
                                                   const char *name);

/**
 * Iterate text properties whose name starts with `prefix`. The callback
 * receives NUL-terminated `name` and `value` borrowed for the call duration —
 * callers must not retain the pointers. Returning a non-zero value from the
 * callback aborts iteration early.
 *
 * # Safety
 * `qos`, `prefix`, `cb` must be valid non-null. `user_data` is opaque.
 */
Int2DdsRet int2dds_participant_qos_get_properties_with_prefix(const struct Int2DdsParticipantQos *qos,
                                                              const char *prefix,
                                                              int32_t (*cb)(const char *name,
                                                                            const char *value,
                                                                            void *user_data),
                                                              void *user_data);

/**
 * Convenience wrapper: set the IPv4 multicast TTL via the well-known
 * `int2dds.transport.UDPv4.multicast_ttl` property.
 *
 * # Safety
 * `qos` must be a valid QoS handle.
 */
Int2DdsRet int2dds_participant_qos_set_multicast_ttl(struct Int2DdsParticipantQos *qos,
                                                     uint8_t ttl);

/**
 * Destroy DomainParticipant QoS
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `qos` must not be used after this call
 */
Int2DdsRet int2dds_participant_qos_destroy(struct Int2DdsParticipantQos *qos);

/**
 * Set deadline QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datawriter_qos_set_deadline(struct Int2DdsDataWriterQos *qos, int64_t period_ns);

/**
 * Set deadline QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_datareader_qos_set_deadline(struct Int2DdsDataReaderQos *qos, int64_t period_ns);

/**
 * Set liveliness QoS for DataWriter
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid liveliness kind (0=Automatic, 1=ManualByParticipant, 2=ManualByTopic)
 */
Int2DdsRet int2dds_datawriter_qos_set_liveliness(struct Int2DdsDataWriterQos *qos,
                                                 int32_t kind,
                                                 int64_t lease_duration_ns);

/**
 * Set liveliness QoS for DataReader
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `kind` must be a valid liveliness kind (0=Automatic, 1=ManualByParticipant, 2=ManualByTopic)
 */
Int2DdsRet int2dds_datareader_qos_set_liveliness(struct Int2DdsDataReaderQos *qos,
                                                 int32_t kind,
                                                 int64_t lease_duration_ns);

/**
 * Create default Publisher QoS
 *
 * # Safety
 * - `qos_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_publisher_qos_create_default(struct Int2DdsPublisherQos **qos_out);

/**
 * Set partition QoS for Publisher
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `partitions` must point to `partition_count` valid C strings
 */
Int2DdsRet int2dds_publisher_qos_set_partition(struct Int2DdsPublisherQos *qos,
                                               const char *const *partitions,
                                               uintptr_t partition_count);

/**
 * Destroy Publisher QoS
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_publisher_qos_destroy(struct Int2DdsPublisherQos *qos);

/**
 * Create default Subscriber QoS
 *
 * # Safety
 * - `qos_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_subscriber_qos_create_default(struct Int2DdsSubscriberQos **qos_out);

/**
 * Set partition QoS for Subscriber
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `partitions` must point to `partition_count` valid C strings
 */
Int2DdsRet int2dds_subscriber_qos_set_partition(struct Int2DdsSubscriberQos *qos,
                                                const char *const *partitions,
                                                uintptr_t partition_count);

/**
 * Destroy Subscriber QoS
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 */
Int2DdsRet int2dds_subscriber_qos_destroy(struct Int2DdsSubscriberQos *qos);

/**
 * Create a ReadCondition on a DataReader.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_readcondition_delete`
 */
Int2DdsRet int2dds_datareader_create_readcondition(const struct Int2DdsDataReader *reader,
                                                   uint32_t sample_state_mask,
                                                   uint32_t view_state_mask,
                                                   uint32_t instance_state_mask,
                                                   struct Int2DdsReadCondition **condition_out);

/**
 * Create a QueryCondition on a DataReader.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `query_expression` must be a valid null-terminated C string
 * - `query_parameters` must be a valid array of `query_parameters_count`
 *   null-terminated C strings, or null if the count is 0
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_readcondition_delete`
 */
Int2DdsRet int2dds_datareader_create_querycondition(const struct Int2DdsDataReader *reader,
                                                    uint32_t sample_state_mask,
                                                    uint32_t view_state_mask,
                                                    uint32_t instance_state_mask,
                                                    const char *query_expression,
                                                    const char *const *query_parameters,
                                                    uintptr_t query_parameters_count,
                                                    struct Int2DdsReadCondition **condition_out);

/**
 * Get the trigger value of a Read/QueryCondition.
 *
 * # Safety
 * - `condition` must be a valid read condition
 * - `value_out` must be a valid pointer
 */
Int2DdsRet int2dds_readcondition_get_trigger_value(const struct Int2DdsReadCondition *condition,
                                                   bool *value_out);

/**
 * Replace the parameters of a QueryCondition's SQL expression.
 *
 * Returns `INT2DDS_RET_INVALID_ARGUMENT` if the condition is a ReadCondition
 * (no query expression) or the parameter count does not match the expression.
 *
 * # Safety
 * - `condition` must be a valid read condition
 * - `query_parameters` must be a valid array of `query_parameters_count`
 *   null-terminated C strings, or null if the count is 0
 */
Int2DdsRet int2dds_querycondition_set_query_parameters(const struct Int2DdsReadCondition *condition,
                                                       const char *const *query_parameters,
                                                       uintptr_t query_parameters_count);

/**
 * Delete a Read/QueryCondition.
 *
 * # Safety
 * - `condition` must be a valid read condition
 * - `condition` must not be used after this call
 * - The condition should be detached from any WaitSets first
 */
Int2DdsRet int2dds_readcondition_delete(struct Int2DdsReadCondition *condition);

/**
 * Take samples matching a Read/QueryCondition, returned as serialized bytes.
 *
 * For a ReadCondition the condition's state masks are applied directly on the
 * serialized cache. For a QueryCondition the SQL content filter is additionally
 * evaluated (requires field descriptors on the topic; see the module note).
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `condition` must be a valid read condition created from `reader`
 * - `seq_out` must be a valid pointer to a null pointer
 * - On `INT2DDS_RET_OK` the returned sequence must be freed with `int2dds_sample_seq_delete`
 */
Int2DdsRet int2dds_datareader_take_serialized_batch_w_readcondition(const struct Int2DdsDataReader *reader,
                                                                    const struct Int2DdsReadCondition *condition,
                                                                    int32_t max_samples,
                                                                    struct Int2DdsSampleSeq **seq_out);

/**
 * Read samples matching a Read/QueryCondition (samples remain in the cache).
 *
 * # Safety
 * - Same as `int2dds_datareader_take_serialized_batch_w_readcondition`
 */
Int2DdsRet int2dds_datareader_read_serialized_batch_w_readcondition(const struct Int2DdsDataReader *reader,
                                                                    const struct Int2DdsReadCondition *condition,
                                                                    int32_t max_samples,
                                                                    struct Int2DdsSampleSeq **seq_out);

/**
 * Get the StatusCondition from a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_statuscondition_delete`
 */
Int2DdsRet int2dds_datareader_get_statuscondition(const struct Int2DdsDataReader *reader,
                                                  struct Int2DdsStatusCondition **condition_out);

/**
 * Get the StatusCondition from a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_statuscondition_delete`
 */
Int2DdsRet int2dds_datawriter_get_statuscondition(const struct Int2DdsDataWriter *writer,
                                                  struct Int2DdsStatusCondition **condition_out);

/**
 * Get the StatusCondition from a DomainParticipant
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_statuscondition_delete`
 */
Int2DdsRet int2dds_participant_get_statuscondition(const struct Int2DdsParticipant *participant,
                                                   struct Int2DdsStatusCondition **condition_out);

/**
 * Get the StatusCondition from a Publisher
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_statuscondition_delete`
 */
Int2DdsRet int2dds_publisher_get_statuscondition(const struct Int2DdsPublisher *publisher,
                                                 struct Int2DdsStatusCondition **condition_out);

/**
 * Get the StatusCondition from a Subscriber
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_statuscondition_delete`
 */
Int2DdsRet int2dds_subscriber_get_statuscondition(const struct Int2DdsSubscriber *subscriber,
                                                  struct Int2DdsStatusCondition **condition_out);

/**
 * Get the StatusCondition from a Topic
 *
 * # Safety
 * - `topic` must be a valid topic
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_statuscondition_delete`
 */
Int2DdsRet int2dds_topic_get_statuscondition(const struct Int2DdsTopic *topic,
                                             struct Int2DdsStatusCondition **condition_out);

/**
 * Get the current status change bitmask from a DataReader.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `mask_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_get_status_changes(const struct Int2DdsDataReader *reader,
                                                 uint32_t *mask_out);

/**
 * Get the current status change bitmask from a DataWriter.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `mask_out` must be a valid pointer
 */
Int2DdsRet int2dds_datawriter_get_status_changes(const struct Int2DdsDataWriter *writer,
                                                 uint32_t *mask_out);

/**
 * Get the current status change bitmask from a DomainParticipant.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `mask_out` must be a valid pointer
 */
Int2DdsRet int2dds_participant_get_status_changes(const struct Int2DdsParticipant *participant,
                                                  uint32_t *mask_out);

/**
 * Get the current status change bitmask from a Publisher.
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `mask_out` must be a valid pointer
 */
Int2DdsRet int2dds_publisher_get_status_changes(const struct Int2DdsPublisher *publisher,
                                                uint32_t *mask_out);

/**
 * Get the current status change bitmask from a Subscriber.
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `mask_out` must be a valid pointer
 */
Int2DdsRet int2dds_subscriber_get_status_changes(const struct Int2DdsSubscriber *subscriber,
                                                 uint32_t *mask_out);

/**
 * Get the current status change bitmask from a Topic.
 *
 * # Safety
 * - `topic` must be a valid topic
 * - `mask_out` must be a valid pointer
 */
Int2DdsRet int2dds_topic_get_status_changes(const struct Int2DdsTopic *topic, uint32_t *mask_out);

/**
 * Set the enabled statuses for a StatusCondition
 *
 * Only the statuses in the mask will trigger the condition.
 *
 * # Safety
 * - `condition` must be a valid status condition
 * - `mask` is a bitmask of INT2DDS_STATUS_* constants
 */
Int2DdsRet int2dds_statuscondition_set_enabled_statuses(const struct Int2DdsStatusCondition *condition,
                                                        uint32_t mask);

/**
 * Get the enabled statuses for a StatusCondition
 *
 * # Safety
 * - `condition` must be a valid status condition
 * - `mask_out` must be a valid pointer
 */
Int2DdsRet int2dds_statuscondition_get_enabled_statuses(const struct Int2DdsStatusCondition *condition,
                                                        uint32_t *mask_out);

/**
 * Get the trigger value of a StatusCondition
 *
 * Returns true if any of the enabled statuses have changed.
 *
 * # Safety
 * - `condition` must be a valid status condition
 * - `value_out` must be a valid pointer
 */
Int2DdsRet int2dds_statuscondition_get_trigger_value(const struct Int2DdsStatusCondition *condition,
                                                     bool *value_out);

/**
 * Delete a StatusCondition
 *
 * # Safety
 * - `condition` must be a valid status condition
 * - `condition` must not be used after this call
 * - The condition should be detached from any WaitSets first
 */
Int2DdsRet int2dds_statuscondition_delete(struct Int2DdsStatusCondition *condition);

/**
 * Create a Subscriber
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `qos` can be null for the default QoS (engages the core resolution chain:
 *   registered default → configured default profile → spec default)
 * - `subscriber_out` must be a valid pointer to a null pointer
 * - The returned subscriber must be freed with `int2dds_delete_subscriber`
 */
Int2DdsRet int2dds_create_subscriber(const struct Int2DdsParticipant *participant,
                                     const struct Int2DdsSubscriberQos *qos,
                                     struct Int2DdsSubscriber **subscriber_out);

/**
 * Create a Subscriber using a QoS profile path
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `subscriber_out` must be a valid pointer to a null pointer
 * - The returned subscriber must be freed with `int2dds_delete_subscriber`
 */
Int2DdsRet int2dds_create_subscriber_with_profile(const struct Int2DdsParticipant *participant,
                                                  const char *qos_path,
                                                  struct Int2DdsSubscriber **subscriber_out);

/**
 * Set QoS on a Subscriber
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `qos` must be a valid subscriber QoS handle
 */
Int2DdsRet int2dds_subscriber_set_qos(const struct Int2DdsSubscriber *subscriber,
                                      const struct Int2DdsSubscriberQos *qos);

/**
 * Get QoS from a Subscriber
 *
 * The returned handle must be freed with `int2dds_subscriber_qos_destroy`.
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `qos_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_subscriber_get_qos(const struct Int2DdsSubscriber *subscriber,
                                      struct Int2DdsSubscriberQos **qos_out);

/**
 * Get the 16-byte instance handle of a Subscriber.
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `handle_out` must point to a 16-byte buffer
 */
Int2DdsRet int2dds_subscriber_get_instance_handle(const struct Int2DdsSubscriber *subscriber,
                                                  uint8_t (*handle_out)[16]);

/**
 * Delete a Subscriber
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `subscriber` must not be used after this call
 * - All DataReaders created by this subscriber must be deleted first
 */
Int2DdsRet int2dds_delete_subscriber(struct Int2DdsSubscriber *subscriber);

/**
 * Create a DataReader
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `topic` must be a valid topic
 * - `qos` can be null for default QoS
 * - `listener` can be null for no listener; `mask` specifies which status
 *   changes trigger callbacks (pass 0 with a null listener)
 * - `reader_out` must be a valid pointer to a null pointer
 * - The returned reader must be freed with `int2dds_delete_datareader`
 * - Listener callbacks must be thread-safe and remain valid until reader is deleted
 */
Int2DdsRet int2dds_create_datareader(const struct Int2DdsSubscriber *subscriber,
                                     const struct Int2DdsTopic *topic,
                                     const struct Int2DdsDataReaderQos *qos,
                                     const struct Int2DdsDataReaderListener *listener,
                                     uint32_t mask,
                                     struct Int2DdsDataReader **reader_out);

/**
 * Create a DataReader using a QoS profile path
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `topic` must be a valid topic
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `listener` can be null for no listener; `mask` specifies which status
 *   changes trigger callbacks (pass 0 with a null listener)
 * - `reader_out` must be a valid pointer to a null pointer
 * - The returned reader must be freed with `int2dds_delete_datareader`
 * - Listener callbacks must be thread-safe and remain valid until reader is deleted
 */
Int2DdsRet int2dds_create_datareader_with_profile(const struct Int2DdsSubscriber *subscriber,
                                                  const struct Int2DdsTopic *topic,
                                                  const char *qos_path,
                                                  const struct Int2DdsDataReaderListener *listener,
                                                  uint32_t mask,
                                                  struct Int2DdsDataReader **reader_out);

/**
 * Create a DataReader using a ContentFilteredTopic
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `cft` must be a valid ContentFilteredTopic
 * - `qos` can be null for default QoS
 * - `listener` can be null for no listener; `mask` specifies which status
 *   changes trigger callbacks (pass 0 with a null listener)
 * - `reader_out` must be a valid pointer to a null pointer
 * - The returned reader must be freed with `int2dds_delete_datareader`
 * - Listener callbacks must be thread-safe and remain valid until reader is deleted
 */
Int2DdsRet int2dds_create_datareader_cft(const struct Int2DdsSubscriber *subscriber,
                                         const struct Int2DdsContentFilteredTopic *cft,
                                         const struct Int2DdsDataReaderQos *qos,
                                         const struct Int2DdsDataReaderListener *listener,
                                         uint32_t mask,
                                         struct Int2DdsDataReader **reader_out);

/**
 * Set or update the listener for a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `listener` can be null to remove the listener
 * - `mask` specifies which status changes trigger callbacks
 * - Listener callbacks must be thread-safe
 */
Int2DdsRet int2dds_datareader_set_listener(struct Int2DdsDataReader *reader,
                                           const struct Int2DdsDataReaderListener *listener,
                                           uint32_t mask);

/**
 * Get the current listener from a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `listener_out` must be a valid pointer to Int2DdsDataReaderListener
 * - Returns a copy of the listener callbacks and user context
 */
Int2DdsRet int2dds_datareader_get_listener(const struct Int2DdsDataReader *reader,
                                           struct Int2DdsDataReaderListener *listener_out);

/**
 * Set QoS on a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `qos` must be a valid datareader QoS handle
 */
Int2DdsRet int2dds_datareader_set_qos(const struct Int2DdsDataReader *reader,
                                      const struct Int2DdsDataReaderQos *qos);

/**
 * Get QoS from a DataReader
 *
 * The returned handle must be freed with `int2dds_datareader_qos_destroy`.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `qos_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_datareader_get_qos(const struct Int2DdsDataReader *reader,
                                      struct Int2DdsDataReaderQos **qos_out);

/**
 * Get the 16-byte RTPS GUID of a DataReader.
 *
 * Writes the reader's endpoint GUID (the same value advertised over SEDP
 * discovery as `endpoint_guid`) into `guid_out`. Read-only.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `guid_out` must be a valid pointer to a 16-byte buffer
 */
Int2DdsRet int2dds_datareader_get_guid(const struct Int2DdsDataReader *reader,
                                       uint8_t (*guid_out)[16]);

/**
 * Look up an instance handle from raw serialized key bytes.
 *
 * Mirrors `int2dds_datawriter_lookup_instance` for the read side. Matches on the
 * serialized key bytes stored per instance; writes `InstanceHandle::NIL` (all zeros)
 * to `handle_out` when the instance is not known to this reader.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `key` must point to at least `key_len` readable bytes
 * - `handle_out` must be a valid pointer to a 16-byte buffer
 */
Int2DdsRet int2dds_datareader_lookup_instance(const struct Int2DdsDataReader *reader,
                                              const uint8_t *key,
                                              uintptr_t key_len,
                                              uint8_t (*handle_out)[16]);

/**
 * Get the raw serialized key bytes for an instance handle.
 *
 * Mirrors `int2dds_datawriter_get_key_value` for the read side. Round-trips with
 * `int2dds_datareader_lookup_instance`.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `handle` must be a valid pointer to a 16-byte instance handle
 * - `key_buf` must point to at least `key_capacity` writable bytes
 * - `key_size_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_get_key_value(const struct Int2DdsDataReader *reader,
                                            const uint8_t (*handle)[16],
                                            uint8_t *key_buf,
                                            uintptr_t key_capacity,
                                            uintptr_t *key_size_out);

/**
 * Check whether a DataReader currently has any cached samples.
 *
 * This is a level-triggered readiness check over the local reader cache.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `has_data_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_has_data(const struct Int2DdsDataReader *reader, bool *has_data_out);

/**
 * Delete a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `reader` must not be used after this call
 */
Int2DdsRet int2dds_delete_datareader(struct Int2DdsDataReader *reader);

/**
 * Get subscription matched status for a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_get_subscription_matched_status(const struct Int2DdsDataReader *reader,
                                                              struct Int2DdsSubscriptionMatchedStatus *status_out);

/**
 * Get liveliness changed status for a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_get_liveliness_changed_status(const struct Int2DdsDataReader *reader,
                                                            struct Int2DdsLivelinessChangedStatus *status_out);

/**
 * Get sample rejected status for a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_get_sample_rejected_status(const struct Int2DdsDataReader *reader,
                                                         struct Int2DdsSampleRejectedStatus *status_out);

/**
 * Get sample lost status for a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_get_sample_lost_status(const struct Int2DdsDataReader *reader,
                                                     struct Int2DdsSampleLostStatus *status_out);

/**
 * Get requested deadline missed status for a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_get_requested_deadline_missed_status(const struct Int2DdsDataReader *reader,
                                                                   struct Int2DdsRequestedDeadlineMissedStatus *status_out);

/**
 * Get requested incompatible QoS status for a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_get_requested_incompatible_qos_status(const struct Int2DdsDataReader *reader,
                                                                    struct Int2DdsRequestedIncompatibleQosStatus *status_out);

/**
 * Get requested incompatible type status for a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_get_requested_incompatible_type_status(const struct Int2DdsDataReader *reader,
                                                                     struct Int2DdsRequestedIncompatibleTypeStatus *status_out);

/**
 * Delete all entities contained by a subscriber
 *
 * This operation deletes all DataReader objects contained by this Subscriber.
 * It also recursively calls delete_contained_entities on each DataReader.
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 */
Int2DdsRet int2dds_subscriber_delete_contained_entities(const struct Int2DdsSubscriber *subscriber);

/**
 * Take pre-serialized data from a DataReader, bypassing TypeSupport deserialization.
 *
 * Copies the raw CDR bytes (including encapsulation header) into the caller's buffer.
 * The sample is removed from the cache only when it fits the caller's buffer.
 *
 * # Parameters
 * - `reader`: A valid datareader
 * - `buffer`: Pointer to the caller's byte buffer for receiving serialized data
 * - `buffer_capacity`: Size of the buffer in bytes
 * - `actual_size_out`: Receives the actual number of bytes written
 * - `valid_data_out`: Set to true if this is a valid data sample (not dispose/unregister)
 *
 * # Returns
 * - INT2DDS_RET_OK on success
 * - INT2DDS_RET_NO_DATA if no samples available
 * - INT2DDS_RET_BUFFER_TOO_SMALL if the buffer is too small; the sample is preserved and
 *   actual_size_out contains the required size, so a retry with a larger buffer succeeds
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `buffer` must point to at least `buffer_capacity` writable bytes
 * - `actual_size_out` and `valid_data_out` must be valid pointers
 */
Int2DdsRet int2dds_datareader_take_serialized(const struct Int2DdsDataReader *reader,
                                              uint8_t *buffer,
                                              uintptr_t buffer_capacity,
                                              uintptr_t *actual_size_out,
                                              bool *valid_data_out);

/**
 * Take pre-serialized data and loan the returned byte slice to the caller.
 *
 * The returned `data_out` pointer remains valid until `loan_out` is passed to
 * `int2dds_datareader_return_serialized_loan`. This avoids copying the payload into a
 * caller-owned buffer for consumers that immediately deserialize the bytes.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `data_out`, `actual_size_out`, `valid_data_out`, and `loan_out` must be valid pointers
 * - if `*loan_out` is non-null, the caller must return it exactly once
 */
Int2DdsRet int2dds_datareader_take_serialized_loaned(const struct Int2DdsDataReader *reader,
                                                     const uint8_t **data_out,
                                                     uintptr_t *actual_size_out,
                                                     bool *valid_data_out,
                                                     struct Int2DdsSerializedLoan **loan_out);

/**
 * Return a serialized data loan produced by `int2dds_datareader_take_serialized_loaned`.
 *
 * # Safety
 * - `loan` must be null or a pointer returned by `int2dds_datareader_take_serialized_loaned`
 * - `loan` must not be used after this call
 */
Int2DdsRet int2dds_datareader_return_serialized_loan(struct Int2DdsSerializedLoan *loan);

/**
 * Read pre-serialized data from a DataReader without removing from cache.
 *
 * Same as `int2dds_datareader_take_serialized` but the sample remains in the cache.
 *
 * # Safety
 * - Same as `int2dds_datareader_take_serialized`
 */
Int2DdsRet int2dds_datareader_read_serialized(const struct Int2DdsDataReader *reader,
                                              uint8_t *buffer,
                                              uintptr_t buffer_capacity,
                                              uintptr_t *actual_size_out,
                                              bool *valid_data_out);

/**
 * Take pre-serialized data with full SampleInfo
 *
 * # Safety
 * - Same as `int2dds_datareader_take_serialized`, plus `info_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_take_serialized_w_info(const struct Int2DdsDataReader *reader,
                                                     uint8_t *buffer,
                                                     uintptr_t buffer_capacity,
                                                     uintptr_t *actual_size_out,
                                                     struct Int2DdsSampleInfo *info_out);

/**
 * Read pre-serialized data with full SampleInfo (sample remains in cache)
 *
 * # Safety
 * - Same as `int2dds_datareader_read_serialized`, plus `info_out` must be a valid pointer
 */
Int2DdsRet int2dds_datareader_read_serialized_w_info(const struct Int2DdsDataReader *reader,
                                                     uint8_t *buffer,
                                                     uintptr_t buffer_capacity,
                                                     uintptr_t *actual_size_out,
                                                     struct Int2DdsSampleInfo *info_out);

/**
 * Take multiple serialized samples as a batch
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `seq_out` must be a valid pointer to a null pointer
 * - On `INT2DDS_RET_OK` the returned sequence must be freed with `int2dds_sample_seq_delete`
 */
Int2DdsRet int2dds_datareader_take_serialized_batch(const struct Int2DdsDataReader *reader,
                                                    int32_t max_samples,
                                                    struct Int2DdsSampleSeq **seq_out);

/**
 * Read multiple serialized samples as a batch (samples remain in cache)
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `seq_out` must be a valid pointer to a null pointer
 * - On `INT2DDS_RET_OK` the returned sequence must be freed with `int2dds_sample_seq_delete`
 */
Int2DdsRet int2dds_datareader_read_serialized_batch(const struct Int2DdsDataReader *reader,
                                                    int32_t max_samples,
                                                    struct Int2DdsSampleSeq **seq_out);

/**
 * Take pre-serialized samples belonging to a single instance, as a batch.
 *
 * `handle` is a 16-byte instance handle (e.g. from `int2dds_datareader_lookup_instance`
 * or a prior sample's info). A nil handle returns `INT2DDS_RET_BAD_PARAMETER`; an
 * unknown handle returns no samples. The mask arguments are bitmasks of the
 * SampleState/ViewState/InstanceState kinds; a mask of 0 selects "any".
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `handle` must point to a 16-byte instance handle
 * - `seq_out` must be a valid pointer to a null pointer
 * - On `INT2DDS_RET_OK` the returned sequence must be freed with `int2dds_sample_seq_delete`
 */
Int2DdsRet int2dds_datareader_take_instance_serialized_batch(const struct Int2DdsDataReader *reader,
                                                             const uint8_t (*handle)[16],
                                                             int32_t max_samples,
                                                             uint32_t sample_state_mask,
                                                             uint32_t view_state_mask,
                                                             uint32_t instance_state_mask,
                                                             struct Int2DdsSampleSeq **seq_out);

/**
 * Read pre-serialized samples belonging to a single instance, as a batch
 * (samples remain in the cache).
 *
 * # Safety
 * - Same as `int2dds_datareader_take_instance_serialized_batch`
 */
Int2DdsRet int2dds_datareader_read_instance_serialized_batch(const struct Int2DdsDataReader *reader,
                                                             const uint8_t (*handle)[16],
                                                             int32_t max_samples,
                                                             uint32_t sample_state_mask,
                                                             uint32_t view_state_mask,
                                                             uint32_t instance_state_mask,
                                                             struct Int2DdsSampleSeq **seq_out);

/**
 * Get the number of samples in a sequence
 */
uintptr_t int2dds_sample_seq_length(const struct Int2DdsSampleSeq *seq);

/**
 * Get serialized data at a given index in the sequence
 *
 * # Safety
 * - `seq` must be a valid sample sequence
 * - `index` must be less than the sequence length
 * - `buffer` must point to at least `buffer_capacity` writable bytes
 * - `actual_size_out` must be a valid pointer
 */
Int2DdsRet int2dds_sample_seq_get_data(const struct Int2DdsSampleSeq *seq,
                                       uintptr_t index,
                                       uint8_t *buffer,
                                       uintptr_t buffer_capacity,
                                       uintptr_t *actual_size_out);

/**
 * Get SampleInfo at a given index in the sequence
 *
 * # Safety
 * - `seq` must be a valid sample sequence
 * - `index` must be less than the sequence length
 * - `info_out` must be a valid pointer
 */
Int2DdsRet int2dds_sample_seq_get_info(const struct Int2DdsSampleSeq *seq,
                                       uintptr_t index,
                                       struct Int2DdsSampleInfo *info_out);

/**
 * Delete a sample sequence and free its memory
 *
 * # Safety
 * - `seq` must be a valid sample sequence or null
 * - `seq` must not be used after this call
 */
Int2DdsRet int2dds_sample_seq_delete(struct Int2DdsSampleSeq *seq);

/**
 * Read a single serialized sample with state condition filter.
 * A state mask of 0 selects "any".
 *
 * # Safety
 * - Same as `int2dds_datareader_read_serialized_w_info`, plus state masks
 */
Int2DdsRet int2dds_datareader_read_serialized_w_states(const struct Int2DdsDataReader *reader,
                                                       uint8_t *buffer,
                                                       uintptr_t buffer_capacity,
                                                       uintptr_t *actual_size_out,
                                                       struct Int2DdsSampleInfo *info_out,
                                                       uint32_t sample_state_mask,
                                                       uint32_t view_state_mask,
                                                       uint32_t instance_state_mask);

/**
 * Take a single serialized sample with state condition filter.
 * A state mask of 0 selects "any".
 *
 * # Safety
 * - Same as `int2dds_datareader_take_serialized_w_info`, plus state masks
 */
Int2DdsRet int2dds_datareader_take_serialized_w_states(const struct Int2DdsDataReader *reader,
                                                       uint8_t *buffer,
                                                       uintptr_t buffer_capacity,
                                                       uintptr_t *actual_size_out,
                                                       struct Int2DdsSampleInfo *info_out,
                                                       uint32_t sample_state_mask,
                                                       uint32_t view_state_mask,
                                                       uint32_t instance_state_mask);

/**
 * Take batch with state condition filter. A state mask of 0 selects "any".
 *
 * # Safety
 * - Same as `int2dds_datareader_take_serialized_batch`, plus state masks
 */
Int2DdsRet int2dds_datareader_take_serialized_batch_w_states(const struct Int2DdsDataReader *reader,
                                                             int32_t max_samples,
                                                             struct Int2DdsSampleSeq **seq_out,
                                                             uint32_t sample_state_mask,
                                                             uint32_t view_state_mask,
                                                             uint32_t instance_state_mask);

/**
 * Read batch with state condition filter. A state mask of 0 selects "any".
 *
 * # Safety
 * - Same as `int2dds_datareader_read_serialized_batch`, plus state masks
 */
Int2DdsRet int2dds_datareader_read_serialized_batch_w_states(const struct Int2DdsDataReader *reader,
                                                             int32_t max_samples,
                                                             struct Int2DdsSampleSeq **seq_out,
                                                             uint32_t sample_state_mask,
                                                             uint32_t view_state_mask,
                                                             uint32_t instance_state_mask);

/**
 * Block until the DataReader has received all historical data from matched
 * TRANSIENT_LOCAL writers, or until the timeout expires.
 *
 * # Safety
 * - `reader` must be a valid datareader
 */
Int2DdsRet int2dds_datareader_wait_for_historical_data(const struct Int2DdsDataReader *reader,
                                                       int64_t timeout_ms);

/**
 * Create a Topic
 *
 * Creates a topic with RawTypeSupport for use with `int2dds_datawriter_write_serialized()`
 * and `int2dds_datareader_take_serialized()`. C users handle CDR serialization themselves
 * using IDL-generated code. Keyed topics require a full TypeObject; use
 * `int2dds_create_topic_with_type_info` or `int2dds_create_topic_with_field_descriptors`.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `topic_name` must be a valid null-terminated C string
 * - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
 * - `extensibility`: -1 = library default (Appendable per spec; frames a DHEADER, so
 *   pass an explicit value or use the type-info path for a Final remote),
 *   0 = Final, 1 = Appendable, 2 = Mutable
 * - `qos` can be null for default QoS
 * - `topic_out` must be a valid pointer to a null pointer
 * - The returned topic must be freed with `int2dds_delete_topic`
 */
Int2DdsRet int2dds_create_topic(const struct Int2DdsParticipant *participant,
                                const char *topic_name,
                                const char *dds_type_name,
                                int32_t extensibility,
                                const struct Int2DdsTopicQos *qos,
                                struct Int2DdsTopic **topic_out);

/**
 * Create a Topic using a QoS profile path
 *
 * Same as `int2dds_create_topic` but uses a QoS profile path instead of a QoS handle.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `topic_name` must be a valid null-terminated C string
 * - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
 * - `extensibility`: -1 = library default (Appendable per spec; frames a DHEADER, so
 *   pass an explicit value or use the type-info path for a Final remote),
 *   0 = Final, 1 = Appendable, 2 = Mutable
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `topic_out` must be a valid pointer to a null pointer
 * - The returned topic must be freed with `int2dds_delete_topic`
 */
Int2DdsRet int2dds_create_topic_with_profile(const struct Int2DdsParticipant *participant,
                                             const char *topic_name,
                                             const char *dds_type_name,
                                             int32_t extensibility,
                                             const char *qos_path,
                                             struct Int2DdsTopic **topic_out);

/**
 * Create a Topic with type information for DDS-XTypes discovery
 *
 * Creates a topic using a pre-built `Int2DdsTypeInfo` which provides
 * TypeIdentifier and TypeObject for DDS discovery parameters (0x0075/0x0069, 0x0072).
 * This enables interoperability with implementations that require type information
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `topic_name` must be a valid null-terminated C string
 * - `type_info` must be a valid `Int2DdsTypeInfo` created by `int2dds_type_info_create`
 * - `qos` can be null for default QoS
 * - `topic_out` must be a valid pointer to a null pointer
 * - The returned topic must be freed with `int2dds_delete_topic`
 */
Int2DdsRet int2dds_create_topic_with_type_info(const struct Int2DdsParticipant *participant,
                                               const char *topic_name,
                                               const struct Int2DdsTypeInfo *type_info,
                                               const struct Int2DdsTopicQos *qos,
                                               struct Int2DdsTopic **topic_out);

/**
 * Set QoS on a Topic
 *
 * # Safety
 * - `topic` must be a valid topic
 * - `qos` must be a valid topic QoS handle
 */
Int2DdsRet int2dds_topic_set_qos(const struct Int2DdsTopic *topic,
                                 const struct Int2DdsTopicQos *qos);

/**
 * Get QoS from a Topic
 *
 * The returned handle must be freed with `int2dds_topic_qos_destroy`.
 *
 * # Safety
 * - `topic` must be a valid topic
 * - `qos_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_topic_get_qos(const struct Int2DdsTopic *topic,
                                 struct Int2DdsTopicQos **qos_out);

/**
 * Delete a Topic
 *
 * # Safety
 * - `topic` must be a valid topic created by `int2dds_create_topic`
 * - `topic` must not be used after this call
 * - All DataReaders and DataWriters using this topic must be deleted first
 */
Int2DdsRet int2dds_delete_topic(struct Int2DdsTopic *topic);

/**
 * Get the inconsistent topic status for a Topic
 *
 * Reports how many times a remote topic with the same name but an
 * incompatible type was discovered. Reading the status resets its
 * `total_count_change` and clears the INCONSISTENT_TOPIC status flag.
 *
 * # Safety
 * - `topic` must be a valid topic
 * - `status_out` must be a valid pointer
 */
Int2DdsRet int2dds_topic_get_inconsistent_topic_status(const struct Int2DdsTopic *topic,
                                                       struct Int2DdsInconsistentTopicStatus *status_out);

/**
 * Get the name of a Topic
 *
 * # Safety
 * - `topic` must be a valid topic
 * - `name_out` must be a valid pointer to a char buffer
 * - `name_size` is the size of the buffer
 */
Int2DdsRet int2dds_topic_get_name(const struct Int2DdsTopic *topic,
                                  char *name_out,
                                  uintptr_t name_size);

/**
 * Get the type name of a Topic
 *
 * # Safety
 * - `topic` must be a valid topic
 * - `type_name_out` must be a valid pointer to a char buffer
 * - `type_name_size` is the size of the buffer
 */
Int2DdsRet int2dds_topic_get_type_name(const struct Int2DdsTopic *topic,
                                       char *type_name_out,
                                       uintptr_t type_name_size);

/**
 * Create a ContentFilteredTopic
 *
 * Creates a content-filtered topic that filters data based on a SQL-like expression.
 * The filter expression uses SQL-92 syntax with parameters referenced as %0, %1, etc.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `topic_name` must be a valid null-terminated C string
 * - `related_topic` must be a valid topic created on the same participant
 * - `filter_expression` must be a valid null-terminated C string (e.g., "color = %0")
 * - `expression_parameters` must be a valid array of null-terminated C strings, or null if count is 0
 * - `expression_parameters_count` is the number of parameters
 * - `cft_out` must be a valid pointer to a null pointer
 * - The returned ContentFilteredTopic must be freed with `int2dds_delete_contentfilteredtopic`
 */
Int2DdsRet int2dds_create_contentfilteredtopic(const struct Int2DdsParticipant *participant,
                                               const char *topic_name,
                                               const struct Int2DdsTopic *related_topic,
                                               const char *filter_expression,
                                               const char *const *expression_parameters,
                                               uintptr_t expression_parameters_count,
                                               struct Int2DdsContentFilteredTopic **cft_out);

/**
 * Delete a ContentFilteredTopic
 *
 * # Safety
 * - `cft` must be a valid ContentFilteredTopic created by `int2dds_create_contentfilteredtopic`
 * - `cft` must not be used after this call
 * - All DataReaders using this ContentFilteredTopic must be deleted first
 */
Int2DdsRet int2dds_delete_contentfilteredtopic(struct Int2DdsContentFilteredTopic *cft);

/**
 * Update the expression parameters of an existing ContentFilteredTopic
 *
 * This updates only the parameter values for the current filter expression.
 *
 * # Safety
 * - `cft` must be a valid ContentFilteredTopic created by `int2dds_create_contentfilteredtopic`
 * - `expression_parameters` must be a valid array of null-terminated C strings, or null if count is 0
 * - `expression_parameters_count` is the number of parameters
 */
Int2DdsRet int2dds_contentfilteredtopic_set_expression_parameters(struct Int2DdsContentFilteredTopic *cft,
                                                                  const char *const *expression_parameters,
                                                                  uintptr_t expression_parameters_count);

Int2DdsRet int2dds_contentfilteredtopic_set_filter_expression(struct Int2DdsContentFilteredTopic *cft,
                                                              const char *filter_expression,
                                                              const char *const *expression_parameters,
                                                              uintptr_t expression_parameters_count);

Int2DdsRet int2dds_contentfilteredtopic_set_enabled(struct Int2DdsContentFilteredTopic *cft,
                                                    bool enabled);

/**
 * Create a Topic with full field descriptors for reader-side CFT filtering.
 *
 * Extends int2dds_create_topic by also providing all field metadata (name, type)
 * needed for get_field_value() support. This enables ContentFilteredTopic
 * reader-side filtering in the serialized path.
 *
 * # Safety
 * - Same as int2dds_create_topic
 * - field_names: array of null-terminated C strings (field_count elements)
 * - field_types: array of u32 type IDs (field_count elements)
 * - field_is_key: array of bool (field_count elements)
 * - field_count: number of fields
 */
Int2DdsRet int2dds_create_topic_with_field_descriptors(const struct Int2DdsParticipant *participant,
                                                       const char *topic_name,
                                                       const char *dds_type_name,
                                                       int32_t extensibility,
                                                       const struct Int2DdsTopicQos *qos,
                                                       const char *const *field_names,
                                                       const uint32_t *field_types,
                                                       const bool *field_is_key,
                                                       uintptr_t field_count,
                                                       struct Int2DdsTopic **topic_out);

Int2DdsRet int2dds_type_info_create(const char *type_name,
                                    int32_t extensibility,
                                    struct Int2DdsTypeInfo **out);

/**
 * Create an enum type info builder. `bit_bound` is the discriminant bit width (IDL enums
 * are i32 -> 32). Populate literals with `int2dds_type_info_add_enum_literal`, then pass
 * the builder to `int2dds_type_info_add_nested_field` (or a collection-of-nested variant)
 * on the parent so an enum-typed member resolves and computes native-Rust keys.
 */
Int2DdsRet int2dds_type_info_create_enum(const char *type_name,
                                         uint16_t bit_bound,
                                         struct Int2DdsTypeInfo **out);

/**
 * Append a literal to an enum builder. `is_default` marks the `@default` literal (0/1);
 * no-op on a non-enum builder.
 */
Int2DdsRet int2dds_type_info_add_enum_literal(struct Int2DdsTypeInfo *type_info,
                                              const char *literal_name,
                                              int32_t value,
                                              int32_t is_default);

/**
 * Create a bitmask type info builder. `bit_bound` is the flag storage bit width (default 32).
 */
Int2DdsRet int2dds_type_info_create_bitmask(const char *type_name,
                                            uint16_t bit_bound,
                                            struct Int2DdsTypeInfo **out);

/**
 * Append a flag to a bitmask builder. `position` is the bit index; no-op on a non-bitmask.
 */
Int2DdsRet int2dds_type_info_add_bitmask_flag(struct Int2DdsTypeInfo *type_info,
                                              const char *flag_name,
                                              uint16_t position);

/**
 * Add a primitive-typed field to the type info builder.
 */
Int2DdsRet int2dds_type_info_add_field(struct Int2DdsTypeInfo *type_info,
                                       const char *field_name,
                                       int32_t field_type,
                                       int32_t flags);

/**
 * Add a (possibly bounded) narrow-string field. `bound == 0` means unbounded.
 *
 * Prefer this over `int2dds_type_info_add_field(.., INT2DDS_FIELD_STRING, ..)` when the
 * IDL declares `string<N>`, so the emitted TypeIdentifier carries the bound and matches
 * strict XTypes peers byte-for-byte.
 */
Int2DdsRet int2dds_type_info_add_string_field(struct Int2DdsTypeInfo *type_info,
                                              const char *field_name,
                                              uint32_t bound,
                                              int32_t flags);

/**
 * Add a (possibly bounded) wide-string (`wstring<N>`) field. `bound == 0` means unbounded.
 */
Int2DdsRet int2dds_type_info_add_wstring_field(struct Int2DdsTypeInfo *type_info,
                                               const char *field_name,
                                               uint32_t bound,
                                               int32_t flags);

/**
 * Add a sequence field to the type info builder.
 */
Int2DdsRet int2dds_type_info_add_sequence_field(struct Int2DdsTypeInfo *type_info,
                                                const char *field_name,
                                                int32_t element_type,
                                                uint32_t bound,
                                                int32_t flags);

/**
 * Add an array field to the type info builder.
 */
Int2DdsRet int2dds_type_info_add_array_field(struct Int2DdsTypeInfo *type_info,
                                             const char *field_name,
                                             int32_t element_type,
                                             uint32_t array_size,
                                             int32_t flags);

/**
 * Add a named (complex) type field to the type info builder.
 */
Int2DdsRet int2dds_type_info_add_named_type_field(struct Int2DdsTypeInfo *type_info,
                                                  const char *field_name,
                                                  const char *type_hash_name,
                                                  int32_t flags);

/**
 * Add a nested struct-typed field, supplying the nested type's own builder.
 *
 * Unlike `int2dds_type_info_add_named_type_field` (which emits a name-hash
 * `MinimalTypeId` reference), this references the nested type by its content-hash
 * `CompleteTypeId` — matching the derive macro byte-for-byte — and captures the nested
 * `TypeObject` so composite (nested-struct) key members resolve and compute native-Rust
 * InstanceHandles. `nested_type_info` is borrowed, not consumed: the caller still owns
 * it and must destroy it.
 */
Int2DdsRet int2dds_type_info_add_nested_field(struct Int2DdsTypeInfo *type_info,
                                              const char *field_name,
                                              const struct Int2DdsTypeInfo *nested_type_info,
                                              int32_t flags);

/**
 * Add a `sequence<Nested>` field whose element is a nested struct/enum/bitmask, supplying
 * the element's own builder. References the element by content-hash `CompleteTypeId`
 * (matching the derive macro) so sequence-of-nested key members resolve and compute
 * native-Rust InstanceHandles. `element_type_info` is borrowed, not consumed.
 */
Int2DdsRet int2dds_type_info_add_sequence_of_nested_field(struct Int2DdsTypeInfo *type_info,
                                                          const char *field_name,
                                                          const struct Int2DdsTypeInfo *element_type_info,
                                                          uint32_t bound,
                                                          int32_t flags);

/**
 * Add a `Nested[N]` fixed-array field whose element is a nested struct/enum/bitmask,
 * supplying the element's own builder. `element_type_info` is borrowed, not consumed.
 */
Int2DdsRet int2dds_type_info_add_array_of_nested_field(struct Int2DdsTypeInfo *type_info,
                                                       const char *field_name,
                                                       const struct Int2DdsTypeInfo *element_type_info,
                                                       uint32_t array_size,
                                                       int32_t flags);

Int2DdsRet int2dds_type_info_add_sequence_of_named_field(struct Int2DdsTypeInfo *type_info,
                                                         const char *field_name,
                                                         const char *element_hash_name,
                                                         uint32_t bound,
                                                         int32_t flags);

Int2DdsRet int2dds_type_info_add_array_of_named_field(struct Int2DdsTypeInfo *type_info,
                                                      const char *field_name,
                                                      const char *element_hash_name,
                                                      uint32_t array_size,
                                                      int32_t flags);

Int2DdsRet int2dds_type_info_to_type_object(const struct Int2DdsTypeInfo *type_info,
                                            struct Int2DdsTypeObject **out);

/**
 * Destroy a type info builder.
 */
void int2dds_type_info_destroy(struct Int2DdsTypeInfo *type_info);

/**
 * Create a new WaitSet
 *
 * # Safety
 * - `waitset_out` must be a valid pointer to a null pointer
 * - The returned waitset must be freed with `int2dds_waitset_delete`
 */
Int2DdsRet int2dds_waitset_new(struct Int2DdsWaitSet **waitset_out);

/**
 * Wait for conditions to be triggered and return the triggered conditions
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `timeout_ms` is the timeout in milliseconds, or -1 for infinite
 * - `conditions_out` must be a valid pointer to a null pointer, or null if the
 *   caller does not need the triggered conditions
 * - The returned condition sequence must be freed with `int2dds_condition_seq_delete`
 *
 * Returns:
 * - INT2DDS_RET_OK if conditions were triggered
 * - INT2DDS_RET_TIMEOUT if the timeout expired
 * - INT2DDS_RET_ERROR for other errors
 */
Int2DdsRet int2dds_waitset_wait_ex(const struct Int2DdsWaitSet *waitset,
                                   int64_t timeout_ms,
                                   struct Int2DdsConditionSeq **conditions_out);

/**
 * Wait for conditions to be triggered and return them, with nanosecond timeout resolution.
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `timeout_ns` is the timeout in nanoseconds, or -1 for infinite
 * - `conditions_out` must be a valid pointer to a null pointer, or null if the
 *   caller does not need the triggered conditions
 * - The returned condition sequence must be freed with `int2dds_condition_seq_delete`
 *
 * Returns:
 * - INT2DDS_RET_OK if conditions were triggered
 * - INT2DDS_RET_TIMEOUT if the timeout expired
 * - INT2DDS_RET_ERROR for other errors
 */
Int2DdsRet int2dds_waitset_wait_ex_ns(const struct Int2DdsWaitSet *waitset,
                                      int64_t timeout_ns,
                                      struct Int2DdsConditionSeq **conditions_out);

/**
 * Get the number of conditions in a condition sequence
 *
 * # Safety
 * - `seq` must be a valid condition sequence
 * - `count_out` must be a valid pointer
 */
Int2DdsRet int2dds_condition_seq_length(const struct Int2DdsConditionSeq *seq,
                                        uintptr_t *count_out);

/**
 * Get a condition from a condition sequence by index
 *
 * # Safety
 * - `seq` must be a valid condition sequence
 * - `index` must be less than the sequence length
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_condition_delete`
 *
 * Note: The returned condition is an owned handle (cloned from the sequence).
 * It remains valid even after the sequence is deleted.
 */
Int2DdsRet int2dds_condition_seq_get(const struct Int2DdsConditionSeq *seq,
                                     uintptr_t index,
                                     const struct Int2DdsCondition **condition_out);

/**
 * Delete a condition sequence
 *
 * # Safety
 * - `seq` must be a valid condition sequence
 * - `seq` must not be used after this call
 */
Int2DdsRet int2dds_condition_seq_delete(struct Int2DdsConditionSeq *seq);

/**
 * Get the trigger value of a condition
 *
 * # Safety
 * - `condition` must be a valid condition
 * - `triggered_out` must be a valid pointer
 */
Int2DdsRet int2dds_condition_get_trigger_value(const struct Int2DdsCondition *condition,
                                               bool *triggered_out);

/**
 * Delete a condition handle
 *
 * # Safety
 * - `condition` must be a valid condition obtained from `int2dds_condition_seq_get`
 * - `condition` must not be used after this call
 */
Int2DdsRet int2dds_condition_delete(struct Int2DdsCondition *condition);

/**
 * Attach a GuardCondition to the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `condition` must be a valid guard condition
 */
Int2DdsRet int2dds_waitset_attach_guardcondition(const struct Int2DdsWaitSet *waitset,
                                                 const struct Int2DdsGuardCondition *condition);

/**
 * Detach a GuardCondition from the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `condition` must be a valid guard condition that was previously attached
 */
Int2DdsRet int2dds_waitset_detach_guardcondition(const struct Int2DdsWaitSet *waitset,
                                                 const struct Int2DdsGuardCondition *condition);

/**
 * Attach a StatusCondition to the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `condition` must be a valid status condition
 */
Int2DdsRet int2dds_waitset_attach_statuscondition(const struct Int2DdsWaitSet *waitset,
                                                  const struct Int2DdsStatusCondition *condition);

/**
 * Detach a StatusCondition from the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `condition` must be a valid status condition that was previously attached
 */
Int2DdsRet int2dds_waitset_detach_statuscondition(const struct Int2DdsWaitSet *waitset,
                                                  const struct Int2DdsStatusCondition *condition);

/**
 * Attach a Read/QueryCondition to the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `condition` must be a valid read condition
 */
Int2DdsRet int2dds_waitset_attach_readcondition(const struct Int2DdsWaitSet *waitset,
                                                const struct Int2DdsReadCondition *condition);

/**
 * Detach a Read/QueryCondition from the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `condition` must be a valid read condition that was previously attached
 */
Int2DdsRet int2dds_waitset_detach_readcondition(const struct Int2DdsWaitSet *waitset,
                                                const struct Int2DdsReadCondition *condition);

/**
 * Delete a WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `waitset` must not be used after this call
 * - All conditions should be detached first
 */
Int2DdsRet int2dds_waitset_delete(struct Int2DdsWaitSet *waitset);

/**
 * Create an empty XML type registry. Destroy with
 * `int2dds_xml_type_registry_destroy`.
 */
Int2DdsRet int2dds_xml_type_registry_create(struct Int2DdsXmlTypeRegistry **out);

/**
 * Create an XML type registry and load `path` into it in one step.
 */
Int2DdsRet int2dds_xml_type_registry_from_file(const char *path,
                                               struct Int2DdsXmlTypeRegistry **out);

/**
 * Load additional types from an XML file into an existing registry.
 */
Int2DdsRet int2dds_xml_type_registry_load_file(struct Int2DdsXmlTypeRegistry *registry,
                                               const char *path);

/**
 * Load additional types from an in-memory XML string into an existing registry.
 */
Int2DdsRet int2dds_xml_type_registry_load_str(struct Int2DdsXmlTypeRegistry *registry,
                                              const char *xml);

/**
 * Look up a loaded type by name and build a dynamic type support (with its full
 * dependency closure). Destroy the result with
 * `int2dds_dynamic_type_support_destroy`.
 */
Int2DdsRet int2dds_xml_type_registry_get_type_support(const struct Int2DdsXmlTypeRegistry *registry,
                                                      const char *name,
                                                      struct Int2DdsDynamicTypeSupport **out);

/**
 * Look up a loaded type by name and return its TypeObject, carrying its full
 * nested-dependency closure so struct/array/sequence members decode correctly.
 * Destroy the result with `int2dds_type_object_destroy`.
 */
Int2DdsRet int2dds_xml_type_registry_get_type_object(const struct Int2DdsXmlTypeRegistry *registry,
                                                     const char *name,
                                                     struct Int2DdsTypeObject **out);

/**
 * Number of types loaded in the registry.
 */
Int2DdsRet int2dds_xml_type_registry_type_count(const struct Int2DdsXmlTypeRegistry *registry,
                                                uintptr_t *out);

/**
 * Copy the fully-qualified name of the type at `index` into `buf`.
 */
Int2DdsRet int2dds_xml_type_registry_type_name(const struct Int2DdsXmlTypeRegistry *registry,
                                               uintptr_t index,
                                               char *buf,
                                               uintptr_t buf_len,
                                               uintptr_t *out_len);

/**
 * Destroy an XML type registry handle. Safe to call with null.
 */
void int2dds_xml_type_registry_destroy(struct Int2DdsXmlTypeRegistry *registry);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus


/* Dynamic primitive sample getters (macro-generated in dynamic.rs;
* manually declared here because cbindgen does not expand macros). */
Int2DdsRet int2dds_dynamic_sample_get_bool  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, bool     *out);
Int2DdsRet int2dds_dynamic_sample_get_i8    (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int8_t   *out);
Int2DdsRet int2dds_dynamic_sample_get_u8    (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint8_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_byte  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint8_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_i16   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int16_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_u16   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint16_t *out);
Int2DdsRet int2dds_dynamic_sample_get_i32   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int32_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_u32   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint32_t *out);
Int2DdsRet int2dds_dynamic_sample_get_i64   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int64_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_u64   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint64_t *out);
Int2DdsRet int2dds_dynamic_sample_get_f32   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, float    *out);
Int2DdsRet int2dds_dynamic_sample_get_f64   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, double   *out);

/* Handle-based DynamicData getters (macro-generated; manually declared). */
Int2DdsRet int2dds_dynamic_data_get_bool (const struct Int2DdsDynamicData *data, const char *field_path, bool     *out);
Int2DdsRet int2dds_dynamic_data_get_i8   (const struct Int2DdsDynamicData *data, const char *field_path, int8_t   *out);
Int2DdsRet int2dds_dynamic_data_get_u8   (const struct Int2DdsDynamicData *data, const char *field_path, uint8_t  *out);
Int2DdsRet int2dds_dynamic_data_get_i16  (const struct Int2DdsDynamicData *data, const char *field_path, int16_t  *out);
Int2DdsRet int2dds_dynamic_data_get_u16  (const struct Int2DdsDynamicData *data, const char *field_path, uint16_t *out);
Int2DdsRet int2dds_dynamic_data_get_i32  (const struct Int2DdsDynamicData *data, const char *field_path, int32_t  *out);
Int2DdsRet int2dds_dynamic_data_get_u32  (const struct Int2DdsDynamicData *data, const char *field_path, uint32_t *out);
Int2DdsRet int2dds_dynamic_data_get_i64  (const struct Int2DdsDynamicData *data, const char *field_path, int64_t  *out);
Int2DdsRet int2dds_dynamic_data_get_u64  (const struct Int2DdsDynamicData *data, const char *field_path, uint64_t *out);
Int2DdsRet int2dds_dynamic_data_get_f32  (const struct Int2DdsDynamicData *data, const char *field_path, float    *out);
Int2DdsRet int2dds_dynamic_data_get_f64  (const struct Int2DdsDynamicData *data, const char *field_path, double   *out);

/* Handle-based DynamicData primitive setters (macro-generated; manually declared). */
Int2DdsRet int2dds_dynamic_data_set_bool (struct Int2DdsDynamicData *data, const char *field, bool     value);
Int2DdsRet int2dds_dynamic_data_set_i8   (struct Int2DdsDynamicData *data, const char *field, int8_t   value);
Int2DdsRet int2dds_dynamic_data_set_u8   (struct Int2DdsDynamicData *data, const char *field, uint8_t  value);
Int2DdsRet int2dds_dynamic_data_set_i16  (struct Int2DdsDynamicData *data, const char *field, int16_t  value);
Int2DdsRet int2dds_dynamic_data_set_u16  (struct Int2DdsDynamicData *data, const char *field, uint16_t value);
Int2DdsRet int2dds_dynamic_data_set_i32  (struct Int2DdsDynamicData *data, const char *field, int32_t  value);
Int2DdsRet int2dds_dynamic_data_set_u32  (struct Int2DdsDynamicData *data, const char *field, uint32_t value);
Int2DdsRet int2dds_dynamic_data_set_i64  (struct Int2DdsDynamicData *data, const char *field, int64_t  value);
Int2DdsRet int2dds_dynamic_data_set_u64  (struct Int2DdsDynamicData *data, const char *field, uint64_t value);
Int2DdsRet int2dds_dynamic_data_set_f32  (struct Int2DdsDynamicData *data, const char *field, float    value);
Int2DdsRet int2dds_dynamic_data_set_f64  (struct Int2DdsDynamicData *data, const char *field, double   value);

/* DynamicValue scalar constructors (macro-generated; manually declared). */
Int2DdsRet int2dds_dynamic_value_bool    (bool     value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_i8      (int8_t   value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_i16     (int16_t  value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_i32     (int32_t  value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_i64     (int64_t  value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_u8      (uint8_t  value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_u16     (uint16_t value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_u32     (uint32_t value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_u64     (uint64_t value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_f32     (float    value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_f64     (double   value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_byte    (uint8_t  value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_bitmask (uint64_t value, struct Int2DdsDynamicValue **out);
Int2DdsRet int2dds_dynamic_value_bitset  (uint64_t value, struct Int2DdsDynamicValue **out);

/* DynamicValue scalar extractors (macro-generated; manually declared). */
Int2DdsRet int2dds_dynamic_value_as_bool (const struct Int2DdsDynamicValue *value, bool     *out);
Int2DdsRet int2dds_dynamic_value_as_i8   (const struct Int2DdsDynamicValue *value, int8_t   *out);
Int2DdsRet int2dds_dynamic_value_as_i16  (const struct Int2DdsDynamicValue *value, int16_t  *out);
Int2DdsRet int2dds_dynamic_value_as_i32  (const struct Int2DdsDynamicValue *value, int32_t  *out);
Int2DdsRet int2dds_dynamic_value_as_i64  (const struct Int2DdsDynamicValue *value, int64_t  *out);
Int2DdsRet int2dds_dynamic_value_as_u8   (const struct Int2DdsDynamicValue *value, uint8_t  *out);
Int2DdsRet int2dds_dynamic_value_as_u16  (const struct Int2DdsDynamicValue *value, uint16_t *out);
Int2DdsRet int2dds_dynamic_value_as_u32  (const struct Int2DdsDynamicValue *value, uint32_t *out);
Int2DdsRet int2dds_dynamic_value_as_u64  (const struct Int2DdsDynamicValue *value, uint64_t *out);
Int2DdsRet int2dds_dynamic_value_as_f32  (const struct Int2DdsDynamicValue *value, float    *out);
Int2DdsRet int2dds_dynamic_value_as_f64  (const struct Int2DdsDynamicValue *value, double   *out);

#endif  /* INT2DDS_FFI_H */
