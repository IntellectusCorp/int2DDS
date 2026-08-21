"""cffi bindings for the int2dds-ffi library.

GENERATED FILE -- edits are overwritten. The declarations below are
`ffi/include/int2dds-ffi.h` with its comments removed and its preprocessor
conditionals resolved for plain pre-C23 C; regenerate with

    python python/tools/generate_bindings.py

ABI mode, so the declarations are resolved against the shared library at call
time and never compiled.
"""

from __future__ import annotations

import cffi

ffi = cffi.FFI()

ffi.cdef("""
enum Int2DdsQosPolicyId
 {
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
};
typedef int32_t Int2DdsQosPolicyId;

enum Int2DdsSampleRejectedStatusKind
 {
  NotRejected = 0,
  RejectedByInstancesLimit = 1,
  RejectedBySamplesLimit = 2,
  RejectedBySamplesPerInstanceLimit = 3,
};
typedef int32_t Int2DdsSampleRejectedStatusKind;

typedef struct Int2DdsCondition Int2DdsCondition;

typedef struct Int2DdsConditionSeq Int2DdsConditionSeq;

typedef struct Int2DdsConfiguredParticipant Int2DdsConfiguredParticipant;

typedef struct Int2DdsContentFilteredTopic Int2DdsContentFilteredTopic;

typedef struct Int2DdsDataReader Int2DdsDataReader;

typedef struct Int2DdsDataReaderQos Int2DdsDataReaderQos;

typedef struct Int2DdsDataWriter Int2DdsDataWriter;

typedef struct Int2DdsDataWriterQos Int2DdsDataWriterQos;

typedef struct Int2DdsDynamicData Int2DdsDynamicData;

typedef struct Int2DdsDynamicDataReader Int2DdsDynamicDataReader;

typedef struct Int2DdsDynamicDataWriter Int2DdsDynamicDataWriter;

typedef struct Int2DdsDynamicTypeSupport Int2DdsDynamicTypeSupport;

typedef struct Int2DdsDynamicValue Int2DdsDynamicValue;

typedef struct Int2DdsGuardCondition Int2DdsGuardCondition;

typedef struct Int2DdsParticipant Int2DdsParticipant;

typedef struct Int2DdsParticipantBuiltinTopicData Int2DdsParticipantBuiltinTopicData;

typedef struct Int2DdsParticipantFactory Int2DdsParticipantFactory;

typedef struct Int2DdsParticipantQos Int2DdsParticipantQos;

typedef struct Int2DdsPublicationBuiltinTopicData Int2DdsPublicationBuiltinTopicData;

typedef struct Int2DdsPublicationBuiltinTopicDataSeq Int2DdsPublicationBuiltinTopicDataSeq;

typedef struct Int2DdsPublisher Int2DdsPublisher;

typedef struct Int2DdsPublisherQos Int2DdsPublisherQos;

typedef struct Int2DdsReadCondition Int2DdsReadCondition;

typedef struct Int2DdsSampleSeq Int2DdsSampleSeq;

typedef struct Int2DdsSerializedLoan Int2DdsSerializedLoan;

typedef struct Int2DdsSerializedWriteLoan Int2DdsSerializedWriteLoan;

typedef struct Int2DdsStatusCondition Int2DdsStatusCondition;

typedef struct Int2DdsSubscriber Int2DdsSubscriber;

typedef struct Int2DdsSubscriberQos Int2DdsSubscriberQos;

typedef struct Int2DdsSubscriptionBuiltinTopicData Int2DdsSubscriptionBuiltinTopicData;

typedef struct Int2DdsSubscriptionBuiltinTopicDataSeq Int2DdsSubscriptionBuiltinTopicDataSeq;

typedef struct Int2DdsTopic Int2DdsTopic;

typedef struct Int2DdsTopicQos Int2DdsTopicQos;

typedef struct Int2DdsTypeInfo Int2DdsTypeInfo;

typedef struct Int2DdsTypeObject Int2DdsTypeObject;

typedef struct Int2DdsWaitSet Int2DdsWaitSet;

typedef struct Int2DdsXmlTypeRegistry Int2DdsXmlTypeRegistry;

typedef int32_t Int2DdsRet;

typedef void (*Int2DdsEndpointDiscoveryCallback)(void *ctx,
                                                 int32_t is_writer,
                                                 int32_t is_alive,
                                                 const struct Int2DdsPublicationBuiltinTopicData *pub_data,
                                                 const struct Int2DdsSubscriptionBuiltinTopicData *sub_data,
                                                 const uint8_t (*guid)[16]);

typedef struct Int2DdsMemberInfo {
  uint32_t member_id;
  int32_t kind;
  int32_t flags;
} Int2DdsMemberInfo;

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

typedef struct Int2DdsPublicationMatchedStatus {

  int32_t total_count;

  int32_t total_count_change;

  int32_t current_count;

  int32_t current_count_change;

  uint8_t last_subscription_handle[16];
} Int2DdsPublicationMatchedStatus;

typedef void *Int2DdsUserContext;

typedef void (*Int2DdsOnPublicationMatchedCallback)(struct Int2DdsDataWriter *writer,
                                                    const struct Int2DdsPublicationMatchedStatus *status,
                                                    Int2DdsUserContext user_context);

typedef struct Int2DdsOfferedDeadlineMissedStatus {

  int32_t total_count;

  int32_t total_count_change;

  uint8_t last_instance_handle[16];
} Int2DdsOfferedDeadlineMissedStatus;

typedef void (*Int2DdsOnOfferedDeadlineMissedCallback)(struct Int2DdsDataWriter *writer,
                                                       const struct Int2DdsOfferedDeadlineMissedStatus *status,
                                                       Int2DdsUserContext user_context);

typedef struct Int2DdsOfferedIncompatibleQosStatus {

  int32_t total_count;

  int32_t total_count_change;

  Int2DdsQosPolicyId last_policy_id;

  uint32_t policies_count;
} Int2DdsOfferedIncompatibleQosStatus;

typedef void (*Int2DdsOnOfferedIncompatibleQosCallback)(struct Int2DdsDataWriter *writer,
                                                        const struct Int2DdsOfferedIncompatibleQosStatus *status,
                                                        Int2DdsUserContext user_context);

typedef struct Int2DdsLivelinessLostStatus {

  int32_t total_count;

  int32_t total_count_change;
} Int2DdsLivelinessLostStatus;

typedef void (*Int2DdsOnLivelinessLostCallback)(struct Int2DdsDataWriter *writer,
                                                const struct Int2DdsLivelinessLostStatus *status,
                                                Int2DdsUserContext user_context);

typedef struct Int2DdsDataWriterListener {
  Int2DdsOnPublicationMatchedCallback on_publication_matched;
  Int2DdsOnOfferedDeadlineMissedCallback on_offered_deadline_missed;
  Int2DdsOnOfferedIncompatibleQosCallback on_offered_incompatible_qos;
  Int2DdsOnLivelinessLostCallback on_liveliness_lost;
  Int2DdsUserContext user_context;
} Int2DdsDataWriterListener;

typedef struct Int2DdsOfferedIncompatibleTypeStatus {

  int32_t total_count;

  int32_t total_count_change;
} Int2DdsOfferedIncompatibleTypeStatus;

typedef void (*Int2DdsOnDataAvailableCallback)(struct Int2DdsDataReader *reader,
                                               Int2DdsUserContext user_context);

typedef struct Int2DdsSubscriptionMatchedStatus {

  int32_t total_count;

  int32_t total_count_change;

  int32_t current_count;

  int32_t current_count_change;

  uint8_t last_publication_handle[16];
} Int2DdsSubscriptionMatchedStatus;

typedef void (*Int2DdsOnSubscriptionMatchedCallback)(struct Int2DdsDataReader *reader,
                                                     const struct Int2DdsSubscriptionMatchedStatus *status,
                                                     Int2DdsUserContext user_context);

typedef struct Int2DdsSampleRejectedStatus {

  int32_t total_count;

  int32_t total_count_change;

  Int2DdsSampleRejectedStatusKind last_reason;

  uint8_t last_instance_handle[16];
} Int2DdsSampleRejectedStatus;

typedef void (*Int2DdsOnSampleRejectedCallback)(struct Int2DdsDataReader *reader,
                                                const struct Int2DdsSampleRejectedStatus *status,
                                                Int2DdsUserContext user_context);

typedef struct Int2DdsLivelinessChangedStatus {

  int32_t alive_count;

  int32_t not_alive_count;

  int32_t alive_count_change;

  int32_t not_alive_count_change;

  uint8_t last_publication_handle[16];
} Int2DdsLivelinessChangedStatus;

typedef void (*Int2DdsOnLivelinessChangedCallback)(struct Int2DdsDataReader *reader,
                                                   const struct Int2DdsLivelinessChangedStatus *status,
                                                   Int2DdsUserContext user_context);

typedef struct Int2DdsRequestedDeadlineMissedStatus {

  int32_t total_count;

  int32_t total_count_change;

  uint8_t last_instance_handle[16];
} Int2DdsRequestedDeadlineMissedStatus;

typedef void (*Int2DdsOnRequestedDeadlineMissedCallback)(struct Int2DdsDataReader *reader,
                                                         const struct Int2DdsRequestedDeadlineMissedStatus *status,
                                                         Int2DdsUserContext user_context);

typedef struct Int2DdsRequestedIncompatibleQosStatus {

  int32_t total_count;

  int32_t total_count_change;

  Int2DdsQosPolicyId last_policy_id;

  uint32_t policies_count;
} Int2DdsRequestedIncompatibleQosStatus;

typedef void (*Int2DdsOnRequestedIncompatibleQosCallback)(struct Int2DdsDataReader *reader,
                                                          const struct Int2DdsRequestedIncompatibleQosStatus *status,
                                                          Int2DdsUserContext user_context);

typedef struct Int2DdsSampleLostStatus {

  int32_t total_count;

  int32_t total_count_change;
} Int2DdsSampleLostStatus;

typedef void (*Int2DdsOnSampleLostCallback)(struct Int2DdsDataReader *reader,
                                            const struct Int2DdsSampleLostStatus *status,
                                            Int2DdsUserContext user_context);

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

typedef struct Int2DdsRequestedIncompatibleTypeStatus {

  int32_t total_count;

  int32_t total_count_change;
} Int2DdsRequestedIncompatibleTypeStatus;

typedef struct Int2DdsInconsistentTopicStatus {

  int32_t total_count;

  int32_t total_count_change;
} Int2DdsInconsistentTopicStatus;

uint32_t int2dds_abi_version(void);

uint64_t int2dds_abi_capabilities(void);

Int2DdsRet int2dds_guardcondition_new(struct Int2DdsGuardCondition **condition_out);

Int2DdsRet int2dds_guardcondition_set_trigger_value(const struct Int2DdsGuardCondition *condition,
                                                    bool value);

Int2DdsRet int2dds_guardcondition_get_trigger_value(const struct Int2DdsGuardCondition *condition,
                                                    bool *value_out);

Int2DdsRet int2dds_guardcondition_delete(struct Int2DdsGuardCondition *condition);

Int2DdsRet int2dds_load_profiles(const char *const *paths, uintptr_t count);

Int2DdsRet int2dds_get_dynamic_type_support(const char *type_name,
                                            struct Int2DdsDynamicTypeSupport **out);

Int2DdsRet int2dds_create_participant_from_config(const struct Int2DdsParticipantFactory *_factory,
                                                  const char *path,
                                                  struct Int2DdsConfiguredParticipant **out);

Int2DdsRet int2dds_configured_participant_get_datawriter(const struct Int2DdsConfiguredParticipant *configured,
                                                         const char *name,
                                                         struct Int2DdsDynamicDataWriter **out);

Int2DdsRet int2dds_configured_participant_get_datareader(const struct Int2DdsConfiguredParticipant *configured,
                                                         const char *name,
                                                         struct Int2DdsDynamicDataReader **out);

void int2dds_configured_participant_destroy(struct Int2DdsConfiguredParticipant *configured);

Int2DdsRet int2dds_domain_participant_factory_get_instance(struct Int2DdsParticipantFactory **factory_out);

Int2DdsRet int2dds_domain_participant_factory_finalize(struct Int2DdsParticipantFactory *factory);

Int2DdsRet int2dds_domain_participant_factory_lookup_participant(const struct Int2DdsParticipantFactory *_factory,
                                                                 int32_t domain_id,
                                                                 struct Int2DdsParticipant **participant_out);

Int2DdsRet int2dds_domain_participant_factory_set_qos(const struct Int2DdsParticipantFactory *_factory,
                                                      bool autoenable_created_entities);

Int2DdsRet int2dds_domain_participant_factory_get_qos(const struct Int2DdsParticipantFactory *_factory,
                                                      bool *autoenable_out);

Int2DdsRet int2dds_domain_participant_factory_set_default_participant_qos(const struct Int2DdsParticipantFactory *_factory,
                                                                          const struct Int2DdsParticipantQos *qos);

Int2DdsRet int2dds_domain_participant_factory_get_default_participant_qos(const struct Int2DdsParticipantFactory *_factory,
                                                                          struct Int2DdsParticipantQos **qos_out);

Int2DdsRet int2dds_participant_get_discovered_participants(const struct Int2DdsParticipant *participant,
                                                           uint8_t (*handles_out)[16],
                                                           uintptr_t capacity,
                                                           uintptr_t *count_out);

Int2DdsRet int2dds_datawriter_get_matched_subscriptions(const struct Int2DdsDataWriter *writer,
                                                        uint8_t (*handles_out)[16],
                                                        uintptr_t capacity,
                                                        uintptr_t *count_out);

Int2DdsRet int2dds_datareader_get_matched_publications(const struct Int2DdsDataReader *reader,
                                                       uint8_t (*handles_out)[16],
                                                       uintptr_t capacity,
                                                       uintptr_t *count_out);

Int2DdsRet int2dds_participant_take_discovered_publications_snapshot(const struct Int2DdsParticipant *participant,
                                                                     int32_t timeout_ms,
                                                                     struct Int2DdsPublicationBuiltinTopicDataSeq **seq_out);

Int2DdsRet int2dds_participant_take_discovered_publications_snapshot_filtered(const struct Int2DdsParticipant *participant,
                                                                              int32_t timeout_ms,
                                                                              uint32_t instance_state_mask,
                                                                              struct Int2DdsPublicationBuiltinTopicDataSeq **seq_out);

Int2DdsRet int2dds_publication_builtin_topic_data_seq_get_instance_state(const struct Int2DdsPublicationBuiltinTopicDataSeq *seq,
                                                                         uintptr_t index,
                                                                         uint32_t *instance_state_out);

Int2DdsRet int2dds_publication_builtin_topic_data_seq_get_instance_handle(const struct Int2DdsPublicationBuiltinTopicDataSeq *seq,
                                                                          uintptr_t index,
                                                                          uint8_t (*handle_out)[16]);

Int2DdsRet int2dds_publication_builtin_topic_data_seq_length(const struct Int2DdsPublicationBuiltinTopicDataSeq *seq,
                                                             uintptr_t *count_out);

Int2DdsRet int2dds_publication_builtin_topic_data_seq_get(const struct Int2DdsPublicationBuiltinTopicDataSeq *seq,
                                                          uintptr_t index,
                                                          struct Int2DdsPublicationBuiltinTopicData **data_out);

Int2DdsRet int2dds_publication_builtin_topic_data_seq_delete(struct Int2DdsPublicationBuiltinTopicDataSeq *seq);

Int2DdsRet int2dds_participant_take_discovered_subscriptions_snapshot(const struct Int2DdsParticipant *participant,
                                                                      int32_t timeout_ms,
                                                                      struct Int2DdsSubscriptionBuiltinTopicDataSeq **seq_out);

Int2DdsRet int2dds_participant_take_discovered_subscriptions_snapshot_filtered(const struct Int2DdsParticipant *participant,
                                                                               int32_t timeout_ms,
                                                                               uint32_t instance_state_mask,
                                                                               struct Int2DdsSubscriptionBuiltinTopicDataSeq **seq_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_seq_get_instance_state(const struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq,
                                                                          uintptr_t index,
                                                                          uint32_t *instance_state_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_seq_get_instance_handle(const struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq,
                                                                           uintptr_t index,
                                                                           uint8_t (*handle_out)[16]);

Int2DdsRet int2dds_subscription_builtin_topic_data_seq_length(const struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq,
                                                              uintptr_t *count_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_seq_get(const struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq,
                                                           uintptr_t index,
                                                           struct Int2DdsSubscriptionBuiltinTopicData **data_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_seq_delete(struct Int2DdsSubscriptionBuiltinTopicDataSeq *seq);

Int2DdsRet int2dds_participant_get_discovered_participant_data(const struct Int2DdsParticipant *participant,
                                                               const uint8_t (*handle)[16],
                                                               struct Int2DdsParticipantBuiltinTopicData **data_out);

Int2DdsRet int2dds_datawriter_get_matched_subscription_data(const struct Int2DdsDataWriter *writer,
                                                            const uint8_t (*handle)[16],
                                                            struct Int2DdsSubscriptionBuiltinTopicData **data_out);

Int2DdsRet int2dds_datareader_get_matched_publication_data(const struct Int2DdsDataReader *reader,
                                                           const uint8_t (*handle)[16],
                                                           struct Int2DdsPublicationBuiltinTopicData **data_out);

Int2DdsRet int2dds_participant_builtin_topic_data_get_key(const struct Int2DdsParticipantBuiltinTopicData *data,
                                                          uint8_t (*key_out)[12]);

Int2DdsRet int2dds_participant_builtin_topic_data_get_user_data(const struct Int2DdsParticipantBuiltinTopicData *data,
                                                                uint8_t *buf,
                                                                uintptr_t capacity,
                                                                uintptr_t *size_out);

Int2DdsRet int2dds_participant_builtin_topic_data_destroy(struct Int2DdsParticipantBuiltinTopicData *data);

Int2DdsRet int2dds_publication_builtin_topic_data_get_key(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                          uint8_t (*key_out)[12]);

Int2DdsRet int2dds_publication_builtin_topic_data_get_endpoint_guid(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                    uint8_t (*guid_out)[16]);

Int2DdsRet int2dds_publication_builtin_topic_data_get_participant_key(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                      uint8_t (*key_out)[12]);

Int2DdsRet int2dds_publication_builtin_topic_data_get_topic_name(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                 uint8_t *buf,
                                                                 uintptr_t capacity,
                                                                 uintptr_t *size_out);

Int2DdsRet int2dds_publication_builtin_topic_data_get_type_name(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                uint8_t *buf,
                                                                uintptr_t capacity,
                                                                uintptr_t *size_out);

Int2DdsRet int2dds_publication_builtin_topic_data_take_type_object(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                   struct Int2DdsTypeObject **out);

Int2DdsRet int2dds_publication_builtin_topic_data_get_reliability_kind(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                       int32_t *kind_out);

Int2DdsRet int2dds_publication_builtin_topic_data_get_durability_kind(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                      int32_t *kind_out);

Int2DdsRet int2dds_publication_builtin_topic_data_get_liveliness_kind(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                      int32_t *kind_out);

Int2DdsRet int2dds_publication_builtin_topic_data_get_liveliness_lease_duration(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                                int32_t *sec_out,
                                                                                uint32_t *nanosec_out);

Int2DdsRet int2dds_publication_builtin_topic_data_get_deadline(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                               int32_t *sec_out,
                                                               uint32_t *nanosec_out);

Int2DdsRet int2dds_publication_builtin_topic_data_get_lifespan(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                               int32_t *sec_out,
                                                               uint32_t *nanosec_out);

Int2DdsRet int2dds_publication_builtin_topic_data_get_user_data(const struct Int2DdsPublicationBuiltinTopicData *data,
                                                                uint8_t *buf,
                                                                uintptr_t capacity,
                                                                uintptr_t *size_out);

Int2DdsRet int2dds_publication_builtin_topic_data_destroy(struct Int2DdsPublicationBuiltinTopicData *data);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_key(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                           uint8_t (*key_out)[12]);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_endpoint_guid(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                     uint8_t (*guid_out)[16]);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_participant_key(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                       uint8_t (*key_out)[12]);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_topic_name(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                  uint8_t *buf,
                                                                  uintptr_t capacity,
                                                                  uintptr_t *size_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_type_name(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                 uint8_t *buf,
                                                                 uintptr_t capacity,
                                                                 uintptr_t *size_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_reliability_kind(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                        int32_t *kind_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_durability_kind(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                       int32_t *kind_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_liveliness_kind(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                       int32_t *kind_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_liveliness_lease_duration(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                                 int32_t *sec_out,
                                                                                 uint32_t *nanosec_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_deadline(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                int32_t *sec_out,
                                                                uint32_t *nanosec_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_get_user_data(const struct Int2DdsSubscriptionBuiltinTopicData *data,
                                                                 uint8_t *buf,
                                                                 uintptr_t capacity,
                                                                 uintptr_t *size_out);

Int2DdsRet int2dds_subscription_builtin_topic_data_destroy(struct Int2DdsSubscriptionBuiltinTopicData *data);

Int2DdsRet int2dds_participant_set_endpoint_discovery_callback(const struct Int2DdsParticipant *participant,
                                                               Int2DdsEndpointDiscoveryCallback callback,
                                                               void *ctx);

Int2DdsRet int2dds_participant_get_builtin_subscriber(const struct Int2DdsParticipant *participant,
                                                      struct Int2DdsSubscriber **out);

Int2DdsRet int2dds_subscriber_take_publication_data(const struct Int2DdsSubscriber *builtin_sub,
                                                    const char *topic_name_filter,
                                                    int32_t timeout_ms,
                                                    struct Int2DdsPublicationBuiltinTopicData **out);

Int2DdsRet int2dds_participant_wait_for_type_object(const struct Int2DdsParticipant *participant,
                                                    const char *topic_name,
                                                    int32_t timeout_ms,
                                                    struct Int2DdsTypeObject **type_obj_out,
                                                    char *type_name_buf,
                                                    uintptr_t type_name_buf_len,
                                                    uintptr_t *out_len);

void int2dds_type_object_destroy(struct Int2DdsTypeObject *t);

Int2DdsRet int2dds_type_object_extensibility(const struct Int2DdsTypeObject *t, int32_t *out);

Int2DdsRet int2dds_type_object_member_count(const struct Int2DdsTypeObject *t, uint32_t *out);

Int2DdsRet int2dds_type_object_member_info(const struct Int2DdsTypeObject *t,
                                           uint32_t index,
                                           struct Int2DdsMemberInfo *out);

Int2DdsRet int2dds_type_object_member_name(const struct Int2DdsTypeObject *t,
                                           uint32_t index,
                                           char *buf,
                                           uintptr_t buf_len,
                                           uintptr_t *out_len);

Int2DdsRet int2dds_type_object_find_member(const struct Int2DdsTypeObject *t,
                                           const char *name,
                                           uint32_t *index_out);

Int2DdsRet int2dds_create_topic_with_type_object(const struct Int2DdsParticipant *participant,
                                                 const char *topic_name,
                                                 const char *type_name,
                                                 const struct Int2DdsTypeObject *type_obj,
                                                 const struct Int2DdsTopicQos *qos,
                                                 struct Int2DdsTopic **out);

Int2DdsRet int2dds_dynamic_sample_get_char8(const uint8_t *bytes,
                                            uintptr_t len,
                                            const struct Int2DdsTypeObject *type_obj,
                                            const char *field_name,
                                            uint8_t *out);

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

void int2dds_dynamic_data_destroy(struct Int2DdsDynamicData *d);

Int2DdsRet int2dds_dynamic_data_get_char8(const struct Int2DdsDynamicData *data,
                                          const char *field_path,
                                          uint8_t *out);

Int2DdsRet int2dds_dynamic_data_get_string(const struct Int2DdsDynamicData *data,
                                           const char *field_path,
                                           char *out_buf,
                                           uintptr_t buf_cap,
                                           uintptr_t *out_len);

Int2DdsRet int2dds_dynamic_data_get_len(const struct Int2DdsDynamicData *data,
                                        const char *field_path,
                                        uintptr_t *out);

Int2DdsRet int2dds_dynamic_data_get_member(const struct Int2DdsDynamicData *data,
                                           const char *field_path,
                                           struct Int2DdsDynamicData **out);

void int2dds_dynamic_type_support_destroy(struct Int2DdsDynamicTypeSupport *s);

Int2DdsRet int2dds_create_topic_dynamic(const struct Int2DdsParticipant *participant,
                                        const char *topic_name,
                                        const struct Int2DdsDynamicTypeSupport *type_support,
                                        const struct Int2DdsTopicQos *qos,
                                        struct Int2DdsTopic **out);

Int2DdsRet int2dds_create_datawriter_dynamic(const struct Int2DdsPublisher *publisher,
                                             const struct Int2DdsTopic *topic,
                                             const struct Int2DdsDynamicTypeSupport *type_support,
                                             const struct Int2DdsDataWriterQos *qos,
                                             struct Int2DdsDynamicDataWriter **out);

Int2DdsRet int2dds_create_datareader_dynamic(const struct Int2DdsSubscriber *subscriber,
                                             const struct Int2DdsTopic *topic,
                                             const struct Int2DdsDynamicTypeSupport *type_support,
                                             const struct Int2DdsDataReaderQos *qos,
                                             struct Int2DdsDynamicDataReader **out);

void int2dds_dynamic_writer_destroy(struct Int2DdsDynamicDataWriter *w);

void int2dds_dynamic_reader_destroy(struct Int2DdsDynamicDataReader *r);

Int2DdsRet int2dds_dynamic_writer_get_qos(const struct Int2DdsDynamicDataWriter *writer,
                                          struct Int2DdsDataWriterQos **qos_out);

Int2DdsRet int2dds_dynamic_reader_get_qos(const struct Int2DdsDynamicDataReader *reader,
                                          struct Int2DdsDataReaderQos **qos_out);

Int2DdsRet int2dds_dynamic_writer_publication_matched_count(const struct Int2DdsDynamicDataWriter *writer,
                                                            int32_t *out);

Int2DdsRet int2dds_dynamic_reader_subscription_matched_count(const struct Int2DdsDynamicDataReader *reader,
                                                             int32_t *out);

Int2DdsRet int2dds_dynamic_data_create(const struct Int2DdsDynamicTypeSupport *type_support,
                                       struct Int2DdsDynamicData **out);

Int2DdsRet int2dds_dynamic_data_set_char8(struct Int2DdsDynamicData *data,
                                          const char *field,
                                          uint8_t value);

Int2DdsRet int2dds_dynamic_data_set_string(struct Int2DdsDynamicData *data,
                                           const char *field,
                                           const char *value);

Int2DdsRet int2dds_dynamic_writer_write(const struct Int2DdsDynamicDataWriter *writer,
                                        const struct Int2DdsDynamicData *data);

Int2DdsRet int2dds_dynamic_reader_take(const struct Int2DdsDynamicDataReader *reader,
                                       struct Int2DdsDynamicData **out_data,
                                       struct Int2DdsSampleInfo *out_info);

Int2DdsRet int2dds_dynamic_value_char8(uint8_t value, struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_string(const char *value, struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_wstring(const char *value, struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_enum(const char *name,
                                      int32_t value,
                                      struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_struct(const struct Int2DdsDynamicData *data,
                                        struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_sequence(struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_array(struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_map(struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_union(struct Int2DdsDynamicValue *discriminator,
                                       struct Int2DdsDynamicValue *value,
                                       struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_push(struct Int2DdsDynamicValue *collection,
                                      struct Int2DdsDynamicValue *element);

Int2DdsRet int2dds_dynamic_value_map_insert(struct Int2DdsDynamicValue *map,
                                            struct Int2DdsDynamicValue *key,
                                            struct Int2DdsDynamicValue *value);

void int2dds_dynamic_value_destroy(struct Int2DdsDynamicValue *value);

Int2DdsRet int2dds_dynamic_data_set_value(struct Int2DdsDynamicData *data,
                                          const char *field,
                                          struct Int2DdsDynamicValue *value);

Int2DdsRet int2dds_dynamic_data_get_value(const struct Int2DdsDynamicData *data,
                                          const char *path,
                                          struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_kind(const struct Int2DdsDynamicValue *value, int32_t *out);

Int2DdsRet int2dds_dynamic_value_as_char8(const struct Int2DdsDynamicValue *value, uint8_t *out);

Int2DdsRet int2dds_dynamic_value_as_string(const struct Int2DdsDynamicValue *value,
                                           char *buf,
                                           uintptr_t buf_len,
                                           uintptr_t *out_len);

Int2DdsRet int2dds_dynamic_value_to_string(const struct Int2DdsDynamicValue *value,
                                           char *buf,
                                           uintptr_t buf_len,
                                           uintptr_t *out_len);

Int2DdsRet int2dds_dynamic_value_as_enum(const struct Int2DdsDynamicValue *value,
                                         char *buf,
                                         uintptr_t buf_len,
                                         uintptr_t *out_len,
                                         int32_t *out_value);

Int2DdsRet int2dds_dynamic_value_as_bitmask(const struct Int2DdsDynamicValue *value, uint64_t *out);

Int2DdsRet int2dds_dynamic_value_as_bitset(const struct Int2DdsDynamicValue *value, uint64_t *out);

Int2DdsRet int2dds_dynamic_value_len(const struct Int2DdsDynamicValue *value, uintptr_t *out);

Int2DdsRet int2dds_dynamic_value_element(const struct Int2DdsDynamicValue *value,
                                         uintptr_t index,
                                         struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_map_key(const struct Int2DdsDynamicValue *value,
                                         uintptr_t index,
                                         struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_map_value(const struct Int2DdsDynamicValue *value,
                                           uintptr_t index,
                                           struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_as_struct(const struct Int2DdsDynamicValue *value,
                                           struct Int2DdsDynamicData **out);

Int2DdsRet int2dds_dynamic_value_union_discriminator(const struct Int2DdsDynamicValue *value,
                                                     struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_dynamic_value_union_value(const struct Int2DdsDynamicValue *value,
                                             struct Int2DdsDynamicValue **out);

Int2DdsRet int2dds_env_set_multicast_ttl(uint8_t ttl);

Int2DdsRet int2dds_env_get_multicast_ttl(uint8_t *ttl_out, bool *has_value_out);

Int2DdsRet int2dds_env_set_qos_profile(const char *path);

Int2DdsRet int2dds_env_set_default_qos_profile(const char *profile);

int32_t int2dds_last_error_message(char *buf, int32_t buf_len);

Int2DdsRet int2dds_create_participant(const struct Int2DdsParticipantFactory *_factory,
                                      int32_t domain_id,
                                      const struct Int2DdsParticipantQos *qos,
                                      struct Int2DdsParticipant **participant_out);

Int2DdsRet int2dds_create_participant_with_profile(const struct Int2DdsParticipantFactory *_factory,
                                                   int32_t domain_id,
                                                   const char *qos_path,
                                                   struct Int2DdsParticipant **participant_out);

Int2DdsRet int2dds_delete_participant(struct Int2DdsParticipant *participant);

Int2DdsRet int2dds_participant_assert_liveliness(const struct Int2DdsParticipant *participant);

Int2DdsRet int2dds_participant_get_domain_id(const struct Int2DdsParticipant *participant,
                                             int32_t *domain_id_out);

Int2DdsRet int2dds_participant_set_qos(const struct Int2DdsParticipant *participant,
                                       const struct Int2DdsParticipantQos *qos);

Int2DdsRet int2dds_participant_get_qos(const struct Int2DdsParticipant *participant,
                                       struct Int2DdsParticipantQos **qos_out);

Int2DdsRet int2dds_participant_delete_contained_entities(const struct Int2DdsParticipant *participant);

Int2DdsRet int2dds_participant_get_current_time(const struct Int2DdsParticipant *participant,
                                                int32_t *sec_out,
                                                uint32_t *nanosec_out);

Int2DdsRet int2dds_participant_contains_entity(const struct Int2DdsParticipant *participant,
                                               const uint8_t (*handle)[16],
                                               bool *result_out);

Int2DdsRet int2dds_participant_find_topic(const struct Int2DdsParticipant *participant,
                                          const char *topic_name,
                                          const char *dds_type_name,
                                          int32_t timeout_ms,
                                          struct Int2DdsTopic **topic_out);

Int2DdsRet int2dds_create_publisher(const struct Int2DdsParticipant *participant,
                                    const struct Int2DdsPublisherQos *qos,
                                    struct Int2DdsPublisher **publisher_out);

Int2DdsRet int2dds_create_publisher_with_profile(const struct Int2DdsParticipant *participant,
                                                 const char *qos_path,
                                                 struct Int2DdsPublisher **publisher_out);

Int2DdsRet int2dds_publisher_set_qos(const struct Int2DdsPublisher *publisher,
                                     const struct Int2DdsPublisherQos *qos);

Int2DdsRet int2dds_publisher_get_qos(const struct Int2DdsPublisher *publisher,
                                     struct Int2DdsPublisherQos **qos_out);

Int2DdsRet int2dds_publisher_get_instance_handle(const struct Int2DdsPublisher *publisher,
                                                 uint8_t (*handle_out)[16]);

Int2DdsRet int2dds_datawriter_set_qos(const struct Int2DdsDataWriter *writer,
                                      const struct Int2DdsDataWriterQos *qos);

Int2DdsRet int2dds_datawriter_get_qos(const struct Int2DdsDataWriter *writer,
                                      struct Int2DdsDataWriterQos **qos_out);

int32_t int2dds_datawriter_data_representation(const struct Int2DdsDataWriter *writer);

Int2DdsRet int2dds_datawriter_get_guid(const struct Int2DdsDataWriter *writer,
                                       uint8_t (*guid_out)[16]);

Int2DdsRet int2dds_delete_publisher(struct Int2DdsPublisher *publisher);

Int2DdsRet int2dds_create_datawriter(const struct Int2DdsPublisher *publisher,
                                     const struct Int2DdsTopic *topic,
                                     const struct Int2DdsDataWriterQos *qos,
                                     const struct Int2DdsDataWriterListener *listener,
                                     uint32_t mask,
                                     struct Int2DdsDataWriter **writer_out);

Int2DdsRet int2dds_create_datawriter_with_profile(const struct Int2DdsPublisher *publisher,
                                                  const struct Int2DdsTopic *topic,
                                                  const char *qos_path,
                                                  const struct Int2DdsDataWriterListener *listener,
                                                  uint32_t mask,
                                                  struct Int2DdsDataWriter **writer_out);

Int2DdsRet int2dds_datawriter_set_listener(struct Int2DdsDataWriter *writer,
                                           const struct Int2DdsDataWriterListener *listener,
                                           uint32_t mask);

Int2DdsRet int2dds_datawriter_get_listener(const struct Int2DdsDataWriter *writer,
                                           struct Int2DdsDataWriterListener *listener_out);

Int2DdsRet int2dds_delete_datawriter(struct Int2DdsDataWriter *writer);

Int2DdsRet int2dds_datawriter_get_publication_matched_status(const struct Int2DdsDataWriter *writer,
                                                             struct Int2DdsPublicationMatchedStatus *status_out);

Int2DdsRet int2dds_datawriter_get_liveliness_lost_status(const struct Int2DdsDataWriter *writer,
                                                         struct Int2DdsLivelinessLostStatus *status_out);

Int2DdsRet int2dds_datawriter_get_offered_deadline_missed_status(const struct Int2DdsDataWriter *writer,
                                                                 struct Int2DdsOfferedDeadlineMissedStatus *status_out);

Int2DdsRet int2dds_datawriter_get_offered_incompatible_qos_status(const struct Int2DdsDataWriter *writer,
                                                                  struct Int2DdsOfferedIncompatibleQosStatus *status_out);

Int2DdsRet int2dds_datawriter_get_offered_incompatible_type_status(const struct Int2DdsDataWriter *writer,
                                                                   struct Int2DdsOfferedIncompatibleTypeStatus *status_out);

Int2DdsRet int2dds_publisher_delete_contained_entities(const struct Int2DdsPublisher *publisher);

Int2DdsRet int2dds_datawriter_write_serialized(const struct Int2DdsDataWriter *writer,
                                               const uint8_t *data,
                                               uintptr_t data_len);

Int2DdsRet int2dds_datawriter_prepare_serialized_write(const struct Int2DdsDataWriter *writer,
                                                       uintptr_t capacity,
                                                       uint8_t **data_out,
                                                       uintptr_t *capacity_out,
                                                       struct Int2DdsSerializedWriteLoan **loan_out);

Int2DdsRet int2dds_datawriter_commit_serialized_write(const struct Int2DdsDataWriter *writer,
                                                      struct Int2DdsSerializedWriteLoan *loan,
                                                      uintptr_t actual_size);

Int2DdsRet int2dds_datawriter_abort_serialized_write(struct Int2DdsSerializedWriteLoan *loan);

Int2DdsRet int2dds_datawriter_write_serialized_w_timestamp(const struct Int2DdsDataWriter *writer,
                                                           const uint8_t *data,
                                                           uintptr_t data_len,
                                                           int32_t timestamp_sec,
                                                           uint32_t timestamp_nanosec);

Int2DdsRet int2dds_datawriter_wait_for_acknowledgments(const struct Int2DdsDataWriter *writer,
                                                       int64_t timeout_ms);

Int2DdsRet int2dds_publisher_wait_for_acknowledgments(const struct Int2DdsPublisher *publisher,
                                                      int64_t timeout_ms);

Int2DdsRet int2dds_datawriter_register_instance(const struct Int2DdsDataWriter *writer,
                                                const uint8_t *key,
                                                uintptr_t key_len,
                                                uint8_t (*handle_out)[16]);

Int2DdsRet int2dds_datawriter_dispose(const struct Int2DdsDataWriter *writer,
                                      const uint8_t *key,
                                      uintptr_t key_len,
                                      const uint8_t (*handle)[16]);

Int2DdsRet int2dds_datawriter_unregister_instance(const struct Int2DdsDataWriter *writer,
                                                  const uint8_t *key,
                                                  uintptr_t key_len,
                                                  const uint8_t (*handle)[16]);

Int2DdsRet int2dds_datawriter_lookup_instance(const struct Int2DdsDataWriter *writer,
                                              const uint8_t *key,
                                              uintptr_t key_len,
                                              uint8_t (*handle_out)[16]);

Int2DdsRet int2dds_datawriter_get_key_value(const struct Int2DdsDataWriter *writer,
                                            const uint8_t (*handle)[16],
                                            uint8_t *key_buf,
                                            uintptr_t key_capacity,
                                            uintptr_t *key_size_out);

Int2DdsRet int2dds_datawriter_assert_liveliness(const struct Int2DdsDataWriter *writer);

Int2DdsRet int2dds_datawriter_qos_create_default(struct Int2DdsDataWriterQos **qos_out);

Int2DdsRet int2dds_datawriter_qos_set_reliability(struct Int2DdsDataWriterQos *qos,
                                                  int32_t kind,
                                                  int64_t max_blocking_time_ns);

Int2DdsRet int2dds_datawriter_qos_set_durability(struct Int2DdsDataWriterQos *qos, int32_t kind);

Int2DdsRet int2dds_datawriter_qos_set_history(struct Int2DdsDataWriterQos *qos,
                                              int32_t kind,
                                              int32_t depth);

Int2DdsRet int2dds_datawriter_qos_set_data_representation(struct Int2DdsDataWriterQos *qos,
                                                          int32_t kind);

Int2DdsRet int2dds_datawriter_qos_set_ownership(struct Int2DdsDataWriterQos *qos, int32_t kind);

Int2DdsRet int2dds_datawriter_qos_set_ownership_strength(struct Int2DdsDataWriterQos *qos,
                                                         int32_t value);

Int2DdsRet int2dds_datawriter_qos_set_data_frag(struct Int2DdsDataWriterQos *qos, int32_t value);

Int2DdsRet int2dds_datawriter_qos_set_resource_limits(struct Int2DdsDataWriterQos *qos,
                                                      int32_t max_samples,
                                                      int32_t max_instances,
                                                      int32_t max_per_instance);

Int2DdsRet int2dds_datawriter_qos_set_lifespan(struct Int2DdsDataWriterQos *qos,
                                               int64_t duration_ns);

Int2DdsRet int2dds_datawriter_qos_set_destination_order(struct Int2DdsDataWriterQos *qos,
                                                        int32_t kind);

Int2DdsRet int2dds_datawriter_qos_set_latency_budget(struct Int2DdsDataWriterQos *qos,
                                                     int64_t duration_ns);

Int2DdsRet int2dds_datawriter_qos_set_transport_priority(struct Int2DdsDataWriterQos *qos,
                                                         int32_t priority);

Int2DdsRet int2dds_datawriter_qos_set_user_data(struct Int2DdsDataWriterQos *qos,
                                                const uint8_t *data,
                                                uintptr_t data_len);

Int2DdsRet int2dds_datawriter_qos_set_writer_data_lifecycle(struct Int2DdsDataWriterQos *qos,
                                                            bool autodispose);

Int2DdsRet int2dds_datawriter_qos_get_reliability(const struct Int2DdsDataWriterQos *qos,
                                                  int32_t *kind_out,
                                                  int64_t *max_blocking_time_ns_out);

Int2DdsRet int2dds_datawriter_qos_get_durability(const struct Int2DdsDataWriterQos *qos,
                                                 int32_t *kind_out);

Int2DdsRet int2dds_datawriter_qos_get_history(const struct Int2DdsDataWriterQos *qos,
                                              int32_t *kind_out,
                                              int32_t *depth_out);

Int2DdsRet int2dds_datawriter_qos_get_ownership(const struct Int2DdsDataWriterQos *qos,
                                                int32_t *kind_out);

Int2DdsRet int2dds_datawriter_qos_get_ownership_strength(const struct Int2DdsDataWriterQos *qos,
                                                         int32_t *value_out);

Int2DdsRet int2dds_datawriter_qos_get_data_frag(const struct Int2DdsDataWriterQos *qos,
                                                int32_t *value_out);

Int2DdsRet int2dds_datawriter_qos_get_resource_limits(const struct Int2DdsDataWriterQos *qos,
                                                      int32_t *max_samples_out,
                                                      int32_t *max_instances_out,
                                                      int32_t *max_per_instance_out);

Int2DdsRet int2dds_datawriter_qos_get_lifespan(const struct Int2DdsDataWriterQos *qos,
                                               int64_t *duration_ns_out);

Int2DdsRet int2dds_datawriter_qos_get_destination_order(const struct Int2DdsDataWriterQos *qos,
                                                        int32_t *kind_out);

Int2DdsRet int2dds_datawriter_qos_get_deadline(const struct Int2DdsDataWriterQos *qos,
                                               int64_t *period_ns_out);

Int2DdsRet int2dds_datawriter_qos_get_liveliness(const struct Int2DdsDataWriterQos *qos,
                                                 int32_t *kind_out,
                                                 int64_t *lease_duration_ns_out);

Int2DdsRet int2dds_datawriter_qos_get_data_representation(const struct Int2DdsDataWriterQos *qos,
                                                          int32_t *kind_out);

int32_t int2dds_default_data_representation(void);

int32_t int2dds_default_extensibility(void);

Int2DdsRet int2dds_datawriter_qos_get_transport_priority(const struct Int2DdsDataWriterQos *qos,
                                                         int32_t *value_out);

Int2DdsRet int2dds_datawriter_qos_get_latency_budget(const struct Int2DdsDataWriterQos *qos,
                                                     int64_t *duration_ns_out);

Int2DdsRet int2dds_datawriter_qos_get_writer_data_lifecycle(const struct Int2DdsDataWriterQos *qos,
                                                            bool *autodispose_out);

Int2DdsRet int2dds_datawriter_qos_destroy(struct Int2DdsDataWriterQos *qos);

Int2DdsRet int2dds_datareader_qos_create_default(struct Int2DdsDataReaderQos **qos_out);

Int2DdsRet int2dds_datareader_qos_set_reliability(struct Int2DdsDataReaderQos *qos,
                                                  int32_t kind,
                                                  int64_t max_blocking_time_ns);

Int2DdsRet int2dds_datareader_qos_set_durability(struct Int2DdsDataReaderQos *qos, int32_t kind);

Int2DdsRet int2dds_datareader_qos_set_history(struct Int2DdsDataReaderQos *qos,
                                              int32_t kind,
                                              int32_t depth);

Int2DdsRet int2dds_datareader_qos_set_data_representation(struct Int2DdsDataReaderQos *qos,
                                                          int32_t kind);

Int2DdsRet int2dds_datareader_qos_set_ownership(struct Int2DdsDataReaderQos *qos, int32_t kind);

Int2DdsRet int2dds_datareader_qos_set_resource_limits(struct Int2DdsDataReaderQos *qos,
                                                      int32_t max_samples,
                                                      int32_t max_instances,
                                                      int32_t max_per_instance);

Int2DdsRet int2dds_datareader_qos_set_destination_order(struct Int2DdsDataReaderQos *qos,
                                                        int32_t kind);

Int2DdsRet int2dds_datareader_qos_set_time_based_filter(struct Int2DdsDataReaderQos *qos,
                                                        int64_t minimum_separation_ns);

Int2DdsRet int2dds_datareader_qos_set_latency_budget(struct Int2DdsDataReaderQos *qos,
                                                     int64_t duration_ns);

Int2DdsRet int2dds_datareader_qos_set_user_data(struct Int2DdsDataReaderQos *qos,
                                                const uint8_t *data,
                                                uintptr_t data_len);

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

Int2DdsRet int2dds_datareader_qos_destroy(struct Int2DdsDataReaderQos *qos);

Int2DdsRet int2dds_topic_qos_create_default(struct Int2DdsTopicQos **qos_out);

Int2DdsRet int2dds_topic_qos_set_reliability(struct Int2DdsTopicQos *qos,
                                             int32_t kind,
                                             int64_t max_blocking_time_ns);

Int2DdsRet int2dds_topic_qos_set_durability(struct Int2DdsTopicQos *qos, int32_t kind);

Int2DdsRet int2dds_topic_qos_set_history(struct Int2DdsTopicQos *qos, int32_t kind, int32_t depth);

Int2DdsRet int2dds_topic_qos_set_deadline(struct Int2DdsTopicQos *qos, int64_t period_ns);

Int2DdsRet int2dds_topic_qos_set_liveliness(struct Int2DdsTopicQos *qos,
                                            int32_t kind,
                                            int64_t lease_duration_ns);

Int2DdsRet int2dds_topic_qos_set_destination_order(struct Int2DdsTopicQos *qos, int32_t kind);

Int2DdsRet int2dds_topic_qos_set_resource_limits(struct Int2DdsTopicQos *qos,
                                                 int32_t max_samples,
                                                 int32_t max_instances,
                                                 int32_t max_per_instance);

Int2DdsRet int2dds_topic_qos_set_transport_priority(struct Int2DdsTopicQos *qos, int32_t priority);

Int2DdsRet int2dds_topic_qos_set_lifespan(struct Int2DdsTopicQos *qos, int64_t duration_ns);

Int2DdsRet int2dds_topic_qos_set_ownership(struct Int2DdsTopicQos *qos, int32_t kind);

Int2DdsRet int2dds_topic_qos_set_data_representation(struct Int2DdsTopicQos *qos, int32_t kind);

Int2DdsRet int2dds_topic_qos_destroy(struct Int2DdsTopicQos *qos);

Int2DdsRet int2dds_participant_qos_create_default(struct Int2DdsParticipantQos **qos_out);

Int2DdsRet int2dds_participant_qos_set_user_data(struct Int2DdsParticipantQos *qos,
                                                 const uint8_t *data,
                                                 uintptr_t data_len);

Int2DdsRet int2dds_participant_qos_add_property(struct Int2DdsParticipantQos *qos,
                                                const char *name,
                                                const char *value,
                                                bool propagate);

Int2DdsRet int2dds_participant_qos_add_binary_property(struct Int2DdsParticipantQos *qos,
                                                       const char *name,
                                                       const uint8_t *data,
                                                       uintptr_t data_len,
                                                       bool propagate);

Int2DdsRet int2dds_participant_qos_find_property(const struct Int2DdsParticipantQos *qos,
                                                 const char *name,
                                                 char *out_buf,
                                                 uintptr_t out_cap,
                                                 uintptr_t *out_len);

Int2DdsRet int2dds_participant_qos_remove_property(struct Int2DdsParticipantQos *qos,
                                                   const char *name);

Int2DdsRet int2dds_participant_qos_get_properties_with_prefix(const struct Int2DdsParticipantQos *qos,
                                                              const char *prefix,
                                                              int32_t (*cb)(const char *name,
                                                                            const char *value,
                                                                            void *user_data),
                                                              void *user_data);

Int2DdsRet int2dds_participant_qos_set_multicast_ttl(struct Int2DdsParticipantQos *qos,
                                                     uint8_t ttl);

Int2DdsRet int2dds_participant_qos_destroy(struct Int2DdsParticipantQos *qos);

Int2DdsRet int2dds_datawriter_qos_set_deadline(struct Int2DdsDataWriterQos *qos, int64_t period_ns);

Int2DdsRet int2dds_datareader_qos_set_deadline(struct Int2DdsDataReaderQos *qos, int64_t period_ns);

Int2DdsRet int2dds_datawriter_qos_set_liveliness(struct Int2DdsDataWriterQos *qos,
                                                 int32_t kind,
                                                 int64_t lease_duration_ns);

Int2DdsRet int2dds_datareader_qos_set_liveliness(struct Int2DdsDataReaderQos *qos,
                                                 int32_t kind,
                                                 int64_t lease_duration_ns);

Int2DdsRet int2dds_publisher_qos_create_default(struct Int2DdsPublisherQos **qos_out);

Int2DdsRet int2dds_publisher_qos_set_partition(struct Int2DdsPublisherQos *qos,
                                               const char *const *partitions,
                                               uintptr_t partition_count);

Int2DdsRet int2dds_publisher_qos_destroy(struct Int2DdsPublisherQos *qos);

Int2DdsRet int2dds_subscriber_qos_create_default(struct Int2DdsSubscriberQos **qos_out);

Int2DdsRet int2dds_subscriber_qos_set_partition(struct Int2DdsSubscriberQos *qos,
                                                const char *const *partitions,
                                                uintptr_t partition_count);

Int2DdsRet int2dds_subscriber_qos_destroy(struct Int2DdsSubscriberQos *qos);

Int2DdsRet int2dds_datareader_create_readcondition(const struct Int2DdsDataReader *reader,
                                                   uint32_t sample_state_mask,
                                                   uint32_t view_state_mask,
                                                   uint32_t instance_state_mask,
                                                   struct Int2DdsReadCondition **condition_out);

Int2DdsRet int2dds_datareader_create_querycondition(const struct Int2DdsDataReader *reader,
                                                    uint32_t sample_state_mask,
                                                    uint32_t view_state_mask,
                                                    uint32_t instance_state_mask,
                                                    const char *query_expression,
                                                    const char *const *query_parameters,
                                                    uintptr_t query_parameters_count,
                                                    struct Int2DdsReadCondition **condition_out);

Int2DdsRet int2dds_readcondition_get_trigger_value(const struct Int2DdsReadCondition *condition,
                                                   bool *value_out);

Int2DdsRet int2dds_querycondition_set_query_parameters(const struct Int2DdsReadCondition *condition,
                                                       const char *const *query_parameters,
                                                       uintptr_t query_parameters_count);

Int2DdsRet int2dds_readcondition_delete(struct Int2DdsReadCondition *condition);

Int2DdsRet int2dds_datareader_take_serialized_batch_w_readcondition(const struct Int2DdsDataReader *reader,
                                                                    const struct Int2DdsReadCondition *condition,
                                                                    int32_t max_samples,
                                                                    struct Int2DdsSampleSeq **seq_out);

Int2DdsRet int2dds_datareader_read_serialized_batch_w_readcondition(const struct Int2DdsDataReader *reader,
                                                                    const struct Int2DdsReadCondition *condition,
                                                                    int32_t max_samples,
                                                                    struct Int2DdsSampleSeq **seq_out);

Int2DdsRet int2dds_datareader_get_statuscondition(const struct Int2DdsDataReader *reader,
                                                  struct Int2DdsStatusCondition **condition_out);

Int2DdsRet int2dds_datawriter_get_statuscondition(const struct Int2DdsDataWriter *writer,
                                                  struct Int2DdsStatusCondition **condition_out);

Int2DdsRet int2dds_participant_get_statuscondition(const struct Int2DdsParticipant *participant,
                                                   struct Int2DdsStatusCondition **condition_out);

Int2DdsRet int2dds_publisher_get_statuscondition(const struct Int2DdsPublisher *publisher,
                                                 struct Int2DdsStatusCondition **condition_out);

Int2DdsRet int2dds_subscriber_get_statuscondition(const struct Int2DdsSubscriber *subscriber,
                                                  struct Int2DdsStatusCondition **condition_out);

Int2DdsRet int2dds_topic_get_statuscondition(const struct Int2DdsTopic *topic,
                                             struct Int2DdsStatusCondition **condition_out);

Int2DdsRet int2dds_datareader_get_status_changes(const struct Int2DdsDataReader *reader,
                                                 uint32_t *mask_out);

Int2DdsRet int2dds_datawriter_get_status_changes(const struct Int2DdsDataWriter *writer,
                                                 uint32_t *mask_out);

Int2DdsRet int2dds_participant_get_status_changes(const struct Int2DdsParticipant *participant,
                                                  uint32_t *mask_out);

Int2DdsRet int2dds_publisher_get_status_changes(const struct Int2DdsPublisher *publisher,
                                                uint32_t *mask_out);

Int2DdsRet int2dds_subscriber_get_status_changes(const struct Int2DdsSubscriber *subscriber,
                                                 uint32_t *mask_out);

Int2DdsRet int2dds_topic_get_status_changes(const struct Int2DdsTopic *topic, uint32_t *mask_out);

Int2DdsRet int2dds_statuscondition_set_enabled_statuses(const struct Int2DdsStatusCondition *condition,
                                                        uint32_t mask);

Int2DdsRet int2dds_statuscondition_get_enabled_statuses(const struct Int2DdsStatusCondition *condition,
                                                        uint32_t *mask_out);

Int2DdsRet int2dds_statuscondition_get_trigger_value(const struct Int2DdsStatusCondition *condition,
                                                     bool *value_out);

Int2DdsRet int2dds_statuscondition_delete(struct Int2DdsStatusCondition *condition);

Int2DdsRet int2dds_create_subscriber(const struct Int2DdsParticipant *participant,
                                     const struct Int2DdsSubscriberQos *qos,
                                     struct Int2DdsSubscriber **subscriber_out);

Int2DdsRet int2dds_create_subscriber_with_profile(const struct Int2DdsParticipant *participant,
                                                  const char *qos_path,
                                                  struct Int2DdsSubscriber **subscriber_out);

Int2DdsRet int2dds_subscriber_set_qos(const struct Int2DdsSubscriber *subscriber,
                                      const struct Int2DdsSubscriberQos *qos);

Int2DdsRet int2dds_subscriber_get_qos(const struct Int2DdsSubscriber *subscriber,
                                      struct Int2DdsSubscriberQos **qos_out);

Int2DdsRet int2dds_subscriber_get_instance_handle(const struct Int2DdsSubscriber *subscriber,
                                                  uint8_t (*handle_out)[16]);

Int2DdsRet int2dds_delete_subscriber(struct Int2DdsSubscriber *subscriber);

Int2DdsRet int2dds_create_datareader(const struct Int2DdsSubscriber *subscriber,
                                     const struct Int2DdsTopic *topic,
                                     const struct Int2DdsDataReaderQos *qos,
                                     const struct Int2DdsDataReaderListener *listener,
                                     uint32_t mask,
                                     struct Int2DdsDataReader **reader_out);

Int2DdsRet int2dds_create_datareader_with_profile(const struct Int2DdsSubscriber *subscriber,
                                                  const struct Int2DdsTopic *topic,
                                                  const char *qos_path,
                                                  const struct Int2DdsDataReaderListener *listener,
                                                  uint32_t mask,
                                                  struct Int2DdsDataReader **reader_out);

Int2DdsRet int2dds_create_datareader_cft(const struct Int2DdsSubscriber *subscriber,
                                         const struct Int2DdsContentFilteredTopic *cft,
                                         const struct Int2DdsDataReaderQos *qos,
                                         const struct Int2DdsDataReaderListener *listener,
                                         uint32_t mask,
                                         struct Int2DdsDataReader **reader_out);

Int2DdsRet int2dds_datareader_set_listener(struct Int2DdsDataReader *reader,
                                           const struct Int2DdsDataReaderListener *listener,
                                           uint32_t mask);

Int2DdsRet int2dds_datareader_get_listener(const struct Int2DdsDataReader *reader,
                                           struct Int2DdsDataReaderListener *listener_out);

Int2DdsRet int2dds_datareader_set_qos(const struct Int2DdsDataReader *reader,
                                      const struct Int2DdsDataReaderQos *qos);

Int2DdsRet int2dds_datareader_get_qos(const struct Int2DdsDataReader *reader,
                                      struct Int2DdsDataReaderQos **qos_out);

Int2DdsRet int2dds_datareader_get_guid(const struct Int2DdsDataReader *reader,
                                       uint8_t (*guid_out)[16]);

Int2DdsRet int2dds_datareader_lookup_instance(const struct Int2DdsDataReader *reader,
                                              const uint8_t *key,
                                              uintptr_t key_len,
                                              uint8_t (*handle_out)[16]);

Int2DdsRet int2dds_datareader_get_key_value(const struct Int2DdsDataReader *reader,
                                            const uint8_t (*handle)[16],
                                            uint8_t *key_buf,
                                            uintptr_t key_capacity,
                                            uintptr_t *key_size_out);

Int2DdsRet int2dds_datareader_has_data(const struct Int2DdsDataReader *reader, bool *has_data_out);

Int2DdsRet int2dds_delete_datareader(struct Int2DdsDataReader *reader);

Int2DdsRet int2dds_datareader_get_subscription_matched_status(const struct Int2DdsDataReader *reader,
                                                              struct Int2DdsSubscriptionMatchedStatus *status_out);

Int2DdsRet int2dds_datareader_get_liveliness_changed_status(const struct Int2DdsDataReader *reader,
                                                            struct Int2DdsLivelinessChangedStatus *status_out);

Int2DdsRet int2dds_datareader_get_sample_rejected_status(const struct Int2DdsDataReader *reader,
                                                         struct Int2DdsSampleRejectedStatus *status_out);

Int2DdsRet int2dds_datareader_get_sample_lost_status(const struct Int2DdsDataReader *reader,
                                                     struct Int2DdsSampleLostStatus *status_out);

Int2DdsRet int2dds_datareader_get_requested_deadline_missed_status(const struct Int2DdsDataReader *reader,
                                                                   struct Int2DdsRequestedDeadlineMissedStatus *status_out);

Int2DdsRet int2dds_datareader_get_requested_incompatible_qos_status(const struct Int2DdsDataReader *reader,
                                                                    struct Int2DdsRequestedIncompatibleQosStatus *status_out);

Int2DdsRet int2dds_datareader_get_requested_incompatible_type_status(const struct Int2DdsDataReader *reader,
                                                                     struct Int2DdsRequestedIncompatibleTypeStatus *status_out);

Int2DdsRet int2dds_subscriber_delete_contained_entities(const struct Int2DdsSubscriber *subscriber);

Int2DdsRet int2dds_datareader_take_serialized(const struct Int2DdsDataReader *reader,
                                              uint8_t *buffer,
                                              uintptr_t buffer_capacity,
                                              uintptr_t *actual_size_out,
                                              bool *valid_data_out);

Int2DdsRet int2dds_datareader_take_serialized_loaned(const struct Int2DdsDataReader *reader,
                                                     const uint8_t **data_out,
                                                     uintptr_t *actual_size_out,
                                                     bool *valid_data_out,
                                                     struct Int2DdsSerializedLoan **loan_out);

Int2DdsRet int2dds_datareader_return_serialized_loan(struct Int2DdsSerializedLoan *loan);

Int2DdsRet int2dds_datareader_read_serialized(const struct Int2DdsDataReader *reader,
                                              uint8_t *buffer,
                                              uintptr_t buffer_capacity,
                                              uintptr_t *actual_size_out,
                                              bool *valid_data_out);

Int2DdsRet int2dds_datareader_take_serialized_w_info(const struct Int2DdsDataReader *reader,
                                                     uint8_t *buffer,
                                                     uintptr_t buffer_capacity,
                                                     uintptr_t *actual_size_out,
                                                     struct Int2DdsSampleInfo *info_out);

Int2DdsRet int2dds_datareader_read_serialized_w_info(const struct Int2DdsDataReader *reader,
                                                     uint8_t *buffer,
                                                     uintptr_t buffer_capacity,
                                                     uintptr_t *actual_size_out,
                                                     struct Int2DdsSampleInfo *info_out);

Int2DdsRet int2dds_datareader_take_serialized_batch(const struct Int2DdsDataReader *reader,
                                                    int32_t max_samples,
                                                    struct Int2DdsSampleSeq **seq_out);

Int2DdsRet int2dds_datareader_read_serialized_batch(const struct Int2DdsDataReader *reader,
                                                    int32_t max_samples,
                                                    struct Int2DdsSampleSeq **seq_out);

Int2DdsRet int2dds_datareader_take_instance_serialized_batch(const struct Int2DdsDataReader *reader,
                                                             const uint8_t (*handle)[16],
                                                             int32_t max_samples,
                                                             uint32_t sample_state_mask,
                                                             uint32_t view_state_mask,
                                                             uint32_t instance_state_mask,
                                                             struct Int2DdsSampleSeq **seq_out);

Int2DdsRet int2dds_datareader_read_instance_serialized_batch(const struct Int2DdsDataReader *reader,
                                                             const uint8_t (*handle)[16],
                                                             int32_t max_samples,
                                                             uint32_t sample_state_mask,
                                                             uint32_t view_state_mask,
                                                             uint32_t instance_state_mask,
                                                             struct Int2DdsSampleSeq **seq_out);

uintptr_t int2dds_sample_seq_length(const struct Int2DdsSampleSeq *seq);

Int2DdsRet int2dds_sample_seq_get_data(const struct Int2DdsSampleSeq *seq,
                                       uintptr_t index,
                                       uint8_t *buffer,
                                       uintptr_t buffer_capacity,
                                       uintptr_t *actual_size_out);

Int2DdsRet int2dds_sample_seq_get_info(const struct Int2DdsSampleSeq *seq,
                                       uintptr_t index,
                                       struct Int2DdsSampleInfo *info_out);

Int2DdsRet int2dds_sample_seq_delete(struct Int2DdsSampleSeq *seq);

Int2DdsRet int2dds_datareader_read_serialized_w_states(const struct Int2DdsDataReader *reader,
                                                       uint8_t *buffer,
                                                       uintptr_t buffer_capacity,
                                                       uintptr_t *actual_size_out,
                                                       struct Int2DdsSampleInfo *info_out,
                                                       uint32_t sample_state_mask,
                                                       uint32_t view_state_mask,
                                                       uint32_t instance_state_mask);

Int2DdsRet int2dds_datareader_take_serialized_w_states(const struct Int2DdsDataReader *reader,
                                                       uint8_t *buffer,
                                                       uintptr_t buffer_capacity,
                                                       uintptr_t *actual_size_out,
                                                       struct Int2DdsSampleInfo *info_out,
                                                       uint32_t sample_state_mask,
                                                       uint32_t view_state_mask,
                                                       uint32_t instance_state_mask);

Int2DdsRet int2dds_datareader_take_serialized_batch_w_states(const struct Int2DdsDataReader *reader,
                                                             int32_t max_samples,
                                                             struct Int2DdsSampleSeq **seq_out,
                                                             uint32_t sample_state_mask,
                                                             uint32_t view_state_mask,
                                                             uint32_t instance_state_mask);

Int2DdsRet int2dds_datareader_read_serialized_batch_w_states(const struct Int2DdsDataReader *reader,
                                                             int32_t max_samples,
                                                             struct Int2DdsSampleSeq **seq_out,
                                                             uint32_t sample_state_mask,
                                                             uint32_t view_state_mask,
                                                             uint32_t instance_state_mask);

Int2DdsRet int2dds_datareader_wait_for_historical_data(const struct Int2DdsDataReader *reader,
                                                       int64_t timeout_ms);

Int2DdsRet int2dds_create_topic(const struct Int2DdsParticipant *participant,
                                const char *topic_name,
                                const char *dds_type_name,
                                int32_t extensibility,
                                const struct Int2DdsTopicQos *qos,
                                struct Int2DdsTopic **topic_out);

Int2DdsRet int2dds_create_topic_with_profile(const struct Int2DdsParticipant *participant,
                                             const char *topic_name,
                                             const char *dds_type_name,
                                             int32_t extensibility,
                                             const char *qos_path,
                                             struct Int2DdsTopic **topic_out);

Int2DdsRet int2dds_create_topic_with_type_info(const struct Int2DdsParticipant *participant,
                                               const char *topic_name,
                                               const struct Int2DdsTypeInfo *type_info,
                                               const struct Int2DdsTopicQos *qos,
                                               struct Int2DdsTopic **topic_out);

Int2DdsRet int2dds_topic_set_qos(const struct Int2DdsTopic *topic,
                                 const struct Int2DdsTopicQos *qos);

Int2DdsRet int2dds_topic_get_qos(const struct Int2DdsTopic *topic,
                                 struct Int2DdsTopicQos **qos_out);

Int2DdsRet int2dds_delete_topic(struct Int2DdsTopic *topic);

Int2DdsRet int2dds_topic_get_inconsistent_topic_status(const struct Int2DdsTopic *topic,
                                                       struct Int2DdsInconsistentTopicStatus *status_out);

Int2DdsRet int2dds_topic_get_name(const struct Int2DdsTopic *topic,
                                  char *name_out,
                                  uintptr_t name_size);

Int2DdsRet int2dds_topic_get_type_name(const struct Int2DdsTopic *topic,
                                       char *type_name_out,
                                       uintptr_t type_name_size);

Int2DdsRet int2dds_create_contentfilteredtopic(const struct Int2DdsParticipant *participant,
                                               const char *topic_name,
                                               const struct Int2DdsTopic *related_topic,
                                               const char *filter_expression,
                                               const char *const *expression_parameters,
                                               uintptr_t expression_parameters_count,
                                               struct Int2DdsContentFilteredTopic **cft_out);

Int2DdsRet int2dds_delete_contentfilteredtopic(struct Int2DdsContentFilteredTopic *cft);

Int2DdsRet int2dds_contentfilteredtopic_set_expression_parameters(struct Int2DdsContentFilteredTopic *cft,
                                                                  const char *const *expression_parameters,
                                                                  uintptr_t expression_parameters_count);

Int2DdsRet int2dds_contentfilteredtopic_set_filter_expression(struct Int2DdsContentFilteredTopic *cft,
                                                              const char *filter_expression,
                                                              const char *const *expression_parameters,
                                                              uintptr_t expression_parameters_count);

Int2DdsRet int2dds_contentfilteredtopic_set_enabled(struct Int2DdsContentFilteredTopic *cft,
                                                    bool enabled);

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

Int2DdsRet int2dds_type_info_create_enum(const char *type_name,
                                         uint16_t bit_bound,
                                         struct Int2DdsTypeInfo **out);

Int2DdsRet int2dds_type_info_add_enum_literal(struct Int2DdsTypeInfo *type_info,
                                              const char *literal_name,
                                              int32_t value,
                                              int32_t is_default);

Int2DdsRet int2dds_type_info_create_bitmask(const char *type_name,
                                            uint16_t bit_bound,
                                            struct Int2DdsTypeInfo **out);

Int2DdsRet int2dds_type_info_add_bitmask_flag(struct Int2DdsTypeInfo *type_info,
                                              const char *flag_name,
                                              uint16_t position);

Int2DdsRet int2dds_type_info_add_field(struct Int2DdsTypeInfo *type_info,
                                       const char *field_name,
                                       int32_t field_type,
                                       int32_t flags);

Int2DdsRet int2dds_type_info_add_string_field(struct Int2DdsTypeInfo *type_info,
                                              const char *field_name,
                                              uint32_t bound,
                                              int32_t flags);

Int2DdsRet int2dds_type_info_add_wstring_field(struct Int2DdsTypeInfo *type_info,
                                               const char *field_name,
                                               uint32_t bound,
                                               int32_t flags);

Int2DdsRet int2dds_type_info_add_sequence_field(struct Int2DdsTypeInfo *type_info,
                                                const char *field_name,
                                                int32_t element_type,
                                                uint32_t bound,
                                                int32_t flags);

Int2DdsRet int2dds_type_info_add_array_field(struct Int2DdsTypeInfo *type_info,
                                             const char *field_name,
                                             int32_t element_type,
                                             uint32_t array_size,
                                             int32_t flags);

Int2DdsRet int2dds_type_info_add_array_field_nd(struct Int2DdsTypeInfo *type_info,
                                                const char *field_name,
                                                int32_t element_type,
                                                const uint32_t *dims,
                                                uintptr_t dims_len,
                                                int32_t flags);

Int2DdsRet int2dds_type_info_add_named_type_field(struct Int2DdsTypeInfo *type_info,
                                                  const char *field_name,
                                                  const char *type_hash_name,
                                                  int32_t flags);

Int2DdsRet int2dds_type_info_add_nested_field(struct Int2DdsTypeInfo *type_info,
                                              const char *field_name,
                                              const struct Int2DdsTypeInfo *nested_type_info,
                                              int32_t flags);

Int2DdsRet int2dds_type_info_add_sequence_of_nested_field(struct Int2DdsTypeInfo *type_info,
                                                          const char *field_name,
                                                          const struct Int2DdsTypeInfo *element_type_info,
                                                          uint32_t bound,
                                                          int32_t flags);

Int2DdsRet int2dds_type_info_add_array_of_nested_field(struct Int2DdsTypeInfo *type_info,
                                                       const char *field_name,
                                                       const struct Int2DdsTypeInfo *element_type_info,
                                                       uint32_t array_size,
                                                       int32_t flags);

Int2DdsRet int2dds_type_info_add_array_of_nested_field_nd(struct Int2DdsTypeInfo *type_info,
                                                          const char *field_name,
                                                          const struct Int2DdsTypeInfo *element_type_info,
                                                          const uint32_t *dims,
                                                          uintptr_t dims_len,
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

Int2DdsRet int2dds_type_info_add_array_of_named_field_nd(struct Int2DdsTypeInfo *type_info,
                                                         const char *field_name,
                                                         const char *element_hash_name,
                                                         const uint32_t *dims,
                                                         uintptr_t dims_len,
                                                         int32_t flags);

Int2DdsRet int2dds_type_info_to_type_object(const struct Int2DdsTypeInfo *type_info,
                                            struct Int2DdsTypeObject **out);

void int2dds_type_info_destroy(struct Int2DdsTypeInfo *type_info);

Int2DdsRet int2dds_waitset_new(struct Int2DdsWaitSet **waitset_out);

Int2DdsRet int2dds_waitset_wait_ex(const struct Int2DdsWaitSet *waitset,
                                   int64_t timeout_ms,
                                   struct Int2DdsConditionSeq **conditions_out);

Int2DdsRet int2dds_waitset_wait_ex_ns(const struct Int2DdsWaitSet *waitset,
                                      int64_t timeout_ns,
                                      struct Int2DdsConditionSeq **conditions_out);

Int2DdsRet int2dds_condition_seq_length(const struct Int2DdsConditionSeq *seq,
                                        uintptr_t *count_out);

Int2DdsRet int2dds_condition_seq_get(const struct Int2DdsConditionSeq *seq,
                                     uintptr_t index,
                                     const struct Int2DdsCondition **condition_out);

Int2DdsRet int2dds_condition_seq_delete(struct Int2DdsConditionSeq *seq);

Int2DdsRet int2dds_condition_get_trigger_value(const struct Int2DdsCondition *condition,
                                               bool *triggered_out);

Int2DdsRet int2dds_condition_delete(struct Int2DdsCondition *condition);

Int2DdsRet int2dds_waitset_attach_guardcondition(const struct Int2DdsWaitSet *waitset,
                                                 const struct Int2DdsGuardCondition *condition);

Int2DdsRet int2dds_waitset_detach_guardcondition(const struct Int2DdsWaitSet *waitset,
                                                 const struct Int2DdsGuardCondition *condition);

Int2DdsRet int2dds_waitset_attach_statuscondition(const struct Int2DdsWaitSet *waitset,
                                                  const struct Int2DdsStatusCondition *condition);

Int2DdsRet int2dds_waitset_detach_statuscondition(const struct Int2DdsWaitSet *waitset,
                                                  const struct Int2DdsStatusCondition *condition);

Int2DdsRet int2dds_waitset_attach_readcondition(const struct Int2DdsWaitSet *waitset,
                                                const struct Int2DdsReadCondition *condition);

Int2DdsRet int2dds_waitset_detach_readcondition(const struct Int2DdsWaitSet *waitset,
                                                const struct Int2DdsReadCondition *condition);

Int2DdsRet int2dds_waitset_delete(struct Int2DdsWaitSet *waitset);

Int2DdsRet int2dds_xml_type_registry_create(struct Int2DdsXmlTypeRegistry **out);

Int2DdsRet int2dds_xml_type_registry_from_file(const char *path,
                                               struct Int2DdsXmlTypeRegistry **out);

Int2DdsRet int2dds_xml_type_registry_load_file(struct Int2DdsXmlTypeRegistry *registry,
                                               const char *path);

Int2DdsRet int2dds_xml_type_registry_load_str(struct Int2DdsXmlTypeRegistry *registry,
                                              const char *xml);

Int2DdsRet int2dds_xml_type_registry_get_type_support(const struct Int2DdsXmlTypeRegistry *registry,
                                                      const char *name,
                                                      struct Int2DdsDynamicTypeSupport **out);

Int2DdsRet int2dds_xml_type_registry_get_type_object(const struct Int2DdsXmlTypeRegistry *registry,
                                                     const char *name,
                                                     struct Int2DdsTypeObject **out);

Int2DdsRet int2dds_xml_type_registry_type_count(const struct Int2DdsXmlTypeRegistry *registry,
                                                uintptr_t *out);

Int2DdsRet int2dds_xml_type_registry_type_name(const struct Int2DdsXmlTypeRegistry *registry,
                                               uintptr_t index,
                                               char *buf,
                                               uintptr_t buf_len,
                                               uintptr_t *out_len);

void int2dds_xml_type_registry_destroy(struct Int2DdsXmlTypeRegistry *registry);

Int2DdsRet int2dds_dynamic_sample_get_bool (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, bool     *out);
Int2DdsRet int2dds_dynamic_sample_get_i8   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int8_t   *out);
Int2DdsRet int2dds_dynamic_sample_get_u8   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint8_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_byte (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint8_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_i16  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int16_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_u16  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint16_t *out);
Int2DdsRet int2dds_dynamic_sample_get_i32  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int32_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_u32  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint32_t *out);
Int2DdsRet int2dds_dynamic_sample_get_i64  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int64_t  *out);
Int2DdsRet int2dds_dynamic_sample_get_u64  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint64_t *out);
Int2DdsRet int2dds_dynamic_sample_get_f32  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, float    *out);
Int2DdsRet int2dds_dynamic_sample_get_f64  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, double   *out);

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
""")


from ._libpath import find_library as _find_library


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
