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

#define INT2DDS_STATUS_DATA_ON_READERS (1 << 0)

#define INT2DDS_STATUS_DATA_AVAILABLE (1 << 1)

#define INT2DDS_STATUS_SAMPLE_REJECTED (1 << 2)

#define INT2DDS_STATUS_LIVELINESS_CHANGED (1 << 3)

#define INT2DDS_STATUS_REQUESTED_DEADLINE_MISSED (1 << 4)

#define INT2DDS_STATUS_REQUESTED_INCOMPATIBLE_QOS (1 << 5)

#define INT2DDS_STATUS_SAMPLE_LOST (1 << 6)

#define INT2DDS_STATUS_SUBSCRIPTION_MATCHED (1 << 7)

#define INT2DDS_STATUS_OFFERED_DEADLINE_MISSED (1 << 8)

#define INT2DDS_STATUS_OFFERED_INCOMPATIBLE_QOS (1 << 9)

#define INT2DDS_STATUS_LIVELINESS_LOST (1 << 10)

#define INT2DDS_STATUS_PUBLICATION_MATCHED (1 << 11)

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

/**
 * Opaque handle to a DomainParticipantFactory
 */
typedef struct Int2DdsParticipantFactory Int2DdsParticipantFactory;

/**
 * Opaque handle to a Publisher
 */
typedef struct Int2DdsPublisher Int2DdsPublisher;

/**
 * Opaque handle to a StatusCondition
 * StatusCondition is wrapped as a trait object to handle the generic QoS type.
 */
typedef struct Int2DdsStatusCondition Int2DdsStatusCondition;

/**
 * Opaque handle to a Subscriber
 */
typedef struct Int2DdsSubscriber Int2DdsSubscriber;

/**
 * Opaque handle to a Topic
 */
typedef struct Int2DdsTopic Int2DdsTopic;

/**
 * Opaque QoS handle for Topic
 */
typedef struct Int2DdsTopicQos Int2DdsTopicQos;

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
Int2DdsRet int2dds_datareader_qos_set_reliability(struct Int2DdsDataReaderQos *qos, int32_t kind);

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
 * Destroy Topic QoS
 *
 * # Safety
 * - `qos` must be a valid QoS handle
 * - `qos` must not be used after this call
 */
Int2DdsRet int2dds_topic_qos_destroy(struct Int2DdsTopicQos *qos);

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
