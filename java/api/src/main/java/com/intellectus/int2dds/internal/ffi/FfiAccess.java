package com.intellectus.int2dds.internal.ffi;

import com.intellectus.int2dds.internal.NativeLoader;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/**
 * Package-visible bridge to the generated {@link Ffi} declarations.
 *
 * <p>Exists so callers get the native library loaded automatically and so that
 * hand-written code never has to live inside the generated file.
 */
public final class FfiAccess {

    static {
        NativeLoader.load();
    }

    private FfiAccess() {}

    /** The library's default type extensibility: 0 Final, 1 Appendable, 2 Mutable. */
    public static int defaultExtensibility() {
        return Ffi.int2dds_default_extensibility();
    }

    /** The library's default data representation. */
    public static int defaultDataRepresentation() {
        return Ffi.int2dds_default_data_representation();
    }

    /**
     * Copies the last error message into {@code buf} as UTF-8 and returns the
     * message's full length, which may exceed {@code buf.length}.
     */
    public static int lastErrorMessage(byte[] buf) {
        return Ffi.int2dds_last_error_message(buf, buf.length);
    }

    /** Native address of a direct ByteBuffer, or 0 if the buffer is not direct. */
    public static long directBufferAddress(ByteBuffer buf) {
        return Ffi.directBufferAddress(buf);
    }

    /**
     * Creates a dynamic value holding {@code value}, writing the handle to the
     * native address {@code out}, which must be at least 8 bytes.
     */
    public static int dynamicValueI32(int value, long out) {
        return Ffi.int2dds_dynamic_value_i32(value, out);
    }

    /**
     * Renders a dynamic value into {@code buf} as UTF-8 and writes the text's
     * byte length to the native address {@code outLen}. The buffer must hold the
     * text plus a trailing NUL.
     */
    public static int dynamicValueToString(long value, byte[] buf, long bufLen, long outLen) {
        return Ffi.int2dds_dynamic_value_to_string(value, buf, bufLen, outLen);
    }

    /** Releases a dynamic value handle. */
    public static void dynamicValueDestroy(long value) {
        Ffi.int2dds_dynamic_value_destroy(value);
    }

    // --- DomainParticipantFactory / DomainParticipant ---

    /**
     * The process-wide participant factory handle, or 0 on failure. No status
     * a caller could act on differently accompanies a failure here — the
     * factory singleton either exists or something is catastrophically wrong
     * with the native library — so, unlike {@link #createParticipant}, this
     * does not report a status code.
     */
    public static long participantFactoryGetInstance() {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_domain_participant_factory_get_instance(directBufferAddress(slot));
        return rc == 0 ? slot.getLong(0) : 0L;
    }

