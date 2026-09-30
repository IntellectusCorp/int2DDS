package kr.co.intellectus.int2dds.core;

import java.nio.charset.Charset;
import java.util.List;
import java.util.Objects;
import kr.co.intellectus.int2dds.exceptions.DdsErrorException;
import kr.co.intellectus.int2dds.internal.QosMarshal;
import kr.co.intellectus.int2dds.internal.ReturnCodes;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;
import kr.co.intellectus.int2dds.qos.ParticipantQos;
import kr.co.intellectus.int2dds.xtypes.DynamicTypeSupport;

/**
 * The process-wide DDS entry point. A singleton: the native factory is not owned by any one Java
 * object, so unlike {@link NativeEntity} subclasses this has no {@code close()} — there is nothing
 * for any particular caller to release.
 *
 * <p>Lazily initialised on first use via the classloader's own once-and-thread-safe guarantee for
 * static nested class initialisation (the initialization-on-demand holder idiom), the same effect
 * as C#'s {@code Lazy<T>} used by the reference binding.
 */
public final class DomainParticipantFactory {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private static final class Holder {
        private static final DomainParticipantFactory INSTANCE = new DomainParticipantFactory();
    }

    private final long handle;

    private DomainParticipantFactory() {
        this.handle = FfiAccess.participantFactoryGetInstance();
    }

    /** The singleton factory instance. */
    public static DomainParticipantFactory getInstance() {
        return Holder.INSTANCE;
    }

    /** The native factory handle, for entities built on top of this class. */
    public long handle() {
        return handle;
    }

    /** The factory's {@code autoenable_created_entities} policy, the only member of its QoS. */
    public boolean getAutoenableCreatedEntities() {
        boolean[] out = new boolean[1];
        ReturnCodes.check(FfiAccess.participantFactoryGetQos(handle, out));
        return out[0];
    }

    /** Sets the factory's {@code autoenable_created_entities} policy. */
    public void setAutoenableCreatedEntities(boolean autoenable) {
        ReturnCodes.check(FfiAccess.participantFactorySetQos(handle, autoenable));
    }

    /**
     * Sets the QoS a participant created with the core's default QoS receives. {@code null} resets
     * it to the built-in default.
     */
    public void setDefaultParticipantQos(ParticipantQos qos) {
        if (qos == null) {
            ReturnCodes.check(FfiAccess.participantFactorySetDefaultParticipantQos(handle, 0L));
            return;
        }
        long qosHandle = FfiAccess.createParticipantQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException("failed to allocate a native ParticipantQos handle");
        }
        try {
            QosMarshal.applyParticipantQos(qosHandle, qos);
            ReturnCodes.check(
                    FfiAccess.participantFactorySetDefaultParticipantQos(handle, qosHandle));
        } finally {
            FfiAccess.destroyParticipantQos(qosHandle);
        }
    }

    /**
     * The default participant QoS as the core resolves it (registered default, then the configured
     * default profile, then the spec default). Only {@code Property} entries can be read back; see
     * {@link QosMarshal#readParticipantQos}.
     */
    public ParticipantQos getDefaultParticipantQos() {
        long[] out = new long[1];
        ReturnCodes.check(FfiAccess.participantFactoryGetDefaultParticipantQos(handle, out));
        try {
            return QosMarshal.readParticipantQos(out[0]);
        } finally {
            FfiAccess.destroyParticipantQos(out[0]);
        }
    }

    /**
     * An existing participant on {@code domainId}, or null when there is none. The result aliases
     * that participant: reads work, but this core cannot create child entities through an alias.
     * Closing the alias deletes the participant for every holder, as DDS {@code lookup_participant}
     * semantics require; a dropped, unclosed alias releases nothing.
     */
    public DomainParticipant lookupParticipant(int domainId) {
        long[] out = new long[1];
        ReturnCodes.check(FfiAccess.participantFactoryLookupParticipant(handle, domainId, out));
        return out[0] == 0L ? null : DomainParticipant.lookedUp(out[0], domainId);
    }

    /**
     * Loads named QoS profiles (and any {@code <types>} they declare) from {@code paths} — JSON
     * profile files — into this process-wide factory singleton. Once loaded, a profile is addressed
     * as {@code "LibraryName::ProfileName"} by the profile-aware creators on {@link Publisher} and
     * {@link Subscriber}.
     *
     * @throws NullPointerException if {@code paths} is null
     */
    public void loadProfiles(List<String> paths) {
        Objects.requireNonNull(paths, "paths");
        byte[][] pathBytes = new byte[paths.size()][];
        for (int i = 0; i < paths.size(); i++) {
            pathBytes[i] = Objects.requireNonNull(paths.get(i), "paths[" + i + "]").getBytes(UTF8);
        }
        int rc = FfiAccess.loadProfiles(pathBytes, pathBytes.length);
        ReturnCodes.check(rc);
    }

    /**
     * Builds a whole participant tree — participant, publishers/subscribers,
     * datawriters/datareaders and topics — from a {@code domain_participant_library} entry,
     * declaratively, instead of building each entity by hand. The XML declaring {@code
     * libraryPath}'s library (a {@code "LibraryName::ParticipantName"} path, e.g. {@code
     * "PL::App"}) must already have been loaded into this factory via {@link #loadProfiles}.
     *
     * @throws NullPointerException if {@code libraryPath} is null
     */
    public ConfiguredParticipant createParticipantFromConfig(String libraryPath) {
        Objects.requireNonNull(libraryPath, "libraryPath");
        long[] out = new long[1];
        int rc = FfiAccess.createParticipantFromConfig(handle(), libraryPath.getBytes(UTF8), out);
        ReturnCodes.check(rc);
        return new ConfiguredParticipant(out[0]);
    }

    /**
     * Builds a {@link DynamicTypeSupport} for {@code typeName}, a type declared in a {@code
     * <types>} XML section previously loaded into this factory singleton via {@link #loadProfiles}.
     * The factory-singleton companion to {@link
     * kr.co.intellectus.int2dds.xtypes.XmlTypeRegistry#getTypeSupport}. Throws if no such type is
     * loaded.
     *
     * @throws NullPointerException if {@code typeName} is null
     */
    public DynamicTypeSupport getDynamicTypeSupport(String typeName) {
        Objects.requireNonNull(typeName, "typeName");
        long[] out = new long[1];
        int rc = FfiAccess.getDynamicTypeSupport(typeName.getBytes(UTF8), out);
        ReturnCodes.check(rc);
        return DynamicTypeSupport.fromHandle(out[0]);
    }
}
