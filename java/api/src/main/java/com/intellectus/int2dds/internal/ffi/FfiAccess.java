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

    /**
     * Creates a dynamic value holding {@code value}, writing the handle to
     * {@code out[0]} on success. Sibling of {@link #dynamicValueI32(int, long)}
     * that follows this file's usual {@code long[] out} idiom instead of a raw
     * native address, for callers building a value with {@link
     * com.intellectus.int2dds.xtypes.DynamicValue}.
     */
    public static int dynamicValueI32(int value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_i32(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates a dynamic value holding a UTF-8 string, writing the handle to
     * {@code out[0]} on success.
     */
    public static int dynamicValueString(byte[] value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_string(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates a dynamic value holding a wide-string (UTF-8-encoded) string,
     * writing the handle to {@code out[0]} on success.
     */
    public static int dynamicValueWstring(byte[] value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_wstring(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code bool} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueBool(boolean value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_bool(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates an {@code int8} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueI8(int value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_i8(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code uint8} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueU8(int value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_u8(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates an {@code int16} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueI16(int value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_i16(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code uint16} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueU16(int value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_u16(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code uint32} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueU32(int value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_u32(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code byte} (octet) dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueByte(int value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_byte(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code char8} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueChar8(int value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_char8(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates an {@code int64} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueI64(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_i64(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code uint64} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueU64(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_u64(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code bitmask} dynamic value from its packed bits, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueBitmask(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_bitmask(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code bitset} dynamic value from its packed bitfields, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueBitset(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_bitset(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code float32} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueF32(float value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_f32(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Creates a {@code float64} dynamic value, writing the handle to {@code out[0]} on success. */
    public static int dynamicValueF64(double value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_f64(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates an empty sequence dynamic value, writing the handle to {@code
     * out[0]} on success. Append elements with {@link #dynamicValuePush}.
     */
    public static int dynamicValueSequence(long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_sequence(directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Appends {@code element} to the sequence/array value {@code collection}.
     * On {@code RET_OK} this consumes {@code element}'s handle -- it is moved
     * into {@code collection} and the caller must not use or destroy it again.
     * On any other code (e.g. {@code collection} is not a sequence/array),
     * {@code element} is left untouched and still owned by the caller.
     */
    public static int dynamicValuePush(long collection, long element) {
        return Ffi.int2dds_dynamic_value_push(collection, element);
    }

    /**
     * Snapshots the populated DynamicData {@code data} into a struct dynamic
     * value, writing the handle to {@code out[0]} on success. {@code data} is
     * cloned natively, not consumed -- the caller still owns and must destroy
     * {@code data} independently.
     */
    public static int dynamicValueStruct(long data, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_struct(data, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates an {@code enum} dynamic value from a literal {@code name} and
     * its numeric {@code value}, writing the handle to {@code out[0]} on
     * success.
     */
    public static int dynamicValueEnum(byte[] name, int value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_enum(name, value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates an empty array dynamic value, writing the handle to {@code
     * out[0]} on success. Append elements with {@link #dynamicValuePush}.
     */
    public static int dynamicValueArray(long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_array(directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
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
     * Loads named QoS profiles (and any {@code <types>}) from {@code count}
     * JSON files into the process-wide factory singleton. Returns the C ABI
     * status code; no out-slot, nothing built here to free.
     */
    public static int loadProfiles(byte[][] paths, long count) {
        return Ffi.int2dds_load_profiles(paths, count);
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
     * Creates a participant whose QoS comes from a loaded profile ({@code
     * qosPath} is a {@code "LibraryName::ProfileName"} path). Same {@code
     * (rc, long[] handleOut)} shape as {@link #createParticipant}, and the
     * resulting participant is a normal participant, released the same way
     * ({@link #deleteParticipant}).
     */
    public static int createParticipantWithProfile(
            long factory, int domainId, byte[] qosPath, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_participant_with_profile(
                factory, domainId, qosPath, directBufferAddress(slot));
        // Same fence as createParticipant.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    // --- Configured participant (declarative XML tree) ---

    /**
     * Builds a whole participant tree -- participant, publishers/
     * subscribers, datawriters/datareaders and topics -- from a {@code
     * domain_participant_library} entry already loaded into this factory via
     * {@link #loadProfiles}. {@code path} is a {@code
     * "LibraryName::ParticipantName"} library path, not a file path. Same
     * {@code (rc, long[] handleOut)} shape as {@link #createParticipant}, but
     * the returned handle names a configured tree, released with {@link
     * #configuredParticipantDestroy}, not {@link #deleteParticipant}.
     */
    public static int createParticipantFromConfig(long factory, byte[] path, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_participant_from_config(factory, path, directBufferAddress(slot));
        // Same fence as createParticipant.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Fetches a datawriter already built into {@code configured} by its
     * {@code "publisher::writer"} name. Returns {@code
     * RET_DYNAMIC_FIELD_NOT_FOUND} (200) if no such name exists in the tree.
     * The returned handle is an Arc-clone sharing the underlying entity with
     * the tree's own copy through one shared atomic deleted flag, so
     * releasing either does not free out from under the other.
     */
    public static int configuredParticipantGetDataWriter(
            long configured, byte[] name, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_configured_participant_get_datawriter(
                configured, name, directBufferAddress(slot));
        // Same fence as createParticipant.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Fetches a datareader by {@code "subscriber::reader"} name. Same shape as {@link #configuredParticipantGetDataWriter}. */
    public static int configuredParticipantGetDataReader(
            long configured, byte[] name, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_configured_participant_get_datareader(
                configured, name, directBufferAddress(slot));
        // Same fence as createParticipant.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Releases a whole configured tree -- participant, publishers/
     * subscribers, datawriters/datareaders and topics it built. Every
     * previously-fetched writer/reader wrapper should be closed first (see
     * {@link com.intellectus.int2dds.core.ConfiguredParticipant#close}); the
     * shared deleted flag {@link #configuredParticipantGetDataWriter} notes
     * makes a second release of the same entity a safe no-op either way.
     */
    public static void configuredParticipantDestroy(long configured) {
        Ffi.int2dds_configured_participant_destroy(configured);
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

    /**
     * The participant's current time (sec, nanosec) since the DDS epoch.
     * Writes it to {@code secOut[0]}/{@code nanosecOut[0]} on success.
     */
    public static int participantGetCurrentTime(long participant, int[] secOut, int[] nanosecOut) {
        ByteBuffer secSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        ByteBuffer nanoSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_participant_get_current_time(
                participant, directBufferAddress(secSlot), directBufferAddress(nanoSlot));
        NativeKeepAlive.keepAlive(secSlot);
        NativeKeepAlive.keepAlive(nanoSlot);
        if (rc == 0) {
            secOut[0] = secSlot.getInt(0);
            nanosecOut[0] = nanoSlot.getInt(0);
        }
        return rc;
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

    // --- Entity setQos bridges ---

    /**
     * Applies {@code qos} to an already-created datawriter. Returns the C ABI
     * status code, which the core maps to {@code RET_IMMUTABLE_POLICY} for a
     * change to an immutable policy on an enabled writer. Unlike the {@code
     * createXQos}/{@code getXxxQos} bridges above, this takes two live
     * handles and returns nothing but a status -- no out-slot, no {@code
     * directBufferAddress}.
     */
    public static int datawriterSetQos(long writer, long qos) {
        return Ffi.int2dds_datawriter_set_qos(writer, qos);
    }

    /** Applies {@code qos} to an already-created datareader. Same shape as {@link #datawriterSetQos}. */
    public static int datareaderSetQos(long reader, long qos) {
        return Ffi.int2dds_datareader_set_qos(reader, qos);
    }

    /** Applies {@code qos} to an already-created publisher. Same shape as {@link #datawriterSetQos}. */
    public static int publisherSetQos(long publisher, long qos) {
        return Ffi.int2dds_publisher_set_qos(publisher, qos);
    }

    /** Applies {@code qos} to an already-created subscriber. Same shape as {@link #datawriterSetQos}. */
    public static int subscriberSetQos(long subscriber, long qos) {
        return Ffi.int2dds_subscriber_set_qos(subscriber, qos);
    }

    /** Applies {@code qos} to an already-created participant. Same shape as {@link #datawriterSetQos}. */
    public static int participantSetQos(long participant, long qos) {
        return Ffi.int2dds_participant_set_qos(participant, qos);
    }

    /** Applies {@code qos} to an already-created topic. Same shape as {@link #datawriterSetQos}. */
    public static int topicSetQos(long topic, long qos) {
        return Ffi.int2dds_topic_set_qos(topic, qos);
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
     * Creates a topic whose QoS comes from a loaded profile ({@code qosPath}
     * is a {@code "LibraryName::ProfileName"} path). Same {@code (rc,
     * long[] handleOut)} shape as {@link #createTopic}, and the resulting
     * topic is a normal topic, released the same way ({@link #deleteTopic}).
     */
    public static int createTopicWithProfile(long participant, byte[] topicName,
            byte[] ddsTypeName, int extensibility, byte[] qosPath, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_topic_with_profile(participant, topicName, ddsTypeName,
                extensibility, qosPath, directBufferAddress(slot));
        // Same fence as createTopic.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates a topic with explicit CDR field descriptors, so a {@code
     * ContentFilteredTopic} built against it can evaluate a SQL filter
     * expression against named fields -- the {@code
     * int2dds_create_topic_with_field_descriptors} path. A plain {@link
     * #createTopic} topic registers no field metadata, so filter evaluation
     * against it errors on every sample and the core treats that as "passes"
     * (see {@code DataReader::passes_content_filter}), silently delivering
     * everything unfiltered.
     *
     * <p>{@code fieldTypeCodes} are the native {@code field_descriptor_type}
     * codes (0=String, 1=Int32, 2=UInt32, 3=Int16, 4=UInt16, 5=Int64,
     * 6=UInt64, 7=Int8, 8=UInt8, 9=Bool) -- a distinct encoding from the
     * XTypes {@code INT2DDS_FIELD_*} constants {@link #typeInfoAddField}
     * uses. The core's flat parser walks {@code fieldNames} in order and
     * stops at the first name match, skipping each field ahead of it by its
     * declared type -- so a field after the one being filtered on may be
     * omitted entirely, but any field before it must still be declared (with
     * a type the parser can skip), or every field after the gap misaligns.
     * {@code fieldTypeCodes} and {@code fieldIsKey} cross as raw native
     * arrays (a direct-buffer address each), not JNI arrays -- the same
     * shape the generated declaration expects.
     */
    public static int createTopicWithFieldDescriptors(long participant, byte[] topicName,
            byte[] typeName, int extensibility, long qos, byte[][] fieldNames,
            int[] fieldTypeCodes, boolean[] fieldIsKey, long fieldCount, long[] handleOut) {
        ByteBuffer typesBuf =
                ByteBuffer.allocateDirect(fieldTypeCodes.length * 4).order(ByteOrder.nativeOrder());
        for (int c : fieldTypeCodes) {
            typesBuf.putInt(c);
        }
        ByteBuffer isKeyBuf =
                ByteBuffer.allocateDirect(fieldIsKey.length).order(ByteOrder.nativeOrder());
        for (boolean k : fieldIsKey) {
            isKeyBuf.put((byte) (k ? 1 : 0));
        }
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_topic_with_field_descriptors(participant, topicName, typeName,
                extensibility, qos, fieldNames, directBufferAddress(typesBuf),
                directBufferAddress(isKeyBuf), fieldCount, directBufferAddress(slot));
        // typesBuf/isKeyBuf/slot were only handed off by native address above;
        // keep them all reachable across the call -- see NativeKeepAlive's own
        // doc for the full argument.
        NativeKeepAlive.keepAlive(typesBuf);
        NativeKeepAlive.keepAlive(isKeyBuf);
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates a ContentFilteredTopic on {@code relatedTopic}: samples not
     * matching {@code filterExpr} (a SQL-92-like WHERE clause, {@code %0}
     * {@code %1}... referencing {@code params} positionally) are not
     * delivered to a reader created on the returned handle. {@code
     * topicName}, {@code filterExpr} and each element of {@code params}
     * cross as UTF-8 {@code byte[]}, never {@code String}.
     */
    public static int createContentFilteredTopic(long participant, byte[] topicName,
            long relatedTopic, byte[] filterExpr, byte[][] params, long paramCount,
            long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_contentfilteredtopic(
                participant, topicName, relatedTopic, filterExpr, params, paramCount,
                directBufferAddress(slot));
        // Same fence as createTopic.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a ContentFilteredTopic. Returns the C ABI status code. */
    public static int deleteContentFilteredTopic(long cft) {
        return Ffi.int2dds_delete_contentfilteredtopic(cft);
    }

    /** Replaces a ContentFilteredTopic's filter expression and parameters. */
    public static int contentFilteredTopicSetFilterExpression(
            long cft, byte[] filterExpr, byte[][] params, long count) {
        return Ffi.int2dds_contentfilteredtopic_set_filter_expression(cft, filterExpr, params, count);
    }

    /** Replaces a ContentFilteredTopic's expression parameters, keeping its filter expression. */
    public static int contentFilteredTopicSetExpressionParameters(
            long cft, byte[][] params, long count) {
        return Ffi.int2dds_contentfilteredtopic_set_expression_parameters(cft, params, count);
    }

    /** Enables or disables a ContentFilteredTopic's filtering (disabled = every sample passes). */
    public static int contentFilteredTopicSetEnabled(long cft, boolean enabled) {
        return Ffi.int2dds_contentfilteredtopic_set_enabled(cft, enabled);
    }

    /**
     * Reads the topic's INCONSISTENT_TOPIC status into the 8-byte native
     * struct at {@code statusOutAddr}. Thin passthrough -- the caller owns
     * the buffer and decodes it.
     */
    public static int topicGetInconsistentTopicStatus(long topic, long statusOutAddr) {
        return Ffi.int2dds_topic_get_inconsistent_topic_status(topic, statusOutAddr);
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
     * Creates a publisher whose QoS comes from a loaded profile ({@code
     * qosPath} is a {@code "LibraryName::ProfileName"} path). Same {@code
     * (rc, long[] handleOut)} shape as {@link #createPublisher}, and the
     * resulting publisher is a normal publisher, released the same way
     * ({@link #deletePublisher}).
     */
    public static int createPublisherWithProfile(long participant, byte[] qosPath, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_publisher_with_profile(
                participant, qosPath, directBufferAddress(slot));
        // Same fence as createPublisher.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** The 16-byte instance handle identifying this publisher. {@code handleOut} must be a 16-byte array. */
    public static int publisherGetInstanceHandle(long publisher, byte[] handleOut) {
        return Ffi.int2dds_publisher_get_instance_handle(publisher, handleOut);
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

    /**
     * The writer's effective data-representation id ({@code 0} XCDR1, {@code 2}
     * XCDR2). A pure getter with no failure path: a null writer yields the
     * default rather than an error code.
     */
    public static int datawriterDataRepresentation(long writer) {
        return Ffi.int2dds_datawriter_data_representation(writer);
    }

    /**
     * Creates a datawriter whose QoS comes from a loaded profile ({@code
     * qosPath} is a {@code "LibraryName::ProfileName"} path). Same {@code
     * (rc, long[] handleOut)} shape as {@link #createDataWriter}, and the
     * resulting writer is a normal typed datawriter, released the same way
     * ({@link #deleteDataWriter}). {@code listener} is {@code 0L} and {@code
     * mask} is {@code 0} — no creation-time listener.
     */
    public static int createDataWriterWithProfile(long publisher, long topic, byte[] qosPath,
            long listener, int mask, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_datawriter_with_profile(
                publisher, topic, qosPath, listener, mask, directBufferAddress(slot));
        // Same fence as createDataWriter.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** The 16-byte entity GUID. {@code guidOut} must be a 16-byte array. */
    public static int datawriterGetGuid(long writer, byte[] guidOut) {
        return Ffi.int2dds_datawriter_get_guid(writer, guidOut);
    }

    /**
     * Reads the writer's PUBLICATION_MATCHED status into the 32-byte native
     * struct at {@code statusOutAddr} (a direct-buffer address, the caller's
     * responsibility to allocate and keep alive). Returns the C ABI status
     * code. Thin passthrough -- the caller owns the buffer and decodes it.
     */
    public static int datawriterGetPublicationMatchedStatus(long writer, long statusOutAddr) {
        return Ffi.int2dds_datawriter_get_publication_matched_status(writer, statusOutAddr);
    }

    /**
     * Reads the writer's LIVELINESS_LOST status into the 8-byte native
     * struct at {@code statusOutAddr}. Thin passthrough -- the caller owns
     * the buffer and decodes it.
     */
    public static int datawriterGetLivelinessLostStatus(long writer, long statusOutAddr) {
        return Ffi.int2dds_datawriter_get_liveliness_lost_status(writer, statusOutAddr);
    }

    /**
     * Reads the writer's OFFERED_DEADLINE_MISSED status into the 24-byte
     * native struct at {@code statusOutAddr}. Thin passthrough -- the caller
     * owns the buffer and decodes it.
     */
    public static int datawriterGetOfferedDeadlineMissedStatus(long writer, long statusOutAddr) {
        return Ffi.int2dds_datawriter_get_offered_deadline_missed_status(writer, statusOutAddr);
    }

    /**
     * Reads the writer's OFFERED_INCOMPATIBLE_QOS status into the 16-byte
     * native struct at {@code statusOutAddr}. Thin passthrough -- the caller
     * owns the buffer and decodes it.
     */
    public static int datawriterGetOfferedIncompatibleQosStatus(long writer, long statusOutAddr) {
        return Ffi.int2dds_datawriter_get_offered_incompatible_qos_status(writer, statusOutAddr);
    }

    /**
     * Reads the writer's OFFERED_INCOMPATIBLE_TYPE status into the 8-byte
     * native struct at {@code statusOutAddr}. Thin passthrough -- the caller
     * owns the buffer and decodes it.
     */
    public static int datawriterGetOfferedIncompatibleTypeStatus(long writer, long statusOutAddr) {
        return Ffi.int2dds_datawriter_get_offered_incompatible_type_status(writer, statusOutAddr);
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

    /** Same as {@link #datawriterWriteSerialized} but with an explicit source timestamp. */
    public static int datawriterWriteSerializedWTimestamp(long writer, long data, long dataLen,
            int tsSec, int tsNanosec) {
        return Ffi.int2dds_datawriter_write_serialized_w_timestamp(
                writer, data, dataLen, tsSec, tsNanosec);
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

    /**
     * Reads a typed datareader's current QoS into a freshly allocated native
     * handle. Same shape as {@link #getWriterQos}.
     */
    public static int getReaderQos(long reader, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datareader_get_qos(reader, directBufferAddress(slot));
        // See createDataWriter's identical fence just above.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    // --- Reliability / liveliness ---

    /** Blocks up to {@code timeoutMs} for all matched reliable readers to ack. Status code, incl. RET_TIMEOUT. */
    public static int datawriterWaitForAcknowledgments(long writer, long timeoutMs) {
        return Ffi.int2dds_datawriter_wait_for_acknowledgments(writer, timeoutMs);
    }

    /** Same as {@link #datawriterWaitForAcknowledgments}, for all of a publisher's writers. */
    public static int publisherWaitForAcknowledgments(long publisher, long timeoutMs) {
        return Ffi.int2dds_publisher_wait_for_acknowledgments(publisher, timeoutMs);
    }

    /** Manually asserts a writer's liveliness (MANUAL_BY_* liveliness QoS). */
    public static int datawriterAssertLiveliness(long writer) {
        return Ffi.int2dds_datawriter_assert_liveliness(writer);
    }

    /** Manually asserts a participant's liveliness, for all its MANUAL_BY_PARTICIPANT writers. */
    public static int participantAssertLiveliness(long participant) {
        return Ffi.int2dds_participant_assert_liveliness(participant);
    }

    /** Blocks up to {@code timeoutMs} for historical (durable) data to arrive. Status code, incl. RET_TIMEOUT. */
    public static int datareaderWaitForHistoricalData(long reader, long timeoutMs) {
        return Ffi.int2dds_datareader_wait_for_historical_data(reader, timeoutMs);
    }

    /** The 16-byte entity GUID. {@code guidOut} must be a 16-byte array. */
    public static int datareaderGetGuid(long reader, byte[] guidOut) {
        return Ffi.int2dds_datareader_get_guid(reader, guidOut);
    }

    /**
     * Reads the reader's SUBSCRIPTION_MATCHED status into the 32-byte native
     * struct at {@code statusOutAddr} (a direct-buffer address, the caller's
     * responsibility to allocate and keep alive). Returns the C ABI status
     * code. Thin passthrough -- the caller owns the buffer and decodes it.
     */
    public static int datareaderGetSubscriptionMatchedStatus(long reader, long statusOutAddr) {
        return Ffi.int2dds_datareader_get_subscription_matched_status(reader, statusOutAddr);
    }

    /**
     * Reads the reader's LIVELINESS_CHANGED status into the 32-byte native
     * struct at {@code statusOutAddr}. Thin passthrough -- the caller owns
     * the buffer and decodes it.
     */
    public static int datareaderGetLivelinessChangedStatus(long reader, long statusOutAddr) {
        return Ffi.int2dds_datareader_get_liveliness_changed_status(reader, statusOutAddr);
    }

    /**
     * Reads the reader's REQUESTED_DEADLINE_MISSED status into the 24-byte
     * native struct at {@code statusOutAddr}. Thin passthrough -- the caller
     * owns the buffer and decodes it.
     */
    public static int datareaderGetRequestedDeadlineMissedStatus(long reader, long statusOutAddr) {
        return Ffi.int2dds_datareader_get_requested_deadline_missed_status(reader, statusOutAddr);
    }

    /**
     * Reads the reader's REQUESTED_INCOMPATIBLE_QOS status into the 16-byte
     * native struct at {@code statusOutAddr}. Thin passthrough -- the caller
     * owns the buffer and decodes it.
     */
    public static int datareaderGetRequestedIncompatibleQosStatus(long reader, long statusOutAddr) {
        return Ffi.int2dds_datareader_get_requested_incompatible_qos_status(reader, statusOutAddr);
    }

    /**
     * Reads the reader's REQUESTED_INCOMPATIBLE_TYPE status into the 8-byte
     * native struct at {@code statusOutAddr}. Thin passthrough -- the caller
     * owns the buffer and decodes it.
     */
    public static int datareaderGetRequestedIncompatibleTypeStatus(long reader, long statusOutAddr) {
        return Ffi.int2dds_datareader_get_requested_incompatible_type_status(reader, statusOutAddr);
    }

    /**
     * Reads the reader's SAMPLE_LOST status into the 8-byte native struct at
     * {@code statusOutAddr}. Thin passthrough -- the caller owns the buffer
     * and decodes it.
     */
    public static int datareaderGetSampleLostStatus(long reader, long statusOutAddr) {
        return Ffi.int2dds_datareader_get_sample_lost_status(reader, statusOutAddr);
    }

    /**
     * Reads the reader's SAMPLE_REJECTED status into the 28-byte native
     * struct at {@code statusOutAddr}. Thin passthrough -- the caller owns
     * the buffer and decodes it.
     */
    public static int datareaderGetSampleRejectedStatus(long reader, long statusOutAddr) {
        return Ffi.int2dds_datareader_get_sample_rejected_status(reader, statusOutAddr);
    }

    /** Whether the reader has any samples available to take/read. Writes it to {@code out[0]} on success. */
    public static int datareaderHasData(long reader, boolean[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datareader_has_data(reader, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) != 0; // bool out is 1 byte, not 4
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

    /** Appends a bounded string field ({@code bound == 0} means unbounded). Returns the C ABI status code. */
    public static int typeInfoAddStringField(long typeInfo, byte[] fieldName, int bound, int flags) {
        return Ffi.int2dds_type_info_add_string_field(typeInfo, fieldName, bound, flags);
    }

    /** Appends a bounded wide-string field ({@code bound == 0} means unbounded). Returns the C ABI status code. */
    public static int typeInfoAddWstringField(long typeInfo, byte[] fieldName, int bound, int flags) {
        return Ffi.int2dds_type_info_add_wstring_field(typeInfo, fieldName, bound, flags);
    }

    /** Appends a fixed-size array field of a primitive {@link com.intellectus.int2dds.xtypes.FieldType}. Returns the C ABI status code. */
    public static int typeInfoAddArrayField(
            long typeInfo, byte[] fieldName, int elementType, int arraySize, int flags) {
        return Ffi.int2dds_type_info_add_array_field(typeInfo, fieldName, elementType, arraySize, flags);
    }

    /**
     * Appends a fixed-size array field whose element is a nested struct, referencing
     * {@code elementTypeInfo}'s own builder. {@code elementTypeInfo} is borrowed, not
     * consumed -- the caller still owns it and must destroy it separately. Returns the
     * C ABI status code.
     */
    public static int typeInfoAddArrayOfNestedField(
            long typeInfo, byte[] fieldName, long elementTypeInfo, int arraySize, int flags) {
        return Ffi.int2dds_type_info_add_array_of_nested_field(
                typeInfo, fieldName, elementTypeInfo, arraySize, flags);
    }

    /**
     * Appends a sequence field whose element is a nested struct ({@code bound == 0}
     * means unbounded), referencing {@code elementTypeInfo}'s own builder. {@code
     * elementTypeInfo} is borrowed, not consumed -- the caller still owns it and must
     * destroy it separately. Returns the C ABI status code.
     */
    public static int typeInfoAddSequenceOfNestedField(
            long typeInfo, byte[] fieldName, long elementTypeInfo, int bound, int flags) {
        return Ffi.int2dds_type_info_add_sequence_of_nested_field(
                typeInfo, fieldName, elementTypeInfo, bound, flags);
    }

    /**
     * Creates an enum type-info builder ({@code bitBound} is the discriminant bit width;
     * IDL enums use 32). Returns the C ABI status code; writes the new builder handle to
     * {@code out[0]} only when it is {@code 0}.
     */
    public static int typeInfoCreateEnum(byte[] typeName, int bitBound, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_type_info_create_enum(typeName, bitBound, directBufferAddress(slot));
        // Same fence as typeInfoCreate -- see NativeKeepAlive's doc for the full argument.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates a bitmask type-info builder ({@code bitBound} is the flag storage bit
     * width). Returns the C ABI status code; writes the new builder handle to {@code
     * out[0]} only when it is {@code 0}.
     */
    public static int typeInfoCreateBitmask(byte[] typeName, int bitBound, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_type_info_create_bitmask(typeName, bitBound, directBufferAddress(slot));
        // Same fence as typeInfoCreate.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Appends a literal to an enum builder. {@code isDefault} marks the {@code @default}
     * literal (0/1). Returns the C ABI status code.
     */
    public static int typeInfoAddEnumLiteral(
            long typeInfo, byte[] literalName, int value, int isDefault) {
        return Ffi.int2dds_type_info_add_enum_literal(typeInfo, literalName, value, isDefault);
    }

    /** Appends a flag to a bitmask builder. {@code position} is the bit index. Returns the C ABI status code. */
    public static int typeInfoAddBitmaskFlag(long typeInfo, byte[] flagName, int position) {
        return Ffi.int2dds_type_info_add_bitmask_flag(typeInfo, flagName, position);
    }

    /**
     * Appends a field referencing another type by name-hash (a {@code MinimalTypeId}
     * computed from {@code typeHashName}), rather than by a borrowed builder handle.
     * Returns the C ABI status code.
     */
    public static int typeInfoAddNamedTypeField(
            long typeInfo, byte[] fieldName, byte[] typeHashName, int flags) {
        return Ffi.int2dds_type_info_add_named_type_field(typeInfo, fieldName, typeHashName, flags);
    }

    /**
     * Appends a sequence field whose element is a type referenced by name-hash rather
     * than a borrowed builder handle ({@code bound == 0} means unbounded). Same
     * discovery-resolved caveat as {@link #typeInfoAddNamedTypeField}. Returns the C ABI
     * status code.
     */
    public static int typeInfoAddSequenceOfNamedField(
            long typeInfo, byte[] fieldName, byte[] elementHashName, int bound, int flags) {
        return Ffi.int2dds_type_info_add_sequence_of_named_field(
                typeInfo, fieldName, elementHashName, bound, flags);
    }

    /**
     * Appends a fixed-size array field whose element is a type referenced by name-hash
     * rather than a borrowed builder handle. Same discovery-resolved caveat as {@link
     * #typeInfoAddNamedTypeField}. Returns the C ABI status code.
     */
    public static int typeInfoAddArrayOfNamedField(
            long typeInfo, byte[] fieldName, byte[] elementHashName, int arraySize, int flags) {
        return Ffi.int2dds_type_info_add_array_of_named_field(
                typeInfo, fieldName, elementHashName, arraySize, flags);
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

    /**
     * Reads a struct TypeObject's member count into {@code out[0]} on success.
     * {@code RET_DYNAMIC_UNSUPPORTED_TYPE} for a non-struct TypeObject.
     */
    public static int typeObjectMemberCount(long t, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_type_object_member_count(t, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0); // native writes a u32 (4 bytes)
        }
        return rc;
    }

    /**
     * Grow-and-retry driver for the member name at {@code index}, mirroring
     * {@link #dynamicDataGetString} exactly: {@code
     * int2dds_type_object_member_name} shares the same {@code copy_str_to_c}
     * contract (out_len excludes the NUL, both on {@code RET_BUFFER_TOO_SMALL}
     * and on success), so the same regrow-to-{@code outLen + 1} logic applies.
     */
    public static int typeObjectMemberName(long t, int index, byte[][] bytesOut) {
        int cap = 64;
        while (true) {
            byte[] buf = new byte[cap];
            ByteBuffer sizeSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = Ffi.int2dds_type_object_member_name(
                    t, index, buf, cap, directBufferAddress(sizeSlot));
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

    /**
     * Finds a struct TypeObject member's index by name, writing it to {@code
     * indexOut[0]} on success. {@code RET_DYNAMIC_FIELD_NOT_FOUND} if no
     * member has that name; {@code RET_DYNAMIC_UNSUPPORTED_TYPE} for a
     * non-struct TypeObject.
     */
    public static int typeObjectFindMember(long t, byte[] name, int[] indexOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_type_object_find_member(t, name, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            indexOut[0] = slot.getInt(0); // native writes a u32 (4 bytes)
        }
        return rc;
    }

    /**
     * Reads a struct TypeObject member's info at {@code index} into the
     * 12-byte {@code Int2DdsMemberInfo} struct at {@code structOutAddr} (a
     * direct-buffer address, the caller's responsibility to allocate and keep
     * alive). Thin passthrough -- the caller owns the buffer and decodes it,
     * the same shape as {@link #datawriterGetPublicationMatchedStatus}.
     */
    public static int typeObjectMemberInfo(long t, int index, long structOutAddr) {
        return Ffi.int2dds_type_object_member_info(t, index, structOutAddr);
    }

    /**
     * Reads a struct TypeObject's extensibility into {@code out[0]} on
     * success: 0 Final, 1 Appendable, 2 Mutable. {@code
     * RET_DYNAMIC_UNSUPPORTED_TYPE} for a non-struct TypeObject.
     */
    public static int typeObjectExtensibility(long t, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_type_object_extensibility(t, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0); // native writes an i32 (4 bytes)
        }
        return rc;
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

    // --- DynamicValue (xtypes read/introspection path) ---

    /**
     * Clones the value at {@code path} into a new, independently-owned
     * DynamicValue handle, writing it to {@code out[0]} on success. The
     * caller owns the returned handle and must destroy it (via {@link
     * #dynamicValueDestroy}) -- {@code d} itself is untouched.
     */
    public static int dynamicDataGetValue(long d, byte[] path, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_data_get_value(d, path, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Reads a dynamic value as {@code bool}, writing it to {@code out[0]} on success. */
    public static int dynamicValueAsBool(long value, boolean[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_bool(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) != 0; // bool out is 1 byte, not 4
        }
        return rc;
    }

    /** Reads a dynamic value as {@code int8}, writing it to {@code out[0]} on success. */
    public static int dynamicValueAsI8(long value, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_i8(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0); // native writes an i8 (1 byte)
        }
        return rc;
    }

    /** Reads a dynamic value as {@code uint8}, writing its unsigned 0-255 value to {@code out[0]} on success. */
    public static int dynamicValueAsU8(long value, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_u8(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) & 0xFF; // native writes a u8 (1 byte); widen unsigned
        }
        return rc;
    }

    /** Reads a dynamic value as {@code int16}, writing it to {@code out[0]} on success. */
    public static int dynamicValueAsI16(long value, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_i16(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getShort(0); // native writes an i16 (2 bytes)
        }
        return rc;
    }

    /** Reads a dynamic value as {@code uint16}, writing its unsigned 0-65535 value to {@code out[0]} on success. */
    public static int dynamicValueAsU16(long value, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_u16(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getShort(0) & 0xFFFF; // native writes a u16 (2 bytes); widen unsigned
        }
        return rc;
    }

    /** Reads a dynamic value as {@code int32}, writing it to {@code out[0]} on success. */
    public static int dynamicValueAsI32(long value, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_i32(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0); // native writes an i32 (4 bytes)
        }
        return rc;
    }

    /**
     * Reads a dynamic value as {@code uint32}, writing its raw 32 bits to
     * {@code out[0]} on success -- callers wanting the unsigned magnitude
     * widen with {@code & 0xFFFFFFFFL} themselves, mirroring {@link
     * #dynamicDataGetU32}.
     */
    public static int dynamicValueAsU32(long value, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_u32(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0); // native writes a u32 (4 bytes); raw bits, not widened
        }
        return rc;
    }

    /** Reads a dynamic value as {@code int64}, writing it to {@code out[0]} on success. */
    public static int dynamicValueAsI64(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_i64(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0); // native writes a full 8-byte i64
        }
        return rc;
    }

    /**
     * Reads a dynamic value as {@code uint64}, writing its raw 64 bits to
     * {@code out[0]} on success -- same "raw bits, caller widens" contract as
     * {@link #dynamicValueAsU32}.
     */
    public static int dynamicValueAsU64(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_u64(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0); // native writes a u64 (8 bytes); raw bits, not widened
        }
        return rc;
    }

    /** Reads a bitmask dynamic value's packed bits into {@code out[0]}. */
    public static int dynamicValueAsBitmask(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_bitmask(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0); // native writes a u64 (8 bytes)
        }
        return rc;
    }

    /** Reads a bitset dynamic value's packed bitfields into {@code out[0]}. */
    public static int dynamicValueAsBitset(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_bitset(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0); // native writes a u64 (8 bytes)
        }
        return rc;
    }

    /** Reads a dynamic value as {@code float32}, writing it to {@code out[0]} on success. */
    public static int dynamicValueAsF32(long value, float[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_f32(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getFloat(0); // native writes a full 4-byte f32
        }
        return rc;
    }

    /** Reads a dynamic value as {@code float64}, writing it to {@code out[0]} on success. */
    public static int dynamicValueAsF64(long value, double[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_f64(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getDouble(0); // native writes a full 8-byte f64
        }
        return rc;
    }

    /**
     * Reads a dynamic value as {@code char8}, writing its unsigned 0-255 byte
     * value to {@code out[0]} on success. Native out is {@code *mut u8}, one
     * byte, the same as {@link #dynamicDataGetChar8} but widened here instead
     * of left raw, to match this file's {@code i8/u8/i16/u16/i32/u32/char8 ->
     * int} carrier convention for {@code DynamicValue} scalar extractors.
     */
    public static int dynamicValueAsChar8(long value, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_char8(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) & 0xFF;
        }
        return rc;
    }

    /**
     * Grow-and-retry driver for a dynamic value's {@code string}/{@code
     * wstring} contents, mirroring {@link #dynamicDataGetString} exactly:
     * {@code int2dds_dynamic_value_as_string} (ffi/src/dynamic_value.rs)
     * shares {@code copy_str_to_c} with {@code int2dds_dynamic_data_get_string},
     * so the required size excludes the trailing NUL both on {@code
     * RET_BUFFER_TOO_SMALL} and on success, and this regrows to {@code
     * outLen + 1} accordingly. Never throws -- policy-free like every other
     * bridge here.
     */
    public static int dynamicValueAsString(long value, byte[][] bytesOut) {
        int cap = 64;
        while (true) {
            byte[] buf = new byte[cap];
            ByteBuffer sizeSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = Ffi.int2dds_dynamic_value_as_string(value, buf, cap, directBufferAddress(sizeSlot));
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

    /**
     * Grow-and-retry driver for an enum dynamic value's literal name, plus a
     * single fixed-size out slot for its numeric value read in the same
     * native call -- mirrors {@link #dynamicValueAsString}'s buffer contract
     * for the name (out_len excludes the NUL, regrow to {@code outLen + 1});
     * {@code int2dds_dynamic_value_as_enum} (ffi/src/dynamic_value.rs) writes
     * the numeric value to {@code out_value} before the string copy, so it is
     * available on every call, but only read here on {@code RET_OK} since a
     * too-small buffer means a retry is coming anyway.
     */
    public static int dynamicValueAsEnum(long value, byte[][] nameOut, int[] valueOut) {
        int cap = 64;
        while (true) {
            byte[] buf = new byte[cap];
            ByteBuffer sizeSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            ByteBuffer valueSlot = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
            int rc = Ffi.int2dds_dynamic_value_as_enum(
                    value, buf, cap, directBufferAddress(sizeSlot), directBufferAddress(valueSlot));
            NativeKeepAlive.keepAlive(sizeSlot);
            NativeKeepAlive.keepAlive(valueSlot);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                cap = (int) sizeSlot.getLong(0) + 1; // out_len excludes the NUL here
                continue;
            }
            if (rc == 0) {
                int n = (int) sizeSlot.getLong(0); // excludes the NUL -- no -1 needed
                byte[] out = new byte[n];
                System.arraycopy(buf, 0, out, 0, n);
                nameOut[0] = out;
                valueOut[0] = valueSlot.getInt(0);
            }
            return rc;
        }
    }

    /**
     * Clones a struct dynamic value's fields into a new, independently-owned
     * DynamicData handle, writing it to {@code out[0]} on success. {@code
     * int2dds_dynamic_value_as_struct} (ffi/src/dynamic_value.rs) clones the
     * inner DynamicData rather than transferring ownership of an existing
     * one, so the source value is untouched and the caller owns the returned
     * handle and must destroy it (via {@link #dynamicDataDestroy}).
     */
    public static int dynamicValueAsStruct(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_as_struct(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Reads a dynamic value's kind (one of {@code DynamicValueKind}'s constants) into {@code out[0]}. */
    public static int dynamicValueKind(long value, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_kind(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0); // native writes an i32 (4 bytes)
        }
        return rc;
    }

    /**
     * Reads the element count of a sequence/array/map dynamic value into
     * {@code out[0]}.
     */
    public static int dynamicValueLen(long value, int[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_len(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = (int) slot.getLong(0); // native writes a usize (8 bytes on this platform)
        }
        return rc;
    }

    /**
     * Clones the element at {@code index} of a sequence/array dynamic value
     * into a new, independently-owned DynamicValue handle, writing it to
     * {@code out[0]} on success. The caller owns the returned handle and must
     * destroy it (via {@link #dynamicValueDestroy}) -- the source value is
     * untouched.
     */
    public static int dynamicValueElement(long value, long index, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_element(value, index, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates an empty map dynamic value, writing the handle to {@code
     * out[0]} on success. Fill it with {@link #dynamicValueMapInsert}.
     */
    public static int dynamicValueMap(long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_map(directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Inserts a key/value pair into the map value {@code map}. On {@code
     * RET_OK} this consumes BOTH {@code key} and {@code value}'s handles --
     * they are moved into {@code map} and the caller must not use or destroy
     * either again. On any other code (e.g. {@code map} is not a map), both
     * are left untouched and still owned by the caller.
     */
    public static int dynamicValueMapInsert(long map, long key, long value) {
        return Ffi.int2dds_dynamic_value_map_insert(map, key, value);
    }

    /**
     * Clones the key at {@code index} of a map dynamic value into a new,
     * independently-owned DynamicValue handle, writing it to {@code out[0]}
     * on success. The caller owns the returned handle and must destroy it --
     * the source value is untouched.
     */
    public static int dynamicValueMapKey(long value, long index, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_map_key(value, index, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Clones the value at {@code index} of a map dynamic value into a new,
     * independently-owned DynamicValue handle, writing it to {@code out[0]}
     * on success. The caller owns the returned handle and must destroy it --
     * the source value is untouched.
     */
    public static int dynamicValueMapValue(long value, long index, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_map_value(value, index, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Constructs a union dynamic value from {@code discriminator} and {@code
     * value}, writing the handle to {@code out[0]} on success. Consumes BOTH
     * {@code discriminator} and {@code value}'s handles unconditionally --
     * the native call frees them via {@code Box::from_raw} right after its
     * null checks and before any fallible step, so unlike {@link
     * #dynamicValuePush} and {@link #dynamicValueMapInsert} there is no
     * failure path that leaves either argument still owned by the caller.
     */
    public static int dynamicValueUnion(long discriminator, long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_union(discriminator, value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Clones a union value's discriminator into a new, independently-owned
     * DynamicValue handle, writing it to {@code out[0]} on success. The
     * caller owns the returned handle and must destroy it -- the source
     * value is untouched.
     */
    public static int dynamicValueUnionDiscriminator(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_union_discriminator(value, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Clones a union value's selected branch value into a new,
     * independently-owned DynamicValue handle, writing it to {@code out[0]}
     * on success. The caller owns the returned handle and must destroy it --
     * the source value is untouched.
     */
    public static int dynamicValueUnionValue(long value, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_value_union_value(value, directBufferAddress(slot));
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

    /**
     * Creates a new registry and loads {@code path} into it in one step,
     * writing the new handle to {@code out[0]} on success -- the same
     * {@code (rc, long[] out)} shape as {@link #xmlTypeRegistryCreate}.
     */
    public static int xmlTypeRegistryFromFile(byte[] path, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_xml_type_registry_from_file(path, directBufferAddress(slot));
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

    /** Loads additional types from {@code path} into an existing registry. */
    public static int xmlTypeRegistryLoadFile(long registry, byte[] path) {
        return Ffi.int2dds_xml_type_registry_load_file(registry, path);
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

    /**
     * Looks up a loaded type by name, writing a TypeObject handle to {@code
     * out[0]} on success. Returns {@code RET_DYNAMIC_FIELD_NOT_FOUND} (200) if
     * no type by that name is loaded -- the same handle type {@link
     * #typeInfoToTypeObject} produces, released the same way.
     */
    public static int xmlTypeRegistryGetTypeObject(long registry, byte[] name, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_xml_type_registry_get_type_object(registry, name, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Grow-and-retry driver for the fully-qualified name of the type at
     * {@code index}, mirroring {@link #dynamicDataGetString} exactly: {@code
     * int2dds_xml_type_registry_type_name} shares {@code copy_str_to_c}'s
     * contract with {@code int2dds_dynamic_data_get_string} (size reported
     * WITHOUT the trailing NUL, both on {@code RET_BUFFER_TOO_SMALL} and on
     * success), so the same regrow-to-{@code outLen + 1} logic applies. An
     * out-of-range {@code index} returns {@code RET_INVALID_ARGUMENT} on the
     * first call, never {@code RET_BUFFER_TOO_SMALL}, so the loop exits
     * immediately in that case. Never throws -- policy-free like every other
     * bridge here.
     */
    public static int xmlTypeRegistryTypeName(long registry, long index, byte[][] bytesOut) {
        int cap = 64;
        while (true) {
            byte[] buf = new byte[cap];
            ByteBuffer sizeSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = Ffi.int2dds_xml_type_registry_type_name(
                    registry, index, buf, cap, directBufferAddress(sizeSlot));
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

    /**
     * Sets field {@code field} to the dynamic value {@code value}. On {@code
     * RET_OK} this consumes {@code value}'s handle -- it is moved into {@code
     * data} and the caller must not use or destroy it again.
     */
    public static int dynamicDataSetValue(long data, byte[] field, long value) {
        return Ffi.int2dds_dynamic_data_set_value(data, field, value);
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

    /**
     * Creates a subscriber whose QoS comes from a loaded profile ({@code
     * qosPath} is a {@code "LibraryName::ProfileName"} path). Same {@code
     * (rc, long[] handleOut)} shape as {@link #createSubscriber}, and the
     * resulting subscriber is a normal subscriber, released the same way
     * ({@link #deleteSubscriber}).
     */
    public static int createSubscriberWithProfile(long participant, byte[] qosPath, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_subscriber_with_profile(
                participant, qosPath, directBufferAddress(slot));
        // Same fence as createSubscriber.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** The 16-byte instance handle identifying this subscriber. {@code handleOut} must be a 16-byte array. */
    public static int subscriberGetInstanceHandle(long subscriber, byte[] handleOut) {
        return Ffi.int2dds_subscriber_get_instance_handle(subscriber, handleOut);
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

    /**
     * Creates a datareader whose QoS comes from a loaded profile ({@code
     * qosPath} is a {@code "LibraryName::ProfileName"} path). Same {@code
     * (rc, long[] handleOut)} shape as {@link #createDataReader}, and the
     * resulting reader is a normal typed datareader, released the same way
     * ({@link #deleteDataReader}). {@code listener} is {@code 0L} and {@code
     * mask} is {@code 0} — no creation-time listener.
     */
    public static int createDataReaderWithProfile(long subscriber, long topic, byte[] qosPath,
            long listener, int mask, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_datareader_with_profile(
                subscriber, topic, qosPath, listener, mask, directBufferAddress(slot));
        // Same fence as createDataReader.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates a datareader on a ContentFilteredTopic rather than a plain
     * Topic. Returns the same {@code Int2DdsDataReader} handle shape as
     * {@link #createDataReader} -- read/take/status calls afterward are
     * identical either way.
     */
    public static int createDataReaderCft(long subscriber, long cft, long qos, long listener,
            int mask, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_datareader_cft(
                subscriber, cft, qos, listener, mask, directBufferAddress(slot));
        // Same fence as createDataReader.
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
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

    /** Same as {@link #datareaderTakeSerialized} but non-removing (read, not take). */
    public static int datareaderReadSerialized(long reader, long buffer, long bufferCapacity,
            long actualSizeOut, long validDataOut) {
        return Ffi.int2dds_datareader_read_serialized(
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
     * Mints a fresh status condition for {@code publisher}, writing its handle
     * to {@code handleOut[0]} on success. Each call returns a new native box
     * the caller owns and must release through {@link #statusConditionDelete}.
     */
    public static int publisherGetStatusCondition(long publisher, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publisher_get_statuscondition(publisher, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Mints a fresh status condition for {@code subscriber}, writing its
     * handle to {@code handleOut[0]} on success. Each call returns a new
     * native box the caller owns and must release through {@link
     * #statusConditionDelete}.
     */
    public static int subscriberGetStatusCondition(long subscriber, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscriber_get_statuscondition(subscriber, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Mints a fresh status condition for {@code participant}, writing its
     * handle to {@code handleOut[0]} on success. Each call returns a new
     * native box the caller owns and must release through {@link
     * #statusConditionDelete}.
     */
    public static int participantGetStatusCondition(long participant, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_participant_get_statuscondition(participant, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Mints a fresh status condition for {@code topic}, writing its handle to
     * {@code handleOut[0]} on success. Each call returns a new native box the
     * caller owns and must release through {@link #statusConditionDelete}.
     */
    public static int topicGetStatusCondition(long topic, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_topic_get_statuscondition(topic, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Reads a reader's pending status-changes mask, writing it to {@code outMask[0]} on success. */
    public static int datareaderGetStatusChanges(long reader, int[] outMask) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datareader_get_status_changes(reader, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            outMask[0] = slot.getInt(0); // mask out is a u32
        }
        return rc;
    }

    /** Reads a writer's pending status-changes mask, writing it to {@code outMask[0]} on success. */
    public static int datawriterGetStatusChanges(long writer, int[] outMask) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_datawriter_get_status_changes(writer, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            outMask[0] = slot.getInt(0); // mask out is a u32
        }
        return rc;
    }

    /** Reads a publisher's pending status-changes mask, writing it to {@code outMask[0]} on success. */
    public static int publisherGetStatusChanges(long publisher, int[] outMask) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_publisher_get_status_changes(publisher, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            outMask[0] = slot.getInt(0); // mask out is a u32
        }
        return rc;
    }

    /** Reads a subscriber's pending status-changes mask, writing it to {@code outMask[0]} on success. */
    public static int subscriberGetStatusChanges(long subscriber, int[] outMask) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_subscriber_get_status_changes(subscriber, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            outMask[0] = slot.getInt(0); // mask out is a u32
        }
        return rc;
    }

    /** Reads a participant's pending status-changes mask, writing it to {@code outMask[0]} on success. */
    public static int participantGetStatusChanges(long participant, int[] outMask) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_participant_get_status_changes(participant, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            outMask[0] = slot.getInt(0); // mask out is a u32
        }
        return rc;
    }

    /** Reads a topic's pending status-changes mask, writing it to {@code outMask[0]} on success. */
    public static int topicGetStatusChanges(long topic, int[] outMask) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_topic_get_status_changes(topic, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            outMask[0] = slot.getInt(0); // mask out is a u32
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

    /**
     * Whether {@code participant} contains the entity identified by the
     * 16-byte {@code handle}. Writes the bool result to {@code out[0]} on
     * success. Same 1-byte bool-slot shape as {@link #datareaderHasData}.
     */
    public static int participantContainsEntity(long participant, byte[] handle, boolean[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_participant_contains_entity(
                participant, handle, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.get(0) != 0; // bool out is 1 byte, not 4
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

    // --- Dynamic entities (xtypes write-path Topic/DataWriter/DataReader) ---

    /**
     * Creates a topic backed by a dynamic type support. Returns rc; writes
     * the handle to {@code out[0]} only when rc == 0 -- the same shape as
     * {@link #createTopic}. {@code qos} is {@code 0L} for the core's default
     * Topic QoS.
     */
    public static int createTopicDynamic(long participant, byte[] topicName, long support, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_topic_dynamic(
                participant, topicName, support, 0L, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates a datawriter backed by a dynamic type support, with the core's
     * default DataWriter QoS. Delegates to {@link
     * #createDataWriterDynamic(long, long, long, long, long[])} with {@code
     * qos = 0L}.
     */
    public static int createDataWriterDynamic(long publisher, long topic, long support, long[] out) {
        return createDataWriterDynamic(publisher, topic, support, 0L, out);
    }

    /**
     * Creates a datawriter backed by a dynamic type support. Returns rc;
     * writes the handle to {@code out[0]} only when rc == 0. {@code qos} is
     * {@code 0L} for the core's default DataWriter QoS, or a handle from
     * {@link #createDataWriterQos()} with policies applied.
     */
    public static int createDataWriterDynamic(
            long publisher, long topic, long support, long qos, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_datawriter_dynamic(
                publisher, topic, support, qos, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Creates a datareader backed by a dynamic type support, with the core's
     * default DataReader QoS. Delegates to {@link
     * #createDataReaderDynamic(long, long, long, long, long[])} with {@code
     * qos = 0L}.
     */
    public static int createDataReaderDynamic(long subscriber, long topic, long support, long[] out) {
        return createDataReaderDynamic(subscriber, topic, support, 0L, out);
    }

    /**
     * Creates a datareader backed by a dynamic type support. Returns rc;
     * writes the handle to {@code out[0]} only when rc == 0. {@code qos} is
     * {@code 0L} for the core's default DataReader QoS, or a handle from
     * {@link #createDataReaderQos()} with policies applied.
     */
    public static int createDataReaderDynamic(
            long subscriber, long topic, long support, long qos, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_create_datareader_dynamic(
                subscriber, topic, support, qos, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Publishes a populated DynamicData sample. Returns the C ABI status code. */
    public static int dynamicWriterWrite(long writer, long data) {
        return Ffi.int2dds_dynamic_writer_write(writer, data);
    }

    /**
     * Takes the next available DynamicData sample. {@code out_info} is passed
     * as {@code 0L} (NULL) -- sample info is not needed here. Returns rc;
     * writes a fresh DynamicData handle to {@code outData[0]} only on
     * success. {@code RET_NO_DATA} (27) means the cache is empty.
     */
    public static int dynamicReaderTake(long reader, long[] outData) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_reader_take(reader, directBufferAddress(slot), 0L);
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            outData[0] = slot.getLong(0);
        }
        return rc;
    }

    /**
     * Reads a dynamic writer's current QoS into a freshly allocated native
     * handle. Same shape as {@link #getWriterQos}: returns the C ABI status
     * code and, only on success, writes the new QoS handle to {@code
     * handleOut[0]}. The handle is the same {@code Int2DdsDataWriterQos}
     * type the typed path returns, readable with {@link
     * com.intellectus.int2dds.internal.QosMarshal#readWriterQos}.
     */
    public static int dynamicWriterGetQos(long writer, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_writer_get_qos(writer, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Reads the number of DataReaders matched to a dynamic writer, writing it to {@code out[0]} on success. */
    public static int dynamicWriterPublicationMatchedCount(long writer, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_writer_publication_matched_count(writer, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0); // native writes an i32 (4 bytes), not 8
        }
        return rc;
    }

    /**
     * Reads a dynamic reader's current QoS into a freshly allocated native
     * handle. Same shape as {@link #dynamicWriterGetQos}: the handle is the
     * same {@code Int2DdsDataReaderQos} type the typed path returns, readable
     * with {@link com.intellectus.int2dds.internal.QosMarshal#readReaderQos}.
     */
    public static int dynamicReaderGetQos(long reader, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_reader_get_qos(reader, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Reads the number of DataWriters matched to a dynamic reader, writing it to {@code out[0]} on success. */
    public static int dynamicReaderSubscriptionMatchedCount(long reader, long[] out) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_reader_subscription_matched_count(reader, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            out[0] = slot.getInt(0); // native writes an i32 (4 bytes), not 8
        }
        return rc;
    }

    /**
     * Mints a fresh status condition for a dynamic {@code reader}, writing its
     * handle to {@code handleOut[0]} on success. Each call returns a new
     * native box the caller owns and must release through {@link
     * #statusConditionDelete}.
     */
    public static int dynamicReaderGetStatusCondition(long reader, long[] handleOut) {
        ByteBuffer slot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = Ffi.int2dds_dynamic_reader_get_statuscondition(reader, directBufferAddress(slot));
        NativeKeepAlive.keepAlive(slot);
        if (rc == 0) {
            handleOut[0] = slot.getLong(0);
        }
        return rc;
    }

    /** Releases a dynamic datawriter. */
    public static void dynamicWriterDestroy(long writer) {
        Ffi.int2dds_dynamic_writer_destroy(writer);
    }

    /** Releases a dynamic datareader. */
    public static void dynamicReaderDestroy(long reader) {
        Ffi.int2dds_dynamic_reader_destroy(reader);
    }

    // --- Instance management (register/unregister/dispose/lookup) ---

    /**
     * Registers the instance identified by the serialized sample at {@code
     * keyAddr}/{@code keyLen} (the core derives the KeyHash from it, the same
     * canonical derivation {@link #datawriterWriteSerialized} relies on),
     * writing the 16-byte instance handle to {@code handleOut}. Requires a
     * keyed topic ({@code RET_PRECONDITION_NOT_MET} otherwise).
     */
    public static int datawriterRegisterInstance(
            long writer, long keyAddr, long keyLen, byte[] handleOut) {
        return Ffi.int2dds_datawriter_register_instance(writer, keyAddr, keyLen, handleOut);
    }

    /**
     * Unregisters the instance identified by the serialized sample at {@code
     * keyAddr}/{@code keyLen}. {@code handle} is the registered instance
     * handle, or 16 zero bytes (NIL) to let the core derive it from the key.
     */
    public static int datawriterUnregisterInstance(
            long writer, long keyAddr, long keyLen, byte[] handle) {
        return Ffi.int2dds_datawriter_unregister_instance(writer, keyAddr, keyLen, handle);
    }

    /**
     * Disposes the instance identified by the serialized sample at {@code
     * keyAddr}/{@code keyLen}. {@code handle} is the registered instance
     * handle, or 16 zero bytes (NIL) to let the core derive it from the key.
     */
    public static int datawriterDispose(long writer, long keyAddr, long keyLen, byte[] handle) {
        return Ffi.int2dds_datawriter_dispose(writer, keyAddr, keyLen, handle);
    }

    /**
     * Looks up the instance handle for the serialized sample at {@code
     * keyAddr}/{@code keyLen}, writing it to {@code handleOut}. All-zero on
     * an unknown instance.
     */
    public static int datawriterLookupInstance(
            long writer, long keyAddr, long keyLen, byte[] handleOut) {
        return Ffi.int2dds_datawriter_lookup_instance(writer, keyAddr, keyLen, handleOut);
    }

    /**
     * Looks up the instance handle for the serialized sample at {@code
     * keyAddr}/{@code keyLen} on a datareader, writing it to {@code
     * handleOut}. All-zero on an unknown instance.
     */
    public static int datareaderLookupInstance(
            long reader, long keyAddr, long keyLen, byte[] handleOut) {
        return Ffi.int2dds_datareader_lookup_instance(reader, keyAddr, keyLen, handleOut);
    }

    /**
     * Fetches the serialized key bytes for the instance {@code handle} on
     * {@code writer} into {@code keyBuf}, writing the required size to {@code
     * keySizeOut}. {@code RET_BUFFER_TOO_SMALL} when {@code keyCapacity} is
     * too small; the grow-and-retry is the caller's job.
     */
    public static int datawriterGetKeyValue(
            long writer, byte[] handle, long keyBuf, long keyCapacity, long keySizeOut) {
        return Ffi.int2dds_datawriter_get_key_value(writer, handle, keyBuf, keyCapacity, keySizeOut);
    }

    /**
     * Fetches the serialized key bytes for the instance {@code handle} on
     * {@code reader}. Same buffer/grow contract as {@link
     * #datawriterGetKeyValue}.
     */
    public static int datareaderGetKeyValue(
            long reader, byte[] handle, long keyBuf, long keyCapacity, long keySizeOut) {
        return Ffi.int2dds_datareader_get_key_value(reader, handle, keyBuf, keyCapacity, keySizeOut);
    }
}