    /**
     * Creates a participant. Returns the C ABI status code and, only on
     * success, writes the new handle to {@code handleOut[0]}; on failure
     * {@code handleOut} is left untouched.
     *
     * <p>Returning the status alongside the handle, rather than folding a
     * failure into a bare 0 handle the way the QoS-handle creators above do,
     * is deliberate: {@link com.intellectus.int2dds.core.DomainParticipant}
     * needs the real code to raise the exception it maps to through {@link
     * com.intellectus.int2dds.internal.ReturnCodes#check}, and a 0 handle
     * alone cannot carry that. Every create bridge added for Topic, Publisher
     * and DataWriter follows this same {@code (rc, long[] handleOut)} shape
     * for that reason.
     */
    public static int createParticipant(long factory, int domainId, long qos, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_participant(factory, domainId, qos, directBufferAddress(slot));
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a participant. Returns the C ABI status code. */
    public static int deleteParticipant(long participant) {
        return Ffi.int2dds_delete_participant(participant);
    }

    /**
     * Reads a participant's resolved domain id into {@code domainIdOut}
     * (a direct address, at least 4 bytes) and returns the C ABI status
     * code. "Resolved" matters specifically for {@code DEFAULT_DOMAIN_ID}
     * (-1): the core substitutes {@code DDS_DOMAIN_ID} (or 0) for -1 before
     * ever constructing the participant, so this reads back that
     * substituted value, not -1 itself.
     */
    public static int participantGetDomainId(long participant, long domainIdOut) {
        return Ffi.int2dds_participant_get_domain_id(participant, domainIdOut);
    }

    /** Creates a standalone DomainParticipant QoS handle, or 0 on failure. */
    public static long createParticipantQos() {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_participant_qos_create_default(directBufferAddress(slot));
        return rc != 0 ? 0L : slot.getLong(0);
    }

    /** Releases a QoS handle from {@link #createParticipantQos()}. */
    public static void destroyParticipantQos(long handle) {
        Ffi.int2dds_participant_qos_destroy(handle);
    }

    /** Creates a standalone Publisher QoS handle, or 0 on failure. */
    public static long createPublisherQos() {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publisher_qos_create_default(directBufferAddress(slot));
        return rc != 0 ? 0L : slot.getLong(0);
    }

    /** Releases a QoS handle from {@link #createPublisherQos()}. */
    public static void destroyPublisherQos(long handle) {
        Ffi.int2dds_publisher_qos_destroy(handle);
    }

    /** Creates a standalone Subscriber QoS handle, or 0 on failure. */
    public static long createSubscriberQos() {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscriber_qos_create_default(directBufferAddress(slot));
        return rc != 0 ? 0L : slot.getLong(0);
    }

    /** Releases a QoS handle from {@link #createSubscriberQos()}. */
    public static void destroySubscriberQos(long handle) {
        Ffi.int2dds_subscriber_qos_destroy(handle);
    }

    /** Creates a standalone Topic QoS handle, or 0 on failure. */
    public static long createTopicQos() {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_topic_qos_create_default(directBufferAddress(slot));
        return rc != 0 ? 0L : slot.getLong(0);
    }

    /** Releases a QoS handle from {@link #createTopicQos()}. */
    public static void destroyTopicQos(long handle) {
        Ffi.int2dds_topic_qos_destroy(handle);
    }

    /** Creates a standalone DataWriter QoS handle, or 0 on failure. */
    public static long createDataWriterQos() {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datawriter_qos_create_default(directBufferAddress(slot));
        return rc != 0 ? 0L : slot.getLong(0);
    }

    /** Releases a QoS handle from {@link #createDataWriterQos()}. */
    public static void destroyDataWriterQos(long handle) {
        Ffi.int2dds_datawriter_qos_destroy(handle);
    }

    /** Creates a standalone DataReader QoS handle, or 0 on failure. */
    public static long createDataReaderQos() {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datareader_qos_create_default(directBufferAddress(slot));
        return rc != 0 ? 0L : slot.getLong(0);
    }

    /** Releases a QoS handle from {@link #createDataReaderQos()}. */
    public static void destroyDataReaderQos(long handle) {
        Ffi.int2dds_datareader_qos_destroy(handle);
    }

    // --- Topic / Publisher ---

    /**
     * Creates a topic. Returns the C ABI status code and, only on success,
     * writes the new handle to {@code handleOut[0]}; on failure {@code
     * handleOut} is left untouched — the same shape as {@link
     * #createParticipant}, which explains why a status code travels
     * alongside the handle here too. {@code topicName} and {@code typeName}
     * cross as UTF-8 {@code byte[]}, never {@code String}: JNI's modified
     * UTF-8 would corrupt either one. {@code qos} is {@code 0L} for the
     * core's default Topic QoS.
     */
    public static int createTopic(long participant, byte[] topicName, byte[] typeName,
            int extensibility, long qos, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_topic(
                participant, topicName, typeName, extensibility, qos, directBufferAddress(slot));
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a topic. Returns the C ABI status code. */
    public static int deleteTopic(long topic) {
        return Ffi.int2dds_delete_topic(topic);
    }

    /**
     * Creates a publisher. Returns the C ABI status code and, only on
     * success, writes the new handle to {@code handleOut[0]}; on failure
     * {@code handleOut} is left untouched. {@code qos} is {@code 0L} for the
     * core's default Publisher QoS.
     */
    public static int createPublisher(long participant, long qos, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_publisher(participant, qos, directBufferAddress(slot));
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a publisher. Returns the C ABI status code. */
    public static int deletePublisher(long publisher) {
        return Ffi.int2dds_delete_publisher(publisher);
    }

    // --- Topic QoS setters ---

    public static int topicQosSetReliability(long qos, int kind, long maxBlockingTimeNs) {
        return Ffi.int2dds_topic_qos_set_reliability(qos, kind, maxBlockingTimeNs);
    }

    public static int topicQosSetDurability(long qos, int kind) {
        return Ffi.int2dds_topic_qos_set_durability(qos, kind);
    }

    public static int topicQosSetHistory(long qos, int kind, int depth) {
        return Ffi.int2dds_topic_qos_set_history(qos, kind, depth);
    }

    public static int topicQosSetDeadline(long qos, long periodNs) {
        return Ffi.int2dds_topic_qos_set_deadline(qos, periodNs);
    }

    public static int topicQosSetLiveliness(long qos, int kind, long leaseDurationNs) {
        return Ffi.int2dds_topic_qos_set_liveliness(qos, kind, leaseDurationNs);
    }

    public static int topicQosSetDestinationOrder(long qos, int kind) {
        return Ffi.int2dds_topic_qos_set_destination_order(qos, kind);
    }

    public static int topicQosSetResourceLimits(
            long qos, int maxSamples, int maxInstances, int maxSamplesPerInstance) {
        return Ffi.int2dds_topic_qos_set_resource_limits(
                qos, maxSamples, maxInstances, maxSamplesPerInstance);
    }

    public static int topicQosSetTransportPriority(long qos, int priority) {
        return Ffi.int2dds_topic_qos_set_transport_priority(qos, priority);
    }

    public static int topicQosSetLifespan(long qos, long durationNs) {
        return Ffi.int2dds_topic_qos_set_lifespan(qos, durationNs);
    }

    public static int topicQosSetOwnership(long qos, int kind) {
        return Ffi.int2dds_topic_qos_set_ownership(qos, kind);
    }

    public static int topicQosSetDataRepresentation(long qos, int kind) {
        return Ffi.int2dds_topic_qos_set_data_representation(qos, kind);
    }

    // --- DataWriter QoS setters ---

    public static int writerQosSetReliability(long qos, int kind, long maxBlockingTimeNs) {
        return Ffi.int2dds_datawriter_qos_set_reliability(qos, kind, maxBlockingTimeNs);
    }

    public static int writerQosSetDurability(long qos, int kind) {
        return Ffi.int2dds_datawriter_qos_set_durability(qos, kind);
    }

    public static int writerQosSetHistory(long qos, int kind, int depth) {
        return Ffi.int2dds_datawriter_qos_set_history(qos, kind, depth);
    }

    public static int writerQosSetOwnership(long qos, int kind) {
        return Ffi.int2dds_datawriter_qos_set_ownership(qos, kind);
    }

    public static int writerQosSetOwnershipStrength(long qos, int value) {
        return Ffi.int2dds_datawriter_qos_set_ownership_strength(qos, value);
    }

    public static int writerQosSetResourceLimits(
            long qos, int maxSamples, int maxInstances, int maxSamplesPerInstance) {
        return Ffi.int2dds_datawriter_qos_set_resource_limits(
                qos, maxSamples, maxInstances, maxSamplesPerInstance);
    }

    public static int writerQosSetLifespan(long qos, long durationNs) {
        return Ffi.int2dds_datawriter_qos_set_lifespan(qos, durationNs);
    }

    public static int writerQosSetDestinationOrder(long qos, int kind) {
        return Ffi.int2dds_datawriter_qos_set_destination_order(qos, kind);
    }

    public static int writerQosSetLatencyBudget(long qos, long durationNs) {
        return Ffi.int2dds_datawriter_qos_set_latency_budget(qos, durationNs);
    }

    public static int writerQosSetTransportPriority(long qos, int priority) {
        return Ffi.int2dds_datawriter_qos_set_transport_priority(qos, priority);
    }

    public static int writerQosSetUserData(long qos, long dataAddr, long dataLen) {
        return Ffi.int2dds_datawriter_qos_set_user_data(qos, dataAddr, dataLen);
    }

    public static int writerQosSetWriterDataLifecycle(long qos, boolean autodispose) {
        return Ffi.int2dds_datawriter_qos_set_writer_data_lifecycle(qos, autodispose);
    }

    public static int writerQosSetDataRepresentation(long qos, int kind) {
        return Ffi.int2dds_datawriter_qos_set_data_representation(qos, kind);
    }

    public static int writerQosSetDeadline(long qos, long periodNs) {
        return Ffi.int2dds_datawriter_qos_set_deadline(qos, periodNs);
    }

    public static int writerQosSetLiveliness(long qos, int kind, long leaseDurationNs) {
        return Ffi.int2dds_datawriter_qos_set_liveliness(qos, kind, leaseDurationNs);
    }

    /** DATA_FRAG max fragment size, an int2DDS extension. */
    public static int writerQosSetDataFrag(long qos, int value) {
        return Ffi.int2dds_datawriter_qos_set_data_frag(qos, value);
    }

    // --- DataWriter QoS getters ---

    public static int writerQosGetReliability(long qos, long kindOut, long maxBlockingTimeNsOut) {
        return Ffi.int2dds_datawriter_qos_get_reliability(qos, kindOut, maxBlockingTimeNsOut);
    }

    public static int writerQosGetDurability(long qos, long kindOut) {
        return Ffi.int2dds_datawriter_qos_get_durability(qos, kindOut);
    }

    public static int writerQosGetHistory(long qos, long kindOut, long depthOut) {
        return Ffi.int2dds_datawriter_qos_get_history(qos, kindOut, depthOut);
    }

    public static int writerQosGetOwnership(long qos, long kindOut) {
        return Ffi.int2dds_datawriter_qos_get_ownership(qos, kindOut);
    }

    public static int writerQosGetOwnershipStrength(long qos, long valueOut) {
        return Ffi.int2dds_datawriter_qos_get_ownership_strength(qos, valueOut);
    }

    public static int writerQosGetResourceLimits(
            long qos, long maxSamplesOut, long maxInstancesOut, long maxPerInstanceOut) {
        return Ffi.int2dds_datawriter_qos_get_resource_limits(
                qos, maxSamplesOut, maxInstancesOut, maxPerInstanceOut);
    }

    public static int writerQosGetLifespan(long qos, long durationNsOut) {
        return Ffi.int2dds_datawriter_qos_get_lifespan(qos, durationNsOut);
    }

    public static int writerQosGetDestinationOrder(long qos, long kindOut) {
        return Ffi.int2dds_datawriter_qos_get_destination_order(qos, kindOut);
    }

    public static int writerQosGetLatencyBudget(long qos, long durationNsOut) {
        return Ffi.int2dds_datawriter_qos_get_latency_budget(qos, durationNsOut);
    }

    public static int writerQosGetTransportPriority(long qos, long valueOut) {
        return Ffi.int2dds_datawriter_qos_get_transport_priority(qos, valueOut);
    }

    public static int writerQosGetWriterDataLifecycle(long qos, long autodisposeOut) {
        return Ffi.int2dds_datawriter_qos_get_writer_data_lifecycle(qos, autodisposeOut);
    }

    public static int writerQosGetDataRepresentation(long qos, long kindOut) {
        return Ffi.int2dds_datawriter_qos_get_data_representation(qos, kindOut);
    }

    public static int writerQosGetDeadline(long qos, long periodNsOut) {
        return Ffi.int2dds_datawriter_qos_get_deadline(qos, periodNsOut);
    }

    public static int writerQosGetLiveliness(long qos, long kindOut, long leaseDurationNsOut) {
        return Ffi.int2dds_datawriter_qos_get_liveliness(qos, kindOut, leaseDurationNsOut);
    }

    /** DATA_FRAG max fragment size, an int2DDS extension. */
    public static int writerQosGetDataFrag(long qos, long valueOut) {
        return Ffi.int2dds_datawriter_qos_get_data_frag(qos, valueOut);
    }

    // --- DataReader QoS setters ---

    public static int readerQosSetReliability(long qos, int kind, long maxBlockingTimeNs) {
        return Ffi.int2dds_datareader_qos_set_reliability(qos, kind, maxBlockingTimeNs);
    }

    public static int readerQosSetDurability(long qos, int kind) {
        return Ffi.int2dds_datareader_qos_set_durability(qos, kind);
    }

    public static int readerQosSetHistory(long qos, int kind, int depth) {
        return Ffi.int2dds_datareader_qos_set_history(qos, kind, depth);
    }

    public static int readerQosSetOwnership(long qos, int kind) {
        return Ffi.int2dds_datareader_qos_set_ownership(qos, kind);
    }

    public static int readerQosSetResourceLimits(
            long qos, int maxSamples, int maxInstances, int maxSamplesPerInstance) {
        return Ffi.int2dds_datareader_qos_set_resource_limits(
                qos, maxSamples, maxInstances, maxSamplesPerInstance);
    }

    public static int readerQosSetDestinationOrder(long qos, int kind) {
        return Ffi.int2dds_datareader_qos_set_destination_order(qos, kind);
    }

    public static int readerQosSetTimeBasedFilter(long qos, long minimumSeparationNs) {
        return Ffi.int2dds_datareader_qos_set_time_based_filter(qos, minimumSeparationNs);
    }

    public static int readerQosSetLatencyBudget(long qos, long durationNs) {
        return Ffi.int2dds_datareader_qos_set_latency_budget(qos, durationNs);
    }

    public static int readerQosSetUserData(long qos, long dataAddr, long dataLen) {
        return Ffi.int2dds_datareader_qos_set_user_data(qos, dataAddr, dataLen);
    }

    public static int readerQosSetReaderDataLifecycle(
            long qos, long autopurgeNowriterNs, long autopurgeDisposedNs) {
        return Ffi.int2dds_datareader_qos_set_reader_data_lifecycle(
                qos, autopurgeNowriterNs, autopurgeDisposedNs);
    }

    public static int readerQosSetDataRepresentation(long qos, int kind) {
        return Ffi.int2dds_datareader_qos_set_data_representation(qos, kind);
    }

    public static int readerQosSetDeadline(long qos, long periodNs) {
        return Ffi.int2dds_datareader_qos_set_deadline(qos, periodNs);
    }

    public static int readerQosSetLiveliness(long qos, int kind, long leaseDurationNs) {
        return Ffi.int2dds_datareader_qos_set_liveliness(qos, kind, leaseDurationNs);
    }

    // --- DataReader QoS getters ---

    public static int readerQosGetReliability(long qos, long kindOut, long maxBlockingTimeNsOut) {
        return Ffi.int2dds_datareader_qos_get_reliability(qos, kindOut, maxBlockingTimeNsOut);
    }

    public static int readerQosGetDurability(long qos, long kindOut) {
        return Ffi.int2dds_datareader_qos_get_durability(qos, kindOut);
    }

    public static int readerQosGetHistory(long qos, long kindOut, long depthOut) {
        return Ffi.int2dds_datareader_qos_get_history(qos, kindOut, depthOut);
    }

    public static int readerQosGetOwnership(long qos, long kindOut) {
        return Ffi.int2dds_datareader_qos_get_ownership(qos, kindOut);
    }

    public static int readerQosGetResourceLimits(
            long qos, long maxSamplesOut, long maxInstancesOut, long maxPerInstanceOut) {
        return Ffi.int2dds_datareader_qos_get_resource_limits(
                qos, maxSamplesOut, maxInstancesOut, maxPerInstanceOut);
    }

    public static int readerQosGetDestinationOrder(long qos, long kindOut) {
        return Ffi.int2dds_datareader_qos_get_destination_order(qos, kindOut);
    }

    public static int readerQosGetTimeBasedFilter(long qos, long minSeparationNsOut) {
        return Ffi.int2dds_datareader_qos_get_time_based_filter(qos, minSeparationNsOut);
    }

    public static int readerQosGetLatencyBudget(long qos, long durationNsOut) {
        return Ffi.int2dds_datareader_qos_get_latency_budget(qos, durationNsOut);
    }

    public static int readerQosGetReaderDataLifecycle(
            long qos, long autopurgeNowriterNsOut, long autopurgeDisposedNsOut) {
        return Ffi.int2dds_datareader_qos_get_reader_data_lifecycle(
                qos, autopurgeNowriterNsOut, autopurgeDisposedNsOut);
    }

    public static int readerQosGetDataRepresentation(long qos, long kindOut) {
        return Ffi.int2dds_datareader_qos_get_data_representation(qos, kindOut);
    }

    public static int readerQosGetDeadline(long qos, long periodNsOut) {
        return Ffi.int2dds_datareader_qos_get_deadline(qos, periodNsOut);
    }

    public static int readerQosGetLiveliness(long qos, long kindOut, long leaseDurationNsOut) {
        return Ffi.int2dds_datareader_qos_get_liveliness(qos, kindOut, leaseDurationNsOut);
    }

    // --- DomainParticipant QoS setters ---

    public static int participantQosSetUserData(long qos, long dataAddr, long dataLen) {
        return Ffi.int2dds_participant_qos_set_user_data(qos, dataAddr, dataLen);
    }

    /** Adds or overwrites one text property entry (PropertyQosPolicy). */
    public static int participantQosAddProperty(
            long qos, byte[] name, byte[] value, boolean propagate) {
        return Ffi.int2dds_participant_qos_add_property(qos, name, value, propagate);
    }

    // --- Publisher / Subscriber QoS setters ---

    public static int publisherQosSetPartition(long qos, byte[][] names, long count) {
        return Ffi.int2dds_publisher_qos_set_partition(qos, names, count);
    }

    public static int subscriberQosSetPartition(long qos, byte[][] names, long count) {
        return Ffi.int2dds_subscriber_qos_set_partition(qos, names, count);
    }

    // --- Type info / dynamic sample (CDR conformance) ---

    /** Creates a type-info builder, or 0 on failure. */
    public static long typeInfoCreate(byte[] typeName, int extensibility) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_type_info_create(typeName, extensibility, directBufferAddress(slot));
        return rc == 0 ? slot.getLong(0) : 0L;
    }

    /** Appends a field. Returns the C ABI status code. */
    public static int typeInfoAddField(long typeInfo, byte[] fieldName, int fieldType, int flags) {
        return Ffi.int2dds_type_info_add_field(typeInfo, fieldName, fieldType, flags);
    }

    /** Builds a type object from a completed builder, or 0 on failure. */
    public static long typeInfoToTypeObject(long typeInfo) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_type_info_to_type_object(typeInfo, directBufferAddress(slot));
        return rc == 0 ? slot.getLong(0) : 0L;
    }

    /** Releases a type-info builder. */
    public static void typeInfoDestroy(long typeInfo) {
        Ffi.int2dds_type_info_destroy(typeInfo);
    }

    /**
     * Releases a type object built by {@link #typeInfoToTypeObject}. This is a
     * distinct native allocation from the builder that produced it — both
     * must be released.
     */
    public static void typeObjectDestroy(long typeObject) {
        Ffi.int2dds_type_object_destroy(typeObject);
    }

    /** Decodes one i32 field out of a serialized sample. */
    public static int dynamicSampleGetI32(long bytes, long len, long typeObj,
            byte[] fieldName, long out) {
        return Ffi.int2dds_dynamic_sample_get_i32(bytes, len, typeObj, fieldName, out);
    }

    /** Decodes one f64 field out of a serialized sample. */
    public static int dynamicSampleGetF64(long bytes, long len, long typeObj,
            byte[] fieldName, long out) {
        return Ffi.int2dds_dynamic_sample_get_f64(bytes, len, typeObj, fieldName, out);
    }
}
