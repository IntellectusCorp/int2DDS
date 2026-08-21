package com.intellectus.int2dds.internal.ffi;

import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.NativeLoader;
import com.intellectus.int2dds.listeners.DataReaderListener;
import com.intellectus.int2dds.listeners.DataWriterListener;
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
        // slot is read below only on the rc == 0 path; on rc != 0 nothing
        // else touches it, so without this fence the JIT could treat it as
        // dead before the native call above actually finishes using the
        // address it was handed, and slot's own JDK-managed Cleaner could
        // free that memory out from under an in-flight native call. See
        // NativeKeepAlive's own doc for the full argument; every other
        // directBufferAddress(slot) call site in this file needs the same
        // fence for the same reason.
        NativeKeepAlive.keepAlive(slot);
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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
        return rc != 0 ? 0L : slot.getLong(0);
    }

    /** Releases a QoS handle from {@link #createDataReaderQos()}. */
    public static void destroyDataReaderQos(long handle) {
        Ffi.int2dds_datareader_qos_destroy(handle);
    }

    // --- Topic / Publisher / DataWriter ---

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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a publisher. Returns the C ABI status code. */
    public static int deletePublisher(long publisher) {
        return Ffi.int2dds_delete_publisher(publisher);
    }

    /**
     * Creates a datawriter. Returns the C ABI status code and, only on
     * success, writes the new handle to {@code handleOut[0]}; on failure
     * {@code handleOut} is left untouched — the same shape as {@link
     * #createTopic} and {@link #createPublisher}. {@code qos} is {@code 0L}
     * for the core's default DataWriter QoS. {@code listener} is {@code 0L}
     * and {@code mask} is {@code 0} in this branch — listeners are a later
     * branch.
     */
    public static int createDataWriter(long publisher, long topic, long qos, long listener,
            int mask, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_datawriter(
                publisher, topic, qos, listener, mask, directBufferAddress(slot));
        // slot is read below only on the rc == 0 path; on rc != 0 nothing
        // else touches it, so without this fence the JIT could treat it as
        // dead before the native call above actually finishes using the
        // address it was handed, and slot's own JDK-managed Cleaner could
        // free that memory out from under an in-flight native call. See
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a datawriter. Returns the C ABI status code. */
    public static int deleteDataWriter(long writer) {
        return Ffi.int2dds_delete_datawriter(writer);
    }

    // --- DataWriter listeners (hand-written trampoline layer) ---

    /**
     * Installs {@code listener} on {@code writer} for {@code mask}. Returns the
     * binding-owned context pointer to pass back to {@link #writerListenerClear},
     * or 0 on failure. Policy-free passthrough to {@link FfiHandwritten}.
     */
    public static long writerListenerSet(long writer, DataWriterListener listener, int mask) {
        return FfiHandwritten.nativeWriterListenerSet(writer, listener, mask);
    }

    /**
     * Clears the listener on {@code writer} and releases {@code ctx}. Returns the
     * C ABI status code. Policy-free passthrough to {@link FfiHandwritten}.
     */
    public static int writerListenerClear(long writer, long ctx) {
        return FfiHandwritten.nativeWriterListenerClear(writer, ctx);
    }

    /**
     * Writes a pre-serialized CDR sample. No key is passed: the core derives
     * the instance key and KeyHash canonically from {@code data}, the full
     * serialized sample, for a keyed topic the same way as an unkeyed one.
     */
    public static int datawriterWriteSerialized(long writer, long data, long dataLen) {
        return Ffi.int2dds_datawriter_write_serialized(writer, data, dataLen);
    }

    /**
     * Reads a datawriter's current QoS into a freshly allocated native
     * handle. Returns the C ABI status code and, only on success, writes the
     * new QoS handle to {@code handleOut[0]}; on failure {@code handleOut} is
     * left untouched — the same shape as {@link #createParticipant}. Unlike
     * {@link #createParticipantQos} and its siblings, this does not fold a
     * failure into a bare {@code 0L}: {@code writer} names a real, possibly
     * already-invalid entity, so the real code is worth preserving for
     * {@link com.intellectus.int2dds.internal.ReturnCodes#check} to map,
     * rather than collapsing every failure into one generic exception.
     */
    public static int getWriterQos(long writer, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datawriter_get_qos(writer, directBufferAddress(slot));
        // See createDataWriter's identical fence just above.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
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
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
        return rc == 0 ? slot.getLong(0) : 0L;
    }

    /** Appends a field. Returns the C ABI status code. */
    public static int typeInfoAddField(long typeInfo, byte[] fieldName, int fieldType, int flags) {
        return Ffi.int2dds_type_info_add_field(typeInfo, fieldName, fieldType, flags);
    }

    /** Appends a sequence field ({@code bound == 0} means unbounded). Returns the C ABI status code. */
    public static int typeInfoAddSequenceField(
            long typeInfo, byte[] fieldName, int elementType, int bound, int flags) {
        return Ffi.int2dds_type_info_add_sequence_field(typeInfo, fieldName, elementType, bound, flags);
    }

    /**
     * Appends a nested struct-typed field, referencing {@code nestedTypeInfo}'s own
     * builder. {@code nestedTypeInfo} is borrowed, not consumed -- the caller still owns
     * it and must destroy it separately. Returns the C ABI status code.
     */
    public static int typeInfoAddNestedField(
            long typeInfo, byte[] fieldName, long nestedTypeInfo, int flags) {
        return Ffi.int2dds_type_info_add_nested_field(typeInfo, fieldName, nestedTypeInfo, flags);
    }

    /** Builds a type object from a completed builder, or 0 on failure. */
    public static long typeInfoToTypeObject(long typeInfo) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_type_info_to_type_object(typeInfo, directBufferAddress(slot));
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
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

    // --- DynamicData (handle-based, xtypes read path) ---

    /**
     * Decodes {@code bytesAddr}/{@code len} (a serialized sample, encapsulation
     * header included) against {@code typeObj} into a live DynamicData handle,
     * writing it to {@code out[0]} only on success. {@code bytesAddr} is a
     * direct-buffer address, the same shape as {@link #dynamicSampleGetI32}'s
     * {@code bytes}.
     */
    public static int dynamicDataFromSample(
            long participant, long bytesAddr, long len, long typeObj, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_from_sample(
                participant, bytesAddr, len, typeObj, directBufferAddress(slot));
        // Same hazard as participantFactoryGetInstance's fence above -- see
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a DynamicData handle. */
    public static void dynamicDataDestroy(long d) {
        Ffi.int2dds_dynamic_data_destroy(d);
    }

    /** Reads an i32 field at {@code path}, writing it to {@code out[0]} on success. */
    public static int dynamicDataGetI32(long d, byte[] path, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_i32(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0); // native writes an i32 (4 bytes), not 8
        }
        return rc;
    }

    /** Reads an f64 field at {@code path}, writing it to {@code out[0]} on success. */
    public static int dynamicDataGetF64(long d, byte[] path, double[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_f64(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getDouble(0); // native writes a full 8-byte f64
        }
        return rc;
    }

    /**
     * Grow-and-retry driver for a string field at {@code path}, mirroring {@link
     * #readGrowableString} but for a different native contract: {@code
     * int2dds_dynamic_data_get_string} (ffi/src/dynamic.rs {@code copy_str_to_c})
     * reports the required size WITHOUT the trailing NUL -- both on {@code
     * RET_BUFFER_TOO_SMALL} and on success -- unlike {@code copy_string_to_c}
     * (discovery.rs), which {@link #readGrowableString} was written for and which
     * includes the NUL in its size. Regrowing to {@code outLen} (not {@code
     * outLen + 1}) here would repeat the same too-small capacity forever, so this
     * regrows to {@code outLen + 1} and does not subtract 1 from the length on
     * the success path. {@code out_buf} crosses as a plain {@code byte[]} here,
     * not a direct-buffer address -- the generated shim marshals it as a JNI
     * array (see {@code Java_..._int2dds_1dynamic_1data_1get_1string} in
     * generated.rs) -- so only {@code sizeSlot}, the one direct buffer, needs
     * the keepAlive fence. Never throws -- policy-free like every other bridge
     * here -- it just returns the final status code and, only on {@code RET_OK},
     * writes the decoded UTF-8 bytes to {@code bytesOut[0]}.
     */
    public static int dynamicDataGetString(long d, byte[] path, byte[][] bytesOut) {
        int cap = 64;
        while (true) {
            byte[] buf = new byte[cap];
            ByteBuffer sizeSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = Ffi.int2dds_dynamic_data_get_string(
                    d, path, buf, cap, directBufferAddress(sizeSlot));
            NativeKeepAlive.keepAlive(sizeSlot);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                cap = (int) sizeSlot.getLong(0) + 1; // out_len excludes the NUL here
                continue;
            }
            if (rc == 0) {
                int n = (int) sizeSlot.getLong(0); // excludes the NUL -- no -1 needed
                byte[] out = new byte[n];
                System.arraycopy(buf, 0, out, 0, n);
                bytesOut[0] = out;
            }
            return rc;
        }
    }

    /** Reads a bool field at {@code path}, writing it to {@code out[0]} on success. */
    public static int dynamicDataGetBool(long d, byte[] path, boolean[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_bool(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) != 0; // bool out is 1 byte, not 4
        }
        return rc;
    }

    /** Reads an i8 field at {@code path}, writing it to {@code out[0]} on success. */
    public static int dynamicDataGetI8(long d, byte[] path, byte[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_i8(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0); // native writes an i8 (1 byte)
        }
        return rc;
    }

    /** Reads a u8 field at {@code path}, writing its unsigned 0-255 value to {@code out[0]} on success. */
    public static int dynamicDataGetU8(long d, byte[] path, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_u8(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) & 0xFF; // native writes a u8 (1 byte); widen unsigned
        }
        return rc;
    }

    /** Reads an i16 field at {@code path}, writing it to {@code out[0]} on success. */
    public static int dynamicDataGetI16(long d, byte[] path, short[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_i16(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getShort(0); // native writes an i16 (2 bytes)
        }
        return rc;
    }

    /** Reads a u16 field at {@code path}, writing its unsigned 0-65535 value to {@code out[0]} on success. */
    public static int dynamicDataGetU16(long d, byte[] path, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_u16(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getShort(0) & 0xFFFF; // native writes a u16 (2 bytes); widen unsigned
        }
        return rc;
    }

    /**
     * Reads a u32 field at {@code path}, writing its raw 32 bits to {@code
     * out[0]} on success -- callers wanting the unsigned magnitude widen with
     * {@code & 0xFFFFFFFFL} themselves, mirroring how {@link
     * #statusConditionGetEnabledStatuses} hands back a raw u32 mask.
     */
    public static int dynamicDataGetU32(long d, byte[] path, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_u32(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0); // native writes a u32 (4 bytes); raw bits, not widened
        }
        return rc;
    }

    /** Reads an i64 field at {@code path}, writing it to {@code out[0]} on success. */
    public static int dynamicDataGetI64(long d, byte[] path, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_i64(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0); // native writes a full 8-byte i64
        }
        return rc;
    }

    /**
     * Reads a u64 field at {@code path}, writing its raw 64 bits to {@code
     * out[0]} on success -- same "raw bits, caller widens" contract as {@link
     * #dynamicDataGetU32}.
     */
    public static int dynamicDataGetU64(long d, byte[] path, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_u64(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0); // native writes a u64 (8 bytes); raw bits, not widened
        }
        return rc;
    }

    /** Reads an f32 field at {@code path}, writing it to {@code out[0]} on success. */
    public static int dynamicDataGetF32(long d, byte[] path, float[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_f32(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getFloat(0); // native writes a full 4-byte f32
        }
        return rc;
    }

    /** Reads a char8 field at {@code path} as its raw byte value, writing it to {@code out[0]} on success. */
    public static int dynamicDataGetChar8(long d, byte[] path, byte[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_char8(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0); // native writes a char8 as one byte
        }
        return rc;
    }

    /**
     * Reads the element count of a sequence/array field at {@code path},
     * writing it to {@code out[0]} on success.
     */
    public static int dynamicDataGetLen(long d, byte[] path, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_len(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0); // native writes a usize (8 bytes on this platform)
        }
        return rc;
    }

    /**
     * Extracts a nested struct field at {@code path} into a new DynamicData
     * handle, writing it to {@code out[0]} on success. An independent native
     * box (ffi/src/dynamic.rs clones the nested value into its own {@code
     * Int2DdsDynamicData}), released through {@link #dynamicDataDestroy} the
     * same as a top-level handle.
     */
    public static int dynamicDataGetMember(long d, byte[] path, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_member(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    // --- XmlTypeRegistry / DynamicTypeSupport / DynamicData (xtypes write path) ---

    /** Creates an XML type registry, writing its handle to {@code out[0]} on success. */
    public static int xmlTypeRegistryCreate(long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_xml_type_registry_create(directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Loads XML type descriptions (as UTF-8 bytes) into a registry. */
    public static int xmlTypeRegistryLoadStr(long registry, byte[] xml) {
        return Ffi.int2dds_xml_type_registry_load_str(registry, xml);
    }

    /** Reads the number of types loaded into a registry, writing it to {@code out[0]} on success. */
    public static int xmlTypeRegistryTypeCount(long registry, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_xml_type_registry_type_count(registry, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0); // native writes a usize (8 bytes on this platform)
        }
        return rc;
    }

    /**
     * Looks up a loaded type by name, writing a DynamicTypeSupport handle to
     * {@code out[0]} on success.
     */
    public static int xmlTypeRegistryGetTypeSupport(long registry, byte[] name, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_xml_type_registry_get_type_support(registry, name, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases an XML type registry. */
    public static void xmlTypeRegistryDestroy(long registry) {
        Ffi.int2dds_xml_type_registry_destroy(registry);
    }

    /** Releases a DynamicTypeSupport handle. */
    public static void dynamicTypeSupportDestroy(long support) {
        Ffi.int2dds_dynamic_type_support_destroy(support);
    }

    /**
     * Creates a writable DynamicData instance from a DynamicTypeSupport,
     * writing its handle to {@code out[0]} on success.
     */
    public static int dynamicDataCreate(long support, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_create(support, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Sets a bool field at {@code field}. Returns the C ABI status code. */
    public static int dynamicDataSetBool(long data, byte[] field, boolean value) {
        return Ffi.int2dds_dynamic_data_set_bool(data, field, value);
    }

    /** Sets an i8 field at {@code field} (passed widened as an int). Returns the C ABI status code. */
    public static int dynamicDataSetI8(long data, byte[] field, int value) {
        return Ffi.int2dds_dynamic_data_set_i8(data, field, value);
    }

    /** Sets a u8 field at {@code field} (passed widened as an int). Returns the C ABI status code. */
    public static int dynamicDataSetU8(long data, byte[] field, int value) {
        return Ffi.int2dds_dynamic_data_set_u8(data, field, value);
    }

    /** Sets an i16 field at {@code field} (passed widened as an int). Returns the C ABI status code. */
    public static int dynamicDataSetI16(long data, byte[] field, int value) {
        return Ffi.int2dds_dynamic_data_set_i16(data, field, value);
    }

    /** Sets a u16 field at {@code field} (passed widened as an int). Returns the C ABI status code. */
    public static int dynamicDataSetU16(long data, byte[] field, int value) {
        return Ffi.int2dds_dynamic_data_set_u16(data, field, value);
    }

    /** Sets an i32 field at {@code field}. Returns the C ABI status code. */
    public static int dynamicDataSetI32(long data, byte[] field, int value) {
        return Ffi.int2dds_dynamic_data_set_i32(data, field, value);
    }

    /** Sets a u32 field at {@code field} (raw bits). Returns the C ABI status code. */
    public static int dynamicDataSetU32(long data, byte[] field, int value) {
        return Ffi.int2dds_dynamic_data_set_u32(data, field, value);
    }

    /** Sets an i64 field at {@code field}. Returns the C ABI status code. */
    public static int dynamicDataSetI64(long data, byte[] field, long value) {
        return Ffi.int2dds_dynamic_data_set_i64(data, field, value);
    }

    /** Sets a u64 field at {@code field} (raw bits). Returns the C ABI status code. */
    public static int dynamicDataSetU64(long data, byte[] field, long value) {
        return Ffi.int2dds_dynamic_data_set_u64(data, field, value);
    }

    /** Sets an f32 field at {@code field}. Returns the C ABI status code. */
    public static int dynamicDataSetF32(long data, byte[] field, float value) {
        return Ffi.int2dds_dynamic_data_set_f32(data, field, value);
    }

    /** Sets an f64 field at {@code field}. Returns the C ABI status code. */
    public static int dynamicDataSetF64(long data, byte[] field, double value) {
        return Ffi.int2dds_dynamic_data_set_f64(data, field, value);
    }

    /** Sets a char8 field at {@code field} (passed widened as an int). Returns the C ABI status code. */
    public static int dynamicDataSetChar8(long data, byte[] field, int value) {
        return Ffi.int2dds_dynamic_data_set_char8(data, field, value);
    }

    /** Sets a string field at {@code field} to {@code value} (UTF-8 bytes). Returns the C ABI status code. */
    public static int dynamicDataSetString(long data, byte[] field, byte[] value) {
        return Ffi.int2dds_dynamic_data_set_string(data, field, value);
    }

    // --- Subscriber / DataReader (raw receive side, for WritePathEndToEndTest only) ---
    //
    // No Subscriber or DataReader Java entity exists yet -- the read branch
    // owns that. These five mirror the create/delete/take_serialized C ABI
    // directly, folding a failed create into a 0 handle the same way
    // createParticipantQos and typeInfoCreate already do above, rather than
    // the (int rc, long[] handleOut) shape createParticipant/createTopic/
    // createPublisher/createDataWriter use: those exist to hand a real,
    // per-code status to a NativeEntity constructor for ReturnCodes.check to
    // map to a specific exception, and nothing here feeds a NativeEntity --
    // the caller is a test that releases these handles by hand and only ever
    // needs to know success from failure.

    /** Creates a subscriber. Returns rc; writes the handle to handleOut[0] only when rc == 0. */
    public static int createSubscriber(long participant, long qos, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_subscriber(participant, qos, directBufferAddress(slot));
        // Same fence as createPublisher: slot's address was handed to the native
        // call, and slot is read below only on the rc == 0 path.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a subscriber. Returns the C ABI status code. */
    public static int deleteSubscriber(long subscriber) {
        return Ffi.int2dds_delete_subscriber(subscriber);
    }

    /** Creates a datareader. Returns rc; writes the handle to handleOut[0] only when rc == 0. */
    public static int createDataReader(long subscriber, long topic, long qos, long listener,
            int mask, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_datareader(
                subscriber, topic, qos, listener, mask, directBufferAddress(slot));
        // Same fence as createDataWriter.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a datareader. Returns the C ABI status code. */
    public static int deleteDataReader(long reader) {
        return Ffi.int2dds_delete_datareader(reader);
    }

    // --- DataReader listeners (hand-written trampoline layer) ---

    /**
     * Installs {@code listener} on {@code reader} for {@code mask}. Returns the
     * binding-owned context pointer to pass back to {@link #readerListenerClear},
     * or 0 on failure. Policy-free passthrough to {@link FfiHandwritten}.
     */
    public static long readerListenerSet(long reader, DataReaderListener listener, int mask) {
        return FfiHandwritten.nativeReaderListenerSet(reader, listener, mask);
    }

    /**
     * Clears the listener on {@code reader} and releases {@code ctx}. Returns the
     * C ABI status code. Policy-free passthrough to {@link FfiHandwritten}.
     */
    public static int readerListenerClear(long reader, long ctx) {
        return FfiHandwritten.nativeReaderListenerClear(reader, ctx);
    }

    /**
     * Takes one serialized sample into a caller-supplied buffer. Returns the
     * C ABI status code — {@code NO_DATA} when nothing is queued, not a
     * loan-based signal: unlike the loaned variants, this copies into {@code
     * buffer} and hands back nothing to return. {@code validDataOut} points
     * at a single {@code bool} (one byte), not an {@code int} — reading it as
     * four bytes picks up three unrelated adjacent ones, the same shape of
     * mistake the QoS read-back hit on the previous branch.
     */
    public static int datareaderTakeSerialized(long reader, long buffer, long bufferCapacity,
            long actualSizeOut, long validDataOut) {
        return Ffi.int2dds_datareader_take_serialized(
                reader, buffer, bufferCapacity, actualSizeOut, validDataOut);
    }

    /** take with full SampleInfo. Copies into the caller buffer; sample removed only if it fits. */
    public static int datareaderTakeSerializedWInfo(long reader, long buffer, long bufferCapacity,
            long actualSizeOut, long infoOut) {
        return Ffi.int2dds_datareader_take_serialized_w_info(
                reader, buffer, bufferCapacity, actualSizeOut, infoOut);
    }

    /** read with full SampleInfo. Same signature as take; does not remove the sample. */
    public static int datareaderReadSerializedWInfo(long reader, long buffer, long bufferCapacity,
            long actualSizeOut, long infoOut) {
        return Ffi.int2dds_datareader_read_serialized_w_info(
                reader, buffer, bufferCapacity, actualSizeOut, infoOut);
    }

    /** Creates a guard condition, writing its handle to {@code handleOut[0]} on success. */
    public static int guardConditionNew(long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_guardcondition_new(directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Sets a guard condition's trigger value. */
    public static void guardConditionSetTrigger(long condition, boolean value) {
        Ffi.int2dds_guardcondition_set_trigger_value(condition, value);
    }

    /**
     * Reads a trigger value through the generic {@code Int2DdsCondition} accessor,
     * writing it to {@code out[0]} on success. Only valid for a handle obtained
     * from {@code condition_seq_get} — that wrapper type (a fat
     * {@code Arc<dyn Condition>}) has a different native layout from a concrete
     * condition's own handle (e.g. a GuardCondition's thin
     * {@code Arc<GuardCondition>}), so calling this on an original handle reads
     * across the mismatch and corrupts memory. Use {@link
     * #guardConditionGetTriggerValue} etc. for a concrete condition's own handle.
     */
    public static int conditionGetTriggerValue(long condition, boolean[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_condition_get_trigger_value(condition, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) != 0; // bool out is 1 byte, not 4
        }
        return rc;
    }

    /**
     * Reads a guard condition's own trigger value (its own handle, not a seq
     * entry), writing it to {@code out[0]} on success.
     */
    public static int guardConditionGetTriggerValue(long condition, boolean[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_guardcondition_get_trigger_value(condition, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) != 0; // bool out is 1 byte, not 4
        }
        return rc;
    }

    /** Releases a guard condition. Returns the C ABI status code. */
    public static int guardConditionDelete(long condition) {
        return Ffi.int2dds_guardcondition_delete(condition);
    }

    /** Releases any condition through the generic deleter. Returns the C ABI status code. */
    public static int conditionDelete(long condition) {
        return Ffi.int2dds_condition_delete(condition);
    }

    /** Creates a WaitSet, writing its handle to {@code handleOut[0]} on success. */
    public static int waitsetNew(long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_waitset_new(directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a WaitSet. Returns the C ABI status code. */
    public static int waitsetDelete(long waitset) {
        return Ffi.int2dds_waitset_delete(waitset);
    }

    /** Attaches a guard condition to a WaitSet. Returns the C ABI status code. */
    public static int waitsetAttachGuard(long waitset, long condition) {
        return Ffi.int2dds_waitset_attach_guardcondition(waitset, condition);
    }

    /** Detaches a guard condition from a WaitSet. Returns the C ABI status code. */
    public static int waitsetDetachGuard(long waitset, long condition) {
        return Ffi.int2dds_waitset_detach_guardcondition(waitset, condition);
    }

    /**
     * Blocks up to {@code timeoutMs} (negative = infinite) for an attached
     * condition to trigger. On success writes the resulting condition
     * sequence's handle to {@code seqOut[0]} — a new pointer sequence
     * unrelated to the attached handles, meant only to be passed to {@link
     * #conditionSeqDelete}, never to {@code condition_seq_get}. Returns
     * {@code RET_TIMEOUT} with {@code seqOut} left untouched when nothing
     * triggered before the timeout.
     */
    public static int waitsetWaitEx(long waitset, long timeoutMs, long[] seqOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_waitset_wait_ex(waitset, timeoutMs, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            seqOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a condition sequence returned by {@link #waitsetWaitEx}. */
    public static void conditionSeqDelete(long seq) {
        Ffi.int2dds_condition_seq_delete(seq);
    }

    /**
     * Mints a fresh status condition for {@code reader}, writing its handle to
     * {@code handleOut[0]} on success. Each call returns a new native box the
     * caller owns and must release through {@link #statusConditionDelete}.
     */
    public static int datareaderGetStatusCondition(long reader, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datareader_get_statuscondition(reader, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Mints a fresh status condition for {@code writer}, writing its handle to
     * {@code handleOut[0]} on success. Each call returns a new native box the
     * caller owns and must release through {@link #statusConditionDelete}.
     */
    public static int datawriterGetStatusCondition(long writer, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datawriter_get_statuscondition(writer, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Reads a status condition's own trigger value (its own handle, not a seq
     * entry), writing it to {@code out[0]} on success.
     */
    public static int statusConditionGetTriggerValue(long condition, boolean[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_statuscondition_get_trigger_value(condition, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) != 0; // bool out is 1 byte, not 4
        }
        return rc;
    }

    /** Reads a status condition's enabled-statuses mask, writing it to {@code outMask[0]} on success. */
    public static int statusConditionGetEnabledStatuses(long condition, int[] outMask) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_statuscondition_get_enabled_statuses(condition, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            outMask[0] = slot.getInt(0); // mask out is a u32
        }
        return rc;
    }

    /** Sets a status condition's enabled-statuses mask. Returns the C ABI status code. */
    public static int statusConditionSetEnabledStatuses(long condition, int mask) {
        return Ffi.int2dds_statuscondition_set_enabled_statuses(condition, mask);
    }

    /** Releases a status condition. Returns the C ABI status code. */
    public static int statusConditionDelete(long condition) {
        return Ffi.int2dds_statuscondition_delete(condition);
    }

    /** Attaches a status condition to a WaitSet. Returns the C ABI status code. */
    public static int waitsetAttachStatus(long waitset, long condition) {
        return Ffi.int2dds_waitset_attach_statuscondition(waitset, condition);
    }

    /** Detaches a status condition from a WaitSet. Returns the C ABI status code. */
    public static int waitsetDetachStatus(long waitset, long condition) {
        return Ffi.int2dds_waitset_detach_statuscondition(waitset, condition);
    }

    /**
     * Creates a read condition on {@code reader} for the given state masks,
     * writing its handle to {@code out[0]} on success. Each call mints a new
     * native box the caller owns and must release through {@link
     * #readConditionDelete}.
     */
    public static int datareaderCreateReadCondition(
            long reader, int sampleMask, int viewMask, int instanceMask, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datareader_create_readcondition(
                reader, sampleMask, viewMask, instanceMask, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates a query condition on {@code reader} for the given state masks
     * and query, writing its handle to {@code out[0]} on success. {@code
     * queryExpr} and {@code queryParams} cross as UTF-8 {@code byte[]} /
     * {@code byte[][]}, never {@code String}. Each call mints a new native
     * box the caller owns and must release through {@link
     * #readConditionDelete} — a query condition is deleted the same way as a
     * read condition.
     */
    public static int datareaderCreateQueryCondition(long reader, int sampleMask, int viewMask,
            int instanceMask, byte[] queryExpr, byte[][] queryParams, long paramsCount,
            long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datareader_create_querycondition(reader, sampleMask, viewMask,
                instanceMask, queryExpr, queryParams, paramsCount, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Reads a read condition's own trigger value (its own handle, not a seq
     * entry), writing it to {@code out[0]} on success. Also valid for a
     * QueryCondition handle, which is a ReadCondition.
     */
    public static int readConditionGetTriggerValue(long condition, boolean[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_readcondition_get_trigger_value(condition, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) != 0; // bool out is 1 byte, not 4
        }
        return rc;
    }

    /** Sets a query condition's query parameters. Returns the C ABI status code. */
    public static int querySetParameters(long condition, byte[][] params, long count) {
        return Ffi.int2dds_querycondition_set_query_parameters(condition, params, count);
    }

    /** Releases a read (or query) condition. Returns the C ABI status code. */
    public static int readConditionDelete(long condition) {
        return Ffi.int2dds_readcondition_delete(condition);
    }

    /** Attaches a read (or query) condition to a WaitSet. Returns the C ABI status code. */
    public static int waitsetAttachRead(long waitset, long condition) {
        return Ffi.int2dds_waitset_attach_readcondition(waitset, condition);
    }

    /** Detaches a read (or query) condition from a WaitSet. Returns the C ABI status code. */
    public static int waitsetDetachRead(long waitset, long condition) {
        return Ffi.int2dds_waitset_detach_readcondition(waitset, condition);
    }

    // --- Discovery: PublicationBuiltinTopicData (materialize-then-destroy) ---

    /** Bridge shape for a {@code copy_string_to_c} getter: (bufAddr, capacity, sizeOutAddr) -> rc. */
    public interface GrowableStringGetter {
        int get(long bufAddr, long capacity, long sizeOutAddr);
    }

    /** Bridge shape for a raw-bytes getter with no NUL convention, e.g. {@code user_data}. */
    public interface GrowableBytesGetter {
        int get(long bufAddr, long capacity, long sizeOutAddr);
    }

    /**
     * Grow-and-retry driver for a {@code copy_string_to_c}-style getter, mirroring
     * {@code DataReader}'s payload-growth idiom on the read path. On {@code
     * RET_BUFFER_TOO_SMALL} the size slot holds the required capacity (including
     * the trailing NUL), so this regrows and retries. Never throws -- policy-free
     * like every other bridge here -- it just returns the final status code and,
     * only on {@code RET_OK}, writes the decoded UTF-8 bytes (NUL trimmed) to
     * {@code bytesOut[0]}.
     */
    public static int readGrowableString(GrowableStringGetter getter, byte[][] bytesOut) {
        int cap = 256;
        while (true) {
            ByteBuffer buf = ByteBuffer.allocateDirect(cap);
            ByteBuffer sizeSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = getter.get(directBufferAddress(buf), cap, directBufferAddress(sizeSlot));
            NativeKeepAlive.keepAlive(buf);
            NativeKeepAlive.keepAlive(sizeSlot);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                cap = (int) sizeSlot.getLong(0);
                continue;
            }
            if (rc == 0) {
                int n = (int) sizeSlot.getLong(0);
                int len = n > 0 ? n - 1 : 0; // sizeOut includes the trailing NUL
                byte[] out = new byte[len];
                ((java.nio.Buffer) buf).position(0);
                buf.get(out, 0, len);
                bytesOut[0] = out;
            }
            return rc;
        }
    }

    /**
     * Grow-and-retry driver for a raw-bytes getter (e.g. {@code user_data}) that
     * has no {@code BUFFER_TOO_SMALL} signal of its own: it always returns
     * {@code RET_OK} and silently truncates when the buffer is too small, always
     * reporting the untruncated length via the size slot. So this regrows and
     * retries whenever the reported size exceeds what was actually copied,
     * rather than switching on the status code. Never throws -- policy-free.
     */
    public static int readGrowableBytes(GrowableBytesGetter getter, byte[][] bytesOut) {
        int cap = 256;
        while (true) {
            ByteBuffer buf = ByteBuffer.allocateDirect(cap);
            ByteBuffer sizeSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = getter.get(directBufferAddress(buf), cap, directBufferAddress(sizeSlot));
            NativeKeepAlive.keepAlive(buf);
            NativeKeepAlive.keepAlive(sizeSlot);
            if (rc != 0) {
                return rc;
            }
            int n = (int) sizeSlot.getLong(0);
            if (n > cap) {
                cap = n;
                continue;
            }
            byte[] out = new byte[n];
            ((java.nio.Buffer) buf).position(0);
            buf.get(out, 0, n);
            bytesOut[0] = out;
            return rc;
        }
    }

    /**
     * Collects a snapshot of discovered publications (the builtin DCPSPublication
     * reader), blocking up to {@code timeoutMs} (negative = infinite). Returns rc;
     * on {@code RET_OK} writes the snapshot sequence handle to {@code seqOut[0]}.
     * The caller owns the sequence and must release it with {@link
     * #pubDataSeqDelete}.
     */
    public static int takeDiscoveredPublicationsSnapshot(long participant, int timeoutMs,
            long[] seqOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_participant_take_discovered_publications_snapshot(
                participant, timeoutMs, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            seqOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Entry count of a publication snapshot. Returns rc; writes it to {@code out[0]} on success. */
    public static int pubDataSeqLength(long seq, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publication_builtin_topic_data_seq_length(seq, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Mints an owned {@code PublicationBuiltinTopicData} box for the entry at
     * {@code index} (the core clones its stored data into it). Returns rc; writes
     * the handle to {@code dataOut[0]} on success. The caller owns the box and
     * must release it with {@link #pubDataDestroy}.
     */
    public static int pubDataSeqGet(long seq, long index, long[] dataOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publication_builtin_topic_data_seq_get(seq, index, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            dataOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a publication snapshot sequence returned by {@link #takeDiscoveredPublicationsSnapshot}. */
    public static void pubDataSeqDelete(long seq) {
        Ffi.int2dds_publication_builtin_topic_data_seq_delete(seq);
    }

    /** Releases one owned {@code PublicationBuiltinTopicData} box minted by {@link #pubDataSeqGet}. */
    public static void pubDataDestroy(long data) {
        Ffi.int2dds_publication_builtin_topic_data_destroy(data);
    }

    /** The 12-byte instance key. {@code keyOut} must be a 12-byte array. */
    public static int pubDataGetKey(long data, byte[] keyOut) {
        return Ffi.int2dds_publication_builtin_topic_data_get_key(data, keyOut);
    }

    /** The 16-byte endpoint GUID. {@code guidOut} must be a 16-byte array. */
    public static int pubDataGetEndpointGuid(long data, byte[] guidOut) {
        return Ffi.int2dds_publication_builtin_topic_data_get_endpoint_guid(data, guidOut);
    }

    /** The 12-byte owning-participant key. {@code keyOut} must be a 12-byte array. */
    public static int pubDataGetParticipantKey(long data, byte[] keyOut) {
        return Ffi.int2dds_publication_builtin_topic_data_get_participant_key(data, keyOut);
    }

    /** Direct passthrough for {@link #readGrowableString}: the topic name getter. */
    public static int pubDataGetTopicName(long data, long buf, long capacity, long sizeOut) {
        return Ffi.int2dds_publication_builtin_topic_data_get_topic_name(data, buf, capacity, sizeOut);
    }

    /** Direct passthrough for {@link #readGrowableString}: the type name getter. */
    public static int pubDataGetTypeName(long data, long buf, long capacity, long sizeOut) {
        return Ffi.int2dds_publication_builtin_topic_data_get_type_name(data, buf, capacity, sizeOut);
    }

    /** Direct passthrough for {@link #readGrowableBytes}: the user_data getter. */
    public static int pubDataGetUserData(long data, long buf, long capacity, long sizeOut) {
        return Ffi.int2dds_publication_builtin_topic_data_get_user_data(data, buf, capacity, sizeOut);
    }

    /** Reliability kind: 0 = BEST_EFFORT, 1 = RELIABLE. Writes it to {@code out[0]} on success. */
    public static int pubDataGetReliabilityKind(long data, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publication_builtin_topic_data_get_reliability_kind(
                data, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0);
        }
        return rc;
    }

    /**
     * Durability kind: 0 = VOLATILE, 1 = TRANSIENT_LOCAL, 2 = TRANSIENT,
     * 3 = PERSISTENT. Writes it to {@code out[0]} on success.
     */
    public static int pubDataGetDurabilityKind(long data, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publication_builtin_topic_data_get_durability_kind(
                data, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0);
        }
        return rc;
    }

    /**
     * Liveliness kind: 0 = AUTOMATIC, 1 = MANUAL_BY_PARTICIPANT,
     * 2 = MANUAL_BY_TOPIC. Writes it to {@code out[0]} on success.
     */
    public static int pubDataGetLivelinessKind(long data, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publication_builtin_topic_data_get_liveliness_kind(
                data, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0);
        }
        return rc;
    }

    /**
     * Deadline period (sec, nanosec); an infinite period reads back as
     * (0x7fffffff, 0x7fffffff). Writes it to {@code secOut[0]}/{@code
     * nanosecOut[0]} on success.
     */
    public static int pubDataGetDeadline(long data, int[] secOut, int[] nanosecOut) {
        ByteBuffer secSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        ByteBuffer nanoSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publication_builtin_topic_data_get_deadline(
                data, directBufferAddress(secSlot), directBufferAddress(nanoSlot));
        NativeKeepAlive.keepAlive(secSlot);
        NativeKeepAlive.keepAlive(nanoSlot);
        if (rc == 0) {
            secOut[0] = secSlot.getInt(0);
            nanosecOut[0] = nanoSlot.getInt(0);
        }
        return rc;
    }

    /**
     * Lifespan duration (sec, nanosec); an infinite duration reads back as
     * (0x7fffffff, 0x7fffffff). Writes it to {@code secOut[0]}/{@code
     * nanosecOut[0]} on success.
     */
    public static int pubDataGetLifespan(long data, int[] secOut, int[] nanosecOut) {
        ByteBuffer secSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        ByteBuffer nanoSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publication_builtin_topic_data_get_lifespan(
                data, directBufferAddress(secSlot), directBufferAddress(nanoSlot));
        NativeKeepAlive.keepAlive(secSlot);
        NativeKeepAlive.keepAlive(nanoSlot);
        if (rc == 0) {
            secOut[0] = secSlot.getInt(0);
            nanosecOut[0] = nanoSlot.getInt(0);
        }
        return rc;
    }

    /**
     * Liveliness lease duration (sec, nanosec); an infinite duration reads back
     * as (0x7fffffff, 0x7fffffff). Writes it to {@code secOut[0]}/{@code
     * nanosecOut[0]} on success.
     */
    public static int pubDataGetLivelinessLeaseDuration(long data, int[] secOut, int[] nanosecOut) {
        ByteBuffer secSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        ByteBuffer nanoSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publication_builtin_topic_data_get_liveliness_lease_duration(
                data, directBufferAddress(secSlot), directBufferAddress(nanoSlot));
        NativeKeepAlive.keepAlive(secSlot);
        NativeKeepAlive.keepAlive(nanoSlot);
        if (rc == 0) {
            secOut[0] = secSlot.getInt(0);
            nanosecOut[0] = nanoSlot.getInt(0);
        }
        return rc;
    }

    // --- Discovery: SubscriptionBuiltinTopicData (materialize-then-destroy) ---

    /**
     * Collects a snapshot of discovered subscriptions (the builtin DCPSSubscription
     * reader), blocking up to {@code timeoutMs} (negative = infinite). Returns rc;
     * on {@code RET_OK} writes the snapshot sequence handle to {@code seqOut[0]}.
     * The caller owns the sequence and must release it with {@link
     * #subDataSeqDelete}.
     */
    public static int takeDiscoveredSubscriptionsSnapshot(long participant, int timeoutMs,
            long[] seqOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_participant_take_discovered_subscriptions_snapshot(
                participant, timeoutMs, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            seqOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Entry count of a subscription snapshot. Returns rc; writes it to {@code out[0]} on success. */
    public static int subDataSeqLength(long seq, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscription_builtin_topic_data_seq_length(seq, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Mints an owned {@code SubscriptionBuiltinTopicData} box for the entry at
     * {@code index} (the core clones its stored data into it). Returns rc; writes
     * the handle to {@code dataOut[0]} on success. The caller owns the box and
     * must release it with {@link #subDataDestroy}.
     */
    public static int subDataSeqGet(long seq, long index, long[] dataOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscription_builtin_topic_data_seq_get(seq, index, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            dataOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a subscription snapshot sequence returned by {@link #takeDiscoveredSubscriptionsSnapshot}. */
    public static void subDataSeqDelete(long seq) {
        Ffi.int2dds_subscription_builtin_topic_data_seq_delete(seq);
    }

    /** Releases one owned {@code SubscriptionBuiltinTopicData} box minted by {@link #subDataSeqGet}. */
    public static void subDataDestroy(long data) {
        Ffi.int2dds_subscription_builtin_topic_data_destroy(data);
    }

    /** The 12-byte instance key. {@code keyOut} must be a 12-byte array. */
    public static int subDataGetKey(long data, byte[] keyOut) {
        return Ffi.int2dds_subscription_builtin_topic_data_get_key(data, keyOut);
    }

    /** The 16-byte endpoint GUID. {@code guidOut} must be a 16-byte array. */
    public static int subDataGetEndpointGuid(long data, byte[] guidOut) {
        return Ffi.int2dds_subscription_builtin_topic_data_get_endpoint_guid(data, guidOut);
    }

    /** The 12-byte owning-participant key. {@code keyOut} must be a 12-byte array. */
    public static int subDataGetParticipantKey(long data, byte[] keyOut) {
        return Ffi.int2dds_subscription_builtin_topic_data_get_participant_key(data, keyOut);
    }

    /** Direct passthrough for {@link #readGrowableString}: the topic name getter. */
    public static int subDataGetTopicName(long data, long buf, long capacity, long sizeOut) {
        return Ffi.int2dds_subscription_builtin_topic_data_get_topic_name(data, buf, capacity, sizeOut);
    }

    /** Direct passthrough for {@link #readGrowableString}: the type name getter. */
    public static int subDataGetTypeName(long data, long buf, long capacity, long sizeOut) {
        return Ffi.int2dds_subscription_builtin_topic_data_get_type_name(data, buf, capacity, sizeOut);
    }

    /** Direct passthrough for {@link #readGrowableBytes}: the user_data getter. */
    public static int subDataGetUserData(long data, long buf, long capacity, long sizeOut) {
        return Ffi.int2dds_subscription_builtin_topic_data_get_user_data(data, buf, capacity, sizeOut);
    }

    /** Reliability kind: 0 = BEST_EFFORT, 1 = RELIABLE. Writes it to {@code out[0]} on success. */
    public static int subDataGetReliabilityKind(long data, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscription_builtin_topic_data_get_reliability_kind(
                data, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0);
        }
        return rc;
    }

    /**
     * Durability kind: 0 = VOLATILE, 1 = TRANSIENT_LOCAL, 2 = TRANSIENT,
     * 3 = PERSISTENT. Writes it to {@code out[0]} on success.
     */
    public static int subDataGetDurabilityKind(long data, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscription_builtin_topic_data_get_durability_kind(
                data, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0);
        }
        return rc;
    }

    /**
     * Liveliness kind: 0 = AUTOMATIC, 1 = MANUAL_BY_PARTICIPANT,
     * 2 = MANUAL_BY_TOPIC. Writes it to {@code out[0]} on success.
     */
    public static int subDataGetLivelinessKind(long data, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscription_builtin_topic_data_get_liveliness_kind(
                data, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0);
        }
        return rc;
    }

    /**
     * Deadline period (sec, nanosec); an infinite period reads back as
     * (0x7fffffff, 0x7fffffff). Writes it to {@code secOut[0]}/{@code
     * nanosecOut[0]} on success.
     */
    public static int subDataGetDeadline(long data, int[] secOut, int[] nanosecOut) {
        ByteBuffer secSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        ByteBuffer nanoSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscription_builtin_topic_data_get_deadline(
                data, directBufferAddress(secSlot), directBufferAddress(nanoSlot));
        NativeKeepAlive.keepAlive(secSlot);
        NativeKeepAlive.keepAlive(nanoSlot);
        if (rc == 0) {
            secOut[0] = secSlot.getInt(0);
            nanosecOut[0] = nanoSlot.getInt(0);
        }
        return rc;
    }

    /**
     * Liveliness lease duration (sec, nanosec); an infinite duration reads back
     * as (0x7fffffff, 0x7fffffff). Writes it to {@code secOut[0]}/{@code
     * nanosecOut[0]} on success.
     */
    public static int subDataGetLivelinessLeaseDuration(long data, int[] secOut, int[] nanosecOut) {
        ByteBuffer secSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        ByteBuffer nanoSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscription_builtin_topic_data_get_liveliness_lease_duration(
                data, directBufferAddress(secSlot), directBufferAddress(nanoSlot));
        NativeKeepAlive.keepAlive(secSlot);
        NativeKeepAlive.keepAlive(nanoSlot);
        if (rc == 0) {
            secOut[0] = secSlot.getInt(0);
            nanosecOut[0] = nanoSlot.getInt(0);
        }
        return rc;
    }

    // --- Discovery: ParticipantBuiltinTopicData (handle-list + per-handle lookup) ---

    /**
     * Writes up to {@code capacity} discovered-participant handles (16 bytes
     * each) into {@code handles}. Returns rc; writes the TRUE total discovered
     * count to {@code countOut[0]} on success, even when it exceeds {@code
     * capacity} (the JNI shim clamps what it actually copies to {@code
     * handles.length / 16}, but still reports the real total so a caller can
     * regrow and retry).
     */
    public static int getDiscoveredParticipants(long participant, byte[] handles, long capacity,
            long[] countOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_participant_get_discovered_participants(
                participant, handles, capacity, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            countOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Mints an owned {@code ParticipantBuiltinTopicData} box for the given
     * 16-byte handle. Returns rc; writes the handle to {@code dataOut[0]} on
     * success. The caller owns the box and must release it with {@link
     * #participantDataDestroy}.
     */
    public static int getDiscoveredParticipantData(long participant, byte[] handle,
            long[] dataOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_participant_get_discovered_participant_data(
                participant, handle, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            dataOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases one owned {@code ParticipantBuiltinTopicData} box minted by {@link #getDiscoveredParticipantData}. */
    public static void participantDataDestroy(long data) {
        Ffi.int2dds_participant_builtin_topic_data_destroy(data);
    }

    /** The 12-byte instance key. {@code keyOut} must be a 12-byte array. */
    public static int participantDataGetKey(long data, byte[] keyOut) {
        return Ffi.int2dds_participant_builtin_topic_data_get_key(data, keyOut);
    }

    /** Direct passthrough for {@link #readGrowableBytes}: the user_data getter. */
    public static int participantDataGetUserData(long data, long buf, long capacity, long sizeOut) {
        return Ffi.int2dds_participant_builtin_topic_data_get_user_data(data, buf, capacity, sizeOut);
    }

    // --- Discovery: matched endpoints (handle-list + per-handle lookup) ---

    /**
     * Writes up to {@code capacity} matched-subscription handles (16 bytes
     * each) into {@code handles}, for the subscriptions currently matched to
     * {@code writer}. Returns rc; writes the TRUE total matched count to
     * {@code countOut[0]} on success, even when it exceeds {@code capacity} --
     * same contract as {@link #getDiscoveredParticipants}.
     */
    public static int getMatchedSubscriptions(long writer, byte[] handles, long capacity,
            long[] countOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datawriter_get_matched_subscriptions(
                writer, handles, capacity, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            countOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Mints an owned {@code SubscriptionBuiltinTopicData} box for the given
     * matched-subscription 16-byte handle. Returns rc; writes the handle to
     * {@code dataOut[0]} on success. The caller owns the box and must release
     * it with {@link #subDataDestroy}.
     */
    public static int getMatchedSubscriptionData(long writer, byte[] handle, long[] dataOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datawriter_get_matched_subscription_data(
                writer, handle, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            dataOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Writes up to {@code capacity} matched-publication handles (16 bytes
     * each) into {@code handles}, for the publications currently matched to
     * {@code reader}. Returns rc; writes the TRUE total matched count to
     * {@code countOut[0]} on success, even when it exceeds {@code capacity} --
     * same contract as {@link #getDiscoveredParticipants}.
     */
    public static int getMatchedPublications(long reader, byte[] handles, long capacity,
            long[] countOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datareader_get_matched_publications(
                reader, handles, capacity, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            countOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Mints an owned {@code PublicationBuiltinTopicData} box for the given
     * matched-publication 16-byte handle. Returns rc; writes the handle to
     * {@code dataOut[0]} on success. The caller owns the box and must release
     * it with {@link #pubDataDestroy}.
     */
    public static int getMatchedPublicationData(long reader, byte[] handle, long[] dataOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datareader_get_matched_publication_data(
                reader, handle, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            dataOut[0] = slot.getLong(0);
        }
        return rc;
    }
}
