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
    #define INT2DDS_QOS_OWNERSHIP_SHARED 0
    #define INT2DDS_QOS_OWNERSHIP_EXCLUSIVE 1
    #define INT2DDS_QOS_DESTINATION_ORDER_BY_RECEPTION 0
    #define INT2DDS_QOS_DESTINATION_ORDER_BY_SOURCE 1
    #define INT2DDS_QOS_LIVELINESS_AUTOMATIC 0
    #define INT2DDS_QOS_LIVELINESS_MANUAL_BY_PARTICIPANT 1
    #define INT2DDS_QOS_LIVELINESS_MANUAL_BY_TOPIC 2
    #define INT2DDS_QOS_DATA_REPRESENTATION_XCDR1 0
    #define INT2DDS_QOS_DATA_REPRESENTATION_XCDR2 2

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
    typedef struct Int2DdsContentFilteredTopic Int2DdsContentFilteredTopic;
    typedef struct Int2DdsWaitSet Int2DdsWaitSet;
    typedef struct Int2DdsGuardCondition Int2DdsGuardCondition;
    typedef struct Int2DdsStatusCondition Int2DdsStatusCondition;
    typedef struct Int2DdsCondition Int2DdsCondition;
    typedef struct Int2DdsConditionSeq Int2DdsConditionSeq;
    typedef struct Int2DdsDataWriterQos Int2DdsDataWriterQos;
    typedef struct Int2DdsDataReaderQos Int2DdsDataReaderQos;
    typedef struct Int2DdsTopicQos Int2DdsTopicQos;
    typedef struct Int2DdsParticipantQos Int2DdsParticipantQos;
    typedef struct Int2DdsPublisherQos Int2DdsPublisherQos;
    typedef struct Int2DdsSubscriberQos Int2DdsSubscriberQos;

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
    Int2DdsRet int2dds_create_participant_with_qos(
        const Int2DdsParticipantFactory *factory,
        const char *name,
        int32_t domain_id,
        const Int2DdsParticipantQos *qos,
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
    Int2DdsRet int2dds_create_publisher_with_qos(
        const Int2DdsParticipant *participant,
        const Int2DdsPublisherQos *qos,
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
    Int2DdsRet int2dds_create_subscriber_with_qos(
        const Int2DdsParticipant *participant,
        const Int2DdsSubscriberQos *qos,
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

    /* Topic with key field metadata */
    Int2DdsRet int2dds_create_topic_keyed_with_key_fields(
        const Int2DdsParticipant *participant,
        const char *topic_name,
        const char *dds_type_name,
        int32_t extensibility,
        bool has_key,
        const Int2DdsTopicQos *qos,
        const uint32_t *field_indices,
        const uint32_t *field_types,
        size_t field_count,
        Int2DdsTopic **topic_out
    );
    /* Topic with full field descriptors for CFT reader-side filtering */
    Int2DdsRet int2dds_create_topic_with_field_descriptors(
        const Int2DdsParticipant *participant,
        const char *topic_name,
        const char *dds_type_name,
        int32_t extensibility,
        bool has_key,
        const Int2DdsTopicQos *qos,
        const char **field_names,
        const uint32_t *field_types,
        const bool *field_is_key,
        size_t field_count,
        Int2DdsTopic **topic_out
    );
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

    /* ContentFilteredTopic */
    Int2DdsRet int2dds_create_contentfilteredtopic(
        const Int2DdsParticipant *participant,
        const char *topic_name,
        const Int2DdsTopic *related_topic,
        const char *filter_expression,
        const char **expression_parameters,
        size_t expression_parameters_count,
        Int2DdsContentFilteredTopic **cft_out
    );
    Int2DdsRet int2dds_delete_contentfilteredtopic(
        Int2DdsContentFilteredTopic *cft
    );

    /* DataReader with ContentFilteredTopic */
    Int2DdsRet int2dds_create_datareader_cft(
        const Int2DdsSubscriber *subscriber,
        const Int2DdsContentFilteredTopic *cft,
        const Int2DdsDataReaderQos *qos,
        Int2DdsDataReader **reader_out
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

    /* --- Writer Status Getters --- */
    Int2DdsRet int2dds_datawriter_get_liveliness_lost_status(
        const Int2DdsDataWriter *writer,
        Int2DdsLivelinessLostStatus *status_out
    );
    Int2DdsRet int2dds_datawriter_get_offered_deadline_missed_status(
        const Int2DdsDataWriter *writer,
        Int2DdsOfferedDeadlineMissedStatus *status_out
    );
    Int2DdsRet int2dds_datawriter_get_offered_incompatible_qos_status(
        const Int2DdsDataWriter *writer,
        Int2DdsOfferedIncompatibleQosStatus *status_out
    );

    Int2DdsRet int2dds_write_serialized(
        const Int2DdsDataWriter *writer,
        const uint8_t *data,
        size_t data_len,
        const uint8_t *key,
        size_t key_len
    );
    /* Instance Management */
    Int2DdsRet int2dds_datawriter_register_instance(
        const Int2DdsDataWriter *writer,
        const uint8_t *key,
        size_t key_len,
        uint8_t handle_out[16]
    );
    Int2DdsRet int2dds_datawriter_unregister_instance(
        const Int2DdsDataWriter *writer,
        const uint8_t *key,
        size_t key_len,
        const uint8_t handle[16]
    );
    Int2DdsRet int2dds_datawriter_dispose(
        const Int2DdsDataWriter *writer,
        const uint8_t *key,
        size_t key_len,
        const uint8_t handle[16]
    );
    Int2DdsRet int2dds_datawriter_lookup_instance(
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

    /* --- Reader Status Getters --- */
    Int2DdsRet int2dds_datareader_get_liveliness_changed_status(
        const Int2DdsDataReader *reader,
        Int2DdsLivelinessChangedStatus *status_out
    );
    Int2DdsRet int2dds_datareader_get_sample_rejected_status(
        const Int2DdsDataReader *reader,
        Int2DdsSampleRejectedStatus *status_out
    );
    Int2DdsRet int2dds_datareader_get_sample_lost_status(
        const Int2DdsDataReader *reader,
        Int2DdsSampleLostStatus *status_out
    );
    Int2DdsRet int2dds_datareader_get_requested_deadline_missed_status(
        const Int2DdsDataReader *reader,
        Int2DdsRequestedDeadlineMissedStatus *status_out
    );
    Int2DdsRet int2dds_datareader_get_requested_incompatible_qos_status(
        const Int2DdsDataReader *reader,
        Int2DdsRequestedIncompatibleQosStatus *status_out
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
    Int2DdsRet int2dds_datawriter_qos_set_ownership(
        Int2DdsDataWriterQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_datawriter_qos_set_ownership_strength(
        Int2DdsDataWriterQos *qos,
        int32_t value
    );
    Int2DdsRet int2dds_datawriter_qos_set_resource_limits(
        Int2DdsDataWriterQos *qos,
        int32_t max_samples,
        int32_t max_instances,
        int32_t max_samples_per_instance
    );
    Int2DdsRet int2dds_datawriter_qos_set_lifespan(
        Int2DdsDataWriterQos *qos,
        int64_t duration_ns
    );
    Int2DdsRet int2dds_datawriter_qos_set_destination_order(
        Int2DdsDataWriterQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_datawriter_qos_set_latency_budget(
        Int2DdsDataWriterQos *qos,
        int64_t duration_ns
    );
    Int2DdsRet int2dds_datawriter_qos_set_transport_priority(
        Int2DdsDataWriterQos *qos,
        int32_t priority
    );
    Int2DdsRet int2dds_datawriter_qos_set_user_data(
        Int2DdsDataWriterQos *qos,
        const uint8_t *data,
        size_t data_len
    );
    Int2DdsRet int2dds_datawriter_qos_set_writer_data_lifecycle(
        Int2DdsDataWriterQos *qos,
        bool autodispose_unregistered_instances
    );
    Int2DdsRet int2dds_datawriter_qos_set_data_representation(
        Int2DdsDataWriterQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_datawriter_qos_set_deadline(
        Int2DdsDataWriterQos *qos,
        int64_t period_ns
    );
    Int2DdsRet int2dds_datawriter_qos_set_liveliness(
        Int2DdsDataWriterQos *qos,
        int32_t kind,
        int64_t lease_duration_ns
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
    Int2DdsRet int2dds_datareader_qos_set_ownership(
        Int2DdsDataReaderQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_datareader_qos_set_resource_limits(
        Int2DdsDataReaderQos *qos,
        int32_t max_samples,
        int32_t max_instances,
        int32_t max_samples_per_instance
    );
    Int2DdsRet int2dds_datareader_qos_set_destination_order(
        Int2DdsDataReaderQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_datareader_qos_set_time_based_filter(
        Int2DdsDataReaderQos *qos,
        int64_t minimum_separation_ns
    );
    Int2DdsRet int2dds_datareader_qos_set_latency_budget(
        Int2DdsDataReaderQos *qos,
        int64_t duration_ns
    );
    Int2DdsRet int2dds_datareader_qos_set_user_data(
        Int2DdsDataReaderQos *qos,
        const uint8_t *data,
        size_t data_len
    );
    Int2DdsRet int2dds_datareader_qos_set_reader_data_lifecycle(
        Int2DdsDataReaderQos *qos,
        int64_t autopurge_nowriter_samples_delay_ns,
        int64_t autopurge_disposed_samples_delay_ns
    );
    Int2DdsRet int2dds_datareader_qos_set_data_representation(
        Int2DdsDataReaderQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_datareader_qos_set_deadline(
        Int2DdsDataReaderQos *qos,
        int64_t period_ns
    );
    Int2DdsRet int2dds_datareader_qos_set_liveliness(
        Int2DdsDataReaderQos *qos,
        int32_t kind,
        int64_t lease_duration_ns
    );
    Int2DdsRet int2dds_datareader_qos_destroy(Int2DdsDataReaderQos *qos);

    /* Topic QoS */
    Int2DdsRet int2dds_topic_qos_create_default(Int2DdsTopicQos **qos_out);
    Int2DdsRet int2dds_topic_qos_set_reliability(
        Int2DdsTopicQos *qos,
        int32_t kind,
        int64_t max_blocking_time_ns
    );
    Int2DdsRet int2dds_topic_qos_set_durability(
        Int2DdsTopicQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_topic_qos_set_history(
        Int2DdsTopicQos *qos,
        int32_t kind,
        int32_t depth
    );
    Int2DdsRet int2dds_topic_qos_set_deadline(
        Int2DdsTopicQos *qos,
        int64_t period_ns
    );
    Int2DdsRet int2dds_topic_qos_set_liveliness(
        Int2DdsTopicQos *qos,
        int32_t kind,
        int64_t lease_duration_ns
    );
    Int2DdsRet int2dds_topic_qos_set_destination_order(
        Int2DdsTopicQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_topic_qos_set_resource_limits(
        Int2DdsTopicQos *qos,
        int32_t max_samples,
        int32_t max_instances,
        int32_t max_samples_per_instance
    );
    Int2DdsRet int2dds_topic_qos_set_transport_priority(
        Int2DdsTopicQos *qos,
        int32_t priority
    );
    Int2DdsRet int2dds_topic_qos_set_lifespan(
        Int2DdsTopicQos *qos,
        int64_t duration_ns
    );
    Int2DdsRet int2dds_topic_qos_set_ownership(
        Int2DdsTopicQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_topic_qos_set_data_representation(
        Int2DdsTopicQos *qos,
        int32_t kind
    );
    Int2DdsRet int2dds_topic_qos_destroy(Int2DdsTopicQos *qos);

    /* Publisher QoS */
    Int2DdsRet int2dds_publisher_qos_create_default(Int2DdsPublisherQos **qos_out);
    Int2DdsRet int2dds_publisher_qos_set_partition(
        Int2DdsPublisherQos *qos,
        const char *const *partitions,
        size_t partition_count
    );
    Int2DdsRet int2dds_publisher_qos_destroy(Int2DdsPublisherQos *qos);

    /* Subscriber QoS */
    Int2DdsRet int2dds_subscriber_qos_create_default(Int2DdsSubscriberQos **qos_out);
    Int2DdsRet int2dds_subscriber_qos_set_partition(
        Int2DdsSubscriberQos *qos,
        const char *const *partitions,
        size_t partition_count
    );
    Int2DdsRet int2dds_subscriber_qos_destroy(Int2DdsSubscriberQos *qos);

    /* Participant QoS */
    Int2DdsRet int2dds_participant_qos_create_default(Int2DdsParticipantQos **qos_out);
    Int2DdsRet int2dds_participant_qos_set_user_data(
        Int2DdsParticipantQos *qos,
        const uint8_t *data,
        size_t data_len
    );
    Int2DdsRet int2dds_participant_qos_add_property(
        Int2DdsParticipantQos *qos,
        const char *name,
        const char *value,
        bool propagate
    );
    Int2DdsRet int2dds_participant_qos_set_multicast_ttl(
        Int2DdsParticipantQos *qos,
        uint8_t ttl
    );
    Int2DdsRet int2dds_participant_qos_destroy(Int2DdsParticipantQos *qos);

    /* Environment-variable configuration helpers (process-wide, take effect
       at next DomainParticipant creation). */
    Int2DdsRet int2dds_env_set_multicast_ttl(uint8_t ttl);
    Int2DdsRet int2dds_env_get_multicast_ttl(uint8_t *ttl_out, bool *has_value_out);

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
        Int2DdsOnOfferedIncompatibleQosCallback on_offered_incompatible_qos;
        Int2DdsOnLivelinessLostCallback on_liveliness_lost;
        Int2DdsUserContext user_context;
    } Int2DdsDataWriterListener;

    /* DataReader Listener */
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

    /* DataReader with ContentFilteredTopic and Listener */
    Int2DdsRet int2dds_create_datareader_cft_with_listener(
        const Int2DdsSubscriber *subscriber,
        const Int2DdsContentFilteredTopic *cft,
        const Int2DdsDataReaderQos *qos,
        const Int2DdsDataReaderListener *listener,
        uint32_t mask,
        Int2DdsDataReader **reader_out
    );

    /* === Dynamic types: TypeObject introspection + DynamicData decoding === */
    typedef struct Int2DdsTypeObject Int2DdsTypeObject;
    typedef struct Int2DdsPublicationBuiltinData Int2DdsPublicationBuiltinData;
    typedef struct Int2DdsTypeInfo Int2DdsTypeInfo;
    typedef struct Int2DdsDynamicData Int2DdsDynamicData;

    typedef struct Int2DdsMemberInfo {
        uint32_t member_id;
        int32_t kind;
        int32_t flags;
    } Int2DdsMemberInfo;

    /* Discovery */
    Int2DdsRet int2dds_get_builtin_subscriber(const Int2DdsParticipant *participant, Int2DdsSubscriber **out);
    Int2DdsRet int2dds_take_publication_data(const Int2DdsSubscriber *builtin_sub, const char *topic_name_filter, int32_t timeout_ms, Int2DdsPublicationBuiltinData **out);
    void int2dds_publication_data_destroy(Int2DdsPublicationBuiltinData *p);
    Int2DdsRet int2dds_publication_data_topic_name(const Int2DdsPublicationBuiltinData *p, char *buf, uintptr_t buf_len, uintptr_t *out_len);
    Int2DdsRet int2dds_publication_data_type_name(const Int2DdsPublicationBuiltinData *p, char *buf, uintptr_t buf_len, uintptr_t *out_len);
    Int2DdsRet int2dds_publication_data_take_type_object(const Int2DdsPublicationBuiltinData *p, Int2DdsTypeObject **out);
    Int2DdsRet int2dds_wait_for_type_object(const Int2DdsParticipant *participant, const char *topic_name, int32_t timeout_ms, Int2DdsTypeObject **type_obj_out, char *type_name_buf, uintptr_t type_name_buf_len, uintptr_t *out_len);

    /* TypeObject introspection */
    void int2dds_type_object_destroy(Int2DdsTypeObject *t);
    Int2DdsRet int2dds_type_object_extensibility(const Int2DdsTypeObject *t, int32_t *out);
    Int2DdsRet int2dds_type_object_member_count(const Int2DdsTypeObject *t, uint32_t *out);
    Int2DdsRet int2dds_type_object_member_info(const Int2DdsTypeObject *t, uint32_t index, Int2DdsMemberInfo *out);
    Int2DdsRet int2dds_type_object_member_name(const Int2DdsTypeObject *t, uint32_t index, char *buf, uintptr_t buf_len, uintptr_t *out_len);
    Int2DdsRet int2dds_type_object_find_member(const Int2DdsTypeObject *t, const char *name, uint32_t *index_out);
    Int2DdsRet int2dds_create_topic_with_type_object(const Int2DdsParticipant *participant, const char *topic_name, const char *type_name, const Int2DdsTypeObject *type_obj, const Int2DdsTopicQos *qos, Int2DdsTopic **out);

    /* TypeInfo builder */
    Int2DdsRet int2dds_type_info_create(const char *type_name, int32_t extensibility, Int2DdsTypeInfo **out);
    Int2DdsRet int2dds_type_info_add_field(Int2DdsTypeInfo *type_info, const char *field_name, int32_t field_type, int32_t flags);
    Int2DdsRet int2dds_type_info_add_string_field(Int2DdsTypeInfo *type_info, const char *field_name, uint32_t bound, int32_t flags);
    Int2DdsRet int2dds_type_info_add_wstring_field(Int2DdsTypeInfo *type_info, const char *field_name, uint32_t bound, int32_t flags);
    Int2DdsRet int2dds_type_info_add_sequence_field(Int2DdsTypeInfo *type_info, const char *field_name, int32_t element_type, uint32_t bound, int32_t flags);
    Int2DdsRet int2dds_type_info_add_array_field(Int2DdsTypeInfo *type_info, const char *field_name, int32_t element_type, uint32_t array_size, int32_t flags);
    Int2DdsRet int2dds_type_info_add_named_type_field(Int2DdsTypeInfo *type_info, const char *field_name, const char *type_hash_name, int32_t flags);
    Int2DdsRet int2dds_type_info_add_sequence_of_named_field(Int2DdsTypeInfo *type_info, const char *field_name, const char *element_hash_name, uint32_t bound, int32_t flags);
    Int2DdsRet int2dds_type_info_add_array_of_named_field(Int2DdsTypeInfo *type_info, const char *field_name, const char *element_hash_name, uint32_t array_size, int32_t flags);
    void int2dds_type_info_destroy(Int2DdsTypeInfo *type_info);
    Int2DdsRet int2dds_type_info_to_type_object(const Int2DdsTypeInfo *type_info, Int2DdsTypeObject **out);
    Int2DdsRet int2dds_create_topic_with_type_info(const Int2DdsParticipant *participant, const char *topic_name, const Int2DdsTypeInfo *type_info, const Int2DdsTopicQos *qos, Int2DdsTopic **out);

    /* DynamicData decoding (dotted/indexed field paths, e.g. "pos.x", "items[2]") */
    Int2DdsRet int2dds_dynamic_data_from_sample(const Int2DdsParticipant *participant, const uint8_t *bytes, uintptr_t len, const Int2DdsTypeObject *type_obj, Int2DdsDynamicData **out);
    void int2dds_dynamic_data_destroy(Int2DdsDynamicData *d);
    Int2DdsRet int2dds_dynamic_data_get_bool(const Int2DdsDynamicData *data, const char *field_path, bool *out);
    Int2DdsRet int2dds_dynamic_data_get_i8(const Int2DdsDynamicData *data, const char *field_path, int8_t *out);
    Int2DdsRet int2dds_dynamic_data_get_u8(const Int2DdsDynamicData *data, const char *field_path, uint8_t *out);
    Int2DdsRet int2dds_dynamic_data_get_i16(const Int2DdsDynamicData *data, const char *field_path, int16_t *out);
    Int2DdsRet int2dds_dynamic_data_get_u16(const Int2DdsDynamicData *data, const char *field_path, uint16_t *out);
    Int2DdsRet int2dds_dynamic_data_get_i32(const Int2DdsDynamicData *data, const char *field_path, int32_t *out);
    Int2DdsRet int2dds_dynamic_data_get_u32(const Int2DdsDynamicData *data, const char *field_path, uint32_t *out);
    Int2DdsRet int2dds_dynamic_data_get_i64(const Int2DdsDynamicData *data, const char *field_path, int64_t *out);
    Int2DdsRet int2dds_dynamic_data_get_u64(const Int2DdsDynamicData *data, const char *field_path, uint64_t *out);
    Int2DdsRet int2dds_dynamic_data_get_f32(const Int2DdsDynamicData *data, const char *field_path, float *out);
    Int2DdsRet int2dds_dynamic_data_get_f64(const Int2DdsDynamicData *data, const char *field_path, double *out);
    Int2DdsRet int2dds_dynamic_data_get_char8(const Int2DdsDynamicData *data, const char *field_path, uint8_t *out);
    Int2DdsRet int2dds_dynamic_data_get_string(const Int2DdsDynamicData *data, const char *field_path, char *out_buf, uintptr_t buf_cap, uintptr_t *out_len);
    Int2DdsRet int2dds_dynamic_data_get_len(const Int2DdsDynamicData *data, const char *field_path, uintptr_t *out);
    Int2DdsRet int2dds_dynamic_data_get_member(const Int2DdsDynamicData *data, const char *field_path, Int2DdsDynamicData **out);
""")

# XML-defined runtime types + dynamic pub/sub + the DynamicValue tree.
ffi.cdef("""
    typedef struct Int2DdsDynamicTypeSupport Int2DdsDynamicTypeSupport;
    typedef struct Int2DdsDynamicDataWriter Int2DdsDynamicDataWriter;
    typedef struct Int2DdsDynamicDataReader Int2DdsDynamicDataReader;
    typedef struct Int2DdsDynamicValue Int2DdsDynamicValue;
    typedef struct Int2DdsXmlTypeRegistry Int2DdsXmlTypeRegistry;

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

    /* XML type registry */
    Int2DdsRet int2dds_xml_type_registry_create(Int2DdsXmlTypeRegistry **out);
    Int2DdsRet int2dds_xml_type_registry_from_file(const char *path, Int2DdsXmlTypeRegistry **out);
    Int2DdsRet int2dds_xml_type_registry_load_file(Int2DdsXmlTypeRegistry *registry, const char *path);
    Int2DdsRet int2dds_xml_type_registry_load_str(Int2DdsXmlTypeRegistry *registry, const char *xml);
    Int2DdsRet int2dds_xml_type_registry_get_type_support(const Int2DdsXmlTypeRegistry *registry, const char *name, Int2DdsDynamicTypeSupport **out);
    Int2DdsRet int2dds_xml_type_registry_get_type_object(const Int2DdsXmlTypeRegistry *registry, const char *name, Int2DdsTypeObject **out);
    Int2DdsRet int2dds_xml_type_registry_type_count(const Int2DdsXmlTypeRegistry *registry, uintptr_t *out);
    Int2DdsRet int2dds_xml_type_registry_type_name(const Int2DdsXmlTypeRegistry *registry, uintptr_t index, char *buf, uintptr_t buf_len, uintptr_t *out_len);
    void int2dds_xml_type_registry_destroy(Int2DdsXmlTypeRegistry *registry);

    /* Dynamic type support + endpoints */
    void int2dds_dynamic_type_support_destroy(Int2DdsDynamicTypeSupport *s);
    Int2DdsRet int2dds_create_topic_dynamic(const Int2DdsParticipant *participant, const char *topic_name, const Int2DdsDynamicTypeSupport *type_support, const Int2DdsTopicQos *qos, Int2DdsTopic **out);
    Int2DdsRet int2dds_create_datawriter_dynamic(const Int2DdsPublisher *publisher, const Int2DdsTopic *topic, const Int2DdsDynamicTypeSupport *type_support, const Int2DdsDataWriterQos *qos, Int2DdsDynamicDataWriter **out);
    Int2DdsRet int2dds_create_datareader_dynamic(const Int2DdsSubscriber *subscriber, const Int2DdsTopic *topic, const Int2DdsDynamicTypeSupport *type_support, const Int2DdsDataReaderQos *qos, Int2DdsDynamicDataReader **out);
    void int2dds_dynamic_writer_destroy(Int2DdsDynamicDataWriter *w);
    void int2dds_dynamic_reader_destroy(Int2DdsDynamicDataReader *r);
    Int2DdsRet int2dds_dynamic_writer_publication_matched_count(const Int2DdsDynamicDataWriter *writer, int32_t *out);
    Int2DdsRet int2dds_dynamic_reader_subscription_matched_count(const Int2DdsDynamicDataReader *reader, int32_t *out);
    Int2DdsRet int2dds_dynamic_writer_write(const Int2DdsDynamicDataWriter *writer, const Int2DdsDynamicData *data);
    Int2DdsRet int2dds_dynamic_reader_take(const Int2DdsDynamicDataReader *reader, Int2DdsDynamicData **out_data, Int2DdsSampleInfo *out_info);

    /* Writable DynamicData */
    Int2DdsRet int2dds_dynamic_data_create(const Int2DdsDynamicTypeSupport *type_support, Int2DdsDynamicData **out);
    Int2DdsRet int2dds_dynamic_data_set_bool(Int2DdsDynamicData *data, const char *field, bool value);
    Int2DdsRet int2dds_dynamic_data_set_i8(Int2DdsDynamicData *data, const char *field, int8_t value);
    Int2DdsRet int2dds_dynamic_data_set_u8(Int2DdsDynamicData *data, const char *field, uint8_t value);
    Int2DdsRet int2dds_dynamic_data_set_i16(Int2DdsDynamicData *data, const char *field, int16_t value);
    Int2DdsRet int2dds_dynamic_data_set_u16(Int2DdsDynamicData *data, const char *field, uint16_t value);
    Int2DdsRet int2dds_dynamic_data_set_i32(Int2DdsDynamicData *data, const char *field, int32_t value);
    Int2DdsRet int2dds_dynamic_data_set_u32(Int2DdsDynamicData *data, const char *field, uint32_t value);
    Int2DdsRet int2dds_dynamic_data_set_i64(Int2DdsDynamicData *data, const char *field, int64_t value);
    Int2DdsRet int2dds_dynamic_data_set_u64(Int2DdsDynamicData *data, const char *field, uint64_t value);
    Int2DdsRet int2dds_dynamic_data_set_f32(Int2DdsDynamicData *data, const char *field, float value);
    Int2DdsRet int2dds_dynamic_data_set_f64(Int2DdsDynamicData *data, const char *field, double value);
    Int2DdsRet int2dds_dynamic_data_set_char8(Int2DdsDynamicData *data, const char *field, uint8_t value);
    Int2DdsRet int2dds_dynamic_data_set_string(Int2DdsDynamicData *data, const char *field, const char *value);
    Int2DdsRet int2dds_dynamic_data_set_value(Int2DdsDynamicData *data, const char *field, Int2DdsDynamicValue *value);
    Int2DdsRet int2dds_dynamic_data_get_value(const Int2DdsDynamicData *data, const char *path, Int2DdsDynamicValue **out);

    /* DynamicValue tree: constructors */
    Int2DdsRet int2dds_dynamic_value_bool(bool value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_i8(int8_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_i16(int16_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_i32(int32_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_i64(int64_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_u8(uint8_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_u16(uint16_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_u32(uint32_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_u64(uint64_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_f32(float value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_f64(double value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_byte(uint8_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_bitmask(uint64_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_bitset(uint64_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_char8(uint8_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_string(const char *value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_wstring(const char *value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_enum(const char *name, int32_t value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_struct(const Int2DdsDynamicData *data, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_sequence(Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_array(Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_map(Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_union(Int2DdsDynamicValue *discriminator, Int2DdsDynamicValue *value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_push(Int2DdsDynamicValue *collection, Int2DdsDynamicValue *element);
    Int2DdsRet int2dds_dynamic_value_map_insert(Int2DdsDynamicValue *map, Int2DdsDynamicValue *key, Int2DdsDynamicValue *value);
    void int2dds_dynamic_value_destroy(Int2DdsDynamicValue *value);

    /* DynamicValue tree: inspection */
    Int2DdsRet int2dds_dynamic_value_kind(const Int2DdsDynamicValue *value, int32_t *out);
    Int2DdsRet int2dds_dynamic_value_as_bool(const Int2DdsDynamicValue *value, bool *out);
    Int2DdsRet int2dds_dynamic_value_as_i8(const Int2DdsDynamicValue *value, int8_t *out);
    Int2DdsRet int2dds_dynamic_value_as_i16(const Int2DdsDynamicValue *value, int16_t *out);
    Int2DdsRet int2dds_dynamic_value_as_i32(const Int2DdsDynamicValue *value, int32_t *out);
    Int2DdsRet int2dds_dynamic_value_as_i64(const Int2DdsDynamicValue *value, int64_t *out);
    Int2DdsRet int2dds_dynamic_value_as_u8(const Int2DdsDynamicValue *value, uint8_t *out);
    Int2DdsRet int2dds_dynamic_value_as_u16(const Int2DdsDynamicValue *value, uint16_t *out);
    Int2DdsRet int2dds_dynamic_value_as_u32(const Int2DdsDynamicValue *value, uint32_t *out);
    Int2DdsRet int2dds_dynamic_value_as_u64(const Int2DdsDynamicValue *value, uint64_t *out);
    Int2DdsRet int2dds_dynamic_value_as_f32(const Int2DdsDynamicValue *value, float *out);
    Int2DdsRet int2dds_dynamic_value_as_f64(const Int2DdsDynamicValue *value, double *out);
    Int2DdsRet int2dds_dynamic_value_as_char8(const Int2DdsDynamicValue *value, uint8_t *out);
    Int2DdsRet int2dds_dynamic_value_as_string(const Int2DdsDynamicValue *value, char *buf, uintptr_t buf_len, uintptr_t *out_len);
    Int2DdsRet int2dds_dynamic_value_as_enum(const Int2DdsDynamicValue *value, char *buf, uintptr_t buf_len, uintptr_t *out_len, int32_t *out_value);
    Int2DdsRet int2dds_dynamic_value_as_bitmask(const Int2DdsDynamicValue *value, uint64_t *out);
    Int2DdsRet int2dds_dynamic_value_as_bitset(const Int2DdsDynamicValue *value, uint64_t *out);
    Int2DdsRet int2dds_dynamic_value_len(const Int2DdsDynamicValue *value, uintptr_t *out);
    Int2DdsRet int2dds_dynamic_value_element(const Int2DdsDynamicValue *value, uintptr_t index, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_map_key(const Int2DdsDynamicValue *value, uintptr_t index, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_map_value(const Int2DdsDynamicValue *value, uintptr_t index, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_as_struct(const Int2DdsDynamicValue *value, Int2DdsDynamicData **out);
    Int2DdsRet int2dds_dynamic_value_union_discriminator(const Int2DdsDynamicValue *value, Int2DdsDynamicValue **out);
    Int2DdsRet int2dds_dynamic_value_union_value(const Int2DdsDynamicValue *value, Int2DdsDynamicValue **out);
""")

# XML configuration: load QoS profiles + build whole participant trees from a
# <domain_participant_library> (mirrors the Rust DomainParticipantFactory config APIs).
ffi.cdef("""
    typedef struct Int2DdsConfiguredParticipant Int2DdsConfiguredParticipant;

    Int2DdsRet int2dds_load_profiles(const Int2DdsParticipantFactory *factory, const char *const *paths, size_t count);
    Int2DdsRet int2dds_get_dynamic_type_support(const Int2DdsParticipantFactory *factory, const char *type_name, Int2DdsDynamicTypeSupport **out);
    Int2DdsRet int2dds_create_participant_from_config(const Int2DdsParticipantFactory *factory, const char *path, Int2DdsConfiguredParticipant **out);
    Int2DdsRet int2dds_configured_participant_get_datawriter(const Int2DdsConfiguredParticipant *configured, const char *name, Int2DdsDynamicDataWriter **out);
    Int2DdsRet int2dds_configured_participant_get_datareader(const Int2DdsConfiguredParticipant *configured, const char *name, Int2DdsDynamicDataReader **out);
    void int2dds_configured_participant_destroy(Int2DdsConfiguredParticipant *configured);
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
    package_dir = Path(__file__).resolve().parent.parent.parent
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
