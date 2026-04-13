#ifndef INT2DDS_FFI_H
#define INT2DDS_FFI_H

#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

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

#define INT2DDS_STATUS_INCONSISTENT_TOPIC (1 << 0)

#define INT2DDS_STATUS_OFFERED_DEADLINE_MISSED (1 << 1)

#define INT2DDS_STATUS_REQUESTED_DEADLINE_MISSED (1 << 2)

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

#define INT2DDS_FIELD_ENUM 14

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

/**
 * Opaque handle to a Publisher
 */
typedef struct Int2DdsPublisher Int2DdsPublisher;

/**
 * Opaque QoS handle for Publisher
 */
typedef struct Int2DdsPublisherQos Int2DdsPublisherQos;

/**
 * Opaque sequence of (serialized data, SampleInfo) pairs for batch read/take
 */
typedef struct Int2DdsSampleSeq Int2DdsSampleSeq;

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
 * Opaque handle to a WaitSet
 */
typedef struct Int2DdsWaitSet Int2DdsWaitSet;

/**
 * FFI return codes
 */
typedef int32_t Int2DdsRet;

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

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Create a new GuardCondition
 *
 * # Safety
 * - `condition_out` must be a valid pointer to a null pointer
 * - The returned condition must be freed with `int2dds_guard_condition_delete`
 */
Int2DdsRet int2dds_guard_condition_new(struct Int2DdsGuardCondition **condition_out);

/**
 * Set the trigger value of a GuardCondition
 *
 * # Safety
 * - `condition` must be a valid guard condition
 * - `value` is the new trigger value (true to trigger, false to reset)
 */
Int2DdsRet int2dds_guard_condition_set_trigger_value(const struct Int2DdsGuardCondition *condition,
                                                     bool value);

/**
 * Get the trigger value of a GuardCondition
 *
 * # Safety
 * - `condition` must be a valid guard condition
 * - `value_out` must be a valid pointer
 */
Int2DdsRet int2dds_guard_condition_get_trigger_value(const struct Int2DdsGuardCondition *condition,
                                                     bool *value_out);

/**
 * Delete a GuardCondition
 *
 * # Safety
 * - `condition` must be a valid guard condition
 * - `condition` must not be used after this call
 * - The condition should be detached from any WaitSets first
 */
Int2DdsRet int2dds_guard_condition_delete(struct Int2DdsGuardCondition *condition);

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
 * Free a SubscriptionBuiltinTopicData obtained from discovery.
 */
Int2DdsRet int2dds_subscription_builtin_topic_data_destroy(struct Int2DdsSubscriptionBuiltinTopicData *data);

/**
 * Create a DomainParticipant
 *
 * # Safety
 * - `name` must be a valid null-terminated C string or null
 * - `participant_out` must be a valid pointer to a null pointer
 * - The returned participant must be freed with `int2dds_delete_participant`
 */
Int2DdsRet int2dds_create_participant(const struct Int2DdsParticipantFactory *_factory,
                                      const char *name,
                                      int32_t domain_id,
                                      struct Int2DdsParticipant **participant_out);

/**
 * Create a DomainParticipant using a QoS profile path
 *
 * # Safety
 * - `name` must be a valid null-terminated C string or null
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `participant_out` must be a valid pointer to a null pointer
 * - The returned participant must be freed with `int2dds_delete_participant`
 */
