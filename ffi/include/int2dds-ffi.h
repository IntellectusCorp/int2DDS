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
 * Write data to a DataWriter
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `data` must point to valid serialized data
 * - `data_size` must be the correct size of the data
 *
 * Note: The data must be pre-serialized using CDR format.
 */
Int2DdsRet int2dds_write(const struct Int2DdsDataWriter *writer,
                         const uint8_t *data,
                         uintptr_t data_size);

/**
 * Write data to a DataWriter with key
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `data` must point to valid serialized data
 * - `data_size` must be the correct size of the data
 * - `key` must point to valid serialized key data
 * - `key_size` must be the correct size of the key
 *
 * Note: The data and key must be pre-serialized using CDR format.
 */
Int2DdsRet int2dds_write_with_key(const struct Int2DdsDataWriter *writer,
                                  const uint8_t *data,
                                  uintptr_t data_size,
                                  const uint8_t *key,
                                  uintptr_t key_size);

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
 * Register an instance for subsequent write operations
 *
 * This operation informs the service that the application intends to modify
 * a particular instance, allowing pre-configuration for improved performance.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `key` must point to valid serialized key data
 * - `key_size` must be the correct size of the key
 * - `handle_out` must be a valid pointer to 16-byte array for the instance handle
 *
 * # Returns
 * Returns the instance handle (16 bytes) that can be used in subsequent write/dispose operations
 */
Int2DdsRet int2dds_register_instance(const struct Int2DdsDataWriter *writer,
                                     const uint8_t *key,
                                     uintptr_t key_size,
                                     uint8_t (*handle_out)[16]);

/**
 * Unregister a previously registered instance
 *
 * This operation reverses register_instance, indicating the application
 * no longer intends to modify the instance.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `key` must point to valid serialized key data
 * - `key_size` must be the correct size of the key
 * - `handle` is a pointer to 16-byte instance handle (use all zeros for NIL)
 */
Int2DdsRet int2dds_unregister_instance(const struct Int2DdsDataWriter *writer,
                                       const uint8_t *key,
                                       uintptr_t key_size,
                                       const uint8_t (*handle)[16]);

/**
 * Dispose an instance, indicating it is no longer valid
 *
 * This operation requests the middleware to delete the data instance.
 * DataReaders will be notified of the disposal through instance state changes.
 *
 * # Safety
 * - `writer` must be a valid datawriter
 * - `key` must point to valid serialized key data
 * - `key_size` must be the correct size of the key
 * - `handle` is a pointer to 16-byte instance handle (use all zeros for NIL)
 */
Int2DdsRet int2dds_dispose(const struct Int2DdsDataWriter *writer,
                           const uint8_t *key,
                           uintptr_t key_size,
                           const uint8_t (*handle)[16]);

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
 * Take data from a DataReader (removes from cache)
 *
 * Returns the raw bytes of the next sample, removing it from the cache.
 * The C application must allocate the buffer and provide the buffer size.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `data_buffer` must point to a buffer of at least `buffer_size` bytes
 * - `data_size_out` will be set to the actual size of the data read
 * - `valid_data_out` will be set to true if valid data was read
 *
 * # Returns
 * - INT2DDS_RET_OK if data was successfully read
 * - INT2DDS_RET_NO_DATA if no data is available
 * - INT2DDS_RET_ERROR if buffer is too small (data_size_out will contain required size)
 */
Int2DdsRet int2dds_take(const struct Int2DdsDataReader *reader,
                        uint8_t *data_buffer,
                        uintptr_t buffer_size,
                        uintptr_t *data_size_out,
                        bool *valid_data_out);

/**
 * Read data from a DataReader (keeps in cache)
 *
 * Returns the raw bytes of the next sample without removing it from the cache.
 *
 * # Safety
 * - `reader` must be a valid datareader
 * - `data_buffer` must point to a buffer of at least `buffer_size` bytes
 * - `data_size_out` will be set to the actual size of the data read
 * - `valid_data_out` will be set to true if valid data was read
 */
Int2DdsRet int2dds_read(const struct Int2DdsDataReader *reader,
                        uint8_t *data_buffer,
                        uintptr_t buffer_size,
                        uintptr_t *data_size_out,
                        bool *valid_data_out);

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
 * Create a Topic
 *
 * Creates a topic using RawData type for FFI. The C application is responsible
 * for serializing/deserializing data in CDR format.
 *
 * # Safety
 * - `participant` must be a valid participant
 * - `topic_name` must be a valid null-terminated C string
 * - `type_name` must be a valid null-terminated C string
 * - `qos` can be null for default QoS
 * - `topic_out` must be a valid pointer to a null pointer
 * - The returned topic must be freed with `int2dds_delete_topic`
 */
Int2DdsRet int2dds_create_topic(const struct Int2DdsParticipant *participant,
                                const char *topic_name,
                                const char *type_name,
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
 * - Returns the number of bytes written (excluding null terminator)
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
 * - The returned condition is valid only while the sequence exists
 *
 * Note: The returned condition borrows from the sequence and should NOT be deleted separately.
 * It becomes invalid when the sequence is deleted.
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