Int2DdsRet int2dds_create_participant_with_profile(const struct Int2DdsParticipantFactory *_factory,
                                                   const char *name,
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
 * Create a Publisher
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `qos` can be null for default QoS
 * - `publisher_out` must be a valid pointer to a null pointer
 * - The returned publisher must be freed with `int2dds_delete_publisher`
 */
Int2DdsRet int2dds_create_publisher(const struct Int2DdsParticipant *participant,
                                    struct Int2DdsPublisher **publisher_out);

/**
 * Create a Publisher with QoS
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `qos` must be a valid publisher QoS handle
 * - `publisher_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_create_publisher_with_qos(const struct Int2DdsParticipant *participant,
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
 * - `writer_out` must be a valid pointer to a null pointer
 * - The returned writer must be freed with `int2dds_delete_datawriter`
 */
Int2DdsRet int2dds_create_datawriter(const struct Int2DdsPublisher *publisher,
                                     const struct Int2DdsTopic *topic,
                                     const struct Int2DdsDataWriterQos *qos,
                                     struct Int2DdsDataWriter **writer_out);

/**
 * Create a DataWriter with listener callbacks
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `topic` must be a valid topic
 * - `qos` can be null for default QoS
 * - `listener` can be null for no listener
 * - `mask` specifies which status changes trigger callbacks
 * - `writer_out` must be a valid pointer to a null pointer
 * - The returned writer must be freed with `int2dds_delete_datawriter`
 * - Listener callbacks must be thread-safe and remain valid until writer is deleted
 */
Int2DdsRet int2dds_create_datawriter_with_listener(const struct Int2DdsPublisher *publisher,
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
 * - `writer_out` must be a valid pointer to a null pointer
 * - The returned writer must be freed with `int2dds_delete_datawriter`
 */
Int2DdsRet int2dds_create_datawriter_with_profile(const struct Int2DdsPublisher *publisher,
                                                  const struct Int2DdsTopic *topic,
                                                  const char *qos_path,
                                                  struct Int2DdsDataWriter **writer_out);

/**
 * Create a DataWriter with listener callbacks using a QoS profile path
 *
 * # Safety
 * - `publisher` must be a valid publisher
 * - `topic` must be a valid topic
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `listener` can be null for no listener
 * - `mask` specifies which status changes trigger callbacks
 * - `writer_out` must be a valid pointer to a null pointer
 * - The returned writer must be freed with `int2dds_delete_datawriter`
 * - Listener callbacks must be thread-safe and remain valid until writer is deleted
 */
Int2DdsRet int2dds_create_datawriter_with_profile_and_listener(const struct Int2DdsPublisher *publisher,
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
 * Get publication matched status
 *
 * Returns the number of matched readers for this DataWriter.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `total_count_out` must be a valid pointer
 * - `current_count_out` must be a valid pointer
 */
Int2DdsRet int2dds_get_publication_matched_status(const struct Int2DdsDataWriter *writer,
                                                  int32_t *total_count_out,
                                                  int32_t *current_count_out);

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
 * - `key`: Pointer to the serialized key bytes (can be null if no key)
 * - `key_len`: Length of the key bytes
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `data` must point to at least `data_len` readable bytes
 * - If `key` is not null, it must point to at least `key_len` readable bytes
 */
Int2DdsRet int2dds_write_serialized(const struct Int2DdsDataWriter *writer,
                                    const uint8_t *data,
                                    uintptr_t data_len,
                                    const uint8_t *key,
                                    uintptr_t key_len);

/**
 * Write pre-serialized data with an explicit source timestamp.
 *
 * Same as `int2dds_write_serialized`, but allows the caller to specify a
 * source timestamp instead of using the current time.
 *
 * # Parameters
 * - `writer`: A valid datawriter
 * - `data`: Pointer to the CDR-serialized byte buffer
 * - `data_len`: Length of the serialized data in bytes
 * - `key`: Pointer to the serialized key bytes (can be null if no key)
 * - `key_len`: Length of the key bytes
 * - `timestamp_sec`: Seconds component of the source timestamp
 * - `timestamp_nanosec`: Nanoseconds component of the source timestamp
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `data` must point to at least `data_len` readable bytes
 * - If `key` is not null, it must point to at least `key_len` readable bytes
 */
Int2DdsRet int2dds_write_serialized_w_timestamp(const struct Int2DdsDataWriter *writer,
                                                const uint8_t *data,
                                                uintptr_t data_len,
                                                const uint8_t *key,
                                                uintptr_t key_len,
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
 * Register an instance with serialized key bytes
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
 * Dispose an instance with serialized key bytes
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `key` must point to at least `key_len` readable bytes
 * - `handle` must be a valid pointer to a 16-byte instance handle (or null for NIL)
 */
Int2DdsRet int2dds_datawriter_dispose(const struct Int2DdsDataWriter *writer,
                                      const uint8_t *key,
                                      uintptr_t key_len,
                                      const uint8_t (*handle)[16]);

/**
 * Unregister an instance with serialized key bytes
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `key` must point to at least `key_len` readable bytes
 * - `handle` must be a valid pointer to a 16-byte instance handle (or null for NIL)
 */
Int2DdsRet int2dds_datawriter_unregister_instance(const struct Int2DdsDataWriter *writer,
                                                  const uint8_t *key,
                                                  uintptr_t key_len,
                                                  const uint8_t (*handle)[16]);

/**
 * Lookup an instance handle from serialized key bytes
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
 * - `qos` can be null for default QoS
 * - `subscriber_out` must be a valid pointer to a null pointer
 * - The returned subscriber must be freed with `int2dds_delete_subscriber`
 */
Int2DdsRet int2dds_create_subscriber(const struct Int2DdsParticipant *participant,
                                     struct Int2DdsSubscriber **subscriber_out);

/**
 * Create a Subscriber with QoS
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `qos` must be a valid subscriber QoS handle
 * - `subscriber_out` must be a valid pointer to a null pointer
 */
Int2DdsRet int2dds_create_subscriber_with_qos(const struct Int2DdsParticipant *participant,
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
 * - `reader_out` must be a valid pointer to a null pointer
 * - The returned reader must be freed with `int2dds_delete_datareader`
 */
Int2DdsRet int2dds_create_datareader(const struct Int2DdsSubscriber *subscriber,
                                     const struct Int2DdsTopic *topic,
                                     const struct Int2DdsDataReaderQos *qos,
                                     struct Int2DdsDataReader **reader_out);

/**
 * Create a DataReader with listener callbacks
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `topic` must be a valid topic
 * - `qos` can be null for default QoS
 * - `listener` can be null for no listener
 * - `mask` specifies which status changes trigger callbacks
 * - `reader_out` must be a valid pointer to a null pointer
 * - The returned reader must be freed with `int2dds_delete_datareader`
 * - Listener callbacks must be thread-safe and remain valid until reader is deleted
 */
Int2DdsRet int2dds_create_datareader_with_listener(const struct Int2DdsSubscriber *subscriber,
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
 * - `reader_out` must be a valid pointer to a null pointer
 * - The returned reader must be freed with `int2dds_delete_datareader`
 */
Int2DdsRet int2dds_create_datareader_with_profile(const struct Int2DdsSubscriber *subscriber,
                                                  const struct Int2DdsTopic *topic,
                                                  const char *qos_path,
                                                  struct Int2DdsDataReader **reader_out);

/**
 * Create a DataReader with listener callbacks using a QoS profile path
 *
 * # Safety
 * - `subscriber` must be a valid subscriber
 * - `topic` must be a valid topic
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `listener` can be null for no listener
 * - `mask` specifies which status changes trigger callbacks
 * - `reader_out` must be a valid pointer to a null pointer
 * - The returned reader must be freed with `int2dds_delete_datareader`
 * - Listener callbacks must be thread-safe and remain valid until reader is deleted
 */
Int2DdsRet int2dds_create_datareader_with_profile_and_listener(const struct Int2DdsSubscriber *subscriber,
                                                               const struct Int2DdsTopic *topic,
                                                               const char *qos_path,
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
 * Delete a DataReader
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `reader` must not be used after this call
 */
Int2DdsRet int2dds_delete_datareader(struct Int2DdsDataReader *reader);

/**
 * Get subscription matched status
 *
 * Returns the number of matched writers for this DataReader.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `total_count_out` must be a valid pointer
 * - `current_count_out` must be a valid pointer
 */
Int2DdsRet int2dds_get_subscription_matched_status(const struct Int2DdsDataReader *reader,
                                                   int32_t *total_count_out,
                                                   int32_t *current_count_out);

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
 * The sample is removed from the cache.
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
 * - INT2DDS_RET_ERROR if buffer is too small (actual_size_out will contain the required size)
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `buffer` must point to at least `buffer_capacity` writable bytes
 * - `actual_size_out` and `valid_data_out` must be valid pointers
 */
Int2DdsRet int2dds_take_serialized(const struct Int2DdsDataReader *reader,
                                   uint8_t *buffer,
                                   uintptr_t buffer_capacity,
                                   uintptr_t *actual_size_out,
                                   bool *valid_data_out);

/**
 * Read pre-serialized data from a DataReader without removing from cache.
 *
 * Same as `int2dds_take_serialized` but the sample remains in the cache.
 *
 * # Safety
 * - Same as `int2dds_take_serialized`
 */
Int2DdsRet int2dds_read_serialized(const struct Int2DdsDataReader *reader,
                                   uint8_t *buffer,
                                   uintptr_t buffer_capacity,
                                   uintptr_t *actual_size_out,
                                   bool *valid_data_out);

/**
 * Take pre-serialized data with full SampleInfo
 *
 * # Safety
 * - Same as `int2dds_take_serialized`, plus `info_out` must be a valid pointer
 */
Int2DdsRet int2dds_take_serialized_w_info(const struct Int2DdsDataReader *reader,
                                          uint8_t *buffer,
                                          uintptr_t buffer_capacity,
                                          uintptr_t *actual_size_out,
                                          struct Int2DdsSampleInfo *info_out);

/**
 * Read pre-serialized data with full SampleInfo (sample remains in cache)
 *
 * # Safety
 * - Same as `int2dds_read_serialized`, plus `info_out` must be a valid pointer
 */
Int2DdsRet int2dds_read_serialized_w_info(const struct Int2DdsDataReader *reader,
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
 * - The returned sequence must be freed with `int2dds_sample_seq_delete`
 */
Int2DdsRet int2dds_take_serialized_batch(const struct Int2DdsDataReader *reader,
                                         int32_t max_samples,
                                         struct Int2DdsSampleSeq **seq_out);

/**
 * Read multiple serialized samples as a batch (samples remain in cache)
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `seq_out` must be a valid pointer to a null pointer
 * - The returned sequence must be freed with `int2dds_sample_seq_delete`
 */
Int2DdsRet int2dds_read_serialized_batch(const struct Int2DdsDataReader *reader,
                                         int32_t max_samples,
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
 * Read a single serialized sample with state condition filter
 *
 * # Safety
 * - Same as `int2dds_read_serialized_w_info`, plus state masks
 */
Int2DdsRet int2dds_read_serialized_w_condition(const struct Int2DdsDataReader *reader,
                                               uint8_t *buffer,
                                               uintptr_t buffer_capacity,
                                               uintptr_t *actual_size_out,
                                               struct Int2DdsSampleInfo *info_out,
                                               uint32_t sample_state_mask,
                                               uint32_t view_state_mask,
                                               uint32_t instance_state_mask);

/**
 * Take a single serialized sample with state condition filter
 *
 * # Safety
 * - Same as `int2dds_take_serialized_w_info`, plus state masks
 */
Int2DdsRet int2dds_take_serialized_w_condition(const struct Int2DdsDataReader *reader,
                                               uint8_t *buffer,
                                               uintptr_t buffer_capacity,
                                               uintptr_t *actual_size_out,
                                               struct Int2DdsSampleInfo *info_out,
                                               uint32_t sample_state_mask,
                                               uint32_t view_state_mask,
                                               uint32_t instance_state_mask);

/**
 * Take batch with state condition filter
 *
 * # Safety
 * - Same as `int2dds_take_serialized_batch`, plus state masks
 */
Int2DdsRet int2dds_take_serialized_batch_w_condition(const struct Int2DdsDataReader *reader,
                                                     int32_t max_samples,
                                                     struct Int2DdsSampleSeq **seq_out,
                                                     uint32_t sample_state_mask,
                                                     uint32_t view_state_mask,
                                                     uint32_t instance_state_mask);

/**
 * Read batch with state condition filter
 *
 * # Safety
 * - Same as `int2dds_read_serialized_batch`, plus state masks
 */
Int2DdsRet int2dds_read_serialized_batch_w_condition(const struct Int2DdsDataReader *reader,
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
 * Creates a topic with RawTypeSupport for use with `int2dds_write_serialized()`
 * and `int2dds_take_serialized()`. C users handle CDR serialization themselves
 * using IDL-generated code.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `topic_name` must be a valid null-terminated C string
 * - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
 * - `extensibility`: 0 = Final, 1 = Appendable, 2 = Mutable
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
 * Create a Topic with key support
 *
 * Same as `int2dds_create_topic` but with an explicit `has_key` parameter.
 * Use this when the data type has key fields for instance management
 * (register_instance, unregister_instance, dispose, lookup_instance).
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `topic_name` must be a valid null-terminated C string
 * - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
 * - `extensibility`: 0 = Final, 1 = Appendable, 2 = Mutable
 * - `has_key`: whether the data type has key fields
 * - `qos` can be null for default QoS
 * - `topic_out` must be a valid pointer to a null pointer
 * - The returned topic must be freed with `int2dds_delete_topic`
 */
Int2DdsRet int2dds_create_topic_keyed(const struct Int2DdsParticipant *participant,
                                      const char *topic_name,
                                      const char *dds_type_name,
                                      int32_t extensibility,
                                      bool has_key,
                                      const struct Int2DdsTopicQos *qos,
                                      struct Int2DdsTopic **topic_out);

/**
 * Create a Topic using a QoS profile path
 *
 * Same as `int2dds_create_topic_keyed` but uses a QoS profile path instead of a QoS handle.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `topic_name` must be a valid null-terminated C string
 * - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
 * - `extensibility`: 0 = Final, 1 = Appendable, 2 = Mutable
 * - `has_key`: whether the data type has key fields
 * - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
 * - `topic_out` must be a valid pointer to a null pointer
 * - The returned topic must be freed with `int2dds_delete_topic`
 */
Int2DdsRet int2dds_create_topic_with_profile(const struct Int2DdsParticipant *participant,
                                             const char *topic_name,
                                             const char *dds_type_name,
                                             int32_t extensibility,
                                             bool has_key,
                                             const char *qos_path,
                                             struct Int2DdsTopic **topic_out);

/**
 * Create a Topic with type information for DDS-XTypes discovery
 *
 * Creates a topic using a pre-built `Int2DdsTypeInfo` which provides
 * TypeIdentifier and TypeObject for DDS discovery parameters (0x0069, 0x0072).
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
 * Create a new type info builder.
 *
 * # Safety
 * - `type_name` must be a valid null-terminated C string
 * - `extensibility`: 0 = Final, 1 = Appendable, 2 = Mutable
 * - `out` must be a valid pointer to a null pointer
 * - The returned type info must be freed with `int2dds_type_info_destroy`
 */
Int2DdsRet int2dds_type_info_create(const char *type_name,
                                    int32_t extensibility,
                                    struct Int2DdsTypeInfo **out);

/**
 * Add a field to the type info builder.
 *
 * # Safety
 * - `type_info` must be a valid type info created by `int2dds_type_info_create`
 * - `field_name` must be a valid null-terminated C string
 * - `field_type`: one of the INT2DDS_FIELD_* constants
 * - `is_key`: non-zero if this field is a key field
 */
Int2DdsRet int2dds_type_info_add_field(struct Int2DdsTypeInfo *type_info,
                                       const char *field_name,
                                       int32_t field_type,
                                       int32_t is_key);

/**
 * Add a sequence field to the type info builder.
 *
 * Creates a `PlainSequenceLarge` TypeIdentifier wrapping the element type,
 * matching how int2DDS-Rust represents `Vec<T>` in DDS-XTypes.
 *
 * # Safety
 * - `type_info` must be a valid type info created by `int2dds_type_info_create`
 * - `field_name` must be a valid null-terminated C string
 * - `element_type`: one of the INT2DDS_FIELD_* constants for the sequence element
 * - `bound`: maximum sequence length (0 = unbounded)
 * - `is_key`: non-zero if this field is a key field
 */
Int2DdsRet int2dds_type_info_add_sequence_field(struct Int2DdsTypeInfo *type_info,
                                                const char *field_name,
                                                int32_t element_type,
                                                uint32_t bound,
                                                int32_t is_key);

/**
 * Add an array field to the type info builder.
 *
 * Creates a `PlainArrayLarge` TypeIdentifier wrapping the element type,
 * matching how int2DDS-Rust represents `[T; N]` in DDS-XTypes.
 *
 * # Safety
 * - `type_info` must be a valid type info created by `int2dds_type_info_create`
 * - `field_name` must be a valid null-terminated C string
 * - `element_type`: one of the INT2DDS_FIELD_* constants for the array element
 * - `array_size`: fixed size of the array
 * - `is_key`: non-zero if this field is a key field
 */
Int2DdsRet int2dds_type_info_add_array_field(struct Int2DdsTypeInfo *type_info,
                                             const char *field_name,
                                             int32_t element_type,
                                             uint32_t array_size,
                                             int32_t is_key);

/**
 * Add a named (complex) type field to the type info builder.
 *
 * Creates a `MinimalTypeId(EquivalenceHash::compute(type_hash_name))` TypeIdentifier,
 * matching how int2DDS-Rust represents struct fields via the derive macro's
 * `Fallback` path in `type_to_identifier`.
 *
 * For direct struct fields: pass the struct name (e.g., "InnerStruct").
 * For `Vec<Struct>` fields: pass "Vec < StructName >" (matching Rust `quote!` formatting).
 *
 * # Safety
 * - `type_info` must be a valid type info created by `int2dds_type_info_create`
 * - `field_name` must be a valid null-terminated C string
 * - `type_hash_name` must be a valid null-terminated C string (the type name to hash)
 * - `is_key`: non-zero if this field is a key field
 */
Int2DdsRet int2dds_type_info_add_named_type_field(struct Int2DdsTypeInfo *type_info,
                                                  const char *field_name,
                                                  const char *type_hash_name,
                                                  int32_t is_key);

/**
 * Destroy a type info builder.
 *
 * # Safety
 * - `type_info` must be a valid type info, or null (no-op)
 * - Must not be used after this call
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
 * Wait for conditions to be triggered
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `timeout_ms` is the timeout in milliseconds, or -1 for infinite
 *
 * Returns:
 * - INT2DDS_RET_OK if conditions were triggered
 * - INT2DDS_RET_TIMEOUT if the timeout expired
 * - INT2DDS_RET_ERROR for other errors
 */
Int2DdsRet int2dds_waitset_wait(const struct Int2DdsWaitSet *waitset, int64_t timeout_ms);

/**
 * Wait for conditions to be triggered and return the triggered conditions
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `timeout_ms` is the timeout in milliseconds, or -1 for infinite
 * - `conditions_out` must be a valid pointer to a null pointer
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
Int2DdsRet int2dds_waitset_attach_guard_condition(const struct Int2DdsWaitSet *waitset,
                                                  const struct Int2DdsGuardCondition *condition);

/**
 * Detach a GuardCondition from the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `condition` must be a valid guard condition that was previously attached
 */
Int2DdsRet int2dds_waitset_detach_guard_condition(const struct Int2DdsWaitSet *waitset,
                                                  const struct Int2DdsGuardCondition *condition);

/**
 * Attach a StatusCondition to the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `condition` must be a valid status condition
 */
Int2DdsRet int2dds_waitset_attach_condition(const struct Int2DdsWaitSet *waitset,
                                            const struct Int2DdsStatusCondition *condition);

/**
 * Detach a StatusCondition from the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `condition` must be a valid status condition that was previously attached
 */
Int2DdsRet int2dds_waitset_detach_condition(const struct Int2DdsWaitSet *waitset,
                                            const struct Int2DdsStatusCondition *condition);

/**
 * Attach a DataReader's status condition to the WaitSet
 *
 * This allows waiting for data to arrive on a DataReader.
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `reader` must be a valid datareader
 */
Int2DdsRet int2dds_waitset_attach_datareader(const struct Int2DdsWaitSet *waitset,
                                             const struct Int2DdsDataReader *reader);

/**
 * Detach a DataReader's status condition from the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `reader` must be a valid datareader that was previously attached
 */
Int2DdsRet int2dds_waitset_detach_datareader(const struct Int2DdsWaitSet *waitset,
                                             const struct Int2DdsDataReader *reader);

/**
 * Attach a DataWriter's status condition to the WaitSet
 *
 * This allows waiting for publication matched events on a DataWriter.
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `writer` must be a valid datawriter
 */
Int2DdsRet int2dds_waitset_attach_datawriter(const struct Int2DdsWaitSet *waitset,
                                             const struct Int2DdsDataWriter *writer);

/**
 * Detach a DataWriter's status condition from the WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `writer` must be a valid datawriter that was previously attached
 */
Int2DdsRet int2dds_waitset_detach_datawriter(const struct Int2DdsWaitSet *waitset,
                                             const struct Int2DdsDataWriter *writer);

/**
 * Delete a WaitSet
 *
 * # Safety
 * - `waitset` must be a valid waitset
 * - `waitset` must not be used after this call
 * - All conditions should be detached first
 */
Int2DdsRet int2dds_waitset_delete(struct Int2DdsWaitSet *waitset);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* INT2DDS_FFI_H */
